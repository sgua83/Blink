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
pub(super) enum Origin {
    Cpu,
    Gpu,
    Input(Input),
    Output(Output),
}
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Residency {
    // Absent 是原来源方程的空解；Origins 的空集合仍表示实际未知位置。
    Absent,
    Origins(BTreeSet<Origin>),
    Selected(Box<bounds::Condition>,Box<Residency>,Box<Residency>),
}
impl Default for Residency{fn default()->Self{Self::Origins(BTreeSet::new())}}
impl Residency {
    pub fn cpu()->Self{Self::Origins(BTreeSet::from([Origin::Cpu]))}
    pub fn gpu()->Self{Self::Origins(BTreeSet::from([Origin::Gpu]))}
    pub fn parameter(parameter:usize)->Self{Self::input(Input::new(parameter))}
    fn input(input:Input)->Self{Self::Origins(BTreeSet::from([Origin::Input(input)]))}
    fn absent(&self)->bool{matches!(self,Self::Absent)}
    fn origins(&self)->BTreeSet<Origin>{match self{Self::Absent=>BTreeSet::new(),Self::Origins(origins)=>origins.clone(),Self::Selected(_,a,b)=>{if a.absent(){return b.origins();}if b.absent(){return a.origins();}let mut value=a.origins();let other=b.origins();if value.is_empty()||other.is_empty(){value.clear();}else{value.extend(other);}value}}}
    fn arm(&self,condition:&bounds::Condition,truth:bool)->Self{match self{Self::Selected(known,a,b)if condition.exclusive() && known.as_ref()==condition=>if truth{a.as_ref().clone()}else{b.as_ref().clone()},_=>self.clone()}}
    pub(super) fn selected(when:owned_presence::Presence,a:Self,b:Self)->Self{Self::conditional(when.condition(),a,b)}
    pub(super) fn conditional(when:bounds::Condition,mut a:Self,mut b:Self)->Self{
        if let Some(truth)=when.boolean_constant(){return if truth{a}else{b};}
        if when.unconstrained(){a.join(&b);return a;}
        if !when.exclusive(){return if a==b{a}else{Self::Selected(Box::new(when),Box::new(a),Box::new(b))};}
        a=a.arm(&when,true);b=b.arm(&when,false);
        if a==b{return a;}
        // 同一组原条件采用统一顺序，调用来源的固定点不重复嵌套同一选择。
        let first=[&a,&b].into_iter().filter_map(|value|if let Self::Selected(condition,..)=value{Some(condition.as_ref())}else{None}).filter(|condition|condition.exclusive() && *condition<&when).min().cloned();
        if let Some(first)=first{
            let yes=Self::conditional(when.clone(),a.arm(&first,true),b.arm(&first,true));
            let no=Self::conditional(when,a.arm(&first,false),b.arm(&first,false));
            return Self::conditional(first,yes,no);
        }
        Self::Selected(Box::new(when),Box::new(a),Box::new(b))
    }
    pub fn join(&mut self,other:&Self){
        if self==other||other.absent(){return;}
        if self.absent(){*self=other.clone();return;}
        if let (Self::Origins(a),Self::Origins(b))=(&mut *self,other){
            if a.is_empty()||b.is_empty(){a.clear();}else{a.extend(b.iter().cloned());}return;
        }
        let condition=[&*self,other].into_iter().filter_map(|value|if let Self::Selected(condition,..)=value{Some(condition.as_ref())}else{None}).min().unwrap().clone();
        let left=matches!(&*self,Self::Selected(known,..)if **known==condition);
        let (mut yes,mut no)=if left{let Self::Selected(_,a,b)=&*self else{unreachable!()};(a.as_ref().clone(),b.as_ref().clone())}else{(self.arm(&condition,true),self.arm(&condition,false))};
        let (other_yes,other_no)=if !left{let Self::Selected(_,a,b)=other else{unreachable!()};(a.as_ref().clone(),b.as_ref().clone())}else{(other.arm(&condition,true),other.arm(&condition,false))};
        yes.join(&other_yes);no.join(&other_no);
        *self=Self::conditional(condition,yes,no);
    }
    pub(super) fn under(&self,path:&bounds::Clause)->Self{match self{
        Self::Selected(when,a,b)=>match when.truth_in(path){Some(true)=>a.under(path),Some(false)=>b.under(path),None=>Self::conditional(when.as_ref().clone(),a.under(&when.assume(path,true)),b.under(&when.assume(path,false)))},
        _=>self.clone(),
    }}
    pub(super) fn resolve_selected(&self,assuming:Option<&owned_presence::Presence>,truth:&impl Fn(&bounds::Condition,Option<&owned_presence::Presence>)->Option<bool>)->Self{
        match self{Self::Selected(when,a,b)=>match truth(when,assuming){Some(true)=>a.resolve_selected(assuming,truth),Some(false)=>b.resolve_selected(assuming,truth),None=>Self::conditional(when.as_ref().clone(),a.resolve_selected(assuming,truth),b.resolve_selected(assuming,truth))},_=>self.clone()}
    }
    pub(super) fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{match self{
        Self::Selected(when,a,b)=>Self::conditional(when.at_call(bindings),a.at_call(bindings),b.at_call(bindings)),
        Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{Origin::Output(output)=>{let mut output=output.clone();output.arguments=output.arguments.iter().map(|(input,value)|(input.clone(),value.at_call(bindings))).collect();output.bindings=output.bindings.iter().map(|(input,value)|(*input,value.at_call(bindings))).collect();Origin::Output(output)},_=>origin.clone()}).collect()),
        Self::Absent=>Self::Absent,
    }}
    pub(super) fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{match self{
        Self::Selected(when,a,b)=>Self::conditional(when.output_bound(bindings),a.output_bound(bindings),b.output_bound(bindings)),
        Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{Origin::Output(output)=>{let mut output=output.clone();output.arguments=output.arguments.iter().map(|(input,value)|(input.clone(),value.output_bound(bindings))).collect();output.bindings=output.bindings.iter().map(|(input,value)|(*input,value.output_bound(bindings))).collect();Origin::Output(output)},_=>origin.clone()}).collect()),
        Self::Absent=>Self::Absent,
    }}
    fn bind(&self,arguments:&Arguments)->Option<Self>{
        if let Self::Selected(when,a,b)=self{return Some(Self::conditional(when.as_ref().clone(),a.bind(arguments).unwrap_or(Self::Absent),b.bind(arguments).unwrap_or(Self::Absent))).filter(|value|!value.absent());}
        let Self::Origins(origins)=self else{return None};
        if origins.is_empty(){return Some(Self::default());}
        let mut result=Self::Absent;
        for origin in origins{match origin{
            Origin::Input(input)=>{if let Some(value)=arguments.get(input){result.join(value);}},
            Origin::Output(output)=>{let mut output=output.clone();output.arguments=output.arguments.iter().filter_map(|(input,value)|value.bind(arguments).map(|value|(input.clone(),value))).collect();result.join(&Self::Origins(BTreeSet::from([Origin::Output(output)])));},
            origin=>result.join(&Self::Origins(BTreeSet::from([origin.clone()]))),
        }}
        (!result.absent()).then_some(result)
    }
}
pub(super) type Requirements = BTreeMap<Input, Target>;
pub(super) type Arguments = BTreeMap<Input, Residency>;
pub(super) struct Call {
    pub function: String,
    pub arguments: Arguments,
    pub path: bounds::Clause,
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
        path: &bounds::Clause,
    ) -> bool {
        let value=value.under(path);
        // compute 是声明能力；原 target 位置及资源合同只接受 CPU／GPU。
        if target == Target::Compute || value.origins().is_empty() {
            return false;
        }
        for origin in &value.origins() {
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
            .origins()
            .iter()
            .any(|origin| matches!(origin, Origin::Output(_)))
        {
            self.uses.push(Use::At {
                value: value.clone(),
                path: path.clone(),
                target,
                source: source.clone(),
                span,
            });
        }
        true
    }
    fn resolve(&self,value:&Residency)->Residency{match value{
        Residency::Origins(origins)=>Residency::Origins(origins.iter().map(|origin|if let Origin::Input(input)=origin{match self.required.get(input){Some(Target::Cpu)=>Origin::Cpu,Some(Target::Gpu)=>Origin::Gpu,_=>origin.clone()}}else{origin.clone()}).collect()),
        Residency::Selected(when,a,b)=>Residency::conditional(when.as_ref().clone(),self.resolve(a),self.resolve(b)),
        Residency::Absent=>Residency::Absent,
    }}
    pub fn same(&mut self, a: &Residency, b: &Residency, source: &Source, span: Span,path:&bounds::Clause) -> bool {
        let a=a.under(path);let b=b.under(path);
        if a.origins()
            .iter()
            .chain(&b.origins())
            .any(|origin| matches!(origin, Origin::Output(_)))
        {
            self.uses.push(Use::Same {
                before: a.clone(),
                after: b.clone(),
                path: path.clone(),
                source: source.clone(),
                span,
            });
            true
        } else {
            self.resolve(&a) == self.resolve(&b)
        }
    }
    pub fn apply(&mut self, required: &Requirements, call: &Call) -> Result<()> {
        for (input, target) in required {
            if let Some(value) = call.arguments.get(input) {
                if !self.require(value, *target, &call.source, call.span,&call.path) {
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
crate::module::codec::variants!(Residency{0=>Absent,1=>Origins(origins),2=>Selected(when,a,b)});

// 编译期调用结果是原资源来源表达式，不形成运行时位置表。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum OutputKind {Returned,Written(usize)}
#[derive(Clone)]
pub(super) struct Output {
    function: String,
    kind: OutputKind,
    fields: Vec<usize>,
    arguments: Arguments,
    bindings: BTreeMap<ValueIdentity,bounds::Proof>,
}
impl PartialEq for Output{fn eq(&self,other:&Self)->bool{self.cmp(other)==std::cmp::Ordering::Equal}}
impl Eq for Output{}
impl PartialOrd for Output{fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl Ord for Output{fn cmp(&self,other:&Self)->std::cmp::Ordering{use crate::module::codec::Encode;(self.function.as_str(),&self.kind,&self.fields,&self.arguments).cmp(&(other.function.as_str(),&other.kind,&other.fields,&other.arguments)).then_with(||self.bindings.iter().map(|(id,proof)|(*id,proof.bytes())).cmp(other.bindings.iter().map(|(id,proof)|(*id,proof.bytes()))))}}

pub(super) enum Use {
    At {
        value: Residency,
        path: bounds::Clause,
        target: Target,
        source: Source,
        span: Span,
    },
    Same {
        before: Residency,
        after: Residency,
        path: bounds::Clause,
        source: Source,
        span: Span,
    },
}

pub(super) type Outputs=BTreeMap<String,BTreeMap<(OutputKind,Vec<usize>),Residency>>;
fn evaluate(value:&Residency,outputs:&Outputs)->Residency{match value{
    Residency::Selected(when,a,b)=>Residency::conditional(when.as_ref().clone(),evaluate(a,outputs),evaluate(b,outputs)),
    Residency::Absent=>Residency::Absent,
    Residency::Origins(origins)=>{
        if origins.is_empty(){return Residency::default();}
        let mut result=Residency::Absent;
        for origin in origins{match origin{
            Origin::Output(output)=>{
                if let Some(value)=outputs[&output.function].get(&(output.kind.clone(),output.fields.clone())){
                    if let Some(value)=value.at_call(&output.bindings).bind(&output.arguments){result.join(&evaluate(&value,outputs));}
                }
            },
            origin=>result.join(&Residency::Origins(BTreeSet::from([origin.clone()]))),
        }}
        result
    },
}}

pub(super) fn output(value: &mut Returned, function: &str,kind:OutputKind, arguments: &Arguments,bindings:&BTreeMap<ValueIdentity,bounds::Proof>) {
    visit(value, &mut Input::new(0), &mut |data, path| {
        data.residency = Residency::Origins(BTreeSet::from([Origin::Output(Output {
                function: function.into(),
                kind:kind.clone(),
                fields: path.fields.clone(),
                arguments: arguments.clone(),
                bindings: bindings.clone(),
            })]));
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
                    .map(|fields| (fields.clone(), Residency::Absent))
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
            let value=value.under(&call.path);
            (!value.absent()).then(||(input.clone(),value))
        })
        .collect();
    Call {
        function: call.function.clone(),
        arguments,
        path: call.path.clone(),
        source: call.source.clone(),
        span: call.span,
    }
}
pub(super) fn resolve_returned(value: &mut Returned, outputs: &Outputs) -> bool {
    fn resolve(value: &mut Returned, outputs: &Outputs) -> bool {
        match value {
            Returned::Dataset(data) => {
                let value = evaluate(&data.residency, outputs);
                let present=!value.absent();
                data.residency=value;
                present
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
                    path,
                    target,
                    source,
                    span,
                } if !final_pass => {
                    let value = evaluate(value, outputs).under(path);
                    if value.absent() {
                        continue;
                    }
                    if !self.require(&value, *target, source, *span,path) {
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
                    path,
                    source,
                    span,
                } if final_pass => {
                    let before = evaluate(before, outputs).under(path);
                    let after = evaluate(after, outputs).under(path);
                    if before.absent() || after.absent()
                    {
                        continue;
                    }
                    if self.resolve(&before) != self.resolve(&after) {
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
    arguments,
    bindings
});
