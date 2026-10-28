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
    Output(Output),
}
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
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
            match origin {
                Origin::Input(input) => {
                    if let Some(value) = arguments.get(input) {
                        present = true;
                        unknown |= value.origins.is_empty();
                        origins.extend(value.origins.iter().cloned());
                    }
                }
                Origin::Output(output) => {
                    let mut output = output.clone();
                    output.arguments = output
                        .arguments
                        .iter()
                        .filter_map(|(input, value)| {
                            value.bind(arguments).map(|value| (input.clone(), value))
                        })
                        .collect();
                    present = true;
                    origins.insert(Origin::Output(output));
                }
                _ => {
                    present = true;
                    origins.insert(origin.clone());
                }
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
    pub uses: Vec<Use>,
}
impl Inputs {
    pub fn require(
        &mut self,
        value: &Residency,
        target: Target,
        source: &Source,
        span: Span,
    ) -> bool {
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
        if value
            .origins
            .iter()
            .any(|origin| matches!(origin, Origin::Output(_)))
        {
            self.uses.push(Use::At {
                value: value.clone(),
                target,
                source: source.clone(),
                span,
            });
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
    pub fn same(&mut self, a: &Residency, b: &Residency, source: &Source, span: Span) -> bool {
        if a.origins
            .iter()
            .chain(&b.origins)
            .any(|origin| matches!(origin, Origin::Output(_)))
        {
            self.uses.push(Use::Same {
                before: a.clone(),
                after: b.clone(),
                source: source.clone(),
                span,
            });
            true
        } else {
            self.resolve(a) == self.resolve(b)
        }
    }
    pub fn apply(&mut self, required: &Requirements, call: &Call) -> Result<()> {
        for (input, target) in required {
            if let Some(value) = call.arguments.get(input) {
                if !self.require(value, *target, &call.source, call.span) {
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
    let resources=view::resource_paths(&value.value.ty).into_iter().enumerate().map(|(index,path)|(path,index)).collect::<BTreeMap<_,_>>();
    visit_owned(value, &mut Input::new(parameter), &mut |data, input| {
        data.handle.proof=data.handle.proof.clone().with_view_sources(0,BTreeSet::from([view::Source::Data(ValueIdentity::ResourceParameter(parameter,resources[&input.fields]))]));
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
                        if value.value.ty.has_variants() {
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
crate::module::codec::variants!(Origin { 0 => Cpu, 1 => Gpu, 2 => Input(input), 3 => Output(output) });
crate::module::codec::record!(Residency { origins });

// 编译期调用结果是原资源来源表达式，不形成运行时位置表。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum OutputKind {Returned,Written(usize)}
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Output {
    function: String,
    kind: OutputKind,
    fields: Vec<usize>,
    arguments: Arguments,
}

pub(super) enum Use {
    At {
        value: Residency,
        target: Target,
        source: Source,
        span: Span,
    },
    Same {
        before: Residency,
        after: Residency,
        source: Source,
        span: Span,
    },
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Location {
    origins: BTreeSet<Origin>,
    // 求解的空集合表示尚无可返回来源；真实未知来源另外保留。
    unknown: bool,
}
impl Location {
    fn extend(&mut self, other: Self) {
        self.origins.extend(other.origins);
        self.unknown |= other.unknown;
    }
    fn residency(&self) -> Residency {
        if self.unknown {
            Residency::default()
        } else {
            Residency {
                origins: self.origins.clone(),
            }
        }
    }
}
pub(super) type Outputs = BTreeMap<String, BTreeMap<(OutputKind,Vec<usize>), Location>>;

fn evaluate(value: &Residency, outputs: &Outputs) -> Location {
    let mut result = Location {
        unknown: value.origins.is_empty(),
        ..Location::default()
    };
    for origin in &value.origins {
        match origin {
            Origin::Output(output) => {
                if let Some(value) = outputs[&output.function].get(&(output.kind.clone(),output.fields.clone())) {
                    result.unknown |= value.unknown;
                    for origin in &value.origins {
                        if let Origin::Input(input) = origin {
                            if let Some(argument) = output.arguments.get(input) {
                                result.extend(evaluate(argument, outputs));
                            }
                        } else {
                            result.origins.insert(origin.clone());
                        }
                    }
                }
            }
            origin => {
                result.origins.insert(origin.clone());
            }
        }
    }
    result
}

pub(super) fn output(value: &mut Returned, function: &str,kind:OutputKind, arguments: &Arguments) {
    visit(value, &mut Input::new(0), &mut |data, path| {
        data.residency = Residency {
            origins: BTreeSet::from([Origin::Output(Output {
                function: function.into(),
                kind:kind.clone(),
                fields: path.fields.clone(),
                arguments: arguments.clone(),
            })]),
        };
    });
}

pub(super) fn solve_outputs<'a>(
    units: impl Iterator<Item = (&'a str, Option<&'a Returned>, &'a [Option<Returned>])>,
) -> Outputs {
    fn collect<'a>(
        value: &'a Returned,
        fields: &mut Vec<usize>,
        values: &mut BTreeMap<Vec<usize>, &'a Residency>,
    ) {
        match value {
            Returned::Dataset(data) => {
                values.insert(fields.clone(), &data.residency);
            }
            Returned::OwnedValue(value) => {
                for (field, payload) in value.payloads.iter().enumerate() {
                    if let Some(payload) = payload {
                        fields.push(field);
                        collect(payload, fields, values);
                        fields.pop();
                    }
                }
            }
            _ => {}
        }
    }
    let equations = units
        .map(|(name, value,written)| {
            let mut values = BTreeMap::new();
            if let Some(value) = value {
                collect(value, &mut Vec::new(), &mut values);
            }
            let mut all=values.into_iter().map(|(fields,value)|((OutputKind::Returned,fields),value)).collect::<BTreeMap<_,_>>();
            for(index,value)in written.iter().enumerate(){if let Some(value)=value{let mut fields=BTreeMap::new();collect(value,&mut Vec::new(),&mut fields);all.extend(fields.into_iter().map(|(fields,value)|((OutputKind::Written(index),fields),value)));}}
            (name.to_owned(), all)
        })
        .collect::<BTreeMap<_, _>>();
    let mut outputs = equations
        .iter()
        .map(|(name, values)| {
            (
                name.clone(),
                values
                    .keys()
                    .map(|fields| (fields.clone(), Location::default()))
                    .collect(),
            )
        })
        .collect::<Outputs>();
    loop {
        let next = equations
            .iter()
            .map(|(name, values)| {
                (
                    name.clone(),
                    values
                        .iter()
                        .map(|(fields, value)| (fields.clone(), evaluate(value, &outputs)))
                        .collect(),
                )
            })
            .collect::<Outputs>();
        if next == outputs {
            return outputs;
        }
        outputs = next;
    }
}

pub(super) fn resolved_call(call: &Call, outputs: &Outputs) -> Call {
    let arguments = call
        .arguments
        .iter()
        .filter_map(|(input, value)| {
            let value = evaluate(value, outputs);
            (value.unknown || !value.origins.is_empty()).then(|| (input.clone(), value.residency()))
        })
        .collect();
    Call {
        function: call.function.clone(),
        arguments,
        source: call.source.clone(),
        span: call.span,
    }
}
pub(super) fn resolve_returned(value: &mut Returned, outputs: &Outputs) -> bool {
    fn resolve(value: &mut Returned, outputs: &Outputs) -> bool {
        match value {
            Returned::Dataset(data) => {
                let value = evaluate(&data.residency, outputs);
                data.residency = value.residency();
                value.unknown || !value.origins.is_empty()
            }
            Returned::OwnedValue(value) => {
                let mut present = true;
                for payload in &mut value.payloads {
                    if let Some(item) = payload {
                        if !resolve(item, outputs) {
                            if value.value.ty.has_variants() {
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
    resolve(value, outputs)
}
impl Inputs {
    pub fn apply_uses(&mut self, uses: &[Use], outputs: &Outputs, final_pass: bool) -> Result<()> {
        for usage in uses {
            match usage {
                Use::At {
                    value,
                    target,
                    source,
                    span,
                } if !final_pass => {
                    let value = evaluate(value, outputs);
                    if !value.unknown && value.origins.is_empty() {
                        continue;
                    }
                    if !self.require(&value.residency(), *target, source, *span) {
                        return source.fail(
                            *span,
                            "B266",
                            "递归调用返回资源的实际来源不满足本次CPU／GPU位置要求",
                        );
                    }
                }
                Use::Same {
                    before,
                    after,
                    source,
                    span,
                } if final_pass => {
                    let before = evaluate(before, outputs);
                    let after = evaluate(after, outputs);
                    if (!before.unknown && before.origins.is_empty())
                        || (!after.unknown && after.origins.is_empty())
                    {
                        continue;
                    }
                    if self.resolve(&before.residency()) != self.resolve(&after.residency()) {
                        return source.fail(
                            *span,
                            "B266",
                            "递归资源结果在循环回边未归还入口相同的CPU／GPU存储位置",
                        );
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}
crate::module::codec::variants!(OutputKind{0=>Returned,1=>Written(parameter)});
crate::module::codec::record!(Output {
    function,
    kind,
    fields,
    arguments
});
