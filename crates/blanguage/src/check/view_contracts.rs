//! view借用与拥有资源沿原输入、循环和调用图交接；访问条件属于函数静态合同。
use super::*;
use view::{Borrow, LoopInput, Requirement, Source as Origin};
// Resource identity is the original root plus its typed element-field path.
// Model discovery order and the amount of known descendant facts do not name an output.
pub(super) type ResourcePath=(usize,Vec<Vec<usize>>);
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Output { Returned(usize), Written(usize,usize), ReturnedResource(ResourcePath), WrittenResource(usize,ResourcePath) }
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct CallResult { pub call:usize, pub output:Output }
pub(super) type Outputs=BTreeMap<Output,Borrow>;
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct PositionAccess {pub layout:address::Layout,pub writes:bool,pub writable:bool,pub initialized:bool}
crate::module::codec::record!(PositionAccess {layout,writes,writable,initialized});
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Access {
    Resource {resources:Borrow,borrows:Borrow,ownership:bool,when:bounds::Condition},
    Position {borrows:Borrow,when:bounds::Condition,position:PositionAccess},
    Memory {borrows:Borrow,when:bounds::Condition,access:super::Access},
    Dataset {borrows:Borrow,when:bounds::Condition,index:Option<bounds::Operand>,access:super::Access,atomic:bool},
    Initialized {borrows:Borrow,initialized:Vec<address::Initialization>,when:bounds::Condition},
}
impl Access {
    fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        match self {
            Self::Resource{resources,borrows,ownership,when}=>Self::Resource{resources:resources.substitute_values(values),borrows:borrows.substitute_values(values),ownership:*ownership,when:when.substitute_values(values)},
            Self::Position{borrows,when,position}=>Self::Position{borrows:borrows.substitute_values(values),when:when.substitute_values(values),position:position.clone()},
            Self::Memory{borrows,when,access}=>Self::Memory{borrows:borrows.substitute_values(values),when:when.substitute_values(values),access:*access},
            Self::Dataset{borrows,when,index,access,atomic}=>{
                let index=index.and_then(|index|match index{bounds::Operand::Value(id)if values.contains_key(&id)=>index.at_call(values),index=>Some(index)});
                Self::Dataset{borrows:borrows.substitute_values(values),when:when.substitute_values(values),index,access:*access,atomic:*atomic}
            },
            Self::Initialized{borrows,initialized,when}=>Self::Initialized{borrows:borrows.substitute_values(values),initialized:initialized.iter().map(|value|value.substitute_values(values)).collect(),when:when.substitute_values(values)},
        }
    }
}
crate::module::codec::variants!(Access {0=>Resource{resources,borrows,ownership,when},1=>Position{borrows,when,position},2=>Memory{borrows,when,access},3=>Dataset{borrows,when,index,access,atomic},4=>Initialized{borrows,initialized,when}});
#[derive(Clone,Default,PartialEq,Eq)]
pub(super) struct Interface {pub outputs:Outputs,pub access:BTreeSet<Access>,pub initialization:Vec<address::Initialization>}
pub(super) struct ValueSnapshot { pub position:Position, pub scalar:Scalar, pub proof:bounds::Proof }
pub(super) struct Call {
    pub(super) function:String,
    pub(super) memory:usize,
    pub(super) arguments:BTreeMap<ValueIdentity,bounds::Proof>,
    initialization:Vec<address::InitializationEffect>,
    outputs:BTreeSet<Output>,
    pub(super) path:bounds::Clause,
    source:crate::Source,
    span:Span,
}
pub(super) struct Use { pub requirement:Requirement, pub source:crate::Source, pub path:bounds::Clause, pub active_loops:BTreeSet<usize> }

#[derive(Clone)]
pub(super) enum ControlInput {Value{name:String,component:usize},Length{name:String,path:Vec<usize>}}
#[derive(Clone,Default)]
pub(super) struct LoopControl {
    pub cursor:Option<Value>,
    pub test:Option<bounds::Condition>,
    pub inputs:BTreeMap<ValueIdentity,ControlInput>,
}
fn binding_value(binding:&Binding)->Option<(&Ty,bounds::Proof)> {
    match binding {
        Binding::Value(value)=>Some((&value.ty,bounds::Proof::capture(value))),
        Binding::Owned(value)|Binding::ViewElement{element:value,..}=>Some((value.place.ty(),value.proof.clone())),
        Binding::OwnedValue(value)=>Some((&value.value.ty,bounds::Proof::capture(&value.value))),
        _=>None,
    }
}
impl LoopControl {
    pub fn capture(test:&Value,environment:&BTreeMap<String,Binding>,cursor:Option<Value>)->Self {
        let condition=bounds::Condition::of(test);let mut inputs=BTreeMap::new();
        for(name,binding)in environment {
            if let Some((ty,proof))=binding_value(binding) {
                for component in 0..ty.components.len() {
                    if let Some(id)=proof.known_identity(component) && condition.mentions(id) {
                        inputs.insert(id,ControlInput::Value{name:name.clone(),component});
                    }
                }
            }
            for(path,data,_)in view::data_paths(binding) {
                let id=data.length.identity();
                if condition.mentions(id){inputs.insert(id,ControlInput::Length{name:name.clone(),path});}
            }
        }
        Self{cursor,test:Some(condition),inputs}
    }
    pub(super) fn bindings(&self,producer:&flow::LoopState)->BTreeMap<ValueIdentity,bounds::Proof> {
        let mut values=BTreeMap::new();
        for(id,input)in &self.inputs {
            let proof=match input {
                ControlInput::Value{name,component}=>producer.env.get(name).and_then(binding_value)
                    .map(|(ty,proof)|proof.project(&[*component],ty.components.len())),
                ControlInput::Length{name,path}=>producer.env.get(name).and_then(|binding|view::data_at(binding,path))
                    .map(|data|bounds::Proof::capture(&data.length)),
            };
            if let Some(proof)=proof{values.insert(*id,proof);}
        }
        if let(Some(cursor),Some(next))=(&self.cursor,&producer.cursor){values.insert(cursor.identity(),bounds::Proof::capture(next));}
        values
    }
}
pub(super) struct LoopResource {
    pub name:String,
    pub path:Vec<usize>,
    pub states:Arc<[flow::LoopState]>,
    pub control:Arc<LoopControl>,
}
pub(super) struct LoopDefinition {pub borrow:Borrow,pub resource:Option<LoopResource>}
#[derive(Default)]
pub(super) struct Function {
    next_scope:usize,
    pub(super) calls:Vec<Call>,
    pub loops:BTreeMap<LoopInput,LoopDefinition>,
    pub independent:BTreeSet<LoopInput>,
    pub outputs:Outputs,
    pub initialization:Vec<address::InitializationEffect>,
    pub normal_returns:Option<bounds::Condition>,
    pub resource_names:BTreeMap<Output,Borrow>,
    pub parameters:BTreeMap<ValueIdentity,ValueIdentity>,
    pub input_width:usize,
    pub uses:Vec<Use>,
}
pub(super) fn components(ty:&Ty)->BTreeSet<usize> {
    ty.borrow_fields().into_iter().map(|(offset,_)|ty.components.iter().position(|(_,at)|*at==offset).unwrap()).collect()
}
pub(super) fn resource_borrows(value:&returns::Returned)->Vec<Borrow>{
    match value{
        returns::Returned::Dataset(value)=>vec![view::resource_borrow(value)],
        returns::Returned::OwnedValue(value)=>view::resource_paths(&value.value.ty).into_iter().map(|path|view::owned_data_at(value,&path).map(view::resource_borrow).unwrap_or_else(||Borrow::from_sources(BTreeSet::from([Origin::Inactive])))).collect(),
        _=>Vec::new(),
    }
}
pub(super) fn resource_value_mut(value:&mut returns::Returned,index:usize)->Option<&mut application::Data>{match value{
    returns::Returned::Dataset(value)if index==0=>Some(value),
    returns::Returned::OwnedValue(value)=>{let path=view::resource_paths(&value.value.ty).get(index)?.clone();view::owned_data_at_mut(value,&path)},_=>None,
}}
pub(super) fn resource_output_paths(value:&returns::Returned)->Vec<ResourcePath> {
    fn visit(ty:&Ty,root:usize,parents:&[Vec<usize>],out:&mut Vec<(usize,Vec<Vec<usize>>)>) {
        for(fields,element)in view::resource_paths(ty).into_iter().zip(application::element_types(ty)){
            let mut path=parents.to_vec();path.push(fields);out.push((root,path.clone()));visit(element,root,&path,out);
        }
    }
    let types=match value{returns::Returned::Dataset(data)=>vec![&data.ty],returns::Returned::OwnedValue(value)=>application::element_types(&value.value.ty),_=>Vec::new()};
    let mut out=(0..types.len()).map(|root|(root,Vec::new())).collect::<Vec<_>>();
    for(root,ty)in types.into_iter().enumerate(){visit(ty,root,&[],&mut out);}out
}
fn root_resource(value:&returns::Returned,index:usize)->Option<&application::Data>{match value{
    returns::Returned::Dataset(data)if index==0=>Some(data),
    returns::Returned::OwnedValue(value)=>view::resource_paths(&value.value.ty).get(index).and_then(|path|view::owned_data_at(value,path)),_=>None,
}}
pub(super) fn resource_outputs(value:&returns::Returned)->Vec<(ResourcePath,Borrow)>{
    resource_output_paths(value).into_iter().map(|resource|{
        let (root,path)=&resource;
        let data=path.iter().fold(root_resource(value,*root),|data,fields|{
            let data=data?;
            if data.length.maximum()==0{None}else{application::model_data(&data.elements,fields)}
        });
        let borrow=if let Some(data)=data {
            let name=resource_output_name(value,&resource).unwrap();
            view::resource_borrow(data).map_sources(|source|match source{
                Origin::Data(ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..))=>name.clone(),
                source=>Borrow::source(source),
            })
        }else{Borrow::source(Origin::Inactive)};
        (resource,borrow)
    }).collect()
}
pub(super) fn resource_output_mut<'a>(value:&'a mut returns::Returned,resource:&ResourcePath)->Option<&'a mut application::Data>{
    let(root,path)=resource;let mut data=resource_value_mut(value,*root)?;
    for fields in path{data=application::model_data_mut(&mut data.elements,fields)?;}Some(data)
}
pub(super) fn resource_output_name(value:&returns::Returned,resource:&ResourcePath)->Option<Borrow>{
    let(root,path)=resource;let data=root_resource(value,*root)?;
    let mut name=view::resource_borrow(data);
    let depth=path.len();for(at,fields)in path.iter().enumerate(){name=Borrow::element(name,fields.clone(),Some(bounds::Operand::Value(ValueIdentity::ElementIndex(depth-1-at))));}Some(name)
}
pub(super) fn set_resource_outputs(value:&mut returns::Returned,mut borrow:impl FnMut(ResourcePath)->Borrow){
    for resource in resource_output_paths(value){if let Some(data)=resource_output_mut(value,&resource){data.root.proof=data.root.proof.clone().with_view_borrow(0,borrow(resource));}}
}
fn disjoint(a:&Borrow,b:&Borrow,independent:&BTreeSet<LoopInput>,active:&BTreeSet<usize>)->bool {
    if !a.calls.is_empty()||!b.calls.is_empty(){return false;}
    if a.sources.is_empty()&&b.sources.is_empty()&&!a.inputs.is_empty()&&!b.inputs.is_empty()
        &&a.inputs.iter().all(|a|b.inputs.iter().all(|b|a.level==b.level&&a.index!=b.index&&independent.contains(a)&&independent.contains(b))){return true;}
    // 活动循环入口已将所有原环境资源和view改写为输入边；本轮裸Data来源属于本轮新资源。
    // 仅在该实际循环内区分当前新资源与入口拥有资源，不能用于循环完成后的消费者。
    let fresh=|input:&Borrow,current:&Borrow|input.sources.is_empty()&&!input.inputs.is_empty()
        &&input.inputs.iter().all(|input|independent.contains(input)&&active.contains(&input.level))
        &&current.inputs.is_empty()&&!current.sources.is_empty()&&current.sources.iter().all(|source|matches!(source,Origin::Data(_)|Origin::Inactive));
    fresh(a,b)||fresh(b,a)
}
impl Function {
    pub fn pending_values(&self)->bool {self.uses.iter().any(|usage|matches!(&usage.requirement,Requirement::Memory{snapshots:Some(snapshots),..}if !snapshots.is_empty()))}
    fn value_context(&self,values:Option<&BTreeMap<ValueIdentity,bounds::Proof>>)->bounds::Clause {
        match values {
            Some(values)=>bounds::Clause::values(self.uses.iter().enumerate().flat_map(|(memory,usage)|{
                let snapshots=match &usage.requirement{Requirement::Memory{snapshots:Some(snapshots),..}=>snapshots.as_ref(),_=>&[]};
                snapshots.iter().enumerate().map(move |(component,snapshot)|{
                    let id=ValueIdentity::MemoryValue(memory,component);(snapshot.scalar,id,&values[&id])
                })
            })),
            None=>bounds::Clause::default(),
        }
    }
    pub fn initialization_when(&self,when:bounds::Condition)->bounds::Condition {
        let normal=match &self.normal_returns {Some(normal)=>normal.clone(),None=>bounds::Condition::constant(false)};
        normal.negate().logical(when,true)
    }
    pub fn scope(&mut self)->usize {let scope=self.next_scope;self.next_scope+=1;scope}
    pub fn call(&mut self,function:&str,memory:usize,arguments:BTreeMap<ValueIdentity,bounds::Proof>,initialization:Vec<address::InitializationEffect>,path:bounds::Clause,source:crate::Source,span:Span)->usize {
        let call=self.calls.len();self.calls.push(Call{function:function.into(),memory,arguments,initialization,outputs:BTreeSet::new(),path,source,span});call
    }
    pub fn result(&mut self,call:usize,output:Output)->Borrow {
        self.calls[call].outputs.insert(output.clone());
        Borrow{calls:view::Edges::from([CallResult{call,output}]),..Borrow::default()}
    }
    pub fn capture_resources(&mut self,value:&returns::Returned,output:impl Fn(ResourcePath)->Output){
        for(resource,borrow)in resource_outputs(value){self.outputs.insert(output(resource),borrow);}
    }
    pub fn capture(&mut self,proof:&bounds::Proof,ty:&Ty,output:impl Fn(usize)->Output) {
        for component in components(ty) {self.outputs.insert(output(component),proof.view_borrow(component));}
    }
    fn input(&self,id:ValueIdentity)->bool {match id{ValueIdentity::Register(n)=>n<self.input_width,ValueIdentity::ParameterContent(..)|ValueIdentity::ResourceParameter(..)|ValueIdentity::ParameterStorage(..)|ValueIdentity::DatasetContent(..)=>true,_=>false}}
    fn address_borrows(&self,borrow:Borrow,when:bounds::Condition)->Option<(Borrow,bounds::Condition)>{
        let selected=borrow.map_sources(|source|match source {
            Origin::Parameter(id,indices)=>self.parameters.get(&id).copied().filter(|id|self.input(*id))
                .map(|id|Borrow::source(Origin::Parameter(id,indices.into_iter().map(|index|input_index(index,self)).collect()))).unwrap_or_else(||Borrow::source(Origin::Unresolved)),
            Origin::Position(origin) if matches!(origin.root,address::Root::Parameter(_)|address::Root::Derived(_)|address::Root::External(_)|address::Root::Dataset{..}) =>
                {
                    let mut origin=(*origin).clone();
                    if let address::Root::External(address::Identity::Storage(borrow))=&mut origin.root {
                        *borrow=borrow.map_sources(|source|match source{Origin::Position(origin)if origin.has_local()=>Borrow::default(),source=>Borrow::source(source)});
                        if !borrow.active(){return Borrow::default();}
                    }
                    canonical(Borrow::source(Origin::Position(Arc::new(origin))),self,None,&BTreeMap::new())
                },
            Origin::Unresolved|Origin::External=>Borrow::source(source),
            _=>Borrow::default(),
        });
        if !selected.active(){return None;}
        let bindings=residency::input_parameters(&self.parameters,self.input_width);
        let when=when.guard_at_call(&bindings);
        let when=bounds::Condition::guard(when.assume(&bounds::Clause::default(),true).canonical());
        let borrows=selected.guards_at_call(&bindings);
        when.active().then_some((borrows,when))
    }
    fn initialization_access(&self,read:&address::InitializationRead,when:bounds::Condition)->Option<Access>{
        if read.missing().is_empty(){return None;}
        let(borrows,when)=self.address_borrows(read.borrow(),when)?;
        let parameters=residency::input_parameters(&self.parameters,self.input_width);
        let initialized:Vec<_>=address::initialization_effects(&read.initialized,&bounds::Clause::default()).into_iter().filter_map(|effect|{
            let address::InitializationEffect::Position(mut fact)=effect else{return None;};
            if fact.origin.has_local(){return None;}
            let value=canonical(Borrow::source(Origin::Position(Arc::new(fact.origin))),self,None,&BTreeMap::new());
            if value.unresolved(){return None;}
            let Origin::Position(origin)=value.sources.iter().next()? else{return None;};
            fact.origin=origin.as_ref().clone();
            fact.when=fact.when.guaranteed_at_call(&parameters);
            Some(fact)
        }).collect();
        Some(Access::Initialized{borrows,initialized:address::merge_initializations(initialized),when})
    }
    fn address_access(&self,borrow:Borrow,make:impl FnOnce(Borrow,bounds::Condition)->Access,when:bounds::Condition)->Option<Access>{
        let(borrows,when)=self.address_borrows(borrow,when)?;
        Some(make(borrows,when))
    }
    fn access(&self,resources:Borrow,borrows:Borrow,ownership:bool,when:bounds::Condition)->Option<Access>{
        let parameters=residency::input_parameters(&self.parameters,self.input_width);
        fn external(function:&Function,borrow:Borrow)->Borrow{
            let parameters=residency::input_parameters(&function.parameters,function.input_width);
            borrow.map_sources(|source|match source{
                Origin::Data(id)=>function.parameters.get(&id).copied().filter(|id|function.input(*id)).map(|id|Borrow::source(Origin::Data(id))).unwrap_or_default(),
                Origin::Parameter(id,indices)=>Borrow::source(function.parameters.get(&id).copied().filter(|id|function.input(*id)).map(|id|Origin::Parameter(id,indices.into_iter().map(|index|input_index(index,function)).collect())).unwrap_or(Origin::Unresolved)),
                Origin::Element{parent,fields,index}=>Borrow::element(external(function,*parent),fields,input_index(index,function)),
                Origin::Projection{parent,fields}=>Borrow::projection(external(function,*parent),fields),
                Origin::Position(origin)=>origin.resources(),
                Origin::Unresolved=>Borrow::source(source),Origin::Inactive|Origin::External=>Borrow::default(),
            }).guards_at_call(&parameters)
        }
        let resources=external(self,resources.resources());let borrows=external(self,if ownership{borrows.owners()}else{borrows.resources()});
        if !resources.active()||!borrows.active(){return None;}
        let when=when.guard_at_call(&parameters);
        let when=bounds::Condition::guard(when.assume(&bounds::Clause::default(),true).canonical());
        when.active().then_some(Access::Resource{resources,borrows,ownership,when})
    }
    fn dataset_access(&self,borrow:Borrow,index:Option<bounds::Operand>,access:super::Access,atomic:bool,when:bounds::Condition)->Option<Access> {
        fn inputs(borrow:&Borrow,function:&Function)->Borrow {
            borrow.map_sources(|source|match source {
                Origin::Data(id) if !function.parameters.get(&id).is_some_and(|id|function.input(*id))=>Borrow::default(),
                Origin::Element{parent,fields,index}=>Borrow::element(inputs(&parent,function),fields,index),
                Origin::Projection{parent,fields}=>Borrow::projection(inputs(&parent,function),fields),
                Origin::Position(origin)=>{
                    let mut origin=origin.map_borrows(|borrow|inputs(borrow,function));
                    if let address::Root::Resource(borrow)=origin.root {origin.root=address::Root::Derived(borrow);}
                    origin.normalize()
                },
                source=>Borrow::source(source),
            })
        }
        let(borrows,when)=self.address_borrows(inputs(&borrow,self),when)?;
        let index=index.and_then(|index|index.map(|id|self.parameters.get(&id).copied().filter(|id|self.input(*id))));
        Some(Access::Dataset{borrows,when,index,access,atomic})
    }
}
#[derive(Clone,Default,PartialEq,Eq)]
struct State { loops:BTreeMap<LoopInput,Borrow>, interface:Interface }
type States=BTreeMap<String,State>;

// Loop sources are solved only for actual output, contract and memory consumers.
fn position_roots(position:&Position,roots:&mut Vec<Borrow>) {
    match position {
        Position::ApplicationInput(input,..)=>roots.push(Borrow{inputs:view::Edges::from([*input]),..Borrow::default()}),
        Position::Borrowed(borrow,..)=>roots.push(borrow.as_ref().clone()),
        Position::Initialization(effect)=>initialization_roots(effect,roots),
        Position::Both(a,b)|Position::Conditional(_,a,b)=>{position_roots(a,roots);position_roots(b,roots);}
        Position::Indexed(choices,_)=>for position in choices.iter(){position_roots(position,roots);},
        _=>{},
    }
}
fn initialization_roots(effect:&address::InitializationEffect,roots:&mut Vec<Borrow>) {
    match effect {
        address::InitializationEffect::Position(fact)=>roots.push(Borrow::source(Origin::Position(Arc::new(fact.origin.clone())))),
        address::InitializationEffect::Call{..}=>{},
        address::InitializationEffect::Iteration{iteration,..}=>{
            for entry in iteration.states.iter(){for position in &entry.initialized{position_roots(position,roots);}}
            for(scope,_)in &iteration.outputs{position_roots(scope,roots);}
        }
    }
}
fn used_loops(functions:&BTreeMap<String,&Function>,closed:&BTreeMap<String,Interface>)->BTreeMap<String,BTreeSet<LoopInput>> {
    let mut contracts=closed.iter().filter_map(|(name,interface)|(!interface.access.is_empty()||!interface.initialization.is_empty()).then_some(name.clone())).collect::<BTreeSet<_>>();
    contracts.extend(functions.iter().filter_map(|(name,function)|function.uses.iter().any(|usage|matches!(usage.requirement,Requirement::Unborrowed{..}|Requirement::Address{..}|Requirement::Dataset{..}|Requirement::Initialized{..})).then_some(name.clone())));
    loop {
        let needed=functions.iter().filter_map(|(name,function)|function.calls.iter().any(|call|contracts.contains(&call.function)).then_some(name.clone())).collect::<BTreeSet<_>>();
        let before=contracts.len();contracts.extend(needed);if before==contracts.len(){break;}
    }
    functions.iter().map(|(name,function)|{
        let mut roots=function.outputs.values().cloned().collect::<Vec<_>>();
        for effect in &function.initialization{initialization_roots(effect,&mut roots);}
        for usage in &function.uses {
            if let Some(borrow)=usage.requirement.borrow(){roots.push(borrow.clone());}
            match &usage.requirement {
                Requirement::Unborrowed{resource,..}=>roots.push(resource.clone()),
                Requirement::Return{owned,..}=>roots.push(owned.clone()),
                Requirement::Initialized{read,..}=>{
                    for position in &read.positions{position_roots(position,&mut roots);}
                    for position in &read.initialized{position_roots(position,&mut roots);}
                }
                Requirement::Memory{snapshots:Some(snapshots),..}=>for snapshot in snapshots.iter(){position_roots(&snapshot.position,&mut roots);},
                Requirement::Address{..}|Requirement::Memory{..}|Requirement::Dataset{..}|Requirement::Nonzero{..}=>{},
            }
        }
        for call in &function.calls{if contracts.contains(&call.function){roots.extend(call.arguments.values().map(bounds::Proof::borrowed_view));}}
        let mut loops=BTreeSet::new();let mut calls=BTreeSet::new();
        while let Some(borrow)=roots.pop() {
            for source in &borrow.sources {
                if let Origin::Element{parent,..}|Origin::Projection{parent,..}=source{roots.push(parent.as_ref().clone());}
                if let Origin::Position(origin)=source {
                    match &origin.root {address::Root::Resource(borrow)|address::Root::Derived(borrow)=>roots.push(borrow.clone()),_=>{}}
                }
            }
            for input in &borrow.inputs{if loops.insert(*input){roots.push(function.loops[input].borrow.clone());}}
            for result in &borrow.calls{if calls.insert(result.call){roots.extend(function.calls[result.call].arguments.values().map(bounds::Proof::borrowed_view));}}
        }
        (name.clone(),loops)
    }).collect()
}

fn evaluate(borrow:&Borrow,function:&Function,state:&State,known:&States,expand_loops:bool)->Borrow {
    let mut value=Borrow{..Borrow::default()};
    for(source,when)in borrow.sources.entries() {
        let resolved=match source {
            Origin::Element{parent,fields,index}=>Borrow::element(evaluate(parent,function,state,known,expand_loops),fields.clone(),*index),
            Origin::Projection{parent,fields}=>Borrow::projection(evaluate(parent,function,state,known,expand_loops),fields.clone()),
            Origin::Position(origin)=>origin.map_borrows(|root|evaluate(root,function,state,known,expand_loops)).normalize(),
            source=>Borrow::source(source.clone()),
        };
        value.union(&resolved.restricted(&when));
    }
    for(input,when)in borrow.inputs.entries(){if expand_loops{value.union(&state.loops[input].restricted(&when));}else{value.inputs.insert_when(*input,when.clone());}}
    for(result,when)in borrow.calls.entries(){
        let call=&function.calls[result.call];
        let Some(output)=known[&call.function].interface.outputs.get(&result.output)else {value.sources.insert_when(Origin::Unresolved,when.clone());continue;};
        let actual=output.at_call(&call.arguments);
        value.union(&evaluate(&actual,function,state,known,expand_loops).restricted(&when));
    }value
}
fn input_index(index:Option<bounds::Operand>,function:&Function)->Option<bounds::Operand>{
    index.and_then(|index|index.map(|id|match id{ValueIdentity::ElementIndex(_)=>Some(id),_=>function.parameters.get(&id).copied().filter(|id|function.input(*id))}))
}
fn canonical(borrow:Borrow,function:&Function,resource:Option<&Borrow>,exposed:&BTreeMap<ValueIdentity,Borrow>)->Borrow {
    let parameters=function.parameters.iter().map(|(a,b)|(*a,bounds::Proof::default().with_identity(*b))).collect();
    let renamed=|source|match source {
        Origin::Data(id)=>{
            let input=function.parameters.get(&id).copied().filter(|id|function.input(*id));
            if let Some(input)=input{Borrow::source(Origin::Data(input))}
            else if let Some(resource)=resource{resource.clone()}
            else if let Some(outputs)=exposed.get(&id){outputs.clone()}
            else{Borrow::source(Origin::Unresolved)}
        }
        Origin::Parameter(id,indices)=>Borrow::source(function.parameters.get(&id).copied().map(|id|Origin::Parameter(id,indices.into_iter().map(|index|input_index(index,function)).collect())).unwrap_or(Origin::Unresolved)),
        Origin::Element{parent,fields,index}=>Borrow::element(canonical(*parent,function,None,exposed),fields,input_index(index,function)),
        Origin::Projection{parent,fields}=>Borrow::projection(canonical(*parent,function,None,exposed),fields),
        Origin::Position(origin)=>{
            let mut value=origin.map_borrows(|borrow|canonical(borrow.clone(),function,None,exposed));
            if let address::Root::External(address::Identity::Value(ids))=&mut value.root {
                *ids=ids.iter().filter_map(|id|function.parameters.get(id).copied().filter(|id|function.input(*id))).collect();
            }
            if let address::Root::Dataset{borrow,index:Some(index)}=&mut value.root {
                match index.map(|id|function.parameters.get(&id).copied().filter(|id|function.input(*id))) {
                    Some(bound)=>*index=bound,
                    None=>*borrow=Borrow::source(Origin::Unresolved),
                }
            }
            if !matches!(value.root,address::Root::Local { .. }) {value.positions=Arc::from([]);}
            Borrow::source(Origin::Position(Arc::new(value)))
        }
        Origin::Unresolved|Origin::Inactive|Origin::External=>Borrow::source(source),
    };
    match resource {
        // 拥有输出的来源改名不附加条件，先投影正式输入关系。
        Some(_)=>borrow.guards_at_call(&parameters).map_sources(renamed),
        // 借用输出需先合成实际拥有者条件，保留来源相关性。
        None=>borrow.map_sources(renamed).guards_at_call(&parameters),
    }
}


fn resolved_position(position:&Position,path:&bounds::Clause,map:&impl Fn(&Borrow)->Borrow,application:&impl Fn(LoopInput,Option<ownership::ApplicationIndex>,u32)->Position)->Position {
    match position {
        Position::ApplicationInput(input,index,word)=>application(*input,*index,*word),
        Position::Borrowed(borrow,layout,word)=>{
            let borrow=address::borrow_in_path(&map(borrow),path);
            let next=address::borrowed_positions(&borrow,layout,&[*word]).remove(0);
            if next==*position{next}else{resolved_position(&next,path,map,application)}
        }
        Position::Both(a,b)=>Position::both(resolved_position(a,path,map,application),resolved_position(b,path,map,application)),
        Position::Conditional(when,a,b)=>match when.condition().truth_in(path) {
            Some(true)=>resolved_position(a,path,map,application),Some(false)=>resolved_position(b,path,map,application),
            None=>Position::selected(when.as_ref().clone(),resolved_position(a,path,map,application),resolved_position(b,path,map,application)),
        },
        Position::Indexed(choices,index)=>Position::Indexed(choices.iter().map(|choice|resolved_position(choice,path,map,application)).collect(),*index),
        _=>position.clone(),
    }
}
fn bind_application_position(position:&Position,input:LoopInput,scope:&Position,bindings:&BTreeMap<ValueIdentity,bounds::Proof>,path:&bounds::Clause)->Position {
    match position {
        Position::ApplicationInput(at,index,word)if *at==input=>scope.application_word(*index,*word),
        Position::Both(a,b)=>Position::both(bind_application_position(a,input,scope,bindings,path),bind_application_position(b,input,scope,bindings,path)),
        Position::Conditional(when,a,b)=>match when.condition().at_call(bindings).truth_in(path) {
            Some(true)=>bind_application_position(a,input,scope,bindings,path),
            Some(false)=>bind_application_position(b,input,scope,bindings,path),
            None=>Position::selected(when.as_ref().clone(),bind_application_position(a,input,scope,bindings,path),bind_application_position(b,input,scope,bindings,path)),
        },
        Position::Indexed(choices,index)=>Position::Indexed(choices.iter().map(|choice|bind_application_position(choice,input,scope,bindings,path)).collect(),*index),
        _=>position.clone(),
    }
}
fn resolved_application_position(input:LoopInput,index:Option<ownership::ApplicationIndex>,word:u32,initialized:&BTreeSet<Position>,path:&bounds::Clause,function:&Function,state:&State,known:&States,visiting:&mut BTreeSet<Position>)->Position {
    let position=Position::ApplicationInput(input,index,word);
    if position.initialized_by(initialized){return Position::Initialized;}
    // An unchanged cycle supplies no counterexample; every actual entry and
    // continued producer is still checked below in the same source graph.
    if !visiting.insert(position.clone()){return Position::Initialized;}
    let resource=function.loops[&input].resource.as_ref().unwrap();
    let mut result=Position::Initialized;
    let mut invariant=function.parameters.keys().map(|id|(*id,bounds::Proof::default().with_identity(*id))).collect::<BTreeMap<_,_>>();
    for(name,binding)in &resource.states[0].env {
        if let Some((ty,proof))=binding_value(binding) {
            for component in 0..ty.components.len() {
                if let Some(id)=proof.known_identity(component) && resource.states.iter().skip(1).all(|producer|
                    producer.env.get(name).and_then(binding_value).is_some_and(|(_,proof)|proof.known_identity(component)==Some(id))) {
                    invariant.insert(id,bounds::Proof::default().with_identity(id));
                }
            }
        }
    }
    for producer in resource.states.iter() {
        let mut bindings=invariant.clone();bindings.extend(resource.control.bindings(producer));
        let mut source_path=producer.path.clone();
        if let Some(test)=&resource.control.test {
            source_path=test.at_call(&bindings).assume(&source_path,true);
        }
        let read_path=bounds::Condition::guard(path.clone()).guard_at_call(&bindings).assume(&bounds::Clause::default(),true);
        source_path=bounds::Clause::both(&source_path,&read_path);
        if !source_path.possible(){continue;}
        let Some(data)=producer.env.get(&resource.name).and_then(|binding|view::data_at(binding,&resource.path))else{continue;};
        let Some(scope)=&data.initialization else{continue;};
        let mut facts=resolved_initialization(&producer.initialized,&source_path,function,state,known);
        facts.extend(initialized.iter().map(|fact|bind_application_position(fact,input,scope,&bindings,&source_path)));
        let wanted=scope.application_word(index,word);
        let ready=wanted.initialized_by(&facts) || index.is_none() && data.length.minimum()==data.length.maximum()
            && scope.application_prefix_initialized(data.length.minimum(),word,&facts);
        let value=if ready{Position::Initialized}else{
            let visit=std::cell::RefCell::new(&mut *visiting);
            resolved_position(&wanted,&source_path,&|borrow|evaluate(borrow,function,state,known,true),
                &|next,at,word|resolved_application_position(next,at,word,&facts,&source_path,function,state,known,&mut visit.borrow_mut()))
        };
        result=Position::both(result,value);
    }
    visiting.remove(&position);result
}

struct InitializedEffects {
    facts:Vec<address::Initialization>,
    iterations:Vec<(BTreeSet<Position>,bounds::Condition)>,
}
impl InitializedEffects {
    fn positions(&self,path:&bounds::Clause)->BTreeSet<Position> {
        let mut result=self.facts.iter().filter(|fact|fact.when.truth_in(path)==Some(true)).flat_map(|fact|fact.positions(path)).collect::<BTreeSet<_>>();
        for(positions,when)in &self.iterations {if when.truth_in(path)==Some(true){result.extend(positions.iter().cloned());}}
        result
    }
}
fn iteration_positions(iteration:&address::Iteration,function:&Function,state:&State,known:&States)->BTreeSet<Position> {
    let states=iteration.states.iter().map(|entry|resolved_initialization(&entry.initialized,&entry.path.assume(&bounds::Clause::default(),true),function,state,known)).collect::<Vec<_>>();
    flow::iteration_positions(iteration,&states)
}
fn initialization_effects(effects:&[address::InitializationEffect],function:&Function,state:&State,known:&States)->InitializedEffects {
    let mut facts=Vec::new();
    let mut iterations=Vec::new();
    for effect in effects {
        match effect {
            address::InitializationEffect::Position(fact)=>facts.push(fact.clone()),
            address::InitializationEffect::Call{call,when}=>{
                let call=&function.calls[*call];
                for fact in &known[&call.function].interface.initialization {
                    let mut fact=fact.at_call(&call.arguments);
                    fact.when=fact.when.logical(when.clone(),false);
                    facts.push(fact);
                }
            }
            address::InitializationEffect::Iteration{iteration,when}=>{
                let positions=iteration_positions(iteration,function,state,known);
                for effect in address::initialization_effects(&positions,&bounds::Clause::default()) {
                    if let address::InitializationEffect::Position(mut fact)=effect {
                        fact.when=fact.when.logical(when.clone(),false);facts.push(fact);
                    }
                }
                iterations.push((positions,when.clone()));
            }
        }
    }
    for fact in &mut facts {fact.origin=fact.origin.map_borrows(|borrow|evaluate(borrow,function,state,known,true));}
    InitializedEffects{facts:address::merge_initializations(facts),iterations}
}
fn resolved_initialization(initialized:&BTreeSet<Position>,path:&bounds::Clause,function:&Function,state:&State,known:&States)->BTreeSet<Position> {
    let map=|borrow:&Borrow|evaluate(borrow,function,state,known,true);
    let effects=address::initialization_effects(initialized,&bounds::Clause::default());
    let effects=initialization_effects(&effects,function,state,known);
    let mut result=initialized.iter().filter(|position|!matches!(position,Position::Initialization(_)))
        .map(|position|resolved_position(position,path,&map,&|input,index,word|Position::ApplicationInput(input,index,word))).collect::<BTreeSet<_>>();
    result.extend(effects.positions(path));result
}
fn resolved_read(read:&address::InitializationRead,path:&bounds::Clause,function:&Function,state:&State,known:&States)->address::InitializationRead {
    let initialized=resolved_initialization(&read.initialized,path,function,state,known);
    let positions=read.positions.iter().map(|position|{
        let visiting=std::cell::RefCell::new(BTreeSet::new());
        resolved_position(position,path,&|borrow|evaluate(borrow,function,state,known,true),
            &|input,index,word|resolved_application_position(input,index,word,&initialized,path,function,state,known,&mut visiting.borrow_mut()))
    }).collect();
    address::InitializationRead{positions,initialized}
}

fn refreshed_at_call(borrow:&Borrow,call:&Call,path:&bounds::Clause,function:&Function,state:&State,known:&States)->Borrow {
    let initialized=initialization_effects(&call.initialization,function,state,known).positions(path);
    evaluate(borrow,function,state,known,true).map_sources(|source|match source {
        Origin::Position(origin)=>{
            let fact=address::Initialization{origin:origin.as_ref().clone(),when:bounds::Condition::constant(true)};
            let positions=fact.positions(path);
            let mut origin=origin.as_ref().clone();
            origin.initialized=origin.initialized.iter().enumerate().map(|(i,ready)|*ready||positions.get(i).is_some_and(|position|position.initialized_by(&initialized))).collect();
            Borrow::source(Origin::Position(Arc::new(origin)))
        }
        source=>Borrow::source(source),
    })
}

fn access_at_call(access:&Access,call:&Call,path:&bounds::Clause,function:&Function,state:&State,known:&States)->Access{
    let(Access::Resource{borrows,when,..}|Access::Position{borrows,when,..}|Access::Memory{borrows,when,..}|Access::Dataset{borrows,when,..}|Access::Initialized{borrows,when,..})=access;
    let borrows=borrows.at_call(&call.arguments);
    let when=when.guard_at_call(&call.arguments).assume(path,true);
    let borrows=if matches!(access,Access::Initialized{..}){evaluate(&borrows,function,state,known,true)}else{refreshed_at_call(&borrows,call,path,function,state,known)};
    let when=bounds::Condition::guard(when);
    match access {
        Access::Resource{resources,ownership,..}=>Access::Resource{resources:evaluate(&resources.at_call(&call.arguments),function,state,known,true),borrows,ownership:*ownership,when},
        Access::Position{position,..}=>Access::Position{borrows,when,position:position.clone()},
        Access::Memory{access,..}=>Access::Memory{borrows,when,access:*access},
        Access::Initialized{initialized,..}=>Access::Initialized{borrows,when,initialized:initialized.iter().map(|fact|fact.at_call(&call.arguments)).collect()},
        Access::Dataset{index,access,atomic,..}=>Access::Dataset{borrows,when,index:index.and_then(|index|index.at_call(&call.arguments)),access:*access,atomic:*atomic},
    }
}


fn output_initialization_read(borrows:&Borrow,initialized:&[address::Initialization],path:&bounds::Clause)->address::InitializationRead {
    let mut positions=Vec::new();
    for(source,when)in borrows.sources.entries(){
        if !bounds::Condition::guard(when).assume(path,true).possible(){continue;}
        if let Origin::Position(origin)=source {
            positions.extend(address::borrowed_positions(&Borrow::source(source.clone()),&origin.layout,&origin.layout.parts.iter().map(|(_,word)|*word).collect::<Vec<_>>()));
        }
    }
    let initialized=initialized.iter().filter(|fact|fact.when.truth_in(path)==Some(true)).flat_map(|fact|fact.positions(path)).collect();
    address::InitializationRead{positions,initialized}
}
fn normalized_condition(when:bounds::Condition)->bounds::Condition {
    bounds::Condition::paths(when.assume(&bounds::Clause::default(),true).canonical(),when.assume(&bounds::Clause::default(),false).canonical())
}
fn exposed_resources(function:&Function,state:&State,known:&States)->BTreeMap<ValueIdentity,Borrow> {
    let mut exposed:BTreeMap<ValueIdentity,Borrow>=BTreeMap::new();
    for(output,name)in &function.resource_names {
        let physical=evaluate(&function.outputs[output],function,state,known,true);
        for(source,when)in physical.sources.entries(){if let Origin::Data(id)=source{if !matches!(id,ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)){exposed.entry(*id).or_default().union(&name.restricted(&when));}}}
    }
    exposed
}
fn initializations(function:&Function,state:&State,known:&States,candidates:bool)->Vec<address::Initialization> {
    let exposed=exposed_resources(function,state,known);
    let parameters=function.parameters.iter().map(|(source,target)|(*source,bounds::Proof::default().with_identity(*target))).collect();
    let exits=function.initialization.iter().map(|effect|match effect {address::InitializationEffect::Position(fact)=>&fact.when,address::InitializationEffect::Call{when,..}|address::InitializationEffect::Iteration{when,..}=>when}).map(|when|when.guard_at_call(&parameters)).collect::<Vec<_>>();
    let facts=initialization_effects(&function.initialization,function,state,known).facts;
    let facts=address::merge_initializations(facts.into_iter().filter_map(|mut fact|{
        // A guarantee names one actual selected location; possible owners are never separate writes.
        loop {
            fact.origin=fact.origin.map_borrows(|borrow|address::borrow_in_path(borrow,&fact.when.assume(&bounds::Clause::default(),true)));
            let value=fact.origin.normalize();
            if !value.inputs.is_empty()||!value.calls.is_empty()||value.sources.iter().count()!=1{break;}
            let(source,when)=value.sources.entries().next().unwrap();
            let Origin::Position(origin)=source else{break;};
            if bounds::Condition::guard(when).truth_in(&bounds::Clause::default())!=Some(true)||**origin==fact.origin{break;}
            fact.origin=origin.as_ref().clone();
        }
        if fact.origin.has_local(){return None;}
        fact.origin=fact.origin.map_borrows(|borrow|canonical(borrow.clone(),function,None,&exposed));
        if let address::Root::Dataset{index:Some(index),..}=&mut fact.origin.root {
            *index=index.map(|id|function.parameters.get(&id).copied().filter(|id|function.input(*id)))?;
        }
        fact.origin.positions=Arc::from([]);
        if candidates{fact.when=bounds::Condition::constant(true);}
        Some(fact)
    }));
    if candidates{return facts;}
    facts.into_iter().filter_map(|mut fact|{
        fact.when=normalized_condition(function.initialization_when(fact.when).guaranteed_at_call(&parameters));
        // Original normal exits form a finite induction domain; recursive scalar substitutions
        // cannot manufacture an unbounded chain of successively expanded path formulas.
        let mut when=bounds::Condition::constant(false);
        if fact.when.truth_in(&bounds::Clause::default())==Some(true){when=bounds::Condition::constant(true);}
        else{for exit in &exits{
            let path=exit.assume(&bounds::Clause::default(),true);
            if path.possible()&&fact.when.truth_in(&path)==Some(true){when=when.logical(exit.clone(),true);}
        }}
        fact.when=normalized_condition(when);
        fact.when.active().then_some(fact)
    }).collect()
}
fn converge_initializations(functions:&BTreeMap<String,&Function>,known:&mut States) {
    // Grow only actual written-location candidates along the original calls.
    loop {
        let mut next=known.clone();
        for(name,function)in functions {next.get_mut(name).unwrap().interface.initialization=initializations(function,&known[name],known,true);}
        if *known==next{break;}*known=next;
    }
    // Descend from those candidates using normal-return guarantees, never possible-source unions.
    loop {
        let mut next=known.clone();
        for(name,function)in functions {
            let values=initializations(function,&known[name],known,false).into_iter().map(|fact|(fact.origin,fact.when)).collect::<BTreeMap<_,_>>();
            next.get_mut(name).unwrap().interface.initialization=known[name].interface.initialization.iter().filter_map(|old|{
                let when=normalized_condition(old.when.clone().logical(values.get(&old.origin)?.clone(),false));
                when.active().then(||address::Initialization{origin:old.origin.clone(),when})
            }).collect();
        }
        if *known==next{break;}*known=next;
    }
}
fn converge_access(functions:&BTreeMap<String,&Function>,known:&mut States) {
    loop {
        let mut next=known.clone();
        for(name,function)in functions {
            let state=&known[name];
            let mut access=BTreeSet::new();
            for usage in &function.uses {
                if let Requirement::Initialized{read,code,..}=&usage.requirement {
                    let read=resolved_read(read,&usage.path,function,state,known);
                    if *code=="B229" {
                        if let Some(value)=function.initialization_access(&read,bounds::Condition::guard(usage.path.clone())){access.insert(value);}
                        continue;
                    }
                    for(source,when)in read.borrow().sources.entries() {
                        if let Origin::Position(origin)=source {
                            if let Some(value)=function.address_access(Borrow::source(source.clone()),|borrows,when|Access::Position{borrows,when,position:PositionAccess{layout:origin.layout.clone(),writes:false,writable:false,initialized:true}},
                                bounds::Condition::guard(usage.path.clone()).logical(bounds::Condition::guard(when),false)){access.insert(value);}
                        }
                    }
                    continue;
                }
                let value=match &usage.requirement {
                    Requirement::Unborrowed{resource,borrow,ownership,..}=>function.access(
                        evaluate(resource,function,state,known,true),evaluate(borrow,function,state,known,true),
                        *ownership,bounds::Condition::guard(usage.path.clone())),
                    Requirement::Address{borrow,layout,writes,writable,initialized,..}=>function.address_access(
                        evaluate(borrow,function,state,known,true),
                        |borrows,when|Access::Position{borrows,when,position:PositionAccess{layout:layout.clone(),writes:*writes,writable:*writable,initialized:*initialized}},
                        bounds::Condition::guard(usage.path.clone())),
                    Requirement::Memory{target:address::MemoryTarget::Call(memory),borrow,access:Some(access),..} if !function.calls.iter().any(|call|call.memory==*memory)=>
                        function.address_access(evaluate(borrow,function,state,known,true),|borrows,when|Access::Memory{borrows,when,access:*access},bounds::Condition::guard(usage.path.clone())),
                    Requirement::Dataset{borrow,index,access,atomic,..}=>function.dataset_access(evaluate(borrow,function,state,known,true),*index,*access,*atomic,bounds::Condition::guard(usage.path.clone())),
                    _=>None,
                };
                if let Some(value)=value{access.insert(value);}
            }
            for call in &function.calls {for requirement in &known[&call.function].interface.access {
                let value=access_at_call(requirement,call,&call.path,function,state,known);
                let value=match value {
                    Access::Position{position,borrows,when}=>{
                        if !position.writes&&!position.initialized{continue;}
                        function.address_access(borrows,|borrows,when|Access::Position{borrows,when,position},when)
                    },
                    Access::Memory{borrows,when,access}=>function.address_access(borrows,|borrows,when|Access::Memory{borrows,when,access},when),
                    Access::Initialized{borrows,initialized,when}=>{
                        let path=when.assume(&bounds::Clause::default(),true);
                        function.initialization_access(&output_initialization_read(&borrows,&initialized,&path),when)
                    },
                    Access::Resource{resources,borrows,ownership,when}=>function.access(resources,borrows,ownership,when),
                    Access::Dataset{borrows,index,access,atomic,when}=>function.dataset_access(borrows,index,access,atomic,when),
                };
                if let Some(value)=value{access.insert(value);}
            }}
            next.get_mut(name).unwrap().interface.access=access;
        }
        if *known==next{break;}*known=next;
    }
}

fn converge(functions:&BTreeMap<String,&Function>,known:&mut States,unknown:&BTreeSet<(String,Output)>){
    loop{
        let mut next=known.clone();
        for(name,function)in functions{
            let state=&known[name];let result=next.get_mut(name).unwrap();
            for input in state.loops.keys(){result.loops.insert(*input,evaluate(&function.loops[input].borrow,function,state,known,true));}
            let physical=function.outputs.iter().map(|(output,definition)|{
                let value=evaluate(definition,function,state,known,true);
                let value=if function.resource_names.contains_key(output){value.resources()}else{value};
                (output.clone(),value)
            }).collect::<Outputs>();
            // 每个真实资源输出命名自己的交付存储；view保留所有可能的实际输出所有者。
            let mut exposed:BTreeMap<ValueIdentity,Borrow>=BTreeMap::new();
            for(output,name)in &function.resource_names{for(source,when)in physical[output].sources.entries(){if let Origin::Data(id)=source{if !matches!(id,ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)){exposed.entry(*id).or_default().union(&name.restricted(&when));}}}}
            for(output,definition)in &physical{
                let mut value=canonical(definition.clone(),function,function.resource_names.get(output),&exposed);
                if unknown.contains(&(name.clone(),output.clone())){value.sources.insert(Origin::Unresolved);}
                result.interface.outputs.insert(output.clone(),value);
            }

        }
        if *known==next{break;}*known=next;
    }
}
pub(super) struct Solved {pub interfaces:BTreeMap<String,Interface>,pub memory:BTreeMap<String,BTreeMap<address::MemoryTarget,address::MemoryEffect>>,pub values:BTreeMap<String,BTreeMap<ValueIdentity,bounds::Proof>>}
pub(super) fn solve(functions:&BTreeMap<String,&Function>,closed:BTreeMap<String,Interface>)->Result<Solved>{
    let loops=used_loops(functions,&closed);
    let mut known=closed.into_iter().map(|(name,interface)|(name,State{interface,..State::default()})).collect::<States>();
    for(name,function)in functions{known.insert(name.clone(),State{
        loops:loops[name].iter().map(|input|(*input,Borrow::default())).collect(),
        interface:Interface{outputs:function.outputs.keys().map(|output|(output.clone(),Borrow::default())).collect(),access:BTreeSet::new(),initialization:Vec::new()},
    });}
    converge(functions,&mut known,&BTreeSet::new());
    let unknown=functions.iter().flat_map(|(name,function)|function.outputs.keys().filter_map(|output|known[name].interface.outputs[output].empty().then_some((name.clone(),output.clone())))).collect::<BTreeSet<_>>();
    if !unknown.is_empty(){converge(functions,&mut known,&unknown);}
    converge_initializations(functions,&mut known);
    converge_access(functions,&mut known);
    let mut memory=BTreeMap::new();let mut scalar_values=BTreeMap::new();
    for(name,function)in functions {
        let mut values:BTreeMap<address::MemoryTarget,address::MemoryEffect>=BTreeMap::new();
        let mut resolved=BTreeMap::new();let mut calls=function.calls.iter().peekable();let mut usages=function.uses.iter().enumerate().peekable();
        while let Some((memory,usage))=usages.next(){
            let Requirement::Memory{target,snapshots,..}=&usage.requirement else{continue;};
            let mut effect=address::MemoryEffect::default();
            if calls.peek().is_some_and(|call|call.memory==memory){
                let call=calls.next().unwrap();
                for access in &known[&call.function].interface.access {
                    let (borrows,when,access)=match access_at_call(access,call,&call.path,function,&known[name],&known).substitute_values(&resolved) {
                        Access::Memory{borrows,when,access}=> (borrows,when,access),
                        Access::Dataset{borrows,when,access,..}=> (borrows,when,access),
                        Access::Position{borrows,when,position} if position.writes||position.initialized=> (borrows,when,match(position.writes,position.initialized){(true,true)=>super::Access::InOut,(true,false)=>super::Access::Write,_=>super::Access::Read}),
                        _=>continue,
                    };
                    let borrow=borrows.restricted(&when.assume(&bounds::Clause::default(),true));
                    effect.union(address::MemoryEffect::of(&borrow,access));
                }
            }else{
                let rest=std::iter::from_fn(||{
                    if matches!(target,address::MemoryTarget::Call(_))&&usages.peek().is_some_and(|(_,usage)|matches!(&usage.requirement,Requirement::Memory{target:other,..}if other==target)){
                        usages.next().map(|(_,usage)|usage)
                    }else{None}
                });
                for usage in std::iter::once(usage).chain(rest){
                    if let Requirement::Memory{borrow,access,..}=&usage.requirement {
                        let borrow=evaluate(borrow,function,&known[name],&known,true).substitute_values(&resolved).restricted(&usage.path.substitute_values(&resolved));
                        if let Some(access)=access{effect.union(address::MemoryEffect::of(&borrow,*access));}
                    }
                }
            }
            if let Some(snapshots)=snapshots{for(component,snapshot)in snapshots.iter().enumerate(){
                let position=match &snapshot.position{Position::Borrowed(borrow,layout,word)=>Position::Borrowed(Arc::new(evaluate(borrow,function,&known[name],&known,true)),layout.clone(),*word),position=>position.clone()};
                let proof=if effect.writes_position(&position){bounds::Proof::default()}else{snapshot.proof.substitute_values(&resolved)};
                resolved.insert(ValueIdentity::MemoryValue(memory,component),proof);
            }}
            values.entry(*target).or_default().union(effect);
        }
        memory.insert(name.clone(),values);
        if function.pending_values(){scalar_values.insert(name.clone(),resolved);}
    }
    for(name,function)in functions{
        let state=&known[name];
        let context=function.value_context(scalar_values.get(name));
        for usage in &function.uses{
            let path=bounds::Clause::both(&usage.path,&context);
            if let Requirement::Nonzero{value,path,span}=&usage.requirement {
                let values=&scalar_values[name];
                let mut value=(**value).clone();
                value.proof=value.proof.substitute_values(values);
                value.constant=value.constant.or_else(||value.proof.known_value(&value.ty));
                Requirement::Nonzero{value:Box::new(value),path:path.substitute_values(values),span:*span}.check(&usage.source)?;
                continue;
            }
            if let Requirement::Initialized{read,code,span,..}=&usage.requirement {
                let read=resolved_read(read,&path,function,state,&known);
                Requirement::Initialized{borrow:read.borrow(),read,code,span:*span}.check(&usage.source)?;
                continue;
            }
            let symbolic=usage.requirement.mapped(|borrow|evaluate(borrow,function,state,&known,false));
            if matches!(&symbolic,Requirement::Unborrowed{resource,borrow,..}if disjoint(resource,borrow,&function.independent,&usage.active_loops)){continue;}
            usage.requirement.mapped(|borrow|evaluate(borrow,function,state,&known,true)).under_path(&path).check(&usage.source)?;
        }
        for call in &function.calls{
            let path=bounds::Clause::both(&call.path,&context);
            for requirement in &known[&call.function].interface.access{
                let value=access_at_call(requirement,call,&path,function,state,&known);
                let(Access::Resource{when,..}|Access::Position{when,..}|Access::Memory{when,..}|Access::Dataset{when,..}|Access::Initialized{when,..})=&value;
                if when.active(){
                    let path=when.assume(&bounds::Clause::default(),true);
                    let requirement=match value {
                        Access::Position{borrows,position,..}=>Requirement::Address{borrow:borrows.restricted(&path),layout:position.layout,writes:position.writes,writable:position.writable,initialized:position.initialized,span:call.span},
                        Access::Resource{resources,borrows,ownership,..}=>Requirement::Unborrowed{resource:resources.restricted(&path),borrow:borrows.restricted(&path),ownership,span:call.span},
                        Access::Memory{..}|Access::Dataset{..}=>continue,
                        Access::Initialized{borrows,initialized,..}=>{
                            let read=output_initialization_read(&borrows,&initialized,&path);
                            Requirement::Initialized{borrow:borrows,read,code:"B229",span:call.span}
                        },
                    };
                    requirement.check(&call.source)?;
                }
            }
            for output in &call.outputs{
                let borrow=known[&call.function].interface.outputs.get(output);
                if matches!(output,Output::Returned(_)|Output::Written(..))&&borrow.is_none_or(|borrow|borrow.empty()||borrow.sources.contains(&Origin::Unresolved)){
                    return call.source.fail(call.span,"B271","view返回或输出没有可闭合的实际借用来源");
                }
            }
        }
    }
    let interfaces=known.into_iter().map(|(name,state)|{
        let mut interface=state.interface;
        if let Some(values)=scalar_values.get_mut(&name){
            let function=functions[&name];
            *values=std::mem::take(values).into_iter().map(|(id,proof)|(id,proof.output_bound(&function.parameters))).collect();
            interface.outputs=interface.outputs.into_iter().map(|(output,borrow)|(output,borrow.substitute_values(values))).collect();
            interface.access=interface.access.iter().map(|access|access.substitute_values(values)).collect();
            interface.initialization=interface.initialization.iter().map(|value|value.substitute_values(values)).collect();
        }
        (name,interface)
    }).collect();
    Ok(Solved{interfaces,memory,values:scalar_values})
}

impl Builder<'_> {
    pub(super) fn dataset_effect(&mut self,place:&Place,access:super::Access,atomic:bool,span:Span)->Result<()> {
        let Place::ApplicationElement{sources,element_layout,index_value,..}=place.storage() else {return Ok(())};
        let borrow=self.data_borrow(sources);
        for ((scalar,_),position) in place.ty().components.iter().zip(place.data_positions()) {
            let mut pending=vec![(position,bounds::Clause::default())];
            while let Some((position,path))=pending.pop() {
                match position {
                    Position::Borrowed(_,_,word)=>{
                        let origin=address::Origin{
                            root:address::Root::Derived(borrow.clone()),layout:address::Layout::of(&Ty::scalar(*scalar)),
                            base:element_layout.clone(),words:Arc::from([word]),positions:Arc::from([]),
                            writable:access.writes(),initialized:Arc::from([access.reads()]),
                        };
                        self.check_view_requirement(Requirement::Dataset{borrow:Borrow::source(Origin::Position(Arc::new(origin))).restricted(&path),index:Some(*index_value),access,atomic,span})?;
                    }
                    Position::Indexed(choices,index)=>pending.extend(choices.iter().enumerate().map(|(i,position)|(position.clone(),bounds::Clause::both(&path,&bounds::Clause::selection(index,i))))),
                    _=>{},
                }
            }
        }
        Ok(())
    }

    pub(super) fn native_dataset_access(&mut self,access:&BTreeSet<Access>,arguments:&BTreeMap<ValueIdentity,bounds::Proof>,span:Span)->Result<Vec<MemoryAccess>> {
        let mut memory=Vec::new();
        for access in access {
            if let Access::Initialized{borrows,initialized,when}=access {
                let path=when.guard_at_call(arguments).assume(&self.path,true);
                if path.possible(){
                    let borrows=borrows.at_call(arguments);
                    let initialized=initialized.iter().map(|fact|fact.at_call(arguments)).collect::<Vec<_>>();
                    let read=output_initialization_read(&borrows,&initialized,&path);
                    Requirement::Initialized{borrow:borrows,read,code:"B229",span}.check(&self.definition.source)?;
                }
                continue;
            }
            let Access::Dataset{borrows,when,index,access,atomic}=access else {continue};
            let path=when.guard_at_call(arguments).assume(&self.path,true);
            if !path.possible(){continue;}
            let cross=!matches!(index.and_then(|index|index.at_call(arguments)),Some(bounds::Operand::Value(ValueIdentity::Register(index))) if index==self.global_index);
            let borrow=self.current_address_borrow(&borrows.at_call(arguments)).restricted(&path);
            for (source,when) in borrow.sources.entries() {
                if !when.possible(){continue;}
                let Origin::Position(origin)=source else {continue};
                memory.extend(origin.aliases().into_iter().map(|alias|MemoryAccess{alias,access:if *atomic{super::Access::Atomic}else{*access}}));
                for ((scalar,_),position) in origin.layout.parts.iter().zip(origin.positions.iter()) {
                    let mut pending=vec![position];
                    while let Some(position)=pending.pop() {
                        match position {
                            Position::Element(binding,word)=>{
                                if access.writes()&&!atomic {
                                    if cross || !matches!(self.view_access[binding],super::Access::Write|super::Access::InOut) {
                                        return self.fail(span,"B144",format!("没有 {} 当前元素的独占修改权",self.program.buffers[*binding].name));
                                    }
                                    self.element_domains.insert(*binding);
                                }
                                if access.reads(){
                                    self.reads.insert(*binding);
                                    if !atomic{self.memory_regions.entry(*binding).or_default().0.insert((cross,*word,*word+scalar.size()));}
                                }
                                if access.writes(){
                                    self.writes.insert(*binding);
                                    self.memory_regions.entry(*binding).or_default().1.insert((*word,*word+scalar.size()));
                                }
                                if cross||*atomic{self.cross_access.insert(*binding);}
                            }
                            Position::Indexed(choices,_)=>pending.extend(choices.iter()),
                            _=>{},
                        }
                    }
                }
            }
        }
        Ok(memory)
    }
}
