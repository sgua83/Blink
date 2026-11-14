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
#[derive(Clone)]
pub(super) enum ContentRead {
    Value {memory:usize,word:u32,borrow:Arc<Borrow>,snapshots:Arc<[ValueSnapshot]>},
    Snapshot {memory:usize,component:usize,position:Position,initial:Arc<bounds::Proof>},
    Call {memory:usize,origin:Arc<address::Origin>,word:u32,snapshots:Option<Arc<[ValueSnapshot]>>},
    Returned {memory:usize},
    Relation {memory:usize,graph:Arc<relation::Graph>,root:usize,snapshots:Option<Arc<[ValueSnapshot]>>},
}
#[derive(PartialEq,Eq,PartialOrd,Ord)]
enum ContentKey<'a>{Value(usize,u32),Snapshot(usize,usize),Call(usize,&'a address::Origin,u32),Returned(usize),Relation(usize,&'a relation::Graph,usize)}
impl ContentRead {
    fn key(&self)->ContentKey<'_>{match self {
        Self::Value{memory,word,..}=>ContentKey::Value(*memory,*word),
        Self::Snapshot{memory,component,..}=>ContentKey::Snapshot(*memory,*component),
        Self::Call{memory,origin,word,..}=>ContentKey::Call(*memory,origin,*word),
        Self::Returned{memory}=>ContentKey::Returned(*memory),
        Self::Relation{memory,graph,root,..}=>ContentKey::Relation(*memory,graph,*root),
    }}
}
impl PartialEq for ContentRead{fn eq(&self,other:&Self)->bool{self.key()==other.key()}}
impl Eq for ContentRead{}
impl PartialOrd for ContentRead{fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl Ord for ContentRead{fn cmp(&self,other:&Self)->std::cmp::Ordering{self.key().cmp(&other.key())}}
#[derive(Clone,PartialEq,Eq)]
pub(super) struct PositionAccess {pub layout:address::Layout,pub written:Option<bounds::Proof>,pub writable:bool,pub initialized:bool}
impl Ord for PositionAccess {
    fn cmp(&self,other:&Self)->std::cmp::Ordering {
        use crate::module::codec::Encode;
        (&self.layout,self.writable,self.initialized).cmp(&(&other.layout,other.writable,other.initialized))
            .then_with(||self.written.bytes().cmp(&other.written.bytes()))
    }
}
impl PartialOrd for PositionAccess {fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
crate::module::codec::record!(PositionAccess {layout,written,writable,initialized});
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Access<Path = bounds::Condition> {
    Resource {resources:Borrow,borrows:Borrow,kind:view::Exclusion,when:Path},
    Position {borrows:Borrow,when:Path,position:PositionAccess},
    Memory {borrows:Borrow,when:Path,access:super::Access},
    Dataset {borrows:Borrow,when:Path,index:Option<bounds::Operand>,access:super::Access,atomic:bool},
    Initialized {borrows:Borrow,initialized:Vec<address::Initialization>,when:Path},
    Stored {borrows:Borrow,word:u32,value:Borrow,when:Path},
}
impl<Path> Access<Path> {
    fn accessed(&self)->Option<(&Borrow,&Path,super::Access)> {
        match self {
            Self::Position{borrows,when,position}=>{
                let access=if position.writable||position.written.is_some(){super::Access::Write}else{super::Access::Read};
                Some((borrows,when,access))
            },
            Self::Memory{borrows,when,access}|Self::Dataset{borrows,when,access,..}=>Some((borrows,when,*access)),
            Self::Stored{borrows,when,..}=>Some((borrows,when,super::Access::Write)),
            Self::Resource{..}|Self::Initialized{..}=>None,
        }
    }
}

impl Access {
    pub(super) fn when_mut(&mut self)->&mut bounds::Condition {match self {
        Self::Resource{when,..}|Self::Position{when,..}|Self::Memory{when,..}|Self::Dataset{when,..}|Self::Initialized{when,..}|Self::Stored{when,..}=>when,
    }}
    pub(super) fn content_context(&self)->bool{match self{
        Self::Resource{resources,borrows,..}=>resources.content_inputs()||borrows.content_inputs(),
        Self::Position{borrows,..}|Self::Memory{borrows,..}|Self::Dataset{borrows,..}|Self::Initialized{borrows,..}=>borrows.content_inputs(),
        Self::Stored{..}=>true,
    }}

    fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self {
        match self {
            Self::Resource{resources,borrows,kind,when}=>Self::Resource{resources:resources.substitute_values(values),borrows:borrows.substitute_values(values),kind:*kind,when:when.substitute_values(values)},
            Self::Position{borrows,when,position}=>Self::Position{borrows:borrows.substitute_values(values),when:when.substitute_values(values),position:PositionAccess{written:position.written.as_ref().map(|proof|proof.substitute_values(values)),..position.clone()}},
            Self::Memory{borrows,when,access}=>Self::Memory{borrows:borrows.substitute_values(values),when:when.substitute_values(values),access:*access},
            Self::Dataset{borrows,when,index,access,atomic}=>{
                let index=index.and_then(|index|match index{bounds::Operand::Value(id)if values.contains_key(&id)=>index.at_call(values),index=>Some(index)});
                Self::Dataset{borrows:borrows.substitute_values(values),when:when.substitute_values(values),index,access:*access,atomic:*atomic}
            },
            Self::Initialized{borrows,initialized,when}=>Self::Initialized{borrows:borrows.substitute_values(values),initialized:initialized.iter().map(|value|value.substitute_values(values)).collect(),when:when.substitute_values(values)},
            Self::Stored{borrows,word,value,when}=>Self::Stored{borrows:borrows.substitute_values(values),word:*word,value:value.substitute_values(values),when:when.substitute_values(values)},
        }
    }
}
crate::module::codec::variants!(Access {0=>Resource{resources,borrows,kind,when},1=>Position{borrows,when,position},2=>Memory{borrows,when,access},3=>Dataset{borrows,when,index,access,atomic},4=>Initialized{borrows,initialized,when},5=>Stored{borrows,word,value,when}});
#[derive(Clone,Default,PartialEq,Eq)]
pub(super) struct Interface {pub outputs:Outputs,pub access:BTreeSet<Access>,pub initialization:Vec<address::Initialization>}
pub(super) struct ValueSnapshot { pub position:Position, pub index:Option<bounds::Operand>, pub scalar:Scalar, pub proof:Arc<bounds::Proof> }
// The original element model and the inputs selected by this read, after earlier Memory values close.
pub(super) struct ElementRead { pub scalar:Scalar, pub proof:bounds::Proof, pub bindings:Arc<BTreeMap<ValueIdentity,bounds::Proof>> }
pub(super) struct Call {
    pub(super) function:String,
    pub(super) memory:usize,
    deferred:bool,
    live_task_places:Borrow,
    pub(super) arguments:BTreeMap<ValueIdentity,bounds::Proof>,
    pub(super) snapshots:Option<Arc<[ValueSnapshot]>>,
    initialization:Vec<address::InitializationEffect>,
    outputs:BTreeSet<Output>,
    pub(super) path:bounds::Clause,
    source:crate::Source,
    span:Span,
}
pub(super) struct Use { pub requirement:Requirement, pub source:crate::Source, pub path:bounds::Clause, pub active_loops:BTreeSet<usize> }

#[derive(Clone)]
pub(super) enum ControlInput {Value{name:String,contents:Option<Vec<usize>>,component:usize},Length{name:String,path:Vec<usize>}}
impl ControlInput {
    fn project(&self, producer: &flow::LoopState) -> Option<bounds::Proof> {
        match self {
            Self::Value { name, contents: None, component } => producer.env.get(name)
                .and_then(binding_value)
                .map(|(ty, proof)| proof.project(&[*component], ty.components.len())),
            Self::Value { name, contents: Some(path), component } => producer.env.get(name)
                .and_then(|binding| view::data_at(binding, path))
                .and_then(|data| data.elements.known())
                .and_then(returns::Returned::view_value)
                .map(|value| bounds::Proof::capture(value).project(&[*component], value.ty.components.len())),
            Self::Length { name, path } => producer.env.get(name)
                .and_then(|binding| view::data_at(binding, path))
                .map(|data| bounds::Proof::capture(&data.length)),
        }
    }
}
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
                        inputs.insert(id,ControlInput::Value{name:name.clone(),contents:None,component});
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
    pub(super) fn bindings(&self, producer: &flow::LoopState) -> BTreeMap<ValueIdentity, bounds::Proof> {
        let mut values = self.inputs.iter().filter_map(|(&id, input)| {
            input.project(producer).map(|proof| (id, proof))
        }).collect::<BTreeMap<_, _>>();
        if let (Some(cursor), Some(next)) = (&self.cursor, &producer.cursor) {
            values.insert(cursor.identity(), bounds::Proof::capture(next));
        }
        values
    }

    fn binding(&self, id: ValueIdentity, producer: &flow::LoopState) -> Option<bounds::Proof> {
        if let (Some(cursor), Some(next)) = (&self.cursor, &producer.cursor)
            && cursor.identity() == id
        {
            return Some(bounds::Proof::capture(next));
        }
        self.inputs.get(&id)?.project(producer)
    }
}
pub(super) struct LoopResource {
    pub name:String,
    pub path:Vec<usize>,
}
pub(super) struct LoopDefinition {
    pub borrow:Borrow,
    pub resource:Option<LoopResource>,
    pub value:Option<(ValueIdentity,Scalar)>,
    // Every input refers to the same actual entry and continued producers.
    pub states:Arc<[flow::LoopState]>,
    pub control:Arc<LoopControl>,
}
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
    pub element_inputs:BTreeMap<residency::Input,(usize,usize)>,
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
    fn visit(data:&application::Data,root:usize,parents:&[Vec<usize>],out:&mut Vec<(usize,Vec<Vec<usize>>)>) {
        for fields in view::resource_paths(&data.ty){
            if let Some(child)=application::model_data(data.elements.known(),&fields){
                let mut path=parents.to_vec();path.push(fields);out.push((root,path.clone()));visit(child,root,&path,out);
            }
        }
    }
    let roots=match value{returns::Returned::Dataset(_)=>1,returns::Returned::OwnedValue(value)=>view::resource_paths(&value.value.ty).len(),_=>0};
    let mut out=(0..roots).map(|root|(root,Vec::new())).collect::<Vec<_>>();
    for root in 0..roots{if let Some(data)=root_resource(value,root){visit(data,root,&[],&mut out);}}out
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
            if data.length.maximum()==0{None}else{application::model_data(data.elements.known(),fields)}
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
    for fields in path{data=application::model_data_mut(data.elements.known_mut(),fields)?;}Some(data)
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
// Resource access binds its element variables once for both location and guard.
// These names belong to one static access, not machine inputs or resource owners.
fn resource_index_values(borrow:&Borrow)->BTreeSet<ValueIdentity>{
    fn visit(borrow:&Borrow,ids:&mut BTreeSet<ValueIdentity>){
        for path in borrow.sources.entries().map(|(_,when)|when).chain(borrow.inputs.entries().map(|(_,when)|when)).chain(borrow.calls.entries().map(|(_,when)|when)){
            ids.extend(bounds::Proof::condition(bounds::Condition::guard(path)).scalar_identities().into_iter().filter(|id|matches!(id,ValueIdentity::ElementIndex(_))));
        }
        for source in &borrow.sources{match source{
            Origin::Element{parent,index,..}=>{if let Some(bounds::Operand::Value(id))=index{ids.insert(*id);}visit(parent,ids);},
            Origin::Parameter(_,indices)=>{for index in indices{if let Some(bounds::Operand::Value(id))=index{ids.insert(*id);}}},
            Origin::Projection{parent,..}=>visit(parent,ids),
            Origin::Position(origin)=>visit(&origin.resources(),ids),
            _=>{},
        }}
    }
    let mut ids=BTreeSet::new();visit(borrow,&mut ids);ids
}
impl Function {
    pub fn pending_values(&self)->bool {self.loops.values().any(|definition|definition.value.is_some())||self.uses.iter().any(|usage|match &usage.requirement {
        Requirement::Memory{snapshots:Some(snapshots),..}=>!snapshots.is_empty(),
        Requirement::Dataset{values:Some(_),..}=>true,
        _=>false,
    })}
    fn value_context(&self,values:Option<&BTreeMap<ValueIdentity,bounds::Proof>>)->bounds::Clause {
        match values {
            Some(values)=>bounds::Clause::values(self.uses.iter().enumerate().flat_map(|(memory,usage)|{
                let snapshots=match &usage.requirement{Requirement::Memory{snapshots:Some(snapshots),..}=>snapshots.as_ref(),_=>&[]};
                let read=match &usage.requirement{Requirement::Dataset{values:Some(read),..}=>Some(read.as_ref()),_=>None};
                snapshots.iter().enumerate().map(move |(component,snapshot)|{
                    let id=ValueIdentity::MemoryValue(memory,component);(snapshot.scalar,id,&values[&id])
                }).chain(read.into_iter().map(move |read|{
                    let id=ValueIdentity::MemoryValue(memory,0);(read[0].scalar,id,&values[&id])
                }))
            }).chain(self.loops.values().filter_map(|definition|definition.value).map(|(id,scalar)|(scalar,id,&values[&id])))),
            None=>bounds::Clause::default(),
        }
    }
    pub fn initialization_when(&self,fact:&address::Initialization)->bounds::Condition {
        // Direct parameter outputs already express the validated normal-return contract.
        if matches!(&fact.source,address::InitializationSource::Position(origin) if matches!(origin.root,address::Root::Parameter(_))) {
            return fact.when.clone();
        }
        let normal=match &self.normal_returns {Some(normal)=>normal.clone(),None=>bounds::Condition::constant(false)};
        normal.negate().logical(fact.when.clone(),true)
    }
    pub fn scope(&mut self)->usize {let scope=self.next_scope;self.next_scope+=1;scope}
    pub fn call(&mut self,function:&str,memory:usize,arguments:BTreeMap<ValueIdentity,bounds::Proof>,snapshots:Option<Arc<[ValueSnapshot]>>,initialization:Vec<address::InitializationEffect>,path:bounds::Clause,source:crate::Source,span:Span,deferred:bool,live_task_places:Borrow)->usize {
        let call=self.calls.len();self.calls.push(Call{function:function.into(),memory,deferred,live_task_places,arguments,snapshots,initialization,outputs:BTreeSet::new(),path,source,span});call
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
    fn address_borrows(&self,borrow:Borrow)->Option<Borrow> {
        let selected=borrow.map_sources(|source|match source {
            Origin::Parameter(id,indices)=>self.parameters.get(&id).copied().filter(|id|self.input(*id))
                .map(|id|Borrow::source(Origin::Parameter(id,indices.into_iter().map(|index|input_index(index,self)).collect()))).unwrap_or_else(||Borrow::source(Origin::Unresolved)),
            Origin::Position(origin) if matches!(origin.root,address::Root::Parameter(_)|address::Root::Derived(_)|address::Root::External(_)|address::Root::Dataset{..}|address::Root::Resource{..}) =>
                {
                    let mut origin=(*origin).clone();
                    if let address::Root::Resource{index,..}|address::Root::Dataset{index,..}=&mut origin.root {
                        *index=index.filter(|index|index.map(|id|self.parameters.get(&id).copied().filter(|id|self.input(*id))).is_some());
                    }
                    if let address::Root::Resource{borrow,..}|address::Root::Dataset{borrow,..}|address::Root::External(address::Identity::Storage(borrow))=&mut origin.root {
                        *borrow=borrow.map_sources(|source|match source{
                            Origin::Data(id) if !self.parameters.get(&id).is_some_and(|id|self.input(*id))=>Borrow::default(),
                            Origin::Position(origin)if origin.has_local()=>Borrow::default(),source=>Borrow::source(source)
                        });
                        if !borrow.active(){return Borrow::default();}
                    }
                    canonical(Borrow::source(Origin::Position(Arc::new(origin))),self,None,&BTreeMap::new())
                },
            Origin::Unresolved|Origin::External|Origin::Contents{..}|Origin::Relation(..)
            |Origin::TaskOutput{projection:view::TaskProjection::Writes|view::TaskProjection::Places,..}=>canonical(Borrow::source(source),self,None,&BTreeMap::new()),
            _=>Borrow::default(),
        });
        if !selected.active(){return None;}
        let bindings=residency::input_parameters(&self.parameters,self.input_width);
        Some(selected.guards_at_call(&bindings))
    }
    fn initialization_access(&self,read:&address::InitializationRead,when:&bounds::Clause)->Option<Access>{
        if read.missing().is_empty(){return None;}
        let parameters=residency::input_parameters(&self.parameters,self.input_width);
        let initialized:Vec<_>=address::initialization_effects(&read.initialized,&bounds::Clause::default()).into_iter().filter_map(|effect|{
            let address::InitializationEffect::Position(mut fact)=effect else{return None;};
            if fact.source.borrow().has_local(){return None;}
            fact.source=match fact.source {
                address::InitializationSource::Position(origin)=>{
                    let value=canonical(Borrow::source(Origin::Position(Arc::new(origin))),self,None,&BTreeMap::new());
                    if value.unresolved(){return None;}
                    let Origin::Position(origin)=value.sources.iter().next()? else{return None;};
                    address::InitializationSource::Position(origin.as_ref().clone())
                },
                address::InitializationSource::Task(borrow)=>{
                    let value=canonical(borrow,self,None,&BTreeMap::new());
                    if value.unresolved(){return None;}
                    address::InitializationSource::Task(value)
                },
            };
            fact.when=fact.when.guaranteed_at_call(&parameters);
            Some(fact)
        }).collect();
        self.address_access(read.borrow(),|borrows,when|Access::Initialized{borrows,initialized:address::merge_initializations(initialized),when},when)
    }
    fn address_access(&self,borrow:Borrow,make:impl FnOnce(Borrow,bounds::Condition)->Access,when:&bounds::Clause)->Option<Access>{
        let borrows = self.address_borrows(borrow)?;
        let bindings = residency::input_parameters(&self.parameters, self.input_width);
        let when = when.project_guard(&bindings);
        when.possible().then(|| make(borrows, bounds::Condition::guard(when)))
    }
    fn access(&self,resources:Borrow,borrows:Borrow,kind:view::Exclusion,when:&bounds::Clause)->Option<Access>{
        if matches!(kind,view::Exclusion::Position(_)) {
            let resources = self.address_borrows(resources)?;
            return self.address_access(borrows, |borrows, when| Access::Resource { resources, borrows, kind, when }, when);
        }
        let mut parameters=residency::input_parameters(&self.parameters,self.input_width);
        let mut indices=resource_index_values(&resources);indices.extend(resource_index_values(&borrows));
        indices.extend(bounds::Proof::condition(bounds::Condition::guard(when.clone())).scalar_identities().into_iter().filter(|id|matches!(id,ValueIdentity::ElementIndex(_))));
        let mut next=0;
        for id in indices{
            if let Some(target)=self.parameters.get(&id).copied().filter(|target|self.input(*target)){
                parameters.entry(id).or_insert_with(||bounds::Proof::default().with_identity(target));
            }else{
                parameters.insert(id,bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(next)));next+=1;
            }
        }
        fn external(function:&Function,borrow:Borrow,parameters:&BTreeMap<ValueIdentity,bounds::Proof>)->Borrow{
            borrow.map_sources(|source|match source{
                Origin::Data(id)=>function.parameters.get(&id).copied().filter(|id|function.input(*id)).map(|id|Borrow::source(Origin::Data(id))).unwrap_or_default(),
                Origin::Parameter(id,indices)=>Borrow::source(function.parameters.get(&id).copied().filter(|id|function.input(*id)).map(|id|Origin::Parameter(id,indices.into_iter().map(|index|index.and_then(|index|index.at_call(parameters))).collect())).unwrap_or(Origin::Unresolved)),
                Origin::Element{parent,fields,index}=>Borrow::element(external(function,*parent,parameters),fields,index.and_then(|index|index.at_call(parameters))),
                Origin::Projection{parent,fields}=>Borrow::projection(external(function,*parent,parameters),fields),
                Origin::Position(origin)=>origin.resources(),
                Origin::Contents{..}|Origin::Relation(..)=>canonical(Borrow::source(source),function,None,&BTreeMap::new()),
                Origin::Task{captures,..}=>external(function,*captures,parameters),
                Origin::TaskOutput{task,projection}=>external(function,*task,parameters).task_projection(projection),
                Origin::Unresolved=>Borrow::source(source),Origin::Inactive|Origin::External=>Borrow::default(),
            }).guards_at_call(parameters)
        }
        let resources=external(self,resources.resources(),&parameters);
        let borrows=external(self,match kind {
            view::Exclusion::Ownership=>borrows.owners(),
            view::Exclusion::View=>borrows.view_resources(),
            _=>borrows.resources(),
        },&parameters);
        if !resources.active()||!borrows.active(){return None;}
        let when=bounds::Condition::guard(when.project_guard(&parameters).canonical());
        when.active().then_some(Access::Resource{resources,borrows,kind,when})
    }
    fn dataset_access(&self,borrow:Borrow,index:Option<bounds::Operand>,access:super::Access,atomic:bool,when:&bounds::Clause,owns_input:&impl Fn(ValueIdentity)->bool)->Option<Access> {
        fn inputs(borrow:&Borrow,function:&Function,owns_input:&impl Fn(ValueIdentity)->bool)->Borrow {
            borrow.map_sources(|source|match source {
                Origin::Data(id) if !function.parameters.get(&id).is_some_and(|id|function.input(*id)&&!owns_input(*id))=>Borrow::default(),
                Origin::Element{parent,fields,index}=>Borrow::element(inputs(&parent,function,owns_input),fields,index),
                Origin::Projection{parent,fields}=>Borrow::projection(inputs(&parent,function,owns_input),fields),
                Origin::Position(origin)=>{
                    let mut origin=origin.map_borrows(|borrow|inputs(borrow,function,owns_input));
                    if let address::Root::Resource{borrow,index:None}=origin.root {origin.root=address::Root::Derived(borrow);}
                    origin.normalize()
                },
                source=>Borrow::source(source),
            })
        }

        let index=index.and_then(|index|index.map(|id|self.parameters.get(&id).copied().filter(|id|self.input(*id))));
        self.address_access(inputs(&borrow,self,owns_input),|borrows,when|Access::Dataset{borrows,when,index,access,atomic},when)
    }
}
#[derive(Clone,Default,PartialEq,Eq)]
struct State<'a> { name: &'a str, loops:BTreeMap<LoopInput,Borrow>, entries:Option<BTreeMap<ValueIdentity,Borrow>>, interface:Interface }
type StorageObserver<'a> = dyn Fn(&str, crate::compiler::BorrowSolveStorage) + 'a;

struct States<'a> {
    functions: BTreeMap<String, State<'a>>,
    observe: Option<&'a StorageObserver<'a>>,
}

impl<'a> std::ops::Deref for States<'a> {
    type Target = BTreeMap<String, State<'a>>;
    fn deref(&self) -> &Self::Target { &self.functions }
}

impl std::ops::DerefMut for States<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.functions }
}


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
        address::InitializationEffect::Position(fact)=>roots.push(fact.source.borrow()),
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
                Requirement::Memory{snapshots,written,..}=>{
                    if let Some(written)=written{roots.push(written.borrowed_view());}
                    if let Some(snapshots)=snapshots{for snapshot in snapshots.iter(){position_roots(&snapshot.position,&mut roots);roots.push(snapshot.proof.borrowed_view());}}
                },
                Requirement::Address{..}|Requirement::Dataset{..}|Requirement::Nonzero{..}=>{},
            }
        }
        for call in &function.calls {
            roots.push(call.live_task_places.clone());
            if contracts.contains(&call.function){roots.extend(call.arguments.values().map(bounds::Proof::borrowed_view));}
        }
        let mut loops=BTreeSet::new();let mut calls=BTreeSet::new();
        while let Some(borrow)=roots.pop() {
            for source in &borrow.sources {
                match source {
                    Origin::Element{parent,..}|Origin::Projection{parent,..}|Origin::TaskOutput{task:parent,..}=>roots.push(parent.as_ref().clone()),
                    Origin::Task{captures,outputs,initialized,..}=>{roots.push(captures.as_ref().clone());roots.push(initialized.as_ref().clone());roots.extend(outputs.iter().cloned());},
                    _=>{},
                }
                if let Origin::Contents{origin,..}=source{roots.push(origin.resources());}
                if let Origin::Relation(graph,root)=source{roots.extend(graph.seed_borrows(*root).into_iter().cloned());}
                if let Origin::Position(origin)=source {
                    match &origin.root {address::Root::Resource{borrow,..}|address::Root::Derived(borrow)=>roots.push(borrow.clone()),_=>{}}
                }
            }
            for input in &borrow.inputs{if loops.insert(*input){roots.push(function.loops[input].borrow.clone());}}
            for result in &borrow.calls{if calls.insert(result.call){roots.extend(function.calls[result.call].arguments.values().map(bounds::Proof::borrowed_view));}}
        }
        (name.clone(),loops)
    }).collect()
}


fn call_content_sources(borrow:&Borrow,call:&Call,arguments:&BTreeMap<ValueIdentity,bounds::Proof>)->Borrow {
    borrow.map_sources(|source|match source {
        Origin::Relation(graph,root)=>Borrow{reads:view::Edges::from([ContentRead::Relation{memory:call.memory,graph:Arc::new(graph.at_call(arguments)),root,snapshots:call.snapshots.clone()}]),..Borrow::default()},
        Origin::Contents{origin,word}=>{
            let origin=origin.map_borrows(|parent|call_content_sources(parent,call,arguments)).bound_at_call(arguments);
            Borrow{reads:view::Edges::from([ContentRead::Call{memory:call.memory,origin:Arc::new(origin),word,snapshots:call.snapshots.clone()}]),..Borrow::default()}
        },
        Origin::Position(origin)=>origin.map_borrows(|parent|call_content_sources(parent,call,arguments)).normalize(),
        Origin::Element{parent,fields,index}=>Borrow::element(call_content_sources(&parent,call,arguments),fields,index),
        Origin::Projection{parent,fields}=>Borrow::projection(call_content_sources(&parent,call,arguments),fields),
        Origin::Task{captures,outputs,initialized,guards,completions}=>Borrow::source(Origin::Task{
            captures:Box::new(call_content_sources(&captures,call,arguments)),
            initialized:Box::new(call_content_sources(&initialized,call,arguments)),
            outputs:outputs.into_iter().map(|output|call_content_sources(&output,call,arguments)).collect(),
            completions:completions.iter().map(|completion|completion.map_task(|task|call_content_sources(task,call,arguments))).collect(),
            guards:guards.into_iter().map(|mut guard|{
                guard.target=call_content_sources(&guard.target,call,arguments);
                guard
            }).collect(),
        }),
        Origin::TaskOutput{task,projection}=>call_content_sources(&task,call,arguments).task_projection(projection),
        source=>Borrow::source(source),
    })
}
fn input_contents(origin:&address::Origin,word:u32)->Borrow {
    match &origin.root {
        address::Root::External(_)=>Borrow::source(Origin::External),
        address::Root::Local{..}|address::Root::Buffer(_)=>Borrow::source(Origin::Unresolved),
        _=>Borrow::source(Origin::Contents{origin:Arc::new(origin.clone()),word}),
    }
}
fn same_content_storage(a:&address::Root,b:&address::Root)->bool {
    match(a,b){
        (address::Root::Derived(a),address::Root::Derived(b))=>a.sources.iter().eq(b.sources.iter())
            &&a.inputs.iter().eq(b.inputs.iter())&&a.calls.iter().eq(b.calls.iter())&&a.reads.iter().eq(b.reads.iter()),
        _=>a==b,
    }
}
fn content_word(origin:&address::Origin,word:u32)->Option<(usize,u32)>{
    let component=origin.layout.parts.iter().position(|(scalar,at)|*at==word&&*scalar==Scalar::U64)?;
    Some((component,origin.words[component]))
}
fn content_position(origin:&address::Origin,word:u32)->Option<Position>{
    origin.position_at(word).filter(|position|!matches!(position,Position::Initialized))
}
#[derive(Default)]
struct ContentAtRead {
    writes: Borrow,
    earlier: Option<bounds::Clause>,
}

impl ContentAtRead {
    fn write(&mut self, incoming: &Borrow, when: &bounds::Condition, complete: bool) {
        let selected=when.assume(&bounds::Clause::default(),true);
        let visible = self.earlier.as_ref().map_or_else(
            || selected.clone(),
            |earlier| bounds::Clause::both(earlier, &selected),
        );
        self.writes.union(&incoming.restricted(&visible));
        if complete {
            let unchanged = when.assume(&bounds::Clause::default(), false);
            self.earlier = Some(match self.earlier.take() {
                Some(earlier) => bounds::Clause::both(&earlier, &unchanged),
                None => unchanged,
            });
        }
    }

    fn with_initial(self, initial: Borrow) -> Borrow {
        let initial = match self.earlier {
            Some(earlier) => initial.restricted(&earlier),
            None => initial,
        };
        initial.joined(self.writes)
    }
}

fn apply_content_write(result:&mut ContentAtRead,query:Option<(&address::Origin,u32)>,position:Option<&Position>,
    target:&address::Origin,word:u32,incoming:&Borrow,when:&bounds::Clause) {
    let(affected,complete)=content_write_overlap(query,position,target,word,when);
    if affected {result.write(incoming,&bounds::Condition::execution(when),complete);}
}
fn content_write_overlap(query:Option<(&address::Origin,u32)>,position:Option<&Position>,target:&address::Origin,word:u32,when:&bounds::Clause)->(bool,bool){
    let complete=query.is_some_and(|(origin,at)|origin.base==target.base&&same_content_storage(&origin.root,&target.root)
        &&content_word(origin,at).zip(content_word(target,word)).is_some_and(|((_,a),(_,b))|a==b))
        ||position.zip(content_position(target,word).as_ref()).is_some_and(|(a,b)|a==b);

    let affected = complete || position.is_some_and(|position| {
        address::MemoryEffect::position_may_write(target, when, position)
    });
    (affected,complete)
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LoopSources {
    Symbolic,
    Deferred,
    Resolved,
}

struct BorrowEvaluation<'a>{
    function:&'a Function,state:&'a State<'a>,known:&'a States<'a>,expand_loops:bool,deferred_reads:bool,
    loop_bindings:Vec<BTreeMap<LoopInput,Borrow>>,
    values:BTreeMap<(LoopSources,ContentRead),Borrow>,
}
impl BorrowEvaluation<'_>{
    fn content_event(&mut self,result:&mut ContentAtRead,query:Option<(&address::Origin,u32)>,position:Option<&Position>,memory:usize){
        let(function,known)=(self.function,self.known);
        let usage=&function.uses[memory];
        let Requirement::Memory{borrow,access,written,address_fields,..}=&usage.requirement else{return;};
        if let Some(call)=function.calls.iter().find(|call|!call.deferred&&call.memory==memory){
            for access in known[&call.function].interface.access.iter().rev(){
                let Access::Stored{borrows,word,value:stored,when}=access else{continue;};
                let when=bounds::Clause::both(&usage.path, &when.projected_case(true, &call.arguments));
                let target=self.evaluate(&call_content_sources(borrows,call,&call.arguments).at_call(&call.arguments));
                let stored=self.evaluate(&call_content_sources(stored,call,&call.arguments).at_call(&call.arguments));
                for(source,selected)in target.sources.entries().rev(){
                    if let Origin::Position(origin) = source {
                        let path = bounds::Clause::both(&when, &selected);
                        if path.possible() {
                            apply_content_write(result, query, position, origin, *word, &stored, &path);
                        }
                    }
                }
            }
            return;
        }
        if !access.is_some_and(super::Access::writes){return;}
        // This opaque write already contributes the same possible external
        // contents for every queried word, without replacing earlier contents.
        if written.is_none()
            && borrow.sources.when(&Origin::External) == bounds::Clause::default()
        {
            let when = &usage.path;
            if when.possible() {
                result.write(&Borrow::source(Origin::External), &bounds::Condition::execution(when), false);
            }
            return;
        }
        let targets=self.evaluate(borrow);
        for(source,selected)in targets.sources.entries().rev(){
            let when=bounds::Clause::both(&usage.path,&selected);
            if !when.possible(){continue;}
            match source {
                Origin::Position(origin)=>{
                    if let Some(written)=written {
                        for &(word,component) in address_fields.iter().rev(){
                            let stored=self.evaluate(&written.view_borrow(component));
                            apply_content_write(result,query,position,origin,word,&stored,&when);
                        }
                    }else{
                        let effect=address::MemoryEffect::of(&Borrow::source(source.clone()).restricted(&when),super::Access::Write,None);
                        if position.is_some_and(|position|effect.written_bytes(position,Scalar::U64,None)!=0){
                            result.write(&Borrow::source(Origin::External),&bounds::Condition::execution(&when),false);
                        }
                    }
                },
                Origin::External=>result.write(&Borrow::source(Origin::External),&bounds::Condition::execution(&when),false),
                _=>{},
            }
        }
    }

    fn content_before(&mut self,origin:&address::Origin,word:u32,memory:usize,snapshots:Option<&[ValueSnapshot]>)->Borrow{

        let position=content_position(origin,word);
        if let Some(snapshots)=snapshots{
            let mut captured=None;
            for snapshot in snapshots.iter(){
                if snapshot.scalar==Scalar::U64 && position.as_ref()==Some(&snapshot.position){
                    let value=self.evaluate(&snapshot.proof.view_borrow(0));
                    if let Some(old)=&mut captured{Borrow::union(old,&value);}else{captured=Some(value);}
                }
            }
            if let Some(value)=captured{return value;}
        }
        let mut result=ContentAtRead::default();
        for at in (0..memory).rev(){self.content_event(&mut result,Some((origin,word)),position.as_ref(),at);}
        result.with_initial(input_contents(origin,word))
    }

    fn stored_candidates(&mut self)->BTreeSet<(Arc<address::Origin>,u32)>{
        let(function,known)=(self.function,self.known);
        let mut result=BTreeSet::new();
        for(memory,usage)in function.uses.iter().enumerate(){
            let Requirement::Memory{borrow,written,address_fields,..}=&usage.requirement else{continue;};
            if let Some(call)=function.calls.iter().find(|call|call.memory==memory){
                for access in &known[&call.function].interface.access{
                    let Access::Stored{borrows,word,when,..}=access else{continue;};
                    let path=bounds::Clause::both(&usage.path, &when.projected_case(true, &call.arguments));
                    let target=self.evaluate(&call_content_sources(borrows,call,&call.arguments).at_call(&call.arguments)).restricted(&path);
                    for(source,when)in target.sources.entries(){if when.possible()&&let Origin::Position(origin)=source{result.insert((origin.clone(),*word));}}
                }
            }else if written.is_some(){
                let target=self.evaluate(borrow).restricted(&usage.path);
                for(source,when)in target.sources.entries(){if when.possible()&&let Origin::Position(origin)=source{
                    for &(word,_) in address_fields.iter(){result.insert((origin.clone(),word));}
                }}
            }
        }
        result
    }

    fn stored_at_exit(&mut self,memory:usize)->Vec<(Arc<address::Origin>,u32,Borrow)>{
        let function=self.function;
        let exposed=exposed_resources(function,function.resource_names.keys().map(|output|(output,self.evaluate(&function.outputs[output]))));
        self.stored_candidates().into_iter().filter_map(|(origin,word)|{
            let target=stored_target(&origin,function,&exposed);
            if !target.active(){return None;}
            let value=self.content_before(&origin,word,memory,None).restricted(&function.uses[memory].path);
            Some((origin,word,value))
        }).collect()
    }

    fn content_read(&mut self,read:&ContentRead,sources:LoopSources)->Borrow{
        let key=(sources,read.clone());
        if let Some(value)=self.values.get(&key){return value.clone();}
        let function=self.function;
        let value=match read {
            ContentRead::Relation{memory,graph,root,snapshots}=>{
                let (graph,root)=graph.bind_incoming(*root,*memory,snapshots.clone(),function,self.state,self.known);
                graph.solve(root,self).restricted(&function.uses[*memory].path)
            },
            ContentRead::Returned{memory}=>{
                let mut value=Borrow::default();
                for(_,_,stored)in self.stored_at_exit(*memory){value.union(&stored);}
                value
            },
            ContentRead::Value{memory,word,borrow,snapshots}=>{
                let targets=self.evaluate(borrow);
                let mut value=Borrow::default();
                for(source,when)in targets.sources.entries(){
                    if let Origin::Position(origin)=source{
                        value.union(&self.content_before(origin,*word,*memory,Some(snapshots)).restricted(&when));
                    }
                }
                value.restricted(&function.uses[*memory].path)
            },
            ContentRead::Snapshot{memory,position,initial,..}=>{
                let value=self.evaluate_sources(&initial.view_borrow(0),sources);
                let mut result=ContentAtRead::default();
                self.content_event(&mut result,None,Some(position),*memory);
                result.with_initial(value).restricted(&function.uses[*memory].path)
            },
            ContentRead::Call{memory,origin,word,snapshots}=>{
                let targets=self.evaluate(&Borrow::source(Origin::Position(origin.clone())));
                let mut value=Borrow::default();
                for(source,when)in targets.sources.entries(){
                    if let Origin::Position(origin)=source{
                        value.union(&self.content_before(origin,*word,*memory,snapshots.as_deref()).restricted(&when));
                    }
                }
                value.restricted(&function.uses[*memory].path)
            },
        };
        self.values.insert(key,value.clone());
        value
    }

    fn evaluate(&mut self,borrow:&Borrow)->Borrow{
        let sources=if self.deferred_reads{LoopSources::Deferred}else if self.expand_loops{LoopSources::Resolved}else{LoopSources::Symbolic};
        self.evaluate_sources(borrow,sources)
    }

    fn evaluate_sources(&mut self,borrow:&Borrow,sources:LoopSources)->Borrow{
        let(function,state,known)=(self.function,self.state,self.known);
        let mut value=Borrow{..Borrow::default()};
        for(source,when)in borrow.sources.entries() {
            let resolved=match source {
                Origin::Relation(graph,root)=>{
                    if matches!(sources,LoopSources::Deferred){Borrow::source(source.clone())}else{
                        if graph.pending(*root){let(graph,root)=graph.bind(*root,function,state,known);graph.solve(root,self)}else{graph.solve(*root,self)}
                    }
                },
                Origin::Element{parent,fields,index}=>Borrow::element(self.evaluate(parent),fields.clone(),*index),
                Origin::Projection{parent,fields}=>Borrow::projection(self.evaluate(parent),fields.clone()),
                Origin::Task{captures,outputs,initialized,guards,completions}=>Borrow::source(Origin::Task{
                    captures:Box::new(self.evaluate(captures)),
                    initialized:Box::new(self.evaluate(initialized)),
                    outputs:outputs.iter().map(|output|self.evaluate(output)).collect(),
                    completions:completions.iter().map(|completion|completion.map_task(|task|self.evaluate(task))).collect(),
                    guards:guards.iter().cloned().map(|mut guard|{
                        guard.target=self.evaluate(&guard.target);
                        guard
                    }).collect(),
                }),
                Origin::TaskOutput{task,projection}=>self.evaluate(task).task_projection(projection.clone()),
                Origin::Position(origin)=>origin.map_borrows(|root|self.evaluate(root)).normalize(),
                Origin::Contents{origin,word}=>Borrow::source(Origin::Contents{origin:Arc::new(origin.map_borrows(|root|self.evaluate(root))),word:*word}),
                source=>Borrow::source(source.clone()),
            };
            value.union(&resolved.restricted(&when));
        }
        for(input,when)in borrow.inputs.entries(){if matches!(sources,LoopSources::Resolved){
            let bound=self.loop_bindings.iter().rev().find_map(|scope|scope.get(input)).cloned();
            let next=match bound{Some(bound)=>bound,None=>self.evaluate_sources(&state.loops[input],sources)};
            value.union(&next.restricted(&when));
        }else{value.inputs.insert_when(*input,when.clone());}}
        for(read,when)in borrow.reads.entries(){if matches!(sources,LoopSources::Deferred){value.reads.insert_when(read.clone(),when);}else{value.union(&self.content_read(read,sources).restricted(&when));}}
        for(result,when)in borrow.calls.entries(){
            let call=&function.calls[result.call];
            let Some(output)=known[&call.function].interface.outputs.get(&result.output)else {value.sources.insert_when(Origin::Unresolved,when.clone());continue;};
            let actual=call_content_sources(output,call,&call.arguments).at_call(&call.arguments);
            value.union(&self.evaluate(&actual).restricted(&when));
        }value
    }
}

// Select storage that survives this function; private stack slots and unreturned
// owned resources have no caller-side stored output.
fn stored_target(origin:&address::Origin,function:&Function,exposed:&BTreeMap<Origin,Borrow>)->Borrow{
    fn escaping(borrow:&Borrow,function:&Function,exposed:&BTreeMap<Origin,Borrow>)->Borrow{
        borrow.map_sources(|source|match source{
            Origin::Data(id) if !function.parameters.get(&id).is_some_and(|id|function.input(*id))&&!exposed.contains_key(&Origin::Data(id))=>Borrow::default(),
            Origin::Position(origin) if origin.has_local()=>Borrow::default(),
            Origin::Element{parent,fields,index}=>Borrow::element(escaping(&parent,function,exposed),fields,index),
            Origin::Projection{parent,fields}=>Borrow::projection(escaping(&parent,function,exposed),fields),
            source=>Borrow::source(source),
        })
    }
    if origin.has_local(){return Borrow::default();}
    let origin=origin.map_borrows(|borrow|escaping(borrow,function,exposed));
    if matches!(&origin.root,address::Root::Resource{borrow,..}|address::Root::Dataset{borrow,..}|address::Root::Derived(borrow)|address::Root::External(address::Identity::Storage(borrow)) if !borrow.active()){return Borrow::default();}
    canonical(Borrow::source(Origin::Position(Arc::new(origin))),function,None,exposed)
}
fn evaluate(borrow:&Borrow,function:&Function,state:&State,known:&States,expand_loops:bool)->Borrow {
    BorrowEvaluation{function,state,known,expand_loops,deferred_reads:false,loop_bindings:Vec::new(),values:BTreeMap::new()}.evaluate(borrow)
}

fn input_index(index:Option<bounds::Operand>,function:&Function)->Option<bounds::Operand>{
    index.and_then(|index|index.map(|id|match id{ValueIdentity::ElementIndex(_)=>Some(id),_=>function.parameters.get(&id).copied().filter(|id|function.input(*id))}))
}
fn canonical(borrow:Borrow,function:&Function,resource:Option<&Borrow>,exposed:&BTreeMap<Origin,Borrow>)->Borrow {
    canonical_relation(borrow,function,resource,exposed,false)
}
fn canonical_relation(borrow:Borrow,function:&Function,resource:Option<&Borrow>,exposed:&BTreeMap<Origin,Borrow>,initialization:bool)->Borrow {
    // Impossible source edges carry known absence, not a live private owner.
    // Keep that knowledge in the existing Inactive representation at the ABI.
    let known=!borrow.sources.is_empty();
    let mut sources=view::Edges::default();
    for(source,when)in borrow.sources.entries(){if when.possible(){sources.insert_when(source.clone(),when);}}
    if known&&sources.is_empty(){sources.insert(Origin::Inactive);}
    let borrow=Borrow{sources,..borrow};
    let parameters=function.parameters.iter().map(|(a,b)|(*a,bounds::Proof::default().with_identity(*b))).collect();
    let address=|origin:&address::Origin|{
        let mut value=origin.map_borrows(|borrow|canonical(borrow.clone(),function,None,exposed));
            if let address::Root::External(address::Identity::Value(ids))=&mut value.root {
                *ids=ids.iter().filter_map(|id|function.parameters.get(id).copied().filter(|id|function.input(*id))).collect();
            }
            if let address::Root::Resource{borrow,index:Some(index)}|address::Root::Dataset{borrow,index:Some(index)}=&mut value.root {
                match index.map(|id|function.parameters.get(&id).copied().filter(|id|function.input(*id))) {
                    Some(bound)=>*index=bound,
                    None=>*borrow=Borrow::source(Origin::Unresolved),
                }
            }
            if !matches!(value.root,address::Root::Local { .. }) {value.positions=Arc::from([]);}
        value
    };
    let renamed=|source|{
        if matches!(&source,Origin::Element{..}|Origin::Projection{..}) && let Some(outputs)=exposed.get(&source){
            return match resource{Some(resource)=>resource.clone(),None=>outputs.clone()};
        }
        match source {
        Origin::Data(id)=>{
            let input=function.parameters.get(&id).copied().filter(|id|function.input(*id));
            if let Some(input)=input{Borrow::source(Origin::Data(input))}
            else if let Some(resource)=resource{resource.clone()}
            else if let Some(outputs)=exposed.get(&Origin::Data(id)){outputs.clone()}
            else{Borrow::source(Origin::Unresolved)}
        }
        Origin::Parameter(id,indices)=>Borrow::source(function.parameters.get(&id).copied().map(|id|Origin::Parameter(id,indices.into_iter().map(|index|input_index(index,function)).collect())).unwrap_or(Origin::Unresolved)),
        Origin::Element{parent,fields,index}=>Borrow::element(canonical(*parent,function,None,exposed),fields,input_index(index,function)),
        Origin::Projection{parent,fields}=>Borrow::projection(canonical(*parent,function,None,exposed),fields),
        Origin::Task{captures,outputs,initialized,guards,completions}=>Borrow::source(Origin::Task{
            captures:Box::new(canonical(*captures,function,None,exposed)),
            initialized:Box::new(canonical_relation(*initialized,function,None,exposed,true)),
            outputs:outputs.into_iter().map(|output|canonical(output,function,None,exposed)).collect(),
            completions:completions.iter().map(|completion|completion.map_task(|task|canonical(task.clone(),function,None,exposed))).collect(),
            guards:guards.into_iter().map(|mut guard|{
                guard.target=canonical(guard.target,function,None,exposed);
                guard.map_arguments(|proof|proof.at_call(&parameters));
                guard.map_inputs(|word|word.map(|id|function.parameters.get(&id).copied()));
                guard
            }).collect(),
        }),
        Origin::TaskOutput{task,projection}=>canonical(*task,function,None,exposed).task_projection(projection.map_result(|id|function.parameters.get(&id).copied())),
        Origin::Position(origin)=>Borrow::source(Origin::Position(Arc::new(address(&origin)))),
        Origin::Contents{origin,word}=>Borrow::source(Origin::Contents{origin:Arc::new(address(&origin)),word}),
        Origin::Relation(graph,root)=>Borrow::source(Origin::Relation(Arc::new(graph.canonical(function,exposed)),root)),
        Origin::Unresolved|Origin::Inactive|Origin::External=>Borrow::source(source),
    }};
    if initialization {
        return borrow.initialization_at_call(&parameters).map_sources(renamed);
    }
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
    let definition=&function.loops[&input];let resource=definition.resource.as_ref().unwrap();
    let mut result=Position::Initialized;
    let mut invariant=function.parameters.keys().map(|id|(*id,bounds::Proof::default().with_identity(*id))).collect::<BTreeMap<_,_>>();
    for(name,binding)in &definition.states[0].env {
        if let Some((ty,proof))=binding_value(binding) {
            for component in 0..ty.components.len() {
                if let Some(id)=proof.known_identity(component) && definition.states.iter().skip(1).all(|producer|
                    producer.env.get(name).and_then(binding_value).is_some_and(|(_,proof)|proof.known_identity(component)==Some(id))) {
                    invariant.insert(id,bounds::Proof::default().with_identity(id));
                }
            }
        }
    }
    for producer in definition.states.iter() {
        let mut bindings=invariant.clone();bindings.extend(definition.control.bindings(producer));
        let mut source_path=producer.path.clone();
        if let Some(test)=&definition.control.test {
            source_path=bounds::Clause::both(&source_path, &test.projected_case(true, &bindings));
        }
        let read_path=path.project_guard(&bindings);
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
    for fact in &mut facts {fact.source=fact.source.map_borrows(|borrow|evaluate(borrow,function,state,known,true));}
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

fn refreshed_at_call<'a>(borrow:Borrow,path:&bounds::Clause,initialized:&impl Fn()->&'a BTreeSet<Position>)->Borrow {
    borrow.map_sources(|source|match source {
        Origin::Position(origin) if origin.initialized.iter().any(|ready| !ready) => {
            let fact=address::Initialization{source:address::InitializationSource::Position(origin.as_ref().clone()),when:bounds::Condition::constant(true)};
            let positions=fact.positions(path);
            let mut origin=origin.as_ref().clone();
            origin.initialized=origin.initialized.iter().enumerate().map(|(i,ready)|*ready||positions.get(i).is_some_and(|position|position.initialized_by(initialized()))).collect();
            Borrow::source(Origin::Position(Arc::new(origin)))
        }
        source=>Borrow::source(source),
    })
}

fn borrow_at_call(borrow: &Borrow, call: &Call, arguments: &BTreeMap<ValueIdentity, bounds::Proof>) -> Borrow {
    call_content_sources(borrow, call, arguments).at_call(arguments)
}

fn access_at_call<'a>(access:&Access,call:&Call,path:&bounds::Clause,evaluation:&mut BorrowEvaluation<'_>,initialized:&impl Fn()->&'a BTreeSet<Position>)->Access<bounds::Clause>{
    let mut apply=|access:&Access,arguments:&BTreeMap<ValueIdentity,bounds::Proof>|{
    let(Access::Resource{borrows,when,..}|Access::Position{borrows,when,..}|Access::Memory{borrows,when,..}|Access::Dataset{borrows,when,..}|Access::Initialized{borrows,when,..}|Access::Stored{borrows,when,..})=access;
    let borrows=borrow_at_call(borrows, call, arguments);
    let when=bounds::Clause::both(path, &when.projected_case(true, arguments));
    let borrows=evaluation.evaluate(&borrows);
    let borrows=if matches!(access,Access::Initialized{..}){borrows}else{refreshed_at_call(borrows,path,initialized)};
    match access {
        Access::Resource{resources,kind,..}=>Access::Resource{resources:evaluation.evaluate(&borrow_at_call(resources,call,arguments)),borrows,kind:*kind,when},
        Access::Position{position,..}=>Access::Position{borrows,when,position:PositionAccess{written:position.written.as_ref().map(|proof|proof.at_call(arguments)),..position.clone()}},
        Access::Memory{access,..}=>Access::Memory{borrows,when,access:*access},
        // The value belongs to content reads. Access consumers use only the write location.
        Access::Stored{word,value,..}=>Access::Stored{borrows,when,word:*word,value:value.clone()},
        Access::Initialized{initialized,..}=>Access::Initialized{borrows,when,initialized:initialized.iter().map(|fact|fact.at_call(arguments)).collect()},
        Access::Dataset{index,access,atomic,..}=>Access::Dataset{borrows,when,index:index.and_then(|index|index.at_call(arguments)),access:*access,atomic:*atomic},
    }
    };
    match access{
        Access::Resource{resources,borrows,when,..}=>{
            let mut indices=resource_index_values(resources);indices.extend(resource_index_values(borrows));
            indices.extend(bounds::Proof::condition(when.clone()).scalar_identities());
            let mut caller=bounds::Proof::condition(bounds::Condition::guard(path.clone())).scalar_identities();
            for value in call.arguments.values(){caller.extend(value.scalar_identities());caller.extend(resource_index_values(&value.borrowed_view()));}
            let mut next=caller.into_iter().filter_map(|id|if let ValueIdentity::ElementIndex(index)=id{Some(index+1)}else{None}).max().unwrap_or(0);
            let mut names=BTreeMap::new();let mut arguments=call.arguments.clone();
            for id in indices{if matches!(id,ValueIdentity::ElementIndex(_)){
                let target=ValueIdentity::ElementIndex(next);next+=1;
                names.insert(id,bounds::Proof::default().with_identity(target));arguments.insert(target,bounds::Proof::default().with_identity(target));
            }}
            apply(&access.substitute_values(&names),&arguments)
        },
        _=>apply(access,&call.arguments),
    }
}

fn output_initialization_read(borrows:&Borrow,initialized:&[address::Initialization],path:&bounds::Clause)->address::InitializationRead {
    let mut positions=Vec::new();
    for(source,when)in borrows.sources.entries(){
        if !bounds::Clause::both(path, &when).possible(){continue;}
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
fn exposed_resources<'a>(function:&Function,physical:impl Iterator<Item=(&'a Output,Borrow)>)->BTreeMap<Origin,Borrow> {
    fn local(source:&Origin,function:&Function)->bool{match source{
        Origin::Data(id)=>matches!(id,ValueIdentity::Register(_))&&!function.parameters.get(id).is_some_and(|id|function.input(*id)),
        Origin::Element{parent,index:Some(_),..}|Origin::Projection{parent,..}=>parent.calls.is_empty()&&parent.inputs.is_empty()&&parent.active()&&parent.sources.iter().all(|source|local(source,function)),
        _=>false,
    }}
    let mut exposed:BTreeMap<Origin,Borrow>=BTreeMap::new();
    for(output,physical)in physical {
        if let Some(name)=function.resource_names.get(output){
            for(source,when)in physical.sources.entries(){
                let named=match source{Origin::Data(id)=>!matches!(id,ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)),source=>local(source,function)};
                if named{exposed.entry(source.clone()).or_default().union(&name.restricted(&when));}
            }
        }
    }
    exposed
}
struct InitializationExitPaths {
    parameters: BTreeMap<ValueIdentity, bounds::Proof>,
    exits: Vec<(bounds::Clause, bounds::Condition)>,
}

impl InitializationExitPaths {
    fn new(function: &Function) -> Self {
        let parameters = function.parameters.iter().map(|(source, target)| {
            (*source, bounds::Proof::default().with_identity(*target))
        }).collect();
        let exits = function.initialization.iter().filter_map(|effect| {
            let when = match effect {
                address::InitializationEffect::Position(fact) => &fact.when,
                address::InitializationEffect::Call { when, .. } | address::InitializationEffect::Iteration { when, .. } => when,
            };
            let path = when.projected_case(true, &parameters);
            path.possible().then(|| (path.clone(), bounds::Condition::guard(path)))
        }).collect();
        Self { parameters, exits }
    }
}

fn initializations(function:&Function,state:&State,known:&States,exit_paths:Option<&InitializationExitPaths>)->Vec<address::Initialization> {
    let exposed=exposed_resources(function,function.resource_names.keys().map(|output|(output,evaluate(&function.outputs[output],function,state,known,true))));
    // Reading a fixed input before this function's first write already requires those bytes on entry.
    // Re-exporting the same bytes as newly initialized adds no fact for any legitimate caller.
    let mut required=Vec::new();
    for(memory,usage)in function.uses.iter().enumerate(){
        if function.calls.iter().any(|call|call.memory==memory)||matches!(&usage.requirement,Requirement::Memory{access:Some(access),..}if access.writes()){break;}
        if let Requirement::Address{borrow,initialized:true,..}=&usage.requirement && !borrow.pending(){
            for(source,when)in borrow.sources.entries(){
                let value=canonical(Borrow::source(source.clone()),function,None,&exposed);
                for selected in value.sources.iter(){if let Origin::Position(origin)=selected{required.push((origin.clone(),bounds::Condition::guard(bounds::Clause::both(&usage.path,&when))));}}
            }
        }
    }
    let facts=initialization_effects(&function.initialization,function,state,known).facts;
    let facts=address::merge_initializations(facts.into_iter().filter_map(|mut fact|{
        fact.source=match fact.source {
            address::InitializationSource::Position(mut origin)=>{
                // A guarantee names one actual selected location; possible owners are never separate writes.
                loop {
                    origin=origin.map_borrows(|borrow|address::borrow_in_path(borrow,&fact.when.assume(&bounds::Clause::default(),true)));
                    let value=origin.normalize();
                    if !value.inputs.is_empty()||!value.calls.is_empty()||value.sources.iter().count()!=1{break;}
                    let(source,when)=value.sources.entries().next().unwrap();
                    let Origin::Position(selected)=source else{break;};
                    if bounds::Condition::guard(when).truth_in(&bounds::Clause::default())!=Some(true)||**selected==origin{break;}
                    origin=selected.as_ref().clone();
                }
                if origin.has_local(){return None;}
                origin=origin.map_borrows(|borrow|canonical(borrow.clone(),function,None,&exposed));
                if let address::Root::Resource{index:Some(index),..}|address::Root::Dataset{index:Some(index),..}=&mut origin.root {
                    *index=index.map(|id|function.parameters.get(&id).copied().filter(|id|function.input(*id)))?;
                }
                let path=fact.when.assume(&bounds::Clause::default(),true);
                if origin.layout.parts.iter().zip(origin.words.iter()).all(|((scalar,_),word)|required.iter().any(|(read,when)|{
                    when.truth_in(&path)==Some(true)&&read.base==origin.base&&same_content_storage(&read.root,&origin.root)
                        &&read.layout.parts.iter().zip(read.words.iter()).any(|((read_scalar,_),read_word)|read_scalar==scalar&&read_word==word)
                })){return None;}
                origin.positions=Arc::from([]);
                address::InitializationSource::Position(origin)
            },
            address::InitializationSource::Task(borrow)=>{
                if borrow.has_local(){return None;}
                address::InitializationSource::Task(canonical(borrow,function,None,&exposed))
            },
        };
        if exit_paths.is_none(){fact.when=bounds::Condition::constant(true);}
        Some(fact)
    }));
    let Some(exit_paths) = exit_paths else { return facts; };
    facts.into_iter().filter_map(|mut fact|{
        fact.when=normalized_condition(function.initialization_when(&fact).guaranteed_at_call(&exit_paths.parameters));
        // Original normal exits form a finite induction domain; recursive scalar substitutions
        // cannot manufacture an unbounded chain of successively expanded path formulas.
        let mut when=bounds::Condition::constant(false);
        if fact.when.truth_in(&bounds::Clause::default())==Some(true){when=bounds::Condition::constant(true);}
        else{for (path, exit) in &exit_paths.exits{
            if fact.when.truth_in(path)==Some(true){when=when.logical(exit.clone(),true);}
        }}
        fact.when=normalized_condition(when);
        fact.when.active().then_some(fact)
    }).collect()
}
fn converge_initializations(functions:&BTreeMap<String,&Function>,known:&mut States) {
    // First grow written locations, then retain only guarantees on normal return.
    for candidates in [true, false] {
        // Original exit paths depend on the declaration and parameter identities.
        let exit_paths: BTreeMap<_, _> = if candidates {
            BTreeMap::new()
        } else {
            functions.iter().filter(|(name, _)| !known[*name].interface.initialization.is_empty())
                .map(|(name, function)| (name, InitializationExitPaths::new(function))).collect()
        };
        loop {
            let mut changed = Vec::new();
            for (name, function) in functions {
                let state = &known[name];
                let old = &state.interface.initialization;
                if !candidates && old.is_empty() {
                    continue;
                }
                let paths = exit_paths.get(name);
                let facts = initializations(function, state, known, paths);
                let facts = if paths.is_some() {
                    let guarantees: BTreeMap<_, _> = facts.into_iter()
                        .map(|fact| (fact.source, fact.when)).collect();
                    old.iter().filter_map(|old| {
                        let when = normalized_condition(old.when.clone().logical(guarantees.get(&old.source)?.clone(), false));
                        when.active().then(|| address::Initialization { source: old.source.clone(), when })
                    }).collect()
                } else {
                    facts
                };
                if facts != state.interface.initialization {
                    changed.push((name, facts));
                }
            }
            if changed.is_empty() {
                break;
            }
            for (name, facts) in changed {
                known.get_mut(name).unwrap().interface.initialization = facts;
            }
        }
    }
}
fn converge_access(functions:&BTreeMap<String,&Function>,known:&mut States,owns_input:&impl Fn(&str,ValueIdentity)->bool) {
    fn feeds_back(name:&str,functions:&BTreeMap<String,&Function>)->bool {
        let mut pending=functions[name].calls.iter().map(|call|call.function.as_str()).collect::<Vec<_>>();
        let mut visited=BTreeSet::new();
        while let Some(next)=pending.pop() {
            if next==name{return true;}
            if visited.insert(next) && let Some(function)=functions.get(next) {
                pending.extend(function.calls.iter().map(|call|call.function.as_str()));
            }
        }
        false
    }
    let feedback=functions.keys().filter(|name|feeds_back(name,functions)).map(|name|name.as_str()).collect::<BTreeSet<_>>();
    fn insert(accesses:&mut BTreeMap<Access,bounds::Condition>,mut access:Access) {
        let when=std::mem::replace(access.when_mut(),bounds::Condition::constant(true));
        accesses.entry(access).and_modify(|old|{if *old!=when {
            *old=old.clone().logical(when.clone(),true);
        }}).or_insert(when);
    }

    // Borrow sources, stored writes and initialization are fixed before this stage.
    // Only the access requirements propagated through calls change between rounds.
    let direct = functions.iter().map(|(name, function)| {
        let state = &known[name];
        let parameters = residency::input_parameters(&function.parameters, function.input_width);
        let mut access=BTreeMap::new();
        let mut evaluation = BorrowEvaluation {
            function, state, known, expand_loops: true, deferred_reads: false,
            loop_bindings: Vec::new(), values: BTreeMap::new(),
        };
        for value in state.interface.access.iter().filter(|access|matches!(access,Access::Stored{..})) {insert(&mut access,value.clone());}
        for usage in &function.uses {
            if let Requirement::Initialized{read,code,..}=&usage.requirement {
                let read=resolved_read(read,&usage.path,function,state,known);
                if *code=="B229" {
                    if let Some(value)=function.initialization_access(&read,&usage.path){insert(&mut access,value);}
                    continue;
                }
                for(source,when)in read.borrow().sources.entries() {
                    if let Origin::Position(origin)=source {
                        if let Some(value)=function.address_access(Borrow::source(source.clone()),|borrows,when|Access::Position{borrows,when,position:PositionAccess{layout:origin.layout.clone(),written:None,writable:false,initialized:true}},
                            &bounds::Clause::both(&usage.path,&when)){insert(&mut access,value);}
                    }
                }
                continue;
            }
            let value=match &usage.requirement {
                Requirement::Unborrowed{resource,borrow,kind,..}=>function.access(
                    evaluation.evaluate(resource),evaluation.evaluate(borrow),
                    *kind,&usage.path),
                Requirement::Address{borrow,layout,written,writable,initialized,..}=>function.address_access(
                    evaluation.evaluate(borrow),
                    |borrows,when|Access::Position{borrows,when,position:PositionAccess{layout:layout.clone(),written:written.as_ref().map(|proof|proof.at_call(&parameters)),writable:*writable,initialized:*initialized}},
                    &usage.path),
                Requirement::Memory{target:address::MemoryTarget::Call(memory),borrow,access:Some(access),..} if !function.calls.iter().any(|call|!call.deferred&&call.memory==*memory)=>
                    function.address_access(evaluation.evaluate(borrow),|borrows,when|Access::Memory{borrows,when,access:*access},&usage.path),
                Requirement::Dataset{borrow,index,access,atomic,..}=>function.dataset_access(evaluation.evaluate(borrow),*index,*access,*atomic,&usage.path,&|id|owns_input(name,id)),
                _=>None,
            };
            if let Some(value)=value{insert(&mut access,value);}
        }
        (name, access)
    }).collect::<BTreeMap<_, _>>();

    // Closed callees supply fixed inputs to this access solve. Keep their
    // contributions separate so every round joins them in source call order.
    let mut closed_calls: BTreeMap<(&str, usize), Vec<Access>> = BTreeMap::new();
    loop {
        let mut changed = Vec::new();
        for(name,function)in functions {
            let normalize=feedback.contains(name.as_str());
            let state=&known[name];
            let parameters=residency::input_parameters(&function.parameters,function.input_width);
            let mut access=direct[name].clone();
            let mut evaluation = BorrowEvaluation {
                function, state, known, expand_loops: true, deferred_reads: false,
                loop_bindings: Vec::new(), values: BTreeMap::new(),
            };
            for (index, call) in function.calls.iter().enumerate().filter(|(_, call)| call.function != *name || state.entries.is_none()) {
                let key = (name.as_str(), index);
                if let Some(accesses) = closed_calls.get(&key) {
                    for value in accesses {
                        insert(&mut access, value.clone());
                    }
                    continue;
                }
                let mut closed = (!functions.contains_key(&call.function)).then(Vec::new);
                let mut emit = |value: Access| {
                    if let Some(closed) = &mut closed {
                        closed.push(value.clone());
                    }
                    insert(&mut access, value);
                };
                let live_task_places=evaluation.evaluate(&call.live_task_places);
                let task_access=live_task_places.active();
                let initialized = std::cell::LazyCell::new(|| initialization_effects(&call.initialization, function, state, known).positions(&call.path));
                for requirement in &known[&call.function].interface.access {
                if !task_access && (matches!(requirement,Access::Stored{..})
                    || matches!(requirement,Access::Position{position,..} if !position.writable && !position.initialized && (call.deferred || position.written.is_none()))) {
                    continue;
                }
                let value=access_at_call(requirement,call,&call.path,&mut evaluation,&|| &*initialized);
                if task_access
                    && let Some((accessed,when,mode))=value.accessed()
                    && let Some(exclusion)=function.access(accessed.clone(),live_task_places.clone(),view::Exclusion::Position(mode),when)
                {
                    emit(exclusion);
                }
                let value=match value {
                    Access::Position{mut position,borrows,when}=>{
                        if call.deferred { position.written=None; }
                        if position.written.is_none()&&!position.initialized&&!position.writable{continue;}
                        position.written=position.written.as_ref().map(|proof|proof.at_call(&parameters));
                        function.address_access(borrows,|borrows,when|Access::Position{borrows,when,position},&when)
                    },
                    Access::Stored{..}=>continue,
                    Access::Memory{borrows,when,access}=>function.address_access(borrows,|borrows,when|Access::Memory{borrows,when,access},&when),
                    Access::Initialized{borrows,initialized,when}=>function.initialization_access(
                        &output_initialization_read(&borrows,&initialized,&when),&when),
                    Access::Resource{resources,borrows,kind,when}=>function.access(resources,borrows,kind,&when),
                    Access::Dataset{borrows,index,access,atomic,when}=>function.dataset_access(borrows,index,access,atomic,&when,&|id|owns_input(name,id)),
                };
                if let Some(value)=value{emit(value);}
                }
                if let Some(closed) = closed {
                    closed_calls.insert(key, closed);
                }
            }
            if let Some(entries)=&state.entries{
                let invoked=function.calls.iter().filter(|call|call.function==*name).map(|call|bounds::Condition::execution(&call.path).guard_at_call(&parameters)).reduce(|a,b|a.logical(b,true)).unwrap();
                let inputs=entries.iter().map(|(input,value)|(*input,bounds::Proof::default().with_view_borrow(0,value.clone()))).collect::<BTreeMap<_,_>>();
                let direct=access.iter().map(|(access,when)|{let mut a=access.clone();*a.when_mut()=when.clone();a}).collect::<Vec<_>>();
                for value in direct{let value=match value{
                    Access::Position{borrows,when,position}=>Access::Position{borrows:borrows.at_call(&inputs),when:when.guard_at_call(&inputs),position:PositionAccess{written:position.written.as_ref().map(|p|p.at_call(&inputs)),..position}},
                    Access::Memory{borrows,when,access}=>Access::Memory{borrows:borrows.at_call(&inputs),when:when.guard_at_call(&inputs),access},
                    Access::Initialized{borrows,when,initialized}=>Access::Initialized{borrows:borrows.at_call(&inputs),when:when.guard_at_call(&inputs),initialized:initialized.iter().map(|f|f.at_call(&inputs)).collect()},
                    Access::Resource{resources,borrows,when,kind}=>Access::Resource{resources:resources.at_call(&inputs),borrows:borrows.at_call(&inputs),when:when.guard_at_call(&inputs),kind},
                    Access::Dataset{borrows,when,index,access,atomic}=>Access::Dataset{borrows:borrows.at_call(&inputs),when:when.guard_at_call(&inputs),index:index.and_then(|i|i.at_call(&inputs)),access,atomic},
                    Access::Stored{..}=>continue,
                };let mut value=value;let when=value.when_mut();*when=when.clone().logical(invoked.clone(),false);insert(&mut access,value);}
            }
            let access = access.into_iter().map(|(mut access, when)| {
                *access.when_mut() = if normalize { normalized_condition(when) } else { when };
                access
            }).collect::<BTreeSet<_>>();
            if access != state.interface.access {
                changed.push((name, access));
            }
        }
        if changed.is_empty() {
            break;
        }
        for (name, access) in changed {
            known.get_mut(name).unwrap().interface.access = access;
        }
    }
}

fn stored_outputs(evaluation:&mut BorrowEvaluation<'_>,exposed:&BTreeMap<Origin,Borrow>)->BTreeSet<Access>{
    let function=evaluation.function;
    let mut stored:BTreeMap<(Borrow,u32),Borrow>=BTreeMap::new();
    for usage in &function.uses{
        let Requirement::Return{borrow,..}=&usage.requirement else{continue;};
        for(read,_)in borrow.reads.entries(){
            let ContentRead::Returned{memory}=read else{continue;};
            for(origin,word,value)in evaluation.stored_at_exit(*memory){
                let target=stored_target(&origin,function,exposed);
                stored.entry((target,word)).or_default().union(&canonical(value,function,None,exposed));
            }
        }
    }
    stored.into_iter().map(|((borrows,word),value)|Access::Stored{borrows,word,value,when:bounds::Condition::constant(true)}).collect()
}

fn converge(functions:&BTreeMap<String,&Function>,known:&mut States,unknown:&BTreeSet<(String,Output)>){
    loop{
        let mut changed = Vec::new();
        for(name,function)in functions{
            let state=&known[name];
            let mut evaluation=BorrowEvaluation{function,state,known,expand_loops:true,deferred_reads:false,loop_bindings:Vec::new(),values:BTreeMap::new()};
            let loops = relation::Graph::loops(function,state,known);
            let entries = relation::Graph::recursive_inputs(name,function,state,known);
            let forwards=|output:&Output,definition:&Borrow|matches!(output,Output::Returned(_))&&definition.calls.iter().any(|result|function.calls[result.call].function==*name&&result.output==*output);
            let entry_bindings=entries.as_ref().filter(|_|function.outputs.iter().any(|(output,definition)|forwards(output,definition))).map(|entries|{
                let mut inverse=BTreeMap::new();for(&local,&formal)in &function.parameters{inverse.entry(formal).or_insert_with(||bounds::Proof::default().with_identity(local).with_view_borrow(0,Borrow::source(Origin::Parameter(local,Vec::new()))));}
                let parameters=residency::input_parameters(&function.parameters,function.input_width);
                function.parameters.iter().filter_map(|(&local,&formal)|{
                    let entry=entries.get(&formal)?.substitute_values(&inverse);
                    let unchanged=function.calls.iter().filter(|call|call.function==*name).all(|call|call.arguments.get(&formal).is_some_and(|value|value.at_call(&parameters).known_identity(0)==Some(formal)));
                    let proof=if unchanged{bounds::Proof::default().with_identity(local)}else{bounds::Proof::default()};
                    Some((local,proof.with_view_borrow(0,entry)))
                }).collect::<BTreeMap<_,_>>()
            });
            let physical=function.outputs.iter().map(|(output,definition)|{
                let basis=match &entry_bindings{
                    Some(bindings)if forwards(output,definition)=>{
                        let mut basis=definition.clone();let calls=basis.calls.iter().filter(|result|function.calls[result.call].function==*name&&result.output==*output).cloned().collect::<Vec<_>>();
                        for call in calls{basis.calls.remove(&call);}
                        std::borrow::Cow::Owned(basis.substitute_values(bindings))
                    },
                    _=>std::borrow::Cow::Borrowed(definition),
                };
                let value=evaluation.evaluate(&basis);
                let value=if function.resource_names.contains_key(output){value.resources()}else{value};
                (output.clone(),value)
            }).collect::<Outputs>();
            // 每个真实资源输出命名自己的交付存储；view保留所有可能的实际输出所有者。
            let exposed=exposed_resources(function,physical.iter().map(|(output,value)|(output,value.clone())));
            let outputs = physical.into_iter().map(|(output, definition)| {
                let mut value=canonical(definition,function,function.resource_names.get(&output),&exposed);
                if unknown.contains(&(name.clone(),output.clone())){value.sources.insert(Origin::Unresolved);}
                (output, value)
            }).collect::<Outputs>();
            let access = stored_outputs(&mut evaluation,&exposed);
            if loops != state.loops || entries != state.entries
                || outputs != state.interface.outputs || access != state.interface.access
            {
                changed.push((name, loops, entries, outputs, access));
            }
        }
        if changed.is_empty() {
            break;
        }
        for (name, loops, entries, outputs, access) in changed {
            let state = known.get_mut(name).unwrap();
            state.loops = loops;
            state.entries = entries;
            state.interface.outputs = outputs;
            state.interface.access = access;
        }
    }
}
struct LoopConditions {
    carried: Vec<LoopInput>,
    terms: Vec<bounds::Condition>,
}
impl LoopConditions {
    fn include(&mut self, source: &bounds::Proof, value: &bounds::Proof) {
        if value.component_condition(0).is_some()
            && let Some(input) = source.carried_loop_condition()
        {
            self.carried.push(input);
        } else {
            self.terms.push(value.condition_for_join(0));
        }
    }

    fn collect(
        &mut self,
        id: ValueIdentity,
        definition: &LoopDefinition,
        source: &bounds::Proof,
        mut proof: bounds::Proof,
        values: &BTreeMap<ValueIdentity, bounds::Proof>,
    ) -> (bounds::Proof, bool) {
        let mut explicit = proof.component_condition(0).is_some();
        if explicit {
            self.include(source, &proof);
        }
        if values.contains_key(&id) {
            for state in &definition.states[1..] {
                let source = definition.control.binding(id, state).unwrap();
                let incoming = source.substitute_values(values);
                let conditioned = incoming.component_condition(0).is_some();
                if explicit || conditioned {
                    if !explicit {
                        self.terms.push(proof.condition_for_join(0));
                    }
                    self.include(&source, &incoming);
                    explicit = true;
                }
                proof.join_fields(&incoming, 1, None);
            }
        }
        (proof, explicit)
    }
}

fn solve_loop_value(
    function: &Function,
    input: LoopInput,
    id: ValueIdentity,
    definition: &LoopDefinition,
    values: &BTreeMap<ValueIdentity, bounds::Proof>,
) -> bounds::Proof {
    let source = definition.control.binding(id, &definition.states[0]).unwrap();
    let proof = source.substitute_values(values);
    if definition.states.len() == 1 || !values.contains_key(&id) {
        return proof;
    }

    let mut conditions = LoopConditions { carried: Vec::new(), terms: Vec::new() };
    let (mut proof, _) = conditions.collect(id, definition, &source, proof, values);
    // Close the original carry edges before building the two disjunctions.
    let mut visited = BTreeSet::from([input]);
    while let Some(input) = conditions.carried.pop() {
        if !visited.insert(input) {
            continue;
        }
        let id = ValueIdentity::LoopValue(input.level, input.index);
        let definition = &function.loops[&input];
        let source = definition.control.binding(id, &definition.states[0]).unwrap();
        let value = source.substitute_values(values);
        let (value, explicit) = conditions.collect(id, definition, &source, value, values);
        if !explicit {
            conditions.terms.push(value.condition_for_join(0));
        }
    }
    proof.set_loop_condition(conditions.terms.into_iter().reduce(bounds::Condition::common));
    proof
}

pub(super) struct Solved {pub interfaces:BTreeMap<String,Interface>,pub memory:BTreeMap<String,BTreeMap<address::MemoryTarget,address::MemoryEffect>>,pub values:BTreeMap<String,BTreeMap<ValueIdentity,bounds::Proof>>}
pub(super) fn solve(
    functions:&BTreeMap<String,&Function>, closed:BTreeMap<String,Interface>,
    owns_input:impl Fn(&str,ValueIdentity)->bool, observe: Option<&StorageObserver<'_>>,
) -> Result<Solved> {
    let loops=used_loops(functions,&closed);
    let mut known = States {
        functions: closed.into_iter().map(|(name,interface)|(name,State{interface,..State::default()})).collect(),
        observe,
    };
    for(name,function)in functions{known.insert(name.clone(),State{
        name,
        loops:loops[name].iter().map(|input|(*input,Borrow::default())).collect(),
        entries:None,
        interface:Interface{outputs:function.outputs.keys().map(|output|(output.clone(),Borrow::default())).collect(),access:BTreeSet::new(),initialization:Vec::new()},
    });}
    converge(functions,&mut known,&BTreeSet::new());
    let unknown=functions.iter().flat_map(|(name,function)|function.outputs.keys().filter_map(|output|known[name].interface.outputs[output].empty().then_some((name.clone(),output.clone())))).collect::<BTreeSet<_>>();
    if !unknown.is_empty(){converge(functions,&mut known,&unknown);}
    converge_initializations(functions,&mut known);
    converge_access(functions,&mut known,&owns_input);
    let mut memory=BTreeMap::new();let mut scalar_values=BTreeMap::new();
    for(name,function)in functions {
        let state = &known[name];
        let mut evaluation = BorrowEvaluation {
            function, state, known: &known, expand_loops: true, deferred_reads: false,
            loop_bindings: Vec::new(), values: BTreeMap::new(),
        };
        let mut values:BTreeMap<address::MemoryTarget,address::MemoryEffect>=BTreeMap::new();
        let mut resolved=BTreeMap::new();
        loop {
        // Only scalar loop inputs feed back; ordinary Memory uses keep their original one-pass order.
        let previous=function.loops.values().any(|definition|definition.value.is_some()).then(||resolved.clone());values.clear();
        let mut calls=function.calls.iter().filter(|call|!call.deferred).peekable();let mut usages=function.uses.iter().enumerate().peekable();
        while let Some((memory,usage))=usages.next(){
            if let Requirement::Dataset{values:Some(read),..}=&usage.requirement {
                let mut proof=read[0].proof.substitute_values(&resolved).at_call(&read[0].bindings);
                for value in &read[1..] {
                    let value=value.proof.substitute_values(&resolved).at_call(&value.bindings);
                    bounds::Proof::join(&mut proof,&value,1,None);
                }
                resolved.insert(ValueIdentity::MemoryValue(memory,0),proof);
                continue;
            }
            let Requirement::Memory{target,snapshots,..}=&usage.requirement else{continue;};
            let snapshots = snapshots.as_ref().filter(|values| !values.is_empty());
            let mut effect=address::MemoryEffect::default();
            if calls.peek().is_some_and(|call|call.memory==memory){
                let call=calls.next().unwrap();
                let initialized = std::cell::LazyCell::new(|| initialization_effects(&call.initialization, function, state, &known).positions(&call.path));
                for access in &known[&call.function].interface.access {
                    let (borrows, when, access, written) = match access {
                        Access::Memory { borrows, when, access } | Access::Dataset { borrows, when, access, .. } =>
                            (borrows, when, *access, None),
                        Access::Position { borrows, when, position } => {
                            let access = match (&position.written, position.initialized) {
                                (Some(_), true) => super::Access::InOut,
                                (Some(_), false) => super::Access::Write,
                                (None, true) => super::Access::Read,
                                (None, false) => continue,
                            };
                            (borrows, when, access, position.written.as_ref())
                        }
                        _ => continue,
                    };
                    let borrows = borrow_at_call(borrows, call, &call.arguments);
                    let borrows = refreshed_at_call(evaluation.evaluate(&borrows), &call.path, &|| &*initialized);
                    let path = bounds::Clause::both(&call.path, &when.projected_case(true, &call.arguments))
                        .substitute_values(&resolved);
                    let borrow = borrows.substitute_values(&resolved);
                    let sources = borrow.sources.entries()
                        .map(|(source, when)| (source, bounds::Clause::both(&when, &path)));
                    let written = written.filter(|_| snapshots.is_some())
                        .map(|proof| proof.at_call(&call.arguments).substitute_values(&resolved));
                    effect.union(address::MemoryEffect::from_sources(sources, access, written.as_ref()));
                }
            }else{
                let rest=std::iter::from_fn(||{
                    if matches!(target,address::MemoryTarget::Call(_))&&usages.peek().is_some_and(|(_,usage)|matches!(&usage.requirement,Requirement::Memory{target:other,..}if other==target)){
                        usages.next().map(|(_,usage)|usage)
                    }else{None}
                });
                for usage in std::iter::once(usage).chain(rest){
                    if let Requirement::Memory{borrow,access:Some(access),written,..}=&usage.requirement {
                        let borrow = evaluation.evaluate(borrow).substitute_values(&resolved);
                        let path = usage.path.substitute_values(&resolved);
                        let sources = borrow.sources.entries()
                            .map(|(source, when)| (source, bounds::Clause::both(&when, &path)));
                        let written = written.as_ref().filter(|_| snapshots.is_some())
                            .map(|proof| proof.substitute_values(&resolved));
                        effect.union(address::MemoryEffect::from_sources(sources, *access, written.as_ref()));
                    }
                }
            }
            if let Some(snapshots)=snapshots {
                let path = usage.path.substitute_values(&resolved);
                let scope = bounds::proof_scope(&path, &BTreeMap::new());
                for(component,snapshot)in snapshots.iter().enumerate(){
                    let position=match &snapshot.position{Position::Borrowed(borrow,layout,word)=>Position::Borrowed(Arc::new(evaluation.evaluate(borrow).substitute_values(&resolved)),layout.clone(),*word),position=>position.clone()};
                    let index=snapshot.index.and_then(|index|match index{bounds::Operand::Value(id)if resolved.contains_key(&id)=>index.at_call(&resolved),index=>Some(index)});
                    let proof=snapshot.proof.substitute_values(&resolved);
                    let proof=effect.written_value(&position,snapshot.scalar,index,&scope(&Ty::scalar(snapshot.scalar), proof));
                    let identity=ValueIdentity::MemoryValue(memory,component);resolved.insert(identity,proof.with_identity(identity));
                }
            }
            values.entry(*target).or_default().union(effect);
        }
        for (input, definition) in &function.loops {
            let Some((id, _)) = definition.value else { continue; };
            let proof = solve_loop_value(function, *input, id, definition, &resolved);
            resolved.insert(id, proof);
        }
        if previous.is_none_or(|previous|resolved==previous){break;}
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
                let path=bounds::Clause::both(path,&context);
                value.proof=bounds::scoped_proof(&value.ty,value.proof.substitute_values(values),&path,&BTreeMap::new());
                value.constant=value.constant.or_else(||value.proof.known_value(&value.ty));
                Requirement::Nonzero{value:Box::new(value),path,span:*span}.check(&usage.source)?;
                continue;
            }
            if let Requirement::Initialized{read,code,span,..}=&usage.requirement {
                let read=resolved_read(read,&path,function,state,&known);
                Requirement::Initialized{borrow:read.borrow(),read,code,span:*span}.check(&usage.source)?;
                continue;
            }
            if let Requirement::Unborrowed { resource, borrow, .. } = &usage.requirement {
                let resource = evaluate(resource, function, state, &known, false);
                let borrow = evaluate(borrow, function, state, &known, false);
                if disjoint(&resource, &borrow, &function.independent, &usage.active_loops) {
                    continue;
                }
            }
            usage.requirement.mapped(|borrow|evaluate(borrow,function,state,&known,true)).under_path(&path).check(&usage.source)?;
        }
        let mut evaluation = BorrowEvaluation {
            function, state, known: &known, expand_loops: true, deferred_reads: false,
            loop_bindings: Vec::new(), values: BTreeMap::new(),
        };
        for call in &function.calls{
            let path=bounds::Clause::both(&call.path,&context);
            let live_task_places=evaluation.evaluate(&call.live_task_places);
            let initialized = std::cell::LazyCell::new(|| initialization_effects(&call.initialization, function, state, &known).positions(&path));
            for requirement in &known[&call.function].interface.access{
                if live_task_places.empty() && matches!(requirement,Access::Memory{..}|Access::Dataset{..}|Access::Stored{..}) {
                    continue;
                }
                let value=access_at_call(requirement,call,&path,&mut evaluation,&|| &*initialized);
                let(Access::Resource{when,..}|Access::Position{when,..}|Access::Memory{when,..}|Access::Dataset{when,..}|Access::Initialized{when,..}|Access::Stored{when,..})=&value;
                if when.possible(){
                    let path=when.clone();
                    if !live_task_places.empty() && let Some((accessed,_,mode))=value.accessed() {
                        Requirement::Unborrowed{
                            resource:accessed.restricted(&path),borrow:live_task_places.restricted(&path),
                            kind:view::Exclusion::Position(mode),span:call.span,
                        }.check(&call.source)?;
                    }
                    let requirement=match value {
                        Access::Position{borrows,position,..}=>Requirement::Address{borrow:borrows.restricted(&path),layout:position.layout,written:position.written,writable:position.writable,initialized:position.initialized,span:call.span},
                        Access::Resource{resources,borrows,kind,..}=>Requirement::Unborrowed{resource:resources.restricted(&path),borrow:borrows.restricted(&path),kind,span:call.span},
                        Access::Memory{..}|Access::Dataset{..}|Access::Stored{..}=>continue,
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
    let interfaces=known.functions.into_iter().map(|(name,state)|{
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
    pub(super) fn dataset_effect(&mut self,place:&Place,access:super::Access,atomic:bool,span:Span)->Result<Vec<(usize,usize)>> {
        let mut reads=Vec::new();
        let Place::ApplicationElement{sources,element_layout,index_value,..}=place.storage() else {return Ok(reads)};
        let borrow=self.data_borrow(sources);
        for (component,((scalar,_),position)) in place.ty().components.iter().zip(place.data_positions()).enumerate() {
            let mut pending=vec![(position,bounds::Clause::default())];
            while let Some((position,path))=pending.pop() {
                match position {
                    Position::Borrowed(_,_,word)=>{
                        let origin=address::Origin{
                            root:address::Root::Derived(borrow.clone()),layout:address::Layout::of(&Ty::scalar(*scalar)),
                            base:element_layout.clone(),words:Arc::from([word]),positions:Arc::from([]),
                            writable:access.writes(),initialized:Arc::from([access.reads()]),
                        };
                        let usage=self.view_model.uses.len();
                        self.check_view_requirement(Requirement::Dataset{borrow:Borrow::source(Origin::Position(Arc::new(origin))).restricted(&path),index:Some(*index_value),access,atomic,values:None,span})?;
                        if access==super::Access::Read&&!atomic {reads.push((component,usage));}
                    }
                    Position::Indexed(choices,index)=>pending.extend(choices.iter().enumerate().map(|(i,position)|(position.clone(),bounds::Clause::both(&path,&bounds::Clause::selection(index,i))))),
                    _=>{},
                }
            }
        }
        Ok(reads)
    }

    pub(super) fn native_dataset_access(&mut self,access:&BTreeSet<Access>,arguments:&BTreeMap<ValueIdentity,bounds::Proof>,span:Span)->Result<Vec<MemoryAccess>> {
        let mut memory=Vec::new();
        for access in access {
            if let Access::Initialized{borrows,initialized,when}=access {
                let path=bounds::Clause::both(&self.path, &when.projected_case(true, arguments));
                if path.possible(){
                    let borrows=borrows.at_call(arguments);
                    let initialized=initialized.iter().map(|fact|fact.at_call(arguments)).collect::<Vec<_>>();
                    let read=output_initialization_read(&borrows,&initialized,&path);
                    Requirement::Initialized{borrow:borrows,read,code:"B229",span}.check(self.scope.source())?;
                }
                continue;
            }
            let Access::Dataset{borrows,when,index,access,atomic}=access else {continue};
            let path=bounds::Clause::both(&self.path, &when.projected_case(true, arguments));
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
                                    self.memory_regions.entry(*binding).or_default().1.insert((*word,*word+scalar.size()),None);
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

pub(super) mod relation;
