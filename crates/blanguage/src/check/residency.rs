//! 资源位置属于函数的静态访问合同；逻辑输入路径不成为运行期状态。
use super::*;
use returns::Returned;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Part { Field(usize), Elements, Selected(Option<bounds::Operand>) }
impl Part {
    fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{match self{
        Self::Selected(index)=>Self::Selected(index.and_then(|index|index.at_call(bindings))),other=>other.clone(),
    }}
    fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{match self{
        Self::Selected(index)=>Self::Selected(index.and_then(|index|index.map(|id|Some(bindings.get(&id).copied().unwrap_or(id))))),other=>other.clone(),
    }}
    fn element_indices(&self,indices:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{match self{
        Self::Selected(Some(bounds::Operand::Value(id)))if indices.contains_key(id)=>Self::Selected(bounds::Operand::Value(*id).at_call(indices)),other=>other.clone(),
    }}
}
fn path_family(fields:&[Part])->Vec<Part>{fields.iter().map(|part|match part{Part::Selected(_)=>Part::Elements,other=>other.clone()}).collect()}
fn path_model(fields:&[Part])->Vec<Part>{
    let mut depth=fields.iter().filter(|part|!matches!(part,Part::Field(_))).count();
    fields.iter().map(|part|match part{
        Part::Elements=>{depth-=1;Part::Selected(Some(bounds::Operand::Value(ValueIdentity::ElementIndex(depth))))},
        Part::Selected(_)=>{depth-=1;part.clone()},other=>other.clone(),
    }).collect()
}
fn path_selection(fields:&[Part])->BTreeMap<ValueIdentity,bounds::Proof>{
    let mut depth=fields.iter().filter(|part|!matches!(part,Part::Field(_))).count();
    fields.iter().filter_map(|part|match part{
        Part::Field(_)=>None,
        part=>{depth-=1;let proof=match part{Part::Selected(Some(index))=>bounds::Proof::element_operand(*index,(0,u32::MAX)),_=>bounds::Proof::default()};Some((ValueIdentity::ElementIndex(depth),proof))},
    }).collect()
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Input {
    pub parameter: usize,
    pub fields: Vec<Part>,
}
impl Input {
    pub(super) fn depth(&self)->usize{self.fields.iter().filter(|part|!matches!(part,Part::Field(_))).count()}
    pub(super) fn family(&self)->Self{Self{parameter:self.parameter,fields:path_family(&self.fields)}}
    fn selection(&self)->BTreeMap<ValueIdentity,bounds::Proof>{path_selection(&self.fields)}
    pub(super) fn model(&self)->Self{Self{parameter:self.parameter,fields:path_model(&self.fields)}}
    fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{
        Self{parameter:self.parameter,fields:self.fields.iter().map(|part|part.at_call(bindings)).collect()}
    }
    fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{
        Self{parameter:self.parameter,fields:self.fields.iter().map(|part|part.output_bound(bindings)).collect()}
    }
    fn element_indices(&self,indices:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{Self{parameter:self.parameter,fields:self.fields.iter().map(|part|part.element_indices(indices)).collect()}}
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
    TaskTransfer,
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
    // Successful task transfer has visited and moved every live owned CPU allocation.
    pub(super) fn task_transfer() -> Self { Self::Origins(BTreeSet::from([Origin::TaskTransfer])) }
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
    // A container's location does not determine the location of an owned child.
    // The original input/output resource path carries this projection.
    pub(super) fn project(&self,fields:&[Part],selection:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{
        if fields.is_empty(){return self.clone();}
        match self{
            Self::Absent=>Self::Absent,
            Self::Selected(when,a,b)=>Self::conditional(when.as_ref().clone(),a.project(fields,selection),b.project(fields,selection)),
            Self::Origins(origins)=>{
                if origins.is_empty(){return Self::default();}
                let mut result=Self::Absent;
                for origin in origins{
                    let value=match origin{
                        Origin::Input(input)=>{let mut input=input.clone();input.fields.extend_from_slice(fields);Self::input(input)},
                        Origin::Output(output)=>{
                            let mut output=output.clone();let depth=fields.iter().filter(|part|!matches!(part,Part::Field(_))).count();
                            output.fields.extend(path_family(fields));
                            output.bindings=output.bindings.into_iter().map(|(id,proof)|(match id{ValueIdentity::ElementIndex(index)=>ValueIdentity::ElementIndex(index+depth),other=>other},proof)).collect();
                            output.bindings.extend(selection.iter().filter(|(id,_)|matches!(id,ValueIdentity::ElementIndex(index)if *index<depth)).map(|(id,proof)|(*id,proof.clone())));
                            Self::Origins(BTreeSet::from([Origin::Output(output)]))
                        },
                        Origin::Cpu|Origin::Gpu=>Self::default(),
                        Origin::TaskTransfer=>Self::task_transfer(),
                    };result.join(&value);
                }
                result
            },
        }
    }
    pub(super) fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{match self{
        Self::Selected(when,a,b)=>Self::conditional(when.at_call(bindings),a.at_call(bindings),b.at_call(bindings)),
        Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{Origin::Input(input)=>Origin::Input(input.at_call(bindings)),Origin::Output(output)=>Origin::Output(Output{function:output.function.clone(),kind:output.kind.clone(),fields:output.fields.clone(),arguments:output.arguments.iter().map(|(input,value)|(input.clone(),value.at_call(&scoped_indices(bindings,input.depth())))).collect(),bindings:output.bindings.iter().map(|(input,value)|(*input,value.at_call(bindings))).collect()}),_=>origin.clone()}).collect()),
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
        Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{Origin::Input(input)=>Origin::Input(input.output_bound(bindings)),Origin::Output(output)=>Origin::Output(Output{function:output.function.clone(),kind:output.kind.clone(),fields:output.fields.clone(),arguments:output.arguments.iter().map(|(input,value)|(input.clone(),value.output_bound(&scoped_names(bindings,input.depth())))).collect(),bindings:output.bindings.iter().map(|(input,value)|(*input,value.output_bound(bindings))).collect()}),_=>origin.clone()}).collect()),
        Self::Absent=>Self::Absent,
    }}
    pub(super) fn element_indices(&self,indices:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{
        if indices.is_empty(){return self.clone();}
        match self{
            Self::Absent=>Self::Absent,
            Self::Selected(when,a,b)=>Self::conditional(when.substitute_values(indices),a.element_indices(indices),b.element_indices(indices)),
            Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{
                Origin::Input(input)=>Origin::Input(input.element_indices(indices)),
                Origin::Output(output)=>{
                    Origin::Output(Output {
                        function: output.function.clone(), kind: output.kind.clone(), fields: output.fields.clone(),
                        arguments: output.arguments.iter().map(|(input,value)|(input.clone(),value.element_indices(&scoped_indices(indices,input.depth())))).collect(),
                        bindings: output.bindings.iter().map(|(id,proof)|(*id,proof.substitute_values(indices))).collect(),
                    })
                },
                other=>other.clone(),
            }).collect()),
        }
    }
    // 模型路径绑定自己的下标；嵌套实参的模型下标不属于外层作用域。
    fn index_names(&self)->BTreeSet<ValueIdentity>{
        let mut names=BTreeSet::new();
        match self{
            Self::Selected(when,a,b)=>{names.extend(bounds::Proof::condition(when.as_ref().clone()).scalar_identities());names.extend(a.index_names());names.extend(b.index_names());},
            Self::Origins(origins)=>for origin in origins{match origin{
                Origin::Input(input)=>names.extend(input.fields.iter().filter_map(|part|match part{Part::Selected(Some(bounds::Operand::Value(id)))=>Some(*id),_=>None})),
                Origin::Output(output)=>{
                    for proof in output.bindings.values(){names.extend(proof.scalar_identities());}
                    for(input,value)in &output.arguments{names.extend(value.index_names().into_iter().filter_map(|id|match id{ValueIdentity::ElementIndex(index)if index>=input.depth()=>Some(ValueIdentity::ElementIndex(index-input.depth())),_=>None}));}
                },
                _=>{},
            }},
            Self::Absent=>{},
        }
        names.retain(|id|matches!(id,ValueIdentity::ElementIndex(_)));names
    }
    pub(super) fn scope_indices(&self,depth:usize,bound:usize)->Self{
        let names=self.index_names().into_iter().filter_map(|id|match id{ValueIdentity::ElementIndex(index)if index>=bound=>Some((id,ValueIdentity::ElementIndex(index+depth))),_=>None}).collect();
        self.output_bound(&names)
    }
    pub(super) fn bind(&self,arguments:&Arguments)->Option<Self>{
        if let Self::Selected(when,a,b)=self{return Some(Self::conditional(when.as_ref().clone(),a.bind(arguments).unwrap_or(Self::Absent),b.bind(arguments).unwrap_or(Self::Absent))).filter(|value|!value.absent());}
        let Self::Origins(origins)=self else{return None};
        if origins.is_empty(){return Some(Self::default());}
        let mut result=Self::Absent;
        for origin in origins{match origin{
            Origin::Input(input)=>{if let Some(value)=argument_value(arguments,input,&input.selection()){result.join(&value);}},
            Origin::Output(output)=>{let output=Output{function:output.function.clone(),kind:output.kind.clone(),fields:output.fields.clone(),arguments:output.arguments.iter().filter_map(|(input,value)|value.bind(arguments).map(|value|(input.clone(),value))).collect(),bindings:output.bindings.clone()};result.join(&Self::Origins(BTreeSet::from([Origin::Output(output)])));},
            origin=>result.join(&Self::Origins(BTreeSet::from([origin.clone()]))),
        }}
        (!result.absent()).then_some(result)
    }
}
pub(super) type Requirements=BTreeMap<(Input,Target),BTreeSet<bounds::Condition>>;

pub(super) type Arguments=BTreeMap<Input,Residency>;
fn argument_value(arguments:&Arguments,input:&Input,selection:&BTreeMap<ValueIdentity,bounds::Proof>)->Option<Residency>{
    let mut prefix=input.family();
    let selected=prefix.model().element_indices(selection);
    loop{
        if let Some(value)=arguments.get(&prefix){
            let end=prefix.fields.len();let removed=input.depth()-prefix.depth();
            let prefix_selection=selection.iter().filter_map(|(id,proof)|match id{
                ValueIdentity::ElementIndex(index)if *index>=removed=>Some((ValueIdentity::ElementIndex(index-removed),proof.clone())),
                _=>None,
            }).collect();
            return Some(value.element_indices(&prefix_selection).project(&selected.fields[end..],selection));
        }
        prefix.fields.pop()?;
    }
}

#[derive(Clone)]
pub(super) struct Call{
    pub function:String,
    pub arguments:Arguments,
    pub bindings:BTreeMap<ValueIdentity,bounds::Proof>,
    pub path:bounds::Clause,
    pub source:Option<Source>,
    pub span:Span,
}
// 每个定义只交接自己的原使用与调用；递归关系不展开成后代路径清单。
#[derive(Clone,Default)]
pub(super) struct Contract {
    pub required:Requirements,
    pub calls:Vec<Call>,
}
impl Contract {
    pub(super) fn normalize_contract(&mut self,normal:&mut bounds::Normalizer) {
        for conditions in self.required.values_mut() {
            let mut result=normal.condition(&bounds::Condition::constant(false));
            for condition in std::mem::take(conditions){result=normal.union(&result,&condition);}
            conditions.insert(result);
        }
        for call in &mut self.calls {
            for value in call.arguments.values_mut(){value.normalize_contract(normal);}
            for value in call.bindings.values_mut(){normal.proof(value);}
            call.path=normal.clause(&call.path);
        }
    }
}
impl Residency {
    pub(super) fn normalize_contract(&mut self,normal:&mut bounds::Normalizer) {
        match self {
            Self::Absent=>{},
            Self::Selected(when,yes,no)=>{**when=normal.condition(when);yes.normalize_contract(normal);no.normalize_contract(normal);},
            Self::Origins(origins)=>{
                let previous=std::mem::take(origins);
                for mut origin in previous {
                    if let Origin::Output(output)=&mut origin {
                        for value in output.arguments.values_mut(){value.normalize_contract(normal);}
                        for value in output.bindings.values_mut(){normal.proof(value);}
                    }
                    origins.insert(origin);
                }
            },
        }
    }
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
        Residency::Origins(origins)=>!origins.is_empty()&&origins.iter().all(|origin|match origin{Origin::Cpu|Origin::TaskTransfer=>target==Target::Cpu,Origin::Gpu=>target==Target::Gpu,_=>true}),
    }
}
fn insert(required:&mut Requirements,input:Input,target:Target,when:bounds::Condition)->bool{
    if !when.active(){return true;}
    if required.iter().any(|((other,place),guards)|*other==input&&*place!=target&&guards.iter().any(|guard|guard.assume(&when.assume(&bounds::Clause::default(),true),true).overlaps(&bounds::Clause::default()))){return false;}
    required.entry((input,target)).or_default().insert(when);true
}

fn consume(required:&mut Requirements,value:&Residency,target:Target,path:&bounds::Clause)->bool{
    if !path.possible(){return true;}
    match value{
        Residency::Absent=>true,
        Residency::Selected(when,a,b)=>consume(required,a,target,&when.assume(path,true))&&consume(required,b,target,&when.assume(path,false)),
        Residency::Origins(origins)=>!origins.is_empty()&&origins.iter().all(|origin|match origin{
            Origin::Cpu|Origin::TaskTransfer=>target==Target::Cpu,Origin::Gpu=>target==Target::Gpu,
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
pub(super) fn infer(inputs:&Inputs,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,outputs:&Outputs,source:&Source,span:Span)->Result<Contract>{
    let mut required=Requirements::new();
    for usage in &inputs.uses{match usage {
        Use::At{value,target,path,source,span}=>{
            let value=evaluate(value,outputs,path);
            if !value.absent()&&!consume(&mut required,&value,*target,path){return source.fail(*span,"B266","实际资源来源不满足本次条件CPU／GPU位置要求");}
        },
        Use::Same{before,after,path,source,span}=>{
            let before=evaluate(before,outputs,path);let after=evaluate(after,outputs,path);
            if !before.absent()&&!after.absent()&&!equal_under(&before,&after,path,&mut|a,b,path|{
                if let Some(target)=physical_target(a){consume(&mut required,b,target,path)}
                else if let Some(target)=physical_target(b){consume(&mut required,a,target,path)}
                else{true}
            }){return source.fail(*span,"B266","循环实际相等位置要求与资源存储位置不一致");}
        },
    }}
    let required=canonical(&required,parameters,source,span)?;
    let calls=inputs.calls.iter().map(|call|Call{
        function:call.function.clone(),
        arguments:call.arguments.iter().map(|(input,value)|(input.clone(),value.at_call(&scoped_indices(parameters,input.depth())))).collect(),
        bindings:call.bindings.iter().map(|(id,proof)|(*id,proof.at_call(parameters))).collect(),
        path:bounds::Condition::guard(call.path.clone()).projected_case(true, parameters).canonical(),
        source:None,span:Span::default(),
    }).collect();
    Ok(Contract{required,calls})
}

// 本次查询只观察具体位置，以及被查询路径的前缀。其余Input仍是原合同
// 中未代入的输入，不因为重复经过同一调用而产生新的具体位置事实。
fn query_value(value:&Residency,wanted:Option<&Input>,outputs:&Outputs,w:&mut crate::module::codec::Writer){
    use crate::module::codec::Encode;
    fn terms(value:&Residency,wanted:Option<&Input>,path:&bounds::Clause,outputs:&Outputs,out:&mut BTreeMap<BTreeSet<Vec<u8>>,bounds::Clause>){match value{
        Residency::Absent=>{},
        Residency::Selected(when,a,b)=>{terms(a,wanted,&when.assume(path,true),outputs,out);terms(b,wanted,&when.assume(path,false),outputs,out);},
        Residency::Origins(origins)=>{
            let key=origins.iter().map(|origin|{
                let mut w=crate::module::codec::Writer(Vec::new());
                match origin{
                    Origin::Input(input)=>{2u8.encode(&mut w);wanted.filter(|wanted|wanted.parameter==input.parameter&&wanted.fields.starts_with(&input.fields)).map(|_|input.clone()).encode(&mut w);},
                    Origin::Output(output)=>Origin::Output(canonical_output(output,outputs)).encode(&mut w),
                    other=>other.encode(&mut w),
                }
                w.0
            }).collect();
            let guard=out.entry(key).or_insert_with(bounds::Clause::impossible);*guard=bounds::Clause::either(guard,path);
        },
    }}
    let mut out=BTreeMap::new();terms(value,wanted,&bounds::Clause::default(),outputs,&mut out);
    out.len().encode(w);
    for(origins,path)in out{origins.encode(w);bounds::Condition::guard(path).encode(w);}
}
fn query_key(call:&Call,wanted:Option<&Input>,outputs:&Outputs)->Vec<u8>{
    use crate::module::codec::Encode;
    let mut w=crate::module::codec::Writer(Vec::new());call.function.encode(&mut w);call.bindings.len().encode(&mut w);
    for(id,proof)in &call.bindings {
        id.encode(&mut w);let identity=proof.known_identity(0).filter(|_|proof.origins.len()==1);identity.encode(&mut w);
        if identity.is_none(){proof.encode(&mut w);}
    }
    call.arguments.len().encode(&mut w);
    for(input,value)in &call.arguments{input.encode(&mut w);query_value(value,wanted,outputs,&mut w);}
    w.0
}
fn query_consume(required:&mut Requirements,deferred:&mut bool,wanted:Option<&Input>,value:&Residency,target:Target,path:&bounds::Clause)->bool{
    match value{
        Residency::Absent=>true,
        Residency::Selected(when,a,b)=>query_consume(required,deferred,wanted,a,target,&when.assume(path,true))&&query_consume(required,deferred,wanted,b,target,&when.assume(path,false)),
        Residency::Origins(origins)=>{
            if origins.is_empty(){return !path.overlaps(&bounds::Clause::default());}
            origins.iter().all(|origin|match origin{
                Origin::Cpu|Origin::TaskTransfer=>target==Target::Cpu||!path.overlaps(&bounds::Clause::default()),
                Origin::Gpu=>target==Target::Gpu||!path.overlaps(&bounds::Clause::default()),
                Origin::Input(input)=>{
                    if (*deferred && wanted!=Some(input))||(wanted.is_none()&&!path.overlaps(&bounds::Clause::default())){return true;}
                    *deferred=true;
                    wanted!=Some(input)||insert(required,input.clone(),target,bounds::Condition::guard(path.clone()))
                },
                Origin::Output(_)=>!path.overlaps(&bounds::Clause::default()),
            })
        },
    }
}
// 声明中的Input映射还引用实参已有的后代模型，不把父位置当作完整子树。
pub(super) fn inputs(value:&Residency,out:&mut BTreeSet<Input>){match value{
        Residency::Selected(_,a,b)=>{inputs(a,out);inputs(b,out);},
        Residency::Origins(origins)=>out.extend(origins.iter().filter_map(|origin|match origin{Origin::Input(input)=>Some(input.clone()),_=>None})),
        Residency::Absent=>{},
    }}
fn bind_arguments(declared:&Arguments,actual:&Arguments,bindings:&BTreeMap<ValueIdentity,bounds::Proof>,outputs:&Outputs)->Arguments{
    fn family(value:&Residency,depth:usize)->Residency{match value{
        Residency::Selected(when,a,b)=>Residency::conditional(when.as_ref().clone(),family(a,depth),family(b,depth)),
        Residency::Origins(origins)=>Residency::Origins(origins.iter().map(|origin|match origin{
            Origin::Input(input)=>{
                let mut left=input.fields.iter().filter(|part|matches!(part,Part::Elements)).count();
                let fields=input.fields.iter().map(|part|match part{Part::Elements=>{left-=1;Part::Selected(Some(bounds::Operand::Value(ValueIdentity::ElementIndex(depth+left))))},other=>other.clone()}).collect();
                Origin::Input(Input{parameter:input.parameter,fields})
            },
            other=>other.clone(),
        }).collect()),
        Residency::Absent=>Residency::Absent,
    }}
    fn bind(value:&Residency,actual:&Arguments,outputs:&Outputs)->Residency{
        let value=match value.bind(actual){Some(value)=>value,None=>Residency::Absent};
        if present(&value,outputs,&bounds::Clause::default(),&mut BTreeSet::new()){value}else{Residency::Absent}
    }
    let sources=declared.iter().map(|(input,value)|(input.clone(),value.at_call(&scoped_indices(bindings,input.depth())))).collect::<Arguments>();
    let mut result=sources.iter().map(|(input,value)|(input.clone(),bind(value,actual,outputs))).collect::<Arguments>();
    for(prefix,value)in &sources{
        let mut origins=BTreeSet::new();inputs(value,&mut origins);let mut children=BTreeSet::new();
        for source in origins{
            let source=source.family();
            for input in actual.keys(){
                if input.parameter==source.parameter&&input.fields.starts_with(&source.fields)&&input.fields.len()>source.fields.len(){
                    let mut child=prefix.clone();child.fields.extend_from_slice(&input.fields[source.fields.len()..]);
                    if !sources.keys().any(|other|other.parameter==child.parameter&&other.fields.len()>prefix.fields.len()&&child.fields.starts_with(&other.fields)){children.insert(child);}
                }
            }
        }
        for child in children{
            let depth=child.depth()-prefix.depth();
            let shift=(0..prefix.depth()).map(|index|(ValueIdentity::ElementIndex(index),bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(index+depth)))).collect();
            let value=family(&value.element_indices(&shift),depth).project(&child.model().fields[prefix.fields.len()..],&child.model().selection());
            result.insert(child,bind(&value,actual,outputs));
        }
    }
    result
}
fn query(call:Call,known:&BTreeMap<String,Contract>,outputs:&Outputs,wanted:Option<&Input>,source:&Source,span:Span,mut deferred:bool)->Result<(Requirements,bool)>{
    enum Work{Enter(Call),Leave(Vec<u8>)}
    let mut work=vec![Work::Enter(call)];let mut active=BTreeSet::new();let mut required=Requirements::new();
    while let Some(next)=work.pop(){
        let call=match next{Work::Enter(call)=>call,Work::Leave(key)=>{active.remove(&key);continue}};
        if !call.path.possible(){continue;}
        let key=query_key(&call,wanted,outputs);
        if !active.insert(key.clone()){continue;}
        work.push(Work::Leave(key));
        let contract=&known[&call.function];
        for((input,target),guards)in &contract.required{let bindings=scoped_indices(&call.bindings,input.depth());for guard in guards{
            let path=bounds::Clause::both(&call.path, &guard.projected_case(true, &bindings));
            let selection=input.selection().into_iter().map(|(index,proof)|(index,proof.at_call(&bindings))).collect();
            let Some(value)=argument_value(&call.arguments,input,&selection)else{continue};let value=evaluate(&value,outputs,&path);
            if !query_consume(&mut required,&mut deferred,wanted,&value,*target,&path){return source.fail(span,"B266",format!("函数 {} 的第 {} 个资源参数（资源路径 {:?}）不满足实际生效条件下的{}存储要求",call.function,input.parameter+1,input.fields,if *target==Target::Cpu{"CPU"}else{"GPU"}));}
        }}
        for next in contract.calls.iter().rev(){
            let path=bounds::Clause::both(&call.path, &bounds::Condition::guard(next.path.clone()).projected_case(true, &call.bindings));
            let arguments=bind_arguments(&next.arguments,&call.arguments,&call.bindings,outputs);
            let bindings=next.bindings.iter().map(|(id,proof)|(*id,proof.at_call(&call.bindings))).collect();
            work.push(Work::Enter(Call{function:next.function.clone(),arguments,bindings,path,source:None,span:Span::default()}));
        }
    }
    Ok((required,deferred))
}
pub(super) fn check_calls<'a>(calls:impl Iterator<Item=&'a Call>,known:&BTreeMap<String,Contract>,outputs:&Outputs)->Result<bool>{
    let mut deferred=false;
    for call in calls{
        deferred=query(call.clone(),known,outputs,None,call.source.as_ref().unwrap(),call.span,deferred)?.1;
    }
    Ok(deferred)
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
fn physical_target(value: &Residency) -> Option<Target> {
    let Residency::Origins(origins) = value else { return None; };
    let location = |origin: &Origin| match origin {
        Origin::Cpu | Origin::TaskTransfer => Some(Target::Cpu),
        Origin::Gpu => Some(Target::Gpu),
        _ => None,
    };
    let first = origins.first().and_then(location)?;
    origins.iter().all(|origin| location(origin) == Some(first)).then_some(first)
}
fn equal_under(a:&Residency,b:&Residency,path:&bounds::Clause,leaf:&mut impl FnMut(&Residency,&Residency,&bounds::Clause)->bool)->bool{
    if !path.possible() { return true; }
    let (a, b) = (a.under(path), b.under(path));
    if a == b || physical_target(&a).is_some_and(|target| physical_target(&b) == Some(target)) {
        return true;
    }
    if let Residency::Selected(when,yes,no)=a{return equal_under(&yes,&b,&when.assume(path,true),leaf)&&equal_under(&no,&b,&when.assume(path,false),leaf);}
    if let Residency::Selected(when,yes,no)=b{return equal_under(&a,&yes,&when.assume(path,true),leaf)&&equal_under(&a,&no,&when.assume(path,false),leaf);}
    leaf(&a,&b,path)
}
pub(super) fn check_loops(name:&str,inputs:&Inputs,contract:&Contract,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,outputs:&Outputs,known:&BTreeMap<String,Contract>,parameter_count:usize)->Result<()>{
    let mut required=contract.required.clone();
    let arguments=(0..parameter_count).map(|parameter|(Input::new(parameter),Residency::parameter(parameter))).collect::<Arguments>();
    let bindings=parameters.values().filter_map(|proof|proof.known_identity(0)).map(|id|(id,bounds::Proof::default().with_identity(id))).collect::<BTreeMap<_,_>>();
    for usage in &inputs.uses{if let Use::Same{before,after,source,span,..}=usage{
        let mut wanted=BTreeSet::new();self::inputs(&evaluate(before,outputs,&bounds::Clause::default()),&mut wanted);self::inputs(&evaluate(after,outputs,&bounds::Clause::default()),&mut wanted);
        for input in wanted{
            let input=input.at_call(parameters);
            let call=Call{function:name.into(),arguments:arguments.clone(),bindings:bindings.clone(),path:bounds::Clause::default(),source:None,span:Span::default()};
            for(key,guards)in query(call,known,outputs,Some(&input),source,*span,false)?.0{required.entry(key).or_default().extend(guards);}
        }
    }}
    let required=local(&required,parameters);
    for usage in &inputs.uses{if let Use::Same{before,after,path,source,span}=usage{
        let before=evaluate(before,outputs,path);let after=evaluate(after,outputs,path);if before.absent()||after.absent(){continue;}
        if !equal_under(&refine(&before,&required),&refine(&after,&required),path,&mut|_,_,path|!path.overlaps(&bounds::Clause::default())){return source.fail(*span,"B266","循环真实回边没有在该路径归还入口相同的CPU／GPU存储位置");}
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
                if let Some(elements)=data.elements.known_mut(){visit(elements, input, apply);}
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
        (Part::Elements,Returned::Dataset(data))=>data.elements.known()?,
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
            data.root.proof.set_view_borrow(0,view::Borrow::source(view::Source::Data(ValueIdentity::ResourceParameter(parameter,resource))));
            data.residency=Residency::input(Input{parameter,fields:path.into_iter().map(Part::Field).collect()});
        }
    }
}
// An empty dataset still has its own location, but has no element resource.
fn argument_data(arguments:&mut Arguments,input:&Input,data:&application::Data){
    arguments.insert(input.clone(),data.residency.clone());
    if data.length.maximum()==0{
        let mut contents=input.clone();contents.fields.push(Part::Elements);
        arguments.insert(contents,Residency::Absent);
    }
}
pub(super) fn argument(arguments: &mut Arguments, parameter: usize, value: &mut application::Data, projection: &[usize]) {
    argument_data(arguments, &Input::new(parameter), value);
    if value.length.maximum() == 0 {
        return;
    }
    let mut elements = value.elements.known_mut();
    for &field in projection {
        elements = match elements {
            Some(Returned::OwnedValue(value)) => value.payloads[field].as_deref_mut(),
            _ => None,
        };
    }
    if let Some(elements) = elements {
        visit(elements, &mut Input { parameter, fields: vec![Part::Elements] }, &mut |data, input| {
            argument_data(arguments, input, data);
        });
    }
}
pub(super) fn owned_argument(
    arguments: &mut Arguments,
    parameter: usize,
    value: &mut owned_values::OwnedValue,
) {
    visit_owned(value, &mut Input::new(parameter), &mut |data, input| {
        argument_data(arguments,input,data);
    });
}
// 这里只交接载荷存在性。位置继续由同一调用的输出路径消费，不能先折成父位置。
fn bind_returned(value: &mut Returned, arguments: &Arguments) -> bool {
    match value {
        Returned::Dataset(data) => match data.residency.bind(arguments) {
            Some(_) => {
                if data.length.maximum()!=0 && let Some(elements)=data.elements.known_mut(){bind_returned(elements,arguments);}
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
pub(super) fn dependencies<'a>(contract:&Contract,models:impl Iterator<Item=&'a Returned>)->BTreeSet<String>{
    fn collect(value:&Residency,names:&mut BTreeSet<String>){match value{
        Residency::Selected(_,a,b)=>{collect(a,names);collect(b,names);},
        Residency::Origins(origins)=>for origin in origins{if let Origin::Output(output)=origin{
            names.insert(output.function.clone());for argument in output.arguments.values(){collect(argument,names);}
        }},
        Residency::Absent=>{},
    }}
    fn returned(value:&Returned,names:&mut BTreeSet<String>){match value{
        Returned::Dataset(data)=>{
            collect(&data.residency,names);
            if data.length.maximum()!=0 && let Some(elements)=data.elements.known(){returned(elements,names);}
        },
        Returned::OwnedValue(value)=>for payload in value.payloads.iter().flatten(){returned(payload,names);},
        _=>{},
    }}
    let mut names=BTreeSet::new();
    for call in &contract.calls{names.insert(call.function.clone());for argument in call.arguments.values(){collect(argument,&mut names);}}
    for model in models{returned(model,&mut names);}
    names
}
// Project the requested resource through its original output definition.
fn output_source(output:&Output,outputs:&Outputs)->Option<Residency>{
    let values=&outputs[&output.function];let fields=path_model(&output.fields);
    let depth=fields.iter().filter(|part|!matches!(part,Part::Field(_))).count();
    let selection=path_selection(&fields);
    let bindings=output.bindings.iter().filter(|(id,_)|!matches!(id,ValueIdentity::ElementIndex(_))).map(|(id,proof)|(*id,proof.clone())).collect();
    let bindings=scoped_indices(&bindings,depth);
    let arguments=output.arguments.iter().map(|(input,value)|(input.clone(),value.scope_indices(depth,input.depth()))).collect();
    let mut prefix=(output.kind.clone(),output.fields.clone());
    loop{
        if let Some(value)=values.get(&prefix){
            let end=prefix.1.len();let removed=fields[end..].iter().filter(|part|!matches!(part,Part::Field(_))).count();
            let value=value.scope_indices(removed,0).project(&fields[end..],&selection).at_call(&bindings);
            return value.bind(&arguments).map(|value|{
                let mut selected=value.index_names().into_iter().filter_map(|id|match id{ValueIdentity::ElementIndex(index)if index>=depth=>Some((id,bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(index-depth)))),_=>None}).collect::<BTreeMap<_,_>>();
                selected.extend(output.bindings.iter().filter(|(id,_)|matches!(id,ValueIdentity::ElementIndex(index)if *index<depth)).map(|(id,proof)|(*id,proof.clone())));
                value.element_indices(&selected)
            });
        }
        prefix.1.pop()?;
    }
}
// A pure source-to-source equation names the same resource. Keep the terminal
// output reference, including its descendant relation, instead of its CPU/GPU value.
fn canonical_output(output:&Output,outputs:&Outputs)->Output{
    let mut output=output.clone();let mut visited=BTreeSet::new();
    while visited.insert(output.clone()){
        match output_source(&output,outputs){
            Some(Residency::Origins(origins)) if origins.len()==1=>match origins.into_iter().next().unwrap(){
                Origin::Output(next)=>output=next,
                _=>break,
            },
            _=>break,
        }
    }
    output
}
// Model presence needs one reachable producer, not every possible storage location.
fn present(value:&Residency,outputs:&Outputs,path:&bounds::Clause,active:&mut BTreeSet<Output>)->bool{
    if !path.possible(){return false;}
    match value{
        Residency::Absent=>false,
        Residency::Selected(when,a,b)=>present(a,outputs,&when.assume(path,true),active)
            ||present(b,outputs,&when.assume(path,false),active),
        Residency::Origins(origins)=>origins.is_empty()||origins.iter().any(|origin|match origin{
            Origin::Output(output)=>{
                if !active.insert(output.clone()){return false;}
                let found=output_source(output,outputs).is_some_and(|value|present(&value,outputs,path,active));
                active.remove(output);
                found
            },
            _=>true,
        }),
    }
}

fn evaluate(value:&Residency,outputs:&Outputs,path:&bounds::Clause)->Residency{
    fn resolve(value:&Residency,outputs:&Outputs,path:&bounds::Clause,active:&mut BTreeSet<Output>)->Residency{
        if !path.possible(){return Residency::Absent;}
        match value{
            Residency::Selected(when,a,b)=>match when.truth_in(path){
                Some(true)=>resolve(a,outputs,path,active),
                Some(false)=>resolve(b,outputs,path,active),
                None=>Residency::conditional(when.as_ref().clone(),resolve(a,outputs,&when.assume(path,true),active),resolve(b,outputs,&when.assume(path,false),active)),
            },
            Residency::Absent=>Residency::Absent,
            Residency::Origins(origins)=>{
                if origins.is_empty(){return Residency::default();}
                let mut result=Residency::Absent;
                for origin in origins{match origin{
                    Origin::Output(output)=>{
                        if !active.insert(output.clone()){continue;}
                        if let Some(value)=output_source(output,outputs){result.join(&resolve(&value,outputs,path,active));}
                        active.remove(output);
                    },
                    origin=>result.join(&Residency::Origins(BTreeSet::from([origin.clone()]))),
                }}result
            },
        }
    }
    resolve(value,outputs,path,&mut BTreeSet::new())
}

pub(super) fn output(value: &mut Returned, function: &str,kind:OutputKind, arguments: &Arguments,bindings:&BTreeMap<ValueIdentity,bounds::Proof>) {
    visit(value, &mut Input::new(0), &mut |data, path| {
        let mut bindings=bindings.iter().map(|(id,proof)|(*id,proof.numeric_facts())).collect::<BTreeMap<_,_>>();
        for index in 0..path.depth(){let id=ValueIdentity::ElementIndex(index);bindings.insert(id,bounds::Proof::default().with_identity(id));}
        data.residency = Residency::Origins(BTreeSet::from([Origin::Output(Output {
                function: function.into(),
                kind:kind.clone(),
                fields: path.fields.clone(),
                arguments: arguments.clone(),
                bindings,
            })]));
    });
}

pub(super) fn output_sources<'a>(
    units: impl Iterator<Item = (&'a str, Option<&'a Returned>, &'a [Option<Returned>])>,
) -> Outputs {
    fn collect<'a>(
        value: &'a Returned,
        kind: &OutputKind,
        fields: &mut Vec<Part>,
        values: &mut BTreeMap<(OutputKind,Vec<Part>), &'a Residency>,
    ) {
        match value {
            Returned::Dataset(data) => {
                values.insert((kind.clone(),fields.clone()), &data.residency);
                fields.push(Part::Elements);
                if data.length.maximum()==0{values.insert((kind.clone(),fields.clone()),&Residency::Absent);}
                else if let Some(elements)=data.elements.known(){collect(elements,kind,fields,values);}
                fields.pop();
            }
            Returned::OwnedValue(value) => {
                for (field, payload) in value.payloads.iter().enumerate() {
                    if let Some(payload) = payload {
                        fields.push(Part::Field(field));
                        collect(payload, kind, fields, values);
                        fields.pop();
                    }
                }
            }
            _ => {}
        }
    }
    units
        .map(|(name, value,written)| {
            let mut values = BTreeMap::new();
            if let Some(value) = value {
                collect(value, &OutputKind::Returned, &mut Vec::new(), &mut values);
            }
            for(index,value)in written.iter().enumerate(){if let Some(value)=value{
                collect(value,&OutputKind::Written(index),&mut Vec::new(),&mut values);
            }}
            let values=values.into_iter().map(|(fields,value)|(fields,(*value).clone())).collect();
            (name.to_owned(), values)
        })
        .collect()
}

pub(super) fn resolve_returned(value: &mut Returned, outputs: &Outputs) -> bool {
    fn resolve(value: &mut Returned, outputs: &Outputs) -> bool {
        match value {
            Returned::Dataset(data) => {
                let present=present(&data.residency,outputs,&bounds::Clause::default(),&mut BTreeSet::new());
                if present && data.length.maximum()!=0 && let Some(elements)=data.elements.known_mut(){resolve(elements,outputs);}
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



// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Part);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        field: ReadCallback,
        elements: ReadCallback,
        selected: ReadCallback,
    }
    unsafe extern "C" fn build_field(context: Slot, destination: Slot) -> ReadStatus {
        let mut field = <usize as Native>::empty();
        #[repr(C)]
        struct Fields {
            field: Slot,
        }
        #[repr(C)]
        struct Host {
            field: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_part_field"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    field: contracts::slot(&mut field),
                },
                Host {
                    field: contracts::read_value::<usize>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Part>() = Part::Field(field);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_elements(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Part>() = Part::Elements;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_selected(context: Slot, destination: Slot) -> ReadStatus {
        let mut index = <Option<bounds::Operand> as Native>::empty();
        #[repr(C)]
        struct Fields {
            index: Slot,
        }
        #[repr(C)]
        struct Host {
            index: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_part_selected"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    index: contracts::slot(&mut index),
                },
                Host {
                    index: contracts::read_value::<Option<bounds::Operand>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Part>() = Part::Selected(index);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_residency_part"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Part {
        fn empty() -> Self {
            Self::Elements
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Field(field) => {
                    #[repr(C)]
                    struct Fields {
                        field: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        field: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_part_field"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                field: contracts::address(field).cast_mut(),
                            },
                            Host {
                                field: contracts::write_value::<usize>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Elements => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_part_elements"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Selected(index) => {
                    #[repr(C)]
                    struct Fields {
                        index: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        index: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_part_selected"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                index: contracts::address(index).cast_mut(),
                            },
                            Host {
                                index: contracts::write_value::<Option<bounds::Operand>>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        field: build_field,
                        elements: build_elements,
                        selected: build_selected,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Input);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        parameter: Slot,
        fields: Slot,
    }
    #[repr(C)]
    struct Writer {
        parameter: WriteCallback,
        fields: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        parameter: ReadCallback,
        fields: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_residency_input"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_residency_input"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Input {
        fn empty() -> Self {
            Self {
                parameter: <usize as Native>::empty(),
                fields: <Vec<Part> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        parameter: contracts::address(&self.parameter).cast_mut(),
                        fields: contracts::address(&self.fields).cast_mut(),
                    },
                    Writer {
                        parameter: contracts::write_value::<usize>,
                        fields: contracts::write_value::<Vec<Part>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        parameter: contracts::slot(&mut output.parameter),
                        fields: contracts::slot(&mut output.fields),
                    },
                    Reader {
                        parameter: contracts::read_value::<usize>,
                        fields: contracts::read_value::<Vec<Part>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Origin);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        cpu: ReadCallback,
        gpu: ReadCallback,
        input: ReadCallback,
        output: ReadCallback,
        task_transfer: ReadCallback,
    }
    unsafe extern "C" fn build_cpu(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Origin>() = Origin::Cpu;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_gpu(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Origin>() = Origin::Gpu;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_input(context: Slot, destination: Slot) -> ReadStatus {
        let mut input = <Input as Native>::empty();
        #[repr(C)]
        struct Fields {
            input: Slot,
        }
        #[repr(C)]
        struct Host {
            input: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_origin_input"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    input: contracts::slot(&mut input),
                },
                Host {
                    input: contracts::read_value::<Input>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Origin>() = Origin::Input(input);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_output(context: Slot, destination: Slot) -> ReadStatus {
        let mut output = <Output as Native>::empty();
        #[repr(C)]
        struct Fields {
            output: Slot,
        }
        #[repr(C)]
        struct Host {
            output: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_origin_output"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    output: contracts::slot(&mut output),
                },
                Host {
                    output: contracts::read_value::<Output>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Origin>() = Origin::Output(output);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_task_transfer(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Origin>() = Origin::TaskTransfer;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_residency_origin"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Origin {
        fn empty() -> Self {
            Self::Cpu
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Cpu => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_cpu"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Gpu => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_gpu"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Input(input) => {
                    #[repr(C)]
                    struct Fields {
                        input: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        input: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_input"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                input: contracts::address(input).cast_mut(),
                            },
                            Host {
                                input: contracts::write_value::<Input>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Output(output) => {
                    #[repr(C)]
                    struct Fields {
                        output: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        output: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_output"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                output: contracts::address(output).cast_mut(),
                            },
                            Host {
                                output: contracts::write_value::<Output>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::TaskTransfer => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_task_transfer"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        cpu: build_cpu,
                        gpu: build_gpu,
                        input: build_input,
                        output: build_output,
                        task_transfer: build_task_transfer,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Residency);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        absent: ReadCallback,
        origins: ReadCallback,
        selected: ReadCallback,
    }
    unsafe extern "C" fn build_absent(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Residency>() = Residency::Absent;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_origins(context: Slot, destination: Slot) -> ReadStatus {
        let mut origins = <BTreeSet<Origin> as Native>::empty();
        #[repr(C)]
        struct Fields {
            origins: Slot,
        }
        #[repr(C)]
        struct Host {
            origins: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_origins"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    origins: contracts::slot(&mut origins),
                },
                Host {
                    origins: contracts::read_value::<BTreeSet<Origin>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Residency>() = Residency::Origins(origins);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_selected(context: Slot, destination: Slot) -> ReadStatus {
        let mut when = <Box<bounds::Condition> as Native>::empty();
        let mut a = <Box<Residency> as Native>::empty();
        let mut b = <Box<Residency> as Native>::empty();
        #[repr(C)]
        struct Fields {
            when: Slot,
            a: Slot,
            b: Slot,
        }
        #[repr(C)]
        struct Host {
            when: ReadCallback,
            a: ReadCallback,
            b: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_selected"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    when: contracts::slot(&mut when),
                    a: contracts::slot(&mut a),
                    b: contracts::slot(&mut b),
                },
                Host {
                    when: contracts::read_value::<Box<bounds::Condition>>,
                    a: contracts::read_value::<Box<Residency>>,
                    b: contracts::read_value::<Box<Residency>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Residency>() = Residency::Selected(when, a, b);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_residency"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Residency {
        fn empty() -> Self {
            Self::Absent
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Absent => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_absent"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Origins(origins) => {
                    #[repr(C)]
                    struct Fields {
                        origins: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        origins: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origins"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                origins: contracts::address(origins).cast_mut(),
                            },
                            Host {
                                origins: contracts::write_value::<BTreeSet<Origin>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Selected(when, a, b) => {
                    #[repr(C)]
                    struct Fields {
                        when: Slot,
                        a: Slot,
                        b: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        when: WriteCallback,
                        a: WriteCallback,
                        b: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_selected"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                when: contracts::address(when).cast_mut(),
                                a: contracts::address(a).cast_mut(),
                                b: contracts::address(b).cast_mut(),
                            },
                            Host {
                                when: contracts::write_value::<Box<bounds::Condition>>,
                                a: contracts::write_value::<Box<Residency>>,
                                b: contracts::write_value::<Box<Residency>>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        absent: build_absent,
                        origins: build_origins,
                        selected: build_selected,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(OutputKind);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        returned: ReadCallback,
        written: ReadCallback,
    }
    unsafe extern "C" fn build_returned(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<OutputKind>() = OutputKind::Returned;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_written(context: Slot, destination: Slot) -> ReadStatus {
        let mut parameter = <usize as Native>::empty();
        #[repr(C)]
        struct Fields {
            parameter: Slot,
        }
        #[repr(C)]
        struct Host {
            parameter: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_output_kind_written"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    parameter: contracts::slot(&mut parameter),
                },
                Host {
                    parameter: contracts::read_value::<usize>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<OutputKind>() = OutputKind::Written(parameter);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_residency_output_kind"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for OutputKind {
        fn empty() -> Self {
            Self::Returned
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Returned => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_output_kind_returned"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Written(parameter) => {
                    #[repr(C)]
                    struct Fields {
                        parameter: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        parameter: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_output_kind_written"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                parameter: contracts::address(parameter).cast_mut(),
                            },
                            Host {
                                parameter: contracts::write_value::<usize>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        returned: build_returned,
                        written: build_written,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Output);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        function: Slot,
        kind: Slot,
        fields: Slot,
        arguments: Slot,
        bindings: Slot,
    }
    #[repr(C)]
    struct Writer {
        function: WriteCallback,
        kind: WriteCallback,
        fields: WriteCallback,
        arguments: WriteCallback,
        bindings: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        function: ReadCallback,
        kind: ReadCallback,
        fields: ReadCallback,
        arguments: ReadCallback,
        bindings: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_residency_output"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_residency_output"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Output {
        fn empty() -> Self {
            Self {
                function: <String as Native>::empty(),
                kind: <OutputKind as Native>::empty(),
                fields: <Vec<Part> as Native>::empty(),
                arguments: <Arguments as Native>::empty(),
                bindings: <BTreeMap<ValueIdentity, bounds::Proof> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        function: contracts::address(&self.function).cast_mut(),
                        kind: contracts::address(&self.kind).cast_mut(),
                        fields: contracts::address(&self.fields).cast_mut(),
                        arguments: contracts::address(&self.arguments).cast_mut(),
                        bindings: contracts::address(&self.bindings).cast_mut(),
                    },
                    Writer {
                        function: contracts::write_value::<String>,
                        kind: contracts::write_value::<OutputKind>,
                        fields: contracts::write_value::<Vec<Part>>,
                        arguments: contracts::write_value::<Arguments>,
                        bindings: contracts::write_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        function: contracts::slot(&mut output.function),
                        kind: contracts::slot(&mut output.kind),
                        fields: contracts::slot(&mut output.fields),
                        arguments: contracts::slot(&mut output.arguments),
                        bindings: contracts::slot(&mut output.bindings),
                    },
                    Reader {
                        function: contracts::read_value::<String>,
                        kind: contracts::read_value::<OutputKind>,
                        fields: contracts::read_value::<Vec<Part>>,
                        arguments: contracts::read_value::<Arguments>,
                        bindings: contracts::read_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Contract);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Address, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        required: Slot,
        calls: Slot,
    }
    #[repr(C)]
    struct Writer {
        required: WriteCallback,
        calls: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        required: ReadCallback,
        calls: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_residency_contract"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_residency_contract"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    unsafe extern "C" fn write_required(value: Address, output: Slot) {
        unsafe {
            contracts::write_map_with(&*value.cast::<Requirements>(), output, write_requirement)
        }
    }
    unsafe extern "C" fn write_requirement(value: Address, output: Slot) {
        let (key, guards) =
            unsafe { &*value.cast::<(&(Input, Target), &BTreeSet<bounds::Condition>)>() };
        let condition = guards
            .iter()
            .cloned()
            .fold(bounds::Condition::constant(false), |a, b| {
                a.logical(b, true)
            });
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_requirement"]
            fn write(
                key: Address,
                condition: Address,
                output: Slot,
                key_write: WriteCallback,
                condition_write: WriteCallback,
                append: unsafe extern "C" fn(Slot, *const u8, usize),
            );
        }
        unsafe {
            write(
                contracts::address(*key),
                contracts::address(&condition),
                output,
                contracts::write_value::<(Input, Target)>,
                contracts::write_value::<bounds::Condition>,
                contracts::append,
            )
        }
    }
    impl Native for Contract {
        fn empty() -> Self {
            Self::default()
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        required: contracts::address(&self.required).cast_mut(),
                        calls: contracts::address(&self.calls).cast_mut(),
                    },
                    Writer {
                        required: write_required,
                        calls: contracts::write_value::<Vec<Call>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut required = <Requirements as Native>::empty();
            let mut calls = <Vec<Call> as Native>::empty();
            let status = unsafe {
                read(
                    context,
                    Fields {
                        required: contracts::slot(&mut required),
                        calls: contracts::slot(&mut calls),
                    },
                    Reader {
                        required: contracts::read_value::<Requirements>,
                        calls: contracts::read_value::<Vec<Call>>,
                    },
                )
            };
            if status == ReadStatus::Success {
                *output = Self { required, calls };
            }
            status
        }
    }
};

crate::codec::contracts::codec!(Call);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        function: Slot,
        arguments: Slot,
        bindings: Slot,
        path: Slot,
    }
    #[repr(C)]
    struct Writer {
        function: WriteCallback,
        arguments: WriteCallback,
        bindings: WriteCallback,
        path: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        function: ReadCallback,
        arguments: ReadCallback,
        bindings: ReadCallback,
        path: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_residency_call"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_residency_call"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Call {
        fn empty() -> Self {
            Self {
                function: String::new(),
                arguments: BTreeMap::new(),
                bindings: BTreeMap::new(),
                path: bounds::Clause::default(),
                source: None,
                span: Span::default(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            let path = bounds::Condition::guard(self.path.clone());
            unsafe {
                write(
                    output,
                    Fields {
                        function: contracts::address(&self.function).cast_mut(),
                        arguments: contracts::address(&self.arguments).cast_mut(),
                        bindings: contracts::address(&self.bindings).cast_mut(),
                        path: contracts::address(&path).cast_mut(),
                    },
                    Writer {
                        function: contracts::write_value::<String>,
                        arguments: contracts::write_value::<Arguments>,
                        bindings: contracts::write_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                        path: contracts::write_value::<bounds::Condition>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut function = <String as Native>::empty();
            let mut arguments = <Arguments as Native>::empty();
            let mut bindings = <BTreeMap<ValueIdentity, bounds::Proof> as Native>::empty();
            let mut path = <bounds::Condition as Native>::empty();
            let status = unsafe {
                read(
                    context,
                    Fields {
                        function: contracts::slot(&mut function),
                        arguments: contracts::slot(&mut arguments),
                        bindings: contracts::slot(&mut bindings),
                        path: contracts::slot(&mut path),
                    },
                    Reader {
                        function: contracts::read_value::<String>,
                        arguments: contracts::read_value::<Arguments>,
                        bindings: contracts::read_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                        path: contracts::read_value::<bounds::Condition>,
                    },
                )
            };
            if status == ReadStatus::Success {
                *output = Self {
                    function,
                    arguments,
                    bindings,
                    path: path.assume(&bounds::Clause::default(), true),
                    source: None,
                    span: Span::default(),
                };
            }
            status
        }
    }
};
