//! CPU view来源沿原函数调用与局部回边闭合；编译关系不进入公共合同。
use super::*;
use view::{Borrow,LoopInput,Requirement,Source as Origin};

#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Output { Returned(usize), Written(usize,usize) }
#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct CallResult { pub call:usize, pub output:Output }
pub(super) type Outputs=BTreeMap<Output,Borrow>;
struct Call {
    function:String,
    arguments:BTreeMap<ValueIdentity,Borrow>,
    outputs:BTreeSet<Output>,
    source:crate::Source,
    span:Span,
}
pub(super) struct Use { pub requirement:Requirement, pub source:crate::Source }
#[derive(Default)]
pub(super) struct Function {
    next_scope:usize,
    calls:Vec<Call>,
    pub loops:BTreeMap<LoopInput,Borrow>,
    pub independent:BTreeSet<LoopInput>,
    pub outputs:Outputs,
    pub parameters:BTreeMap<ValueIdentity,ValueIdentity>,
    pub uses:Vec<Use>,
}
pub(super) fn components(ty:&Ty)->BTreeSet<usize> {
    ty.view_fields().into_iter().map(|(offset,_)|ty.components.iter().position(|(_,at)|*at==offset).unwrap()).collect()
}
pub(super) fn pending(proof:&bounds::Proof,ty:&Ty)->bool {
    components(ty).into_iter().any(|component|{let borrow=proof.view_borrow(component);borrow.empty()||borrow.pending()||borrow.sources.contains(&Origin::Unresolved)})
}
fn disjoint(a:&Borrow,b:&Borrow,independent:&BTreeSet<LoopInput>)->bool {
    a.sources.is_empty()&&b.sources.is_empty()&&a.calls.is_empty()&&b.calls.is_empty()&&!a.inputs.is_empty()&&!b.inputs.is_empty()
        &&a.inputs.iter().all(|a|b.inputs.iter().all(|b|a.level==b.level&&a.index!=b.index&&independent.contains(a)&&independent.contains(b)))
}
impl Function {
    pub fn scope(&mut self)->usize {let scope=self.next_scope;self.next_scope+=1;scope}
    pub fn call(&mut self,function:&str,arguments:BTreeMap<ValueIdentity,bounds::Proof>,source:crate::Source,span:Span)->usize {
        let arguments=arguments.into_iter().map(|(identity,proof)| {
            let mut borrow=proof.view_borrow(0);
            if borrow.empty() {borrow.sources.insert(proof.known_identity(0).map(Origin::Data).unwrap_or(Origin::Unresolved));}
            (identity,borrow)
        }).collect();
        let call=self.calls.len();self.calls.push(Call{function:function.into(),arguments,outputs:BTreeSet::new(),source,span});call
    }
    pub fn result(&mut self,call:usize,output:Output)->Borrow {
        self.calls[call].outputs.insert(output);
        Borrow{calls:BTreeSet::from([CallResult{call,output}]),..Borrow::default()}
    }
    pub fn capture(&mut self,proof:&bounds::Proof,ty:&Ty,output:impl Fn(usize)->Output) {
        for component in components(ty) {self.outputs.insert(output(component),proof.view_borrow(component));}
    }
}
#[derive(Clone,Default,PartialEq,Eq)]
struct State { loops:BTreeMap<LoopInput,Borrow>, outputs:Outputs }
type States=BTreeMap<String,State>;
fn evaluate(borrow:&Borrow,function:&Function,state:&State,known:&States,expand_loops:bool)->Borrow {
    let mut value=Borrow::from_sources(borrow.sources.clone());
    for input in &borrow.inputs {if expand_loops{value.union(&state.loops[input]);}else{value.inputs.insert(*input);}}
    for result in &borrow.calls {
        let call=&function.calls[result.call];
        let Some(output)=known[&call.function].outputs.get(&result.output)else {
            value.sources.insert(Origin::Unresolved);continue;
        };
        for source in &output.sources {
            match source {
                Origin::Data(identity)|Origin::Parameter(identity)=>{
                    if let Some(argument)=call.arguments.get(identity) {value.union(&evaluate(argument,function,state,known,expand_loops));}
                    else {value.sources.insert(Origin::Unresolved);}
                }
                Origin::Unresolved=>{value.sources.insert(Origin::Unresolved);}
            }
        }
    }
    value
}
fn canonical(mut borrow:Borrow,parameters:&BTreeMap<ValueIdentity,ValueIdentity>)->Borrow {
    borrow.sources=borrow.sources.into_iter().map(|source|match source {
        Origin::Data(id)=>parameters.get(&id).copied().map(Origin::Data).unwrap_or(Origin::Unresolved),
        Origin::Parameter(id)=>parameters.get(&id).copied().map(Origin::Parameter).unwrap_or(Origin::Unresolved),
        Origin::Unresolved=>source,
    }).collect();borrow
}
fn converge(functions:&BTreeMap<String,&Function>,known:&mut States,unknown:&BTreeSet<(String,Output)>) {
    loop {
        let mut next=known.clone();
        for (name,function)in functions {
            let state=&known[name];let result=next.get_mut(name).unwrap();
            for (input,definition)in &function.loops {result.loops.insert(*input,evaluate(definition,function,state,known,true));}
            for (output,definition)in &function.outputs {
                let mut value=canonical(evaluate(definition,function,state,known,true),&function.parameters);
                if unknown.contains(&(name.clone(),*output)){value.sources.insert(Origin::Unresolved);}
                result.outputs.insert(*output,value);
            }
        }
        if *known==next {break;}*known=next;
    }
}
pub(super) fn solve(functions:&BTreeMap<String,&Function>,closed:BTreeMap<String,Outputs>)->Result<BTreeMap<String,Outputs>> {
    let mut known=closed.into_iter().map(|(name,outputs)|(name,State{outputs,..State::default()})).collect::<States>();
    for (name,function)in functions {
        known.insert(name.clone(),State{
            loops:function.loops.keys().map(|input|(*input,Borrow::default())).collect(),
            outputs:function.outputs.keys().map(|output|(*output,Borrow::default())).collect(),
        });
    }
    converge(functions,&mut known,&BTreeSet::new());
    // 无真实输入／构造来源的纯递归环仍是未知，不能交付成无借用。
    let unknown=functions.iter().flat_map(|(name,function)|function.outputs.keys().filter_map(|output| {
        known[name].outputs[output].empty().then_some((name.clone(),*output))
    })).collect::<BTreeSet<_>>();
    if !unknown.is_empty(){converge(functions,&mut known,&unknown);}
    for (name,function)in functions {
        let state=&known[name];
        for usage in &function.uses {
            let symbolic=usage.requirement.mapped(|borrow|evaluate(borrow,function,state,&known,false));
            if matches!(&symbolic,Requirement::Unborrowed{resource,borrow,..}if disjoint(resource,borrow,&function.independent)){continue;}
            usage.requirement.mapped(|borrow|evaluate(borrow,function,state,&known,true)).check(&usage.source)?;
        }
        for call in &function.calls {
            for output in &call.outputs {
                let borrow=known[&call.function].outputs.get(output);
                if borrow.is_none_or(|borrow|borrow.empty()||borrow.sources.contains(&Origin::Unresolved)) {
                    return call.source.fail(call.span,"B271","view返回或输出没有可闭合的实际借用来源");
                }
            }
        }
    }
    Ok(known.into_iter().map(|(name,state)|(name,state.outputs)).collect())
}
