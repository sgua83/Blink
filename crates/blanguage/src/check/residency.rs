//! 资源位置属于函数的静态访问合同；逻辑输入路径不成为运行期状态。
use super::*;
use returns::Returned;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Part { Field(usize), Elements, Selected(Option<bounds::Operand>) }
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Input {
    pub parameter: usize,
    pub fields: Vec<Part>,
}
impl Input {
    fn depth(&self)->usize{self.fields.iter().filter(|part|!matches!(part,Part::Field(_))).count()}
    fn family(&self)->Self{Self{parameter:self.parameter,fields:self.fields.iter().map(|part|match part{Part::Selected(_)=>Part::Elements,other=>other.clone()}).collect()}}
    pub(super) fn model(&self)->Self{
        let mut depth=self.depth();Self{parameter:self.parameter,fields:self.fields.iter().map(|part|match part{
            Part::Elements=>{depth-=1;Part::Selected(Some(bounds::Operand::Value(ValueIdentity::ElementIndex(depth))))},
            other=>other.clone(),
        }).collect()}
    }
    fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{
        Self{parameter:self.parameter,fields:self.fields.iter().map(|part|match part{Part::Selected(index)=>Part::Selected(index.and_then(|index|index.at_call(bindings))),other=>other.clone()}).collect()}
    }
    fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{
        Self{parameter:self.parameter,fields:self.fields.iter().map(|part|match part{Part::Selected(index)=>Part::Selected(index.and_then(|index|index.map(|id|Some(bindings.get(&id).copied().unwrap_or(id))))),other=>other.clone()}).collect()}
    }
    fn element_indices(&self,indices:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{
        Self{parameter:self.parameter,fields:self.fields.iter().map(|part|match part{
            Part::Selected(Some(bounds::Operand::Value(id)))if indices.contains_key(id)=>Part::Selected(bounds::Operand::Value(*id).at_call(indices)),
            other=>other.clone(),
        }).collect()}
    }
    fn selection(&self)->BTreeMap<ValueIdentity,bounds::Proof>{
        let mut depth=self.depth();self.fields.iter().filter_map(|part|match part{
            Part::Field(_)=>None,
            part=>{depth-=1;let proof=match part{Part::Selected(Some(index))=>bounds::Proof::element_operand(*index,(0,u32::MAX)),_=>bounds::Proof::default()};Some((ValueIdentity::ElementIndex(depth),proof))},
        }).collect()
    }
    pub fn new(parameter: usize) -> Self {
        Self {
            parameter,
            fields: Vec::new(),
        }
    }
}
fn scoped_indices(bindings:&BTreeMap<ValueIdentity,bounds::Proof>,depth:usize)->BTreeMap<ValueIdentity,bounds::Proof>{
    let mut out=bindings.iter().map(|(id,value)|(match id{ValueIdentity::ElementIndex(index)=>ValueIdentity::ElementIndex(index+depth),other=>*other},value.scope_element_indices(depth))).collect::<BTreeMap<_,_>>();
    for index in 0..depth{let id=ValueIdentity::ElementIndex(index);out.insert(id,bounds::Proof::default().with_identity(id));}out
}
fn scoped_names(bindings:&BTreeMap<ValueIdentity,ValueIdentity>,depth:usize)->BTreeMap<ValueIdentity,ValueIdentity>{
    let shift=|id|match id{ValueIdentity::ElementIndex(index)=>ValueIdentity::ElementIndex(index+depth),other=>other};
    bindings.iter().map(|(a,b)|(shift(*a),shift(*b))).collect()
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
    pub(super) fn input(input:Input)->Self{Self::Origins(BTreeSet::from([Origin::Input(input)]))}
    fn absent(&self)->bool{matches!(self,Self::Absent)}
    fn arm(&self,condition:&bounds::Condition,truth:bool)->Self{match self{Self::Selected(known,a,b)if condition.exclusive() && known.as_ref()==condition=>if truth{a.as_ref().clone()}else{b.as_ref().clone()},_=>self.clone()}}
    pub(super) fn selected(when:owned_presence::Presence,a:Self,b:Self)->Self{Self::conditional(when.condition(),a,b)}
    pub(super) fn conditional(when:bounds::Condition,mut a:Self,mut b:Self)->Self{
        if when.impossible(){return Self::Absent;}
        if let Some(truth)=when.boolean_constant(){return if truth{a}else{b};}
        if when.unconstrained(){a.join(&b);return a;}
        // 投影后条件可重叠；只吸收同一条件下已有的同来源分支，不扩张其有效域。
        if let Self::Selected(inner,yes,no)=&b && **inner==when && a==**yes{return Self::conditional(when,a,no.as_ref().clone());}
        if let Self::Selected(inner,yes,no)=&a && **inner==when && b==**no{return Self::conditional(when,yes.as_ref().clone(),b);}
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
        Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{Origin::Input(input)=>Origin::Input(input.at_call(bindings)),Origin::Output(output)=>{let mut output=output.clone();output.arguments=output.arguments.iter().map(|(input,value)|(input.clone(),value.at_call(&scoped_indices(bindings,input.depth())))).collect();output.bindings=output.bindings.iter().map(|(input,value)|(*input,value.at_call(bindings))).collect();Origin::Output(output)},_=>origin.clone()}).collect()),
        Self::Absent=>Self::Absent,
    }}
    // 合并重复来源而保留其成立域；Absent 不被提升为真实资源来源。
    fn canonical(&self)->Self{
        fn collect(value:&Residency,path:&bounds::Clause,terms:&mut BTreeMap<BTreeSet<Origin>,bounds::Clause>){match value{
            Residency::Absent=>{},
            Residency::Origins(origins)=>{let guard=terms.entry(origins.clone()).or_insert_with(bounds::Clause::impossible);*guard=bounds::Clause::either(guard,path);},
            Residency::Selected(when,a,b)=>{collect(a,&when.assume(path,true),terms);collect(b,&when.assume(path,false),terms);},
        }}
        let mut terms=BTreeMap::new();collect(self,&bounds::Clause::default(),&mut terms);
        terms.into_iter().fold(Self::Absent,|other,(origins,path)|Self::conditional(bounds::Condition::paths(path.canonical(),bounds::Clause::default()),Self::Origins(origins),other))
    }
    pub(super) fn with_aliases(&self,aliases:&bounds::Clause)->Self{match self{
        Self::Selected(when,a,b)=>{
            let yes=when.assume(aliases,true);let no=when.assume(aliases,false);
            Self::conditional(when.with_aliases(aliases),a.with_aliases(&yes),b.with_aliases(&no))
        },_=>self.clone(),
    }}
    pub(super) fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{match self{
        Self::Selected(when,a,b)=>Self::conditional(when.output_bound(bindings),a.output_bound(bindings),b.output_bound(bindings)),
        Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{Origin::Input(input)=>Origin::Input(input.output_bound(bindings)),Origin::Output(output)=>{let mut output=output.clone();output.arguments=output.arguments.iter().map(|(input,value)|(input.clone(),value.output_bound(&scoped_names(bindings,input.depth())))).collect();output.bindings=output.bindings.iter().map(|(input,value)|(*input,value.output_bound(bindings))).collect();Origin::Output(output)},_=>origin.clone()}).collect()),
        Self::Absent=>Self::Absent,
    }}
    fn element_indices(&self,indices:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{
        if indices.is_empty(){return self.clone();}
        match self{
            Self::Absent=>Self::Absent,
            Self::Selected(when,a,b)=>Self::conditional(when.substitute_values(indices),a.element_indices(indices),b.element_indices(indices)),
            Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{
                Origin::Input(input)=>Origin::Input(input.element_indices(indices)),
                Origin::Output(output)=>{
                    let mut output=output.clone();
                    output.arguments=output.arguments.iter().map(|(input,value)|(input.clone(),value.element_indices(&scoped_indices(indices,input.depth())))).collect();
                    output.bindings=output.bindings.iter().map(|(id,proof)|(*id,proof.substitute_values(indices))).collect();
                    Origin::Output(output)
                },
                other=>other.clone(),
            }).collect()),
        }
    }
    fn bind(&self,arguments:&Arguments)->Option<Self>{
        if let Self::Selected(when,a,b)=self{return Some(Self::conditional(when.as_ref().clone(),a.bind(arguments).unwrap_or(Self::Absent),b.bind(arguments).unwrap_or(Self::Absent))).filter(|value|!value.absent());}
        let Self::Origins(origins)=self else{return None};
        if origins.is_empty(){return Some(Self::default());}
        let mut result=Self::Absent;
        for origin in origins{match origin{
            Origin::Input(input)=>{if let Some(value)=argument_value(arguments,input,&input.selection()){result.join(&value);}},
            Origin::Output(output)=>{let mut output=output.clone();output.arguments=output.arguments.iter().filter_map(|(input,value)|value.bind(arguments).map(|value|(input.clone(),value))).collect();result.join(&Self::Origins(BTreeSet::from([Origin::Output(output)])));},
            origin=>result.join(&Self::Origins(BTreeSet::from([origin.clone()]))),
        }}
        (!result.absent()).then_some(result)
    }
}
pub(super) type Requirements=BTreeMap<(Input,Target),BTreeSet<bounds::Condition>>;

pub(super) type Arguments=BTreeMap<Input,Residency>;
fn argument_value(arguments:&Arguments,input:&Input,selection:&BTreeMap<ValueIdentity,bounds::Proof>)->Option<Residency>{arguments.get(&input.family()).map(|value|value.element_indices(selection))}

pub(super) struct Call{
    pub function:String,
    pub arguments:Arguments,
    pub bindings:BTreeMap<ValueIdentity,bounds::Proof>,
    pub path:bounds::Clause,
    pub source:Source,
    pub span:Span,
}
#[derive(Default)]
pub(super) struct Inputs{
    pub calls:Vec<Call>,
    pub uses:Vec<Use>,
}
// 原使用点是局部检查与公共要求的共同来源，不维护另一份无条件输入状态。
impl Inputs{
    pub(super) fn known_at(&self,value:&Residency,target:Target,path:&bounds::Clause)->bool {
        let location=match target{Target::Cpu=>Residency::cpu(),Target::Gpu=>Residency::gpu(),Target::Compute=>return false};
        equal_under(value,&location,path,&mut|_,_,_|false)||self.uses.iter().any(|usage|matches!(usage,
            Use::At{value:used,target:actual,path:when,..}if *actual==target
                &&bounds::Condition::guard(when.clone()).truth_in(path)==Some(true)
                &&equal_under(value,used,path,&mut|_,_,_|false)))
    }

    pub fn require(&mut self,value:&Residency,target:Target,source:&Source,span:Span,path:&bounds::Clause)->bool{
        if target==Target::Compute||!known(value,target,path){return false;}
        self.uses.push(Use::At{value:value.clone(),target,path:path.clone(),source:source.clone(),span});true
    }
    pub fn same(&mut self,a:&Residency,b:&Residency,source:&Source,span:Span,path:&bounds::Clause)->bool{
        self.uses.push(Use::Same{before:a.clone(),after:b.clone(),path:path.clone(),source:source.clone(),span});true
    }
}
fn known(value:&Residency,target:Target,path:&bounds::Clause)->bool{
    if !path.possible(){return true;}
    match value{
        Residency::Absent=>true,
        Residency::Selected(when,a,b)=>known(a,target,&when.assume(path,true))&&known(b,target,&when.assume(path,false)),
        Residency::Origins(origins)=>!origins.is_empty()&&origins.iter().all(|origin|match origin{Origin::Cpu=>target==Target::Cpu,Origin::Gpu=>target==Target::Gpu,_=>true}),
    }
}
fn insert(required:&mut Requirements,input:Input,target:Target,when:bounds::Condition)->bool{
    if !when.active(){return true;}
    if required.iter().any(|((other,place),guards)|*other==input&&*place!=target&&guards.iter().any(|guard|guard.overlaps(&when))){return false;}
    let guards=required.entry((input,target)).or_default();
    if guards.iter().any(|guard|guard.covers(&when)){return true;}
    guards.retain(|guard|!when.covers(guard));guards.insert(when);true
}

fn consume(required:&mut Requirements,value:&Residency,target:Target,path:&bounds::Clause)->bool{
    if !path.possible(){return true;}
    match value{
        Residency::Absent=>true,
        Residency::Selected(when,a,b)=>consume(required,a,target,&when.assume(path,true))&&consume(required,b,target,&when.assume(path,false)),
        Residency::Origins(origins)=>!origins.is_empty()&&origins.iter().all(|origin|match origin{
            Origin::Cpu=>target==Target::Cpu,Origin::Gpu=>target==Target::Gpu,
            Origin::Input(input)=>insert(required,input.clone(),target,bounds::Condition::guard(path.clone())),
            Origin::Output(_)=>false,
        }),
    }
}
pub(super) fn input_parameters(parameters:&BTreeMap<ValueIdentity,ValueIdentity>,width:usize)->BTreeMap<ValueIdentity,bounds::Proof>{
    parameters.iter().filter(|(_,target)|match target{ValueIdentity::Register(n)=>*n<width,ValueIdentity::ParameterContent(..)|ValueIdentity::ResourceParameter(..)=>true,_=>false}).map(|(source,target)|(*source,bounds::Proof::default().with_identity(*target))).collect()
}
pub(super) fn canonical(required:&Requirements,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,source:&Source,span:Span)->Result<Requirements>{
    let mut result=Requirements::new();
    for((input,target),guards)in required{let bindings=scoped_indices(parameters,input.depth());for guard in guards{
        if !insert(&mut result,input.at_call(&bindings),*target,guard.guard_at_call(&bindings)){return source.fail(span,"B266","输入资源在同一接口条件下同时要求CPU和GPU存储");}
    }}Ok(result)
}
fn local(required:&Requirements,parameters:&BTreeMap<ValueIdentity,bounds::Proof>)->Requirements{
    let mut inverse=BTreeMap::new();
    for(source,target)in parameters{
        let target=target.known_identity(0).unwrap();
        let entry=inverse.entry(target).or_insert_with(||bounds::Proof::default().with_identity(*source));
        if !matches!(target,ValueIdentity::Register(_))&&*source==target{*entry=bounds::Proof::default().with_identity(*source);}
    }
    required.iter().fold(Requirements::new(),|mut result,((input,need),guards)|{let bindings=scoped_indices(&inverse,input.depth());result.entry((input.at_call(&bindings),*need)).or_default().extend(guards.iter().map(|guard|guard.guard_at_call(&bindings)));result})
}
pub(super) fn infer(inputs:&Inputs,known:&BTreeMap<String,Requirements>,outputs:&Outputs)->Result<Requirements>{
    let mut required=Requirements::new();
    for usage in &inputs.uses{match usage {
        Use::At{value,target,path,source,span}=>{
            let value=evaluate(value,outputs).under(path);
            if !value.absent()&&!consume(&mut required,&value,*target,path){return source.fail(*span,"B266","实际资源来源不满足本次条件CPU／GPU位置要求");}
        },
        Use::Same{before,after,path,source,span}=>{
            fn target(value:&Residency)->Option<Target>{
                let Residency::Origins(origins)=value else{return None};let mut origins=origins.iter();
                match(origins.next(),origins.next()){(Some(Origin::Cpu),None)=>Some(Target::Cpu),(Some(Origin::Gpu),None)=>Some(Target::Gpu),_=>None}
            }
            let before=evaluate(before,outputs);let after=evaluate(after,outputs);
            if !before.absent()&&!after.absent()&&!equal_under(&before,&after,path,&mut|a,b,path|{
                if let Some(target)=target(a){consume(&mut required,b,target,path)}
                else if let Some(target)=target(b){consume(&mut required,a,target,path)}
                else{true}
            }){return source.fail(*span,"B266","循环实际相等位置要求与资源存储位置不一致");}
        },
    }}
    for call in &inputs.calls{
        for((input,target),guards)in &known[&call.function]{
            let bindings=scoped_indices(&call.bindings,input.depth());
            for guard in guards{
                let path=guard.guard_at_call(&bindings).assume(&call.path,true);
                let selection=input.selection().into_iter().map(|(index,proof)|(index,proof.at_call(&bindings))).collect();
                let Some(value)=argument_value(&call.arguments,input,&selection)else{continue};let value=evaluate(&value,outputs);
                if value.absent(){continue;}
                if !consume(&mut required,&value,*target,&path){return call.source.fail(call.span,"B266",format!("函数 {} 的第 {} 个资源参数（资源路径 {:?}）不满足实际生效条件下的{}存储要求",call.function,input.parameter+1,input.fields,if *target==Target::Cpu{"CPU"}else{"GPU"}));}
            }
        }
    }
    Ok(required)
}

fn refine(value:&Residency,required:&Requirements)->Residency{match value{
    Residency::Selected(when,a,b)=>Residency::conditional(when.as_ref().clone(),refine(a,required),refine(b,required)),
    Residency::Absent=>Residency::Absent,
    Residency::Origins(origins)=>{
        if origins.is_empty(){return value.clone();}let mut all=Residency::Absent;
        for origin in origins{
            let mut item=Residency::Origins(BTreeSet::from([origin.clone()]));
            if let Origin::Input(input)=origin{for((other,target),guards)in required{if input==other{for guard in guards{item=Residency::conditional(guard.clone(),if *target==Target::Cpu{Residency::cpu()}else{Residency::gpu()},item);}}}}
            all.join(&item);
        }all
    },
}}
fn equal_under(a:&Residency,b:&Residency,path:&bounds::Clause,leaf:&mut impl FnMut(&Residency,&Residency,&bounds::Clause)->bool)->bool{
    if !path.possible(){return true;}let a=a.under(path);let b=b.under(path);if a==b{return true;}
    if let Residency::Selected(when,yes,no)=a{return equal_under(&yes,&b,&when.assume(path,true),leaf)&&equal_under(&no,&b,&when.assume(path,false),leaf);}
    if let Residency::Selected(when,yes,no)=b{return equal_under(&a,&yes,&when.assume(path,true),leaf)&&equal_under(&a,&no,&when.assume(path,false),leaf);}
    leaf(&a,&b,path)
}
pub(super) fn check_loops(inputs:&Inputs,required:&Requirements,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,outputs:&Outputs)->Result<()>{
    let required=local(required,parameters);
    for usage in &inputs.uses{if let Use::Same{before,after,path,source,span}=usage{
        let before=evaluate(before,outputs);let after=evaluate(after,outputs);if before.absent()||after.absent(){continue;}
        if !equal_under(&refine(&before,&required),&refine(&after,&required),path,&mut|_,_,_|false){return source.fail(*span,"B266","循环真实回边没有在该路径归还入口相同的CPU／GPU存储位置");}
    }}Ok(())
}
pub(super) fn visit(
    value: &mut Returned,
    input: &mut Input,
    apply: &mut impl FnMut(&mut application::Data, &Input),
) {
    match value {
        Returned::Dataset(data) => {
            apply(data, input);
            if data.length.maximum() != 0 {
                input.fields.push(Part::Elements);
                visit(&mut data.elements, input, apply);
                input.fields.pop();
            }
        },
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
            input.fields.push(Part::Field(field));
            visit(payload, input, apply);
            input.fields.pop();
        }
    }
}
// Only location facts are selected here; ordinary value and borrow joins remain owned by Returned.
pub(super) fn model_location<'a>(mut model:&'a Returned,path:&[Part])->Option<&'a application::Data>{
    for part in path{model=match(part,model){
        (Part::Field(field),Returned::OwnedValue(value))=>value.payloads[*field].as_deref()?,
        (Part::Elements,Returned::Dataset(data))=>&data.elements,
        _=>return None,
    };}
    if let Returned::Dataset(data)=model{Some(data)}else{None}
}
pub(super) fn select_elements(target:&mut Returned,incoming:&Returned,previous:&Returned,choice:&bounds::Condition,indices:usize){
    visit(target,&mut Input::new(0),&mut|data,input|{
        let depth=input.fields.iter().filter(|part|matches!(part,Part::Elements)).count();
        let shifts=(0..indices).map(|i|(ValueIdentity::ElementIndex(i),ValueIdentity::ElementIndex(i+depth))).collect();
        let choice=choice.output_bound(&shifts);
        data.residency=match(model_location(incoming,&input.fields),model_location(previous,&input.fields)){
            (Some(a),Some(b))=>Residency::conditional(choice,a.residency.clone(),b.residency.clone()),
            (Some(a),None)=>Residency::conditional(choice,a.residency.clone(),Residency::Absent),
            (None,Some(b))=>Residency::conditional(choice,Residency::Absent,b.residency.clone()),
            (None,None)=>Residency::Absent,
        };
    });
}
pub(super) fn parameter(value: &mut owned_values::OwnedValue, parameter: usize) {
    for (resource,path) in view::resource_paths(&value.value.ty).into_iter().enumerate() {
        if let Some(data)=view::owned_data_at_mut(value,&path) {
            data.root.proof=data.root.proof.clone().with_view_sources(0,BTreeSet::from([view::Source::Data(ValueIdentity::ResourceParameter(parameter,resource))]));
            data.residency=Residency::input(Input{parameter,fields:path.into_iter().map(Part::Field).collect()});
        }
    }
}
pub(super) fn argument(arguments: &mut Arguments, parameter: usize, value: &mut application::Data) {
    arguments.insert(Input::new(parameter), value.residency.clone());
    if value.length.maximum()!=0 {
        visit(&mut value.elements,&mut Input{parameter,fields:vec![Part::Elements]},&mut |data,input| {
            arguments.insert(input.clone(),data.residency.clone());
        });
    }
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
                if data.length.maximum()!=0 { bind_returned(&mut data.elements,arguments); }
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
crate::module::codec::variants!(Part { 0 => Field(field), 1 => Elements, 2 => Selected(index) });
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
    fields: Vec<Part>,
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

pub(super) type Outputs=BTreeMap<String,BTreeMap<(OutputKind,Vec<Part>),Residency>>;
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
        let mut bindings=bindings.clone();
        for index in 0..path.depth(){let id=ValueIdentity::ElementIndex(index);bindings.insert(id,bounds::Proof::default().with_identity(id));}
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
        fields: &mut Vec<Part>,
        values: &mut BTreeMap<Vec<Part>, &'a Residency>,
    ) {
        match value {
            Returned::Dataset(data) => {
                values.insert(fields.clone(), &data.residency);
                if data.length.maximum()!=0 {
                    fields.push(Part::Elements);
                    collect(&data.elements,fields,values);
                    fields.pop();
                }
            }
            Returned::OwnedValue(value) => {
                for (field, payload) in value.payloads.iter().enumerate() {
                    if let Some(payload) = payload {
                        fields.push(Part::Field(field));
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
                        .map(|(fields, value)| (fields.clone(), evaluate(value, &outputs).canonical()))
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

pub(super) fn resolve_returned(value: &mut Returned, outputs: &Outputs) -> bool {
    fn resolve(value: &mut Returned, outputs: &Outputs) -> bool {
        match value {
            Returned::Dataset(data) => {
                let value = evaluate(&data.residency, outputs);
                let present=!value.absent();
                data.residency=value;
                if present && data.length.maximum()!=0 { resolve(&mut data.elements,outputs); }
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
crate::module::codec::variants!(OutputKind{0=>Returned,1=>Written(parameter)});
crate::module::codec::record!(Output {
    function,
    kind,
    fields,
    arguments,
    bindings
});
