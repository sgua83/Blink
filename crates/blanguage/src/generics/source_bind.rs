//! B reads the one sealed syntax store and selects the instance's binding.
//! These adapters construct the existing semantic values in their native owner.
use super::*;
use crate::syntax::{self, wire};
use std::{ffi::c_void, mem::{self, MaybeUninit}, sync::Arc};

unsafe extern "C" {
    #[link_name="bootstrap.source_bind::arguments"]
    fn arguments_b(storage:wire::StorageView, arguments:wire::Slice, checked:u32, context:*mut c_void,
        value:unsafe extern "C" fn(*mut c_void,wire::Text)->wire::Optional<u32>,
        shape:unsafe extern "C" fn(*mut c_void,wire::ShapeRef)->wire::Optional<u32>,
        reserve:unsafe extern "C" fn(*mut c_void,u64),
        check:unsafe extern "C" fn(*mut c_void,wire::TypeRef)->bool,
        append_type:unsafe extern "C" fn(*mut c_void,wire::TypeRef)->bool,
        append_value:unsafe extern "C" fn(*mut c_void,u32))->bool;
    #[link_name="bootstrap.source_bind::type_name"]
    fn type_b(source:wire::SignatureNode,context:*mut c_void,
        actual:unsafe extern "C" fn(*mut c_void,wire::Text)->*const c_void,
        value:unsafe extern "C" fn(*mut c_void,wire::Text)->wire::Optional<u32>,
        shape:unsafe extern "C" fn(*mut c_void,wire::ShapeRef)->wire::Optional<u32>,
        reserve:unsafe extern "C" fn(*mut c_void,u64),
        check:unsafe extern "C" fn(*mut c_void,wire::TypeRef)->bool,
        append_type:unsafe extern "C" fn(*mut c_void,wire::TypeRef)->bool,
        append_value:unsafe extern "C" fn(*mut c_void,u32),
        begin_contracts:unsafe extern "C" fn(*mut c_void,u64),
        append_predicate:unsafe extern "C" fn(*mut c_void,wire::PredicateRef,u64),
        construct:unsafe extern "C" fn(*mut c_void,wire::Text,u32,wire::Optional<u32>,u32),
        clone_actual:unsafe extern "C" fn(*mut c_void,*const c_void),
        wrap:unsafe extern "C" fn(*mut c_void,u32,u32,u32,wire::Optional<u32>));
    #[link_name="bootstrap.source_bind::condition"]
    fn condition_b(source:wire::SignatureNode,parameters:wire::Optional<u64>,output:u32,context:*mut c_void,
        value:unsafe extern "C" fn(*mut c_void,wire::Text)->wire::Optional<u32>,
        construct:unsafe extern "C" fn(*mut c_void,wire::ConditionNode));
    #[link_name="bootstrap.source_bind::predicate"]
    fn predicate_b(source:wire::SignatureNode,parameters:wire::Optional<u64>,context:*mut c_void,
        reserve:unsafe extern "C" fn(*mut c_void,u64),
        append_interval:unsafe extern "C" fn(*mut c_void,wire::ConditionRef,wire::ConditionRef),
        construct:unsafe extern "C" fn(*mut c_void,wire::PredicateNode,u32));
    #[link_name="bootstrap.source_bind::runtime_contract"]
    fn runtime_contract_b(source:wire::SignatureNode,context:*mut c_void,
        value:unsafe extern "C" fn(*mut c_void,wire::Text)->wire::Optional<u32>)->bool;
}

fn bound_value(source:syntax::SourceView<'_>,values:&BTreeMap<String,u32>,name:wire::Text)->wire::Optional<u32> {
    match values.get(source.text(name)) {
        Some(&value)=>wire::Optional{present:1,value},
        None=>wire::Optional{present:0,value:0},
    }
}
fn count(value:wire::Optional<u32>)->Option<Shape> {
    (value.present!=0).then_some(Shape::Literal(value.value))
}

struct TypeFrame<'a> {
    source:syntax::SourceView<'a>,bindings:&'a Arc<Bindings>,world:&'a World,
    context:(&'a Source,&'a str,&'a [String]),span:Span,
    arguments:Vec<GenericArgument>,contracts:Vec<ContractPredicate>,
    result:MaybeUninit<TypeName>,error:Option<Diagnostic>,
}
impl<'a> TypeFrame<'a> {
    fn new(source:syntax::SourceView<'a>,bindings:&'a Arc<Bindings>,world:&'a World,
        context:(&'a Source,&'a str,&'a [String]),span:Span)->Self {
        Self{source,bindings,world,context,span,arguments:Vec::new(),contracts:Vec::new(),result:MaybeUninit::uninit(),error:None}
    }
}
unsafe fn type_frame<'a>(raw:*mut c_void)->&'a mut TypeFrame<'a> {unsafe{&mut *raw.cast()}}
unsafe extern "C" fn type_value(raw:*mut c_void,name:wire::Text)->wire::Optional<u32> {
    let f=unsafe{type_frame(raw)};bound_value(f.source,&f.bindings.values,name)
}
unsafe extern "C" fn actual_type(raw:*mut c_void,name:wire::Text)->*const c_void {
    let f=unsafe{type_frame(raw)};
    f.bindings.types.get(f.source.text(name)).map_or(std::ptr::null(),|ty|(ty as *const TypeName).cast())
}
unsafe extern "C" fn source_shape(raw:*mut c_void,shape:wire::ShapeRef)->wire::Optional<u32> {
    let f=unsafe{type_frame(raw)};let (source,module,imports)=f.context;
    match f.world.source_shape(&f.source.node(shape.offset),f.bindings,source,f.span,module,imports) {
        Ok(value)=>wire::Optional{present:1,value},
        Err(error)=>{f.error=Some(error);wire::Optional{present:0,value:0}},
    }
}
unsafe extern "C" fn reserve_arguments(raw:*mut c_void,count:u64) {
    unsafe{type_frame(raw)}.arguments.reserve(count as usize);
}
unsafe extern "C" fn check_type(raw:*mut c_void,reference:wire::TypeRef)->bool {
    let f=unsafe{type_frame(raw)};
    match f.bindings.check_type(&f.source.node(reference.offset),f.context.0,f.span) {
        Ok(())=>true,Err(error)=>{f.error=Some(error);false},
    }
}
unsafe extern "C" fn append_type(raw:*mut c_void,reference:wire::TypeRef)->bool {
    let f=unsafe{type_frame(raw)};
    match type_name(f.bindings,f.world,&f.source.node(reference.offset),f.context,f.span) {
        Ok(ty)=>{f.arguments.push(GenericArgument::Type(ty));true},
        Err(error)=>{f.error=Some(error);false},
    }
}
unsafe extern "C" fn append_value(raw:*mut c_void,value:u32) {
    unsafe{type_frame(raw)}.arguments.push(GenericArgument::Value(Shape::Literal(value)));
}
unsafe extern "C" fn begin_contracts(raw:*mut c_void,count:u64) {
    unsafe{type_frame(raw)}.contracts.reserve(count as usize);
}
unsafe extern "C" fn append_predicate(raw:*mut c_void,reference:wire::PredicateRef,parameters:u64) {
    let f=unsafe{type_frame(raw)};
    f.contracts.push(predicate(f.source,&f.bindings.values,reference,wire::Optional{present:1,value:parameters}));
}
unsafe extern "C" fn construct_type(raw:*mut c_void,name:wire::Text,canonical:u32,size:wire::Optional<u32>,contracts:u32) {
    let f=unsafe{type_frame(raw)};
    f.result.write(TypeName{name:f.source.text(name).into(),canonical:canonical!=0,
        arguments:mem::take(&mut f.arguments),dataset:false,view:false,tile:false,count:count(size),
        contracts:(contracts!=0).then(||Box::new(mem::take(&mut f.contracts)))});
}
unsafe extern "C" fn clone_actual(raw:*mut c_void,actual:*const c_void) {
    unsafe{type_frame(raw)}.result.write(unsafe{&*actual.cast::<TypeName>()}.clone());
}
unsafe extern "C" fn wrap_type(raw:*mut c_void,dataset:u32,view:u32,tile:u32,size:wire::Optional<u32>) {
    // B has constructed this exact result before requesting its source wrapper.
    let result=unsafe{type_frame(raw).result.assume_init_mut()};
    result.dataset=dataset!=0;result.view=view!=0;result.tile=tile!=0;result.count=count(size);
}
pub(super) fn type_name(bindings:&Arc<Bindings>,world:&World,source:&syntax::Type,
    context:(&Source,&str,&[String]),span:Span)->Result<TypeName> {
    let mut frame=TypeFrame::new(source.source_view(),bindings,world,context,span);
    unsafe{type_b(source.signature_node(),(&mut frame as *mut TypeFrame<'_>).cast(),
        actual_type,type_value,source_shape,reserve_arguments,check_type,append_type,append_value,
        begin_contracts,append_predicate,construct_type,clone_actual,wrap_type)};
    match frame.error {Some(error)=>Err(error),None=>Ok(unsafe{frame.result.assume_init()})}
}
pub(super) fn arguments(bindings:&Arc<Bindings>,world:&World,arguments:&syntax::List<wire::ArgumentNode>,
    context:(&Source,&str,&[String]),span:Span)->Result<Vec<GenericArgument>> {
    let mut frame=TypeFrame::new(arguments.source_view(),bindings,world,context,span);
    unsafe{arguments_b(frame.source.storage(),arguments.source_slice(),1,(&mut frame as *mut TypeFrame<'_>).cast(),
        type_value,source_shape,reserve_arguments,check_type,append_type,append_value)};
    match frame.error {Some(error)=>Err(error),None=>Ok(frame.arguments)}
}

struct ConditionFrame<'a> {
    source:syntax::SourceView<'a>,values:&'a BTreeMap<String,u32>,
    parameters:wire::Optional<u64>,output:u32,result:MaybeUninit<OutputCondition>,
}
unsafe extern "C" fn condition_value(raw:*mut c_void,name:wire::Text)->wire::Optional<u32> {
    let f=unsafe{&*raw.cast::<ConditionFrame<'_>>()};bound_value(f.source,f.values,name)
}
unsafe extern "C" fn construct_condition(raw:*mut c_void,node:wire::ConditionNode) {
    let f=unsafe{&mut *raw.cast::<ConditionFrame<'_>>()};
    // The descriptor is selected by B. Native construction lends each actual
    // child destination back to the same B binder; no Rust source walk remains.
    let child=|reference:wire::ConditionRef|condition(f.source,f.values,reference,f.parameters,f.output);
    let text=|value|f.source.text(value).to_owned();
    let value=match node {
        wire::ConditionNode::Name(v)=>OutputCondition::Name(text(v)),
        wire::ConditionNode::Number(v)=>OutputCondition::Number(text(v)),
        wire::ConditionNode::Bool(v)=>OutputCondition::Bool(v!=0),
        wire::ConditionNode::StaticU32(v)=>OutputCondition::StaticU32(v),
        wire::ConditionNode::Member(v)=>OutputCondition::Member(Box::new(child(v.base)),text(v.field)),
        wire::ConditionNode::Unary(v)=>OutputCondition::Unary(text(v.operation),Box::new(child(v.value))),
        wire::ConditionNode::Binary(v)=>OutputCondition::Binary(text(v.operation),Box::new(child(v.left)),Box::new(child(v.right))),
    };
    f.result.write(value);
}
fn condition(source:syntax::SourceView<'_>,values:&BTreeMap<String,u32>,reference:wire::ConditionRef,
    parameters:wire::Optional<u64>,output:u32)->OutputCondition {
    let mut frame=ConditionFrame{source,values,parameters,output,result:MaybeUninit::uninit()};
    unsafe{condition_b(wire::SignatureNode{storage:source.storage(),offset:reference.offset},parameters,output,
        (&mut frame as *mut ConditionFrame<'_>).cast(),condition_value,construct_condition);
        frame.result.assume_init()}
}
struct PredicateFrame<'a> {
    source:syntax::SourceView<'a>,values:&'a BTreeMap<String,u32>,parameters:wire::Optional<u64>,
    intervals:Vec<(OutputCondition,OutputCondition)>,result:MaybeUninit<ContractPredicate>,
}
unsafe extern "C" fn reserve_intervals(raw:*mut c_void,count:u64) {
    unsafe{&mut *raw.cast::<PredicateFrame<'_>>()}.intervals.reserve(count as usize);
}
unsafe extern "C" fn append_interval(raw:*mut c_void,lower:wire::ConditionRef,upper:wire::ConditionRef) {
    let f=unsafe{&mut *raw.cast::<PredicateFrame<'_>>()};
    f.intervals.push((condition(f.source,f.values,lower,f.parameters,0),condition(f.source,f.values,upper,f.parameters,0)));
}
unsafe extern "C" fn construct_predicate(raw:*mut c_void,node:wire::PredicateNode,output:u32) {
    let f=unsafe{&mut *raw.cast::<PredicateFrame<'_>>()};
    let text=|value|f.source.text(value).to_owned();
    let child=|reference|condition(f.source,f.values,reference,f.parameters,output);
    let value=match node {
        wire::PredicateNode::Prefix(v)=>ContractPredicate::Prefix{data:text(v.data),count:text(v.count),field:text(v.field)},
        wire::PredicateNode::Nonzero(v)=>ContractPredicate::Nonzero(text(v)),
        wire::PredicateNode::Nonempty(v)=>ContractPredicate::Nonempty(text(v)),
        wire::PredicateNode::Contiguous(v)=>ContractPredicate::Contiguous(text(v)),
        wire::PredicateNode::Memory(v)=>ContractPredicate::Memory((v.present!=0).then_some(v.value)),
        wire::PredicateNode::Index(v)=>ContractPredicate::Index{value:text(v.value),resource:text(v.resource)},
        wire::PredicateNode::Range(v)=>ContractPredicate::Range{value:text(v.value),intervals:mem::take(&mut f.intervals)},
        wire::PredicateNode::SameLength(v)=>ContractPredicate::SameLength{left:text(v.left),right:text(v.right)},
        wire::PredicateNode::Initialized(v)=>ContractPredicate::Initialized{value:text(v.value),condition:child(v.condition)},
        wire::PredicateNode::Cleanup(v)=>ContractPredicate::Cleanup{value:text(v.value),condition:child(v.condition)},
    };
    f.result.write(value);
}
fn predicate(source:syntax::SourceView<'_>,values:&BTreeMap<String,u32>,reference:wire::PredicateRef,
    parameters:wire::Optional<u64>)->ContractPredicate {
    let mut frame=PredicateFrame{source,values,parameters,intervals:Vec::new(),result:MaybeUninit::uninit()};
    unsafe{predicate_b(wire::SignatureNode{storage:source.storage(),offset:reference.offset},parameters,
        (&mut frame as *mut PredicateFrame<'_>).cast(),reserve_intervals,append_interval,construct_predicate);
        frame.result.assume_init()}
}
struct ContractFrame<'a> {source:syntax::SourceView<'a>,values:&'a BTreeMap<String,u32>}
unsafe extern "C" fn contract_value(raw:*mut c_void,name:wire::Text)->wire::Optional<u32> {
    let f=unsafe{&*raw.cast::<ContractFrame<'_>>()};bound_value(f.source,f.values,name)
}
pub(super) fn contract(contract:syntax::Contract,values:&BTreeMap<String,u32>)->Option<FunctionContract> {
    let source=contract.source_view();let mut frame=ContractFrame{source,values};
    if !unsafe{runtime_contract_b(contract.signature_node(),(&mut frame as *mut ContractFrame<'_>).cast(),contract_value)} {return None;}
    Some(FunctionContract{kind:contract.kind(),
        predicate:predicate(source,values,wire::PredicateRef{offset:contract.predicate().signature_node().offset},wire::Optional{present:0,value:0}),
        span:contract.span()})
}
