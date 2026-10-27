//! 资源位置属于函数的静态访问合同；逻辑输入路径不成为运行期状态。
use super::*;
use returns::Returned;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Input {
    parameter: usize,
    fields: Vec<usize>,
}
impl Input {
    fn new(parameter: usize) -> Self {
        Self {
            parameter,
            fields: Vec::new(),
        }
    }
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    Cpu,
    Gpu,
    Input(Input),
}
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Residency {
    // 空集是尚未得到的位置，不能据此授予任何访问。
    origins: BTreeSet<Origin>,
}
impl Residency {
    pub fn cpu() -> Self {
        Self {
            origins: BTreeSet::from([Origin::Cpu]),
        }
    }
    pub fn gpu() -> Self {
        Self {
            origins: BTreeSet::from([Origin::Gpu]),
        }
    }
    pub fn parameter(parameter: usize) -> Self {
        Self::input(Input::new(parameter))
    }
    fn input(input: Input) -> Self {
        Self {
            origins: BTreeSet::from([Origin::Input(input)]),
        }
    }
    pub fn join(&mut self, other: &Self) {
        if self.origins.is_empty() || other.origins.is_empty() {
            self.origins.clear();
        } else {
            self.origins.extend(other.origins.iter().cloned());
        }
    }
    fn bind(&self, arguments: &Arguments) -> Option<Self> {
        if self.origins.is_empty() {
            return Some(Self::default());
        }
        let mut origins = BTreeSet::new();
        let mut present = false;
        let mut unknown = false;
        for origin in &self.origins {
            if let Origin::Input(input) = origin {
                if let Some(value) = arguments.get(input) {
                    present = true;
                    unknown |= value.origins.is_empty();
                    origins.extend(value.origins.iter().cloned());
                }
            } else {
                present = true;
                origins.insert(origin.clone());
            }
        }
        present.then(|| {
            if unknown {
                Self::default()
            } else {
                Self { origins }
            }
        })
    }
}
pub(super) type Requirements = BTreeMap<Input, Target>;
pub(super) type Arguments = BTreeMap<Input, Residency>;
pub(super) struct Call {
    pub function: String,
    pub arguments: Arguments,
    pub source: Source,
    pub span: Span,
}
#[derive(Default)]
pub(super) struct Inputs {
    pub required: Requirements,
    pub calls: Vec<Call>,
}
impl Inputs {
    pub fn require(&mut self, value: &Residency, target: Target) -> bool {
        // compute 是声明能力；原 target 位置及资源合同只接受 CPU／GPU。
        if target == Target::Compute || value.origins.is_empty() {
            return false;
        }
        for origin in &value.origins {
            match origin {
                Origin::Input(input) => {
                    if self
                        .required
                        .get(input)
                        .is_some_and(|known| *known != target)
                    {
                        return false;
                    }
                    self.required.insert(input.clone(), target);
                }
                Origin::Cpu if target != Target::Cpu => return false,
                Origin::Gpu if target != Target::Gpu => return false,
                _ => {}
            }
        }
        true
    }
    fn resolve(&self, value: &Residency) -> BTreeSet<Origin> {
        value
            .origins
            .iter()
            .map(|origin| {
                if let Origin::Input(input) = origin {
                    match self.required.get(input) {
                        Some(Target::Cpu) => return Origin::Cpu,
                        Some(Target::Gpu) => return Origin::Gpu,
                        _ => {}
                    }
                }
                origin.clone()
            })
            .collect()
    }
    pub fn same(&self, a: &Residency, b: &Residency) -> bool {
        self.resolve(a) == self.resolve(b)
    }
    pub fn apply(&mut self, required: &Requirements, call: &Call) -> Result<()> {
        for (input, target) in required {
            if let Some(value) = call.arguments.get(input) {
                if !self.require(value, *target) {
                    return call.source.fail(
                        call.span,
                        "B266",
                        format!(
                            "函数 {} 的第 {} 个资源参数（字段路径 {:?}）需要 {} 存储；实际位置不满足函数合同",
                            call.function,
                            input.parameter + 1,
                            input.fields,
                            if *target == Target::Cpu { "CPU" } else { "GPU" },
                        ),
                    );
                }
            }
        }
        Ok(())
    }
}
fn visit(
    value: &mut Returned,
    input: &mut Input,
    apply: &mut impl FnMut(&mut application::Data, &Input),
) {
    match value {
        Returned::Dataset(data) => apply(data, input),
        Returned::OwnedValue(value) => visit_owned(value, input, apply),
        _ => {}
    }
}
fn visit_owned(
    value: &mut owned_values::OwnedValue,
    input: &mut Input,
    apply: &mut impl FnMut(&mut application::Data, &Input),
) {
    for (field, payload) in value.payloads.iter_mut().enumerate() {
        if let Some(payload) = payload {
            input.fields.push(field);
            visit(payload, input, apply);
            input.fields.pop();
        }
    }
}
pub(super) fn parameter(value: &mut owned_values::OwnedValue, parameter: usize) {
    visit_owned(value, &mut Input::new(parameter), &mut |data, input| {
        data.residency = Residency::input(input.clone());
    });
}
pub(super) fn argument(arguments: &mut Arguments, parameter: usize, data: &application::Data) {
    arguments.insert(Input::new(parameter), data.residency.clone());
}
pub(super) fn owned_argument(
    arguments: &mut Arguments,
    parameter: usize,
    value: &mut owned_values::OwnedValue,
) {
    visit_owned(value, &mut Input::new(parameter), &mut |data, input| {
        arguments.insert(input.clone(), data.residency.clone());
    });
}
// 沿原资源载荷交接存在性；不存在的输入分支不能变成未知位置的活动资源。
fn bind_returned(value: &mut Returned, arguments: &Arguments) -> bool {
    match value {
        Returned::Dataset(data) => match data.residency.bind(arguments) {
            Some(bound) => {
                data.residency = bound;
                true
            }
            None => {
                data.residency = Residency::default();
                false
            }
        },
        Returned::OwnedValue(value) => {
            let mut present = true;
            for payload in &mut value.payloads {
                if let Some(item) = payload {
                    if !bind_returned(item, arguments) {
                        if value.value.ty.result {
                            *payload = None;
                        } else {
                            present = false;
                        }
                    }
                }
            }
            present
        }
        _ => true,
    }
}
pub(super) fn returned(value: &mut Returned, arguments: &Arguments) {
    bind_returned(value, arguments);
}
crate::module::codec::record!(Input { parameter, fields });
crate::module::codec::variants!(Origin { 0 => Cpu, 1 => Gpu, 2 => Input(input) });
crate::module::codec::record!(Residency { origins });
