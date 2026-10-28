//! view借用与拥有资源沿原输入、循环和调用图交接；访问条件属于函数静态合同。
use super::*;
use view::{Borrow, LoopInput, Requirement, Source as Origin};
#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Output { Returned(usize), Written(usize,usize), ReturnedResource(usize), WrittenResource(usize,usize) }
#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct CallResult { pub call:usize, pub output:Output }
pub(super) type Outputs=BTreeMap<Output,Borrow>;
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Access {
    resources:Borrow,
    borrows:Borrow,
    when:bounds::Condition,
}
crate::module::codec::record!(Access {resources,borrows,when});
#[derive(Clone,Default,PartialEq,Eq)]
pub(super) struct Interface {pub outputs:Outputs,pub access:BTreeSet<Access>}
struct Call {
    function:String,
    arguments:BTreeMap<ValueIdentity,bounds::Proof>,
    outputs:BTreeSet<Output>,
    path:bounds::Clause,
    source:crate::Source,
    span:Span,
}
pub(super) struct Use { pub requirement:Requirement, pub source:crate::Source, pub path:bounds::Clause, pub active_loops:BTreeSet<usize> }
#[derive(Default)]
pub(super) struct Function {
    next_scope:usize,
    calls:Vec<Call>,
    pub loops:BTreeMap<LoopInput,Borrow>,
    pub independent:BTreeSet<LoopInput>,
    pub outputs:Outputs,
    pub resource_names:BTreeMap<Output,ValueIdentity>,
    pub parameters:BTreeMap<ValueIdentity,ValueIdentity>,
    pub input_width:usize,
    pub uses:Vec<Use>,
}
pub(super) fn components(ty:&Ty)->BTreeSet<usize> {
    ty.view_fields().into_iter().map(|(offset,_)|ty.components.iter().position(|(_,at)|*at==offset).unwrap()).collect()
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
    pub fn scope(&mut self)->usize {let scope=self.next_scope;self.next_scope+=1;scope}
    pub fn call(&mut self,function:&str,arguments:BTreeMap<ValueIdentity,bounds::Proof>,path:bounds::Clause,source:crate::Source,span:Span)->usize {
        let call=self.calls.len();self.calls.push(Call{function:function.into(),arguments,outputs:BTreeSet::new(),path,source,span});call
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
    fn input(&self,id:ValueIdentity)->bool {match id{ValueIdentity::Register(n)=>n<self.input_width,ValueIdentity::ParameterContent(..)|ValueIdentity::ResourceParameter(..)=>true,_=>false}}
    fn access(&self,resources:Borrow,borrows:Borrow,when:bounds::Condition)->Option<Access>{
        let parameters=residency::input_parameters(&self.parameters,self.input_width);
        let external=|borrow:Borrow|borrow.map_sources(|source|match source{
            Origin::Data(id)=>self.parameters.get(&id).copied().filter(|id|self.input(*id)).map(|id|Borrow::source(Origin::Data(id))).unwrap_or_default(),
            Origin::Parameter(id)=>Borrow::source(self.parameters.get(&id).copied().filter(|id|self.input(*id)).map(Origin::Parameter).unwrap_or(Origin::Unresolved)),
            Origin::Unresolved=>Borrow::source(source),Origin::Inactive=>Borrow::default(),
        }).guards_at_call(&parameters);
        let resources=external(resources);let borrows=external(borrows);
        if !resources.active()||!borrows.active(){return None;}
        let when=when.guard_at_call(&parameters);
        let when=bounds::Condition::guard(when.assume(&bounds::Clause::default(),true).canonical());
        when.active().then_some(Access{resources,borrows,when})
    }
}
#[derive(Clone,Default,PartialEq,Eq)]
struct State { loops:BTreeMap<LoopInput,Borrow>, interface:Interface }
type States=BTreeMap<String,State>;
fn evaluate(borrow:&Borrow,function:&Function,state:&State,known:&States,expand_loops:bool)->Borrow {
    let mut value=Borrow{sources:borrow.sources.clone(),..Borrow::default()};
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
    borrow.map_sources(|source|match source {
        Origin::Data(id)=>{
            let input=function.parameters.get(&id).copied().filter(|id|function.input(*id));
            if let Some(input)=input{Borrow::source(Origin::Data(input))}
            else if let Some(resource)=resource{Borrow::source(Origin::Data(resource))}
            else if let Some(outputs)=exposed.get(&id){outputs.clone()}
            else{Borrow::source(Origin::Unresolved)}
        }
        Origin::Parameter(id)=>Borrow::source(function.parameters.get(&id).copied().map(Origin::Parameter).unwrap_or(Origin::Unresolved)),
        Origin::Unresolved|Origin::Inactive=>Borrow::source(source),
    }).guards_at_call(&parameters)
}
fn access_at_call(access:&Access,call:&Call,function:&Function,state:&State,known:&States)->(Borrow,Borrow,bounds::Condition){
    let resources=access.resources.at_call(&call.arguments);
    let borrows=access.borrows.at_call(&call.arguments);
    let when=access.when.guard_at_call(&call.arguments).assume(&call.path,true);
    (evaluate(&resources,function,state,known,true),evaluate(&borrows,function,state,known,true),bounds::Condition::guard(when))
}
fn converge(functions:&BTreeMap<String,&Function>,known:&mut States,unknown:&BTreeSet<(String,Output)>){
    loop{
        let mut next=known.clone();
        for(name,function)in functions{
            let state=&known[name];let result=next.get_mut(name).unwrap();
            for(input,definition)in &function.loops{result.loops.insert(*input,evaluate(definition,function,state,known,true));}
            let physical=function.outputs.iter().map(|(output,definition)|(*output,evaluate(definition,function,state,known,true))).collect::<Outputs>();
            // 每个真实资源输出命名自己的交付存储；view保留所有可能的实际输出所有者。
            let mut exposed:BTreeMap<ValueIdentity,Borrow>=BTreeMap::new();
            for(output,name)in &function.resource_names{for(source,when)in physical[output].sources.entries(){if let Origin::Data(id)=source{exposed.entry(*id).or_default().union(&Borrow::source(Origin::Data(*name)).restricted(&when));}}}
            for(output,definition)in &physical{
                let mut value=canonical(definition.clone(),function,function.resource_names.get(output).copied(),&exposed);
                if unknown.contains(&(name.clone(),*output)){value.sources.insert(Origin::Unresolved);}
                result.interface.outputs.insert(*output,value);
            }
            let mut access=BTreeSet::new();
            for usage in &function.uses{if let Requirement::Unborrowed{resource,borrow,..}=&usage.requirement{
                let resource=evaluate(resource,function,state,known,true);let borrow=evaluate(borrow,function,state,known,true);
                if let Some(value)=function.access(resource,borrow,bounds::Condition::guard(usage.path.clone())){access.insert(value);}
            }}
            for call in &function.calls{for requirement in &known[&call.function].interface.access{
                let(resource,borrow,when)=access_at_call(requirement,call,function,state,known);
                if let Some(value)=function.access(resource,borrow,when){access.insert(value);}
            }}
            result.interface.access=access;
        }
        if *known==next{break;}*known=next;
    }
}
pub(super) fn solve(functions:&BTreeMap<String,&Function>,closed:BTreeMap<String,Interface>)->Result<BTreeMap<String,Interface>>{
    let mut known=closed.into_iter().map(|(name,interface)|(name,State{interface,..State::default()})).collect::<States>();
    for(name,function)in functions{known.insert(name.clone(),State{
        loops:function.loops.keys().map(|input|(*input,Borrow::default())).collect(),
        interface:Interface{outputs:function.outputs.keys().map(|output|(*output,Borrow::default())).collect(),access:BTreeSet::new()},
    });}
    converge(functions,&mut known,&BTreeSet::new());
    let unknown=functions.iter().flat_map(|(name,function)|function.outputs.keys().filter_map(|output|known[name].interface.outputs[output].empty().then_some((name.clone(),*output)))).collect::<BTreeSet<_>>();
    if !unknown.is_empty(){converge(functions,&mut known,&unknown);}
    for(name,function)in functions{
        let state=&known[name];
        for usage in &function.uses{
            let symbolic=usage.requirement.mapped(|borrow|evaluate(borrow,function,state,&known,false));
            if matches!(&symbolic,Requirement::Unborrowed{resource,borrow,..}if disjoint(resource,borrow,&function.independent,&usage.active_loops)){continue;}
            usage.requirement.mapped(|borrow|evaluate(borrow,function,state,&known,true)).under_path(&usage.path).check(&usage.source)?;
        }
        for call in &function.calls{
            for requirement in &known[&call.function].interface.access{
                let(resource,borrow,when)=access_at_call(requirement,call,function,state,&known);
                if when.active(){let path=when.assume(&bounds::Clause::default(),true);Requirement::Unborrowed{resource:resource.restricted(&path),borrow:borrow.restricted(&path),span:call.span}.check(&call.source)?;}
            }
            for output in &call.outputs{
                let borrow=known[&call.function].interface.outputs.get(output);
                if matches!(output,Output::Returned(_)|Output::Written(..))&&borrow.is_none_or(|borrow|borrow.empty()||borrow.sources.contains(&Origin::Unresolved)){
                    return call.source.fail(call.span,"B271","view返回或输出没有可闭合的实际借用来源");
                }
            }
        }
    }
    Ok(known.into_iter().map(|(name,state)|(name,state.interface)).collect())
}
