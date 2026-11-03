//! view借用与拥有资源沿原输入、循环和调用图交接；访问条件属于函数静态合同。
use super::*;
use view::{Borrow, LoopInput, Requirement, Source as Origin};
#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Output { Returned(usize), Written(usize,usize), ReturnedResource(usize), WrittenResource(usize,usize) }
#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct CallResult { pub call:usize, pub output:Output }
pub(super) type Outputs=BTreeMap<Output,Borrow>;
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct PositionAccess {pub layout:address::Layout,pub writes:bool,pub initialized:bool}
crate::module::codec::record!(PositionAccess {layout,writes,initialized});
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Access {
    Resource {resources:Borrow,borrows:Borrow,when:bounds::Condition},
    Position {borrows:Borrow,when:bounds::Condition,position:PositionAccess},
    Memory {borrows:Borrow,when:bounds::Condition},
}
crate::module::codec::variants!(Access {0=>Resource{resources,borrows,when},1=>Position{borrows,when,position},2=>Memory{borrows,when}});
#[derive(Clone,Default,PartialEq,Eq)]
pub(super) struct Interface {pub outputs:Outputs,pub access:BTreeSet<Access>,pub initialization:Vec<address::Initialization>}
struct Call {
    function:String,
    memory:usize,
    arguments:BTreeMap<ValueIdentity,bounds::Proof>,
    initialization:Vec<address::InitializationEffect>,
    outputs:BTreeSet<Output>,
    path:bounds::Clause,
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
    fn bindings(&self,producer:&flow::LoopState)->BTreeMap<ValueIdentity,bounds::Proof> {
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
    calls:Vec<Call>,
    pub loops:BTreeMap<LoopInput,LoopDefinition>,
    pub independent:BTreeSet<LoopInput>,
    pub outputs:Outputs,
    pub initialization:Vec<address::InitializationEffect>,
    pub normal_returns:Option<bounds::Condition>,
    pub resource_names:BTreeMap<Output,ValueIdentity>,
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
pub(super) fn set_resource_borrows(value:&mut returns::Returned,mut borrow:impl FnMut(usize)->Borrow){
    let count=resource_borrows(value).len();for index in 0..count{if let Some(data)=resource_value_mut(value,index){data.handle.proof=data.handle.proof.clone().with_view_borrow(0,borrow(index));}}
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
    pub fn initialization_when(&self,when:bounds::Condition)->bounds::Condition {
        let normal=match &self.normal_returns {Some(normal)=>normal.clone(),None=>bounds::Condition::constant(false)};
        normal.negate().logical(when,true)
    }
    pub fn scope(&mut self)->usize {let scope=self.next_scope;self.next_scope+=1;scope}
    pub fn call(&mut self,function:&str,memory:usize,arguments:BTreeMap<ValueIdentity,bounds::Proof>,initialization:Vec<address::InitializationEffect>,path:bounds::Clause,source:crate::Source,span:Span)->usize {
        let call=self.calls.len();self.calls.push(Call{function:function.into(),memory,arguments,initialization,outputs:BTreeSet::new(),path,source,span});call
    }
    pub fn result(&mut self,call:usize,output:Output)->Borrow {
        self.calls[call].outputs.insert(output);
        Borrow{calls:view::Edges::from([CallResult{call,output}]),..Borrow::default()}
    }
    pub fn capture_resources(&mut self,value:&returns::Returned,output:impl Fn(usize)->Output){
        for(index,borrow)in resource_borrows(value).into_iter().enumerate(){self.outputs.insert(output(index),borrow);}
    }
    pub fn capture(&mut self,proof:&bounds::Proof,ty:&Ty,output:impl Fn(usize)->Output) {
        for component in components(ty) {self.outputs.insert(output(component),proof.view_borrow(component));}
    }
    fn input(&self,id:ValueIdentity)->bool {match id{ValueIdentity::Register(n)=>n<self.input_width,ValueIdentity::ParameterContent(..)|ValueIdentity::ResourceParameter(..)|ValueIdentity::ParameterStorage(..)|ValueIdentity::DatasetContent(..)=>true,_=>false}}
    fn address_access(&self,borrow:Borrow,position:Option<PositionAccess>,when:bounds::Condition)->Option<Access>{
        let selected=borrow.map_sources(|source|match source {
            Origin::Parameter(id)=>self.parameters.get(&id).copied().filter(|id|self.input(*id))
                .map(|id|Borrow::source(Origin::Parameter(id))).unwrap_or_else(||Borrow::source(Origin::Unresolved)),
            Origin::Position(origin) if matches!(origin.root,address::Root::Parameter(_)|address::Root::Derived(_)) =>
                canonical(Borrow::source(Origin::Position(origin)),self,None,&BTreeMap::new()),
            Origin::Unresolved|Origin::External=>Borrow::source(source),
            _=>Borrow::default(),
        });
        if !selected.active(){return None;}
        let bindings=residency::input_parameters(&self.parameters,self.input_width);
        let when=when.guard_at_call(&bindings);
        let when=bounds::Condition::guard(when.assume(&bounds::Clause::default(),true).canonical());
        let borrows=selected.guards_at_call(&bindings);
        when.active().then_some(match position{Some(position)=>Access::Position{borrows,when,position},None=>Access::Memory{borrows,when}})
    }
    fn access(&self,resources:Borrow,borrows:Borrow,when:bounds::Condition)->Option<Access>{
        let parameters=residency::input_parameters(&self.parameters,self.input_width);
        let external=|borrow:Borrow|borrow.map_sources(|source|match source{
            Origin::Data(id)=>self.parameters.get(&id).copied().filter(|id|self.input(*id)).map(|id|Borrow::source(Origin::Data(id))).unwrap_or_default(),
            Origin::Parameter(id)=>Borrow::source(self.parameters.get(&id).copied().filter(|id|self.input(*id)).map(Origin::Parameter).unwrap_or(Origin::Unresolved)),
            Origin::Position(origin)=>origin.resources(),
            Origin::Unresolved=>Borrow::source(source),Origin::Inactive|Origin::External=>Borrow::default(),
        }).guards_at_call(&parameters);
        let resources=external(resources.resources());let borrows=external(borrows.resources());
        if !resources.active()||!borrows.active(){return None;}
        let when=when.guard_at_call(&parameters);
        let when=bounds::Condition::guard(when.assume(&bounds::Clause::default(),true).canonical());
        when.active().then_some(Access::Resource{resources,borrows,when})
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
    contracts.extend(functions.iter().filter_map(|(name,function)|function.uses.iter().any(|usage|matches!(usage.requirement,Requirement::Unborrowed{..}|Requirement::Address{..}|Requirement::Initialized{..})).then_some(name.clone())));
    loop {
        let needed=functions.iter().filter_map(|(name,function)|function.calls.iter().any(|call|contracts.contains(&call.function)).then_some(name.clone())).collect::<BTreeSet<_>>();
        let before=contracts.len();contracts.extend(needed);if before==contracts.len(){break;}
    }
    functions.iter().map(|(name,function)|{
        let mut roots=function.outputs.values().cloned().collect::<Vec<_>>();
        for effect in &function.initialization{initialization_roots(effect,&mut roots);}
        for usage in &function.uses {
            roots.push(usage.requirement.borrow().clone());
            match &usage.requirement {
                Requirement::Unborrowed{resource,..}=>roots.push(resource.clone()),
                Requirement::Return{owned,..}=>roots.push(owned.clone()),
                Requirement::Initialized{read,..}=>{
                    for position in &read.positions{position_roots(position,&mut roots);}
                    for position in &read.initialized{position_roots(position,&mut roots);}
                }
                Requirement::Address{..}|Requirement::Memory{..}=>{},
            }
        }
        for call in &function.calls{if contracts.contains(&call.function){roots.extend(call.arguments.values().map(bounds::Proof::borrowed_view));}}
        let mut loops=BTreeSet::new();let mut calls=BTreeSet::new();
        while let Some(borrow)=roots.pop() {
            for source in &borrow.sources {
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
fn canonical(borrow:Borrow,function:&Function,resource:Option<ValueIdentity>,exposed:&BTreeMap<ValueIdentity,Borrow>)->Borrow {
    let parameters=function.parameters.iter().map(|(a,b)|(*a,bounds::Proof::default().with_identity(*b))).collect();
    let renamed=|source|match source {
        Origin::Data(id)=>{
            let input=function.parameters.get(&id).copied().filter(|id|function.input(*id));
            if let Some(input)=input{Borrow::source(Origin::Data(input))}
            else if let Some(resource)=resource{Borrow::source(Origin::Data(resource))}
            else if let Some(outputs)=exposed.get(&id){outputs.clone()}
            else{Borrow::source(Origin::Unresolved)}
        }
        Origin::Parameter(id)=>Borrow::source(function.parameters.get(&id).copied().map(Origin::Parameter).unwrap_or(Origin::Unresolved)),
        Origin::Position(origin)=>{
            let mut value=origin.map_borrows(|borrow|canonical(borrow.clone(),function,None,exposed));
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

fn refreshed_at_call(borrow:&Borrow,call:&Call,function:&Function,state:&State,known:&States)->Borrow {
    let initialized=initialization_effects(&call.initialization,function,state,known).positions(&call.path);
    evaluate(borrow,function,state,known,true).map_sources(|source|match source {
        Origin::Position(origin)=>{
            let fact=address::Initialization{origin:origin.as_ref().clone(),when:bounds::Condition::constant(true)};
            let positions=fact.positions(&call.path);
            let mut origin=origin.as_ref().clone();
            origin.initialized=origin.initialized.iter().enumerate().map(|(i,ready)|*ready||positions.get(i).is_some_and(|position|position.initialized_by(&initialized))).collect();
            Borrow::source(Origin::Position(Arc::new(origin)))
        }
        source=>Borrow::source(source),
    })
}

fn access_at_call(access:&Access,call:&Call,function:&Function,state:&State,known:&States)->Access{
    let(Access::Resource{borrows,when,..}|Access::Position{borrows,when,..}|Access::Memory{borrows,when})=access;
    let borrows=borrows.at_call(&call.arguments);
    let when=when.guard_at_call(&call.arguments).assume(&call.path,true);
    let borrows=refreshed_at_call(&borrows,call,function,state,known);
    let when=bounds::Condition::guard(when);
    match access {
        Access::Resource{resources,..}=>Access::Resource{resources:evaluate(&resources.at_call(&call.arguments),function,state,known,true),borrows,when},
        Access::Position{position,..}=>{
            let mut position=position.clone();
            if position.initialized&&borrows.sources.iter().all(|source|matches!(source,Origin::Position(origin)if origin.initialized.iter().all(|ready|*ready))) {position.initialized=false;}
            Access::Position{borrows,when,position}
        },
        Access::Memory{..}=>Access::Memory{borrows,when},
    }
}


fn normalized_condition(when:bounds::Condition)->bounds::Condition {
    bounds::Condition::paths(when.assume(&bounds::Clause::default(),true).canonical(),when.assume(&bounds::Clause::default(),false).canonical())
}
fn exposed_resources(function:&Function,state:&State,known:&States)->BTreeMap<ValueIdentity,Borrow> {
    let mut exposed:BTreeMap<ValueIdentity,Borrow>=BTreeMap::new();
    for(output,name)in &function.resource_names {
        let physical=evaluate(&function.outputs[output],function,state,known,true);
        for(source,when)in physical.sources.entries(){if let Origin::Data(id)=source{exposed.entry(*id).or_default().union(&Borrow::source(Origin::Data(*name)).restricted(&when));}}
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
                if let Requirement::Initialized{read,..}=&usage.requirement {
                    let read=resolved_read(read,&usage.path,function,state,known);
                    for(source,when)in read.borrow().sources.entries() {
                        if let Origin::Position(origin)=source {
                            if let Some(value)=function.address_access(Borrow::source(source.clone()),Some(PositionAccess{layout:origin.layout.clone(),writes:false,initialized:true}),
                                bounds::Condition::guard(usage.path.clone()).logical(bounds::Condition::guard(when),false)){access.insert(value);}
                        }
                    }
                    continue;
                }
                let value=match &usage.requirement {
                    Requirement::Unborrowed{resource,borrow,..}=>function.access(
                        evaluate(resource,function,state,known,true),evaluate(borrow,function,state,known,true),
                        bounds::Condition::guard(usage.path.clone())),
                    Requirement::Address{borrow,layout,writes,initialized,..}=>function.address_access(
                        evaluate(borrow,function,state,known,true),
                        Some(PositionAccess{layout:layout.clone(),writes:*writes,initialized:*initialized}),
                        bounds::Condition::guard(usage.path.clone())),
                    Requirement::Memory{borrow,..}=>function.address_access(evaluate(borrow,function,state,known,true),None,bounds::Condition::guard(usage.path.clone())),
                    _=>None,
                };
                if let Some(value)=value{access.insert(value);}
            }
            for call in &function.calls {for requirement in &known[&call.function].interface.access {
                let value=access_at_call(requirement,call,function,state,known);
                let value=match value {
                    Access::Position{position,borrows,when}=>{
                        if !position.writes&&!position.initialized{continue;}
                        function.address_access(borrows,Some(position),when)
                    },
                    Access::Memory{borrows,when}=>function.address_access(borrows,None,when),
                    Access::Resource{resources,borrows,when}=>function.access(resources,borrows,when),
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
            let physical=function.outputs.iter().map(|(output,definition)|(*output,evaluate(definition,function,state,known,true))).collect::<Outputs>();
            // 每个真实资源输出命名自己的交付存储；view保留所有可能的实际输出所有者。
            let mut exposed:BTreeMap<ValueIdentity,Borrow>=BTreeMap::new();
            for(output,name)in &function.resource_names{for(source,when)in physical[output].sources.entries(){if let Origin::Data(id)=source{exposed.entry(*id).or_default().union(&Borrow::source(Origin::Data(*name)).restricted(&when));}}}
            for(output,definition)in &physical{
                let mut value=canonical(definition.clone(),function,function.resource_names.get(output).copied(),&exposed);
                if unknown.contains(&(name.clone(),*output)){value.sources.insert(Origin::Unresolved);}
                result.interface.outputs.insert(*output,value);
            }

        }
        if *known==next{break;}*known=next;
    }
}
pub(super) struct Solved {pub interfaces:BTreeMap<String,Interface>,pub memory:BTreeMap<String,BTreeMap<address::MemoryTarget,address::MemoryEffect>>}
pub(super) fn solve(functions:&BTreeMap<String,&Function>,closed:BTreeMap<String,Interface>)->Result<Solved>{
    let loops=used_loops(functions,&closed);
    let mut known=closed.into_iter().map(|(name,interface)|(name,State{interface,..State::default()})).collect::<States>();
    for(name,function)in functions{known.insert(name.clone(),State{
        loops:loops[name].iter().map(|input|(*input,Borrow::default())).collect(),
        interface:Interface{outputs:function.outputs.keys().map(|output|(*output,Borrow::default())).collect(),access:BTreeSet::new(),initialization:Vec::new()},
    });}
    converge(functions,&mut known,&BTreeSet::new());
    let unknown=functions.iter().flat_map(|(name,function)|function.outputs.keys().filter_map(|output|known[name].interface.outputs[output].empty().then_some((name.clone(),*output)))).collect::<BTreeSet<_>>();
    if !unknown.is_empty(){converge(functions,&mut known,&unknown);}
    converge_initializations(functions,&mut known);
    converge_access(functions,&mut known);
    for(name,function)in functions{
        let state=&known[name];
        for usage in &function.uses{
            if let Requirement::Initialized{read,code,span,..}=&usage.requirement {
                let read=resolved_read(read,&usage.path,function,state,&known);
                Requirement::Initialized{borrow:read.borrow(),read,code,span:*span}.check(&usage.source)?;
                continue;
            }
            let symbolic=usage.requirement.mapped(|borrow|evaluate(borrow,function,state,&known,false));
            if matches!(&symbolic,Requirement::Unborrowed{resource,borrow,..}if disjoint(resource,borrow,&function.independent,&usage.active_loops)){continue;}
            usage.requirement.mapped(|borrow|evaluate(borrow,function,state,&known,true)).under_path(&usage.path).check(&usage.source)?;
        }
        for call in &function.calls{
            for requirement in &known[&call.function].interface.access{
                let value=access_at_call(requirement,call,function,state,&known);
                let(Access::Resource{when,..}|Access::Position{when,..}|Access::Memory{when,..})=&value;
                if when.active(){
                    let path=when.assume(&bounds::Clause::default(),true);
                    let requirement=match value {
                        Access::Position{borrows,position,..}=>Requirement::Address{borrow:borrows.restricted(&path),layout:position.layout,writes:position.writes,initialized:position.initialized,span:call.span},
                        Access::Resource{resources,borrows,..}=>Requirement::Unborrowed{resource:resources.restricted(&path),borrow:borrows.restricted(&path),span:call.span},
                        Access::Memory{..}=>continue,
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
    let mut memory=BTreeMap::new();
    for(name,function)in functions {
        let mut values:BTreeMap<address::MemoryTarget,address::MemoryEffect>=BTreeMap::new();
        for usage in &function.uses {
            if let Requirement::Memory{target,borrow,..}=&usage.requirement {
                let borrow=evaluate(borrow,function,&known[name],&known,true).restricted(&usage.path);
                let effect=address::MemoryEffect::of(&borrow);
                match values.entry(*target) {
                    std::collections::btree_map::Entry::Vacant(entry)=>{entry.insert(effect);}
                    std::collections::btree_map::Entry::Occupied(mut entry)=>entry.get_mut().union(effect),
                }
            }
        }
        for call in &function.calls {
            for access in &known[&call.function].interface.access {
                if let Access::Memory{borrows,when}=access_at_call(access,call,function,&known[name],&known) {
                    let borrow=borrows.restricted(&when.assume(&bounds::Clause::default(),true));
                    values.entry(address::MemoryTarget::Call(call.memory)).or_insert_with(||address::MemoryEffect::Known(Vec::new())).union(address::MemoryEffect::of(&borrow));
                }
            }
        }
        memory.insert(name.clone(),values);
    }
    Ok(Solved{interfaces:known.into_iter().map(|(name,state)|(name,state.interface)).collect(),memory})
}
