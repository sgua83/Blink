//! B reads the one sealed syntax store and selects the instance's binding.
//! These adapters construct the existing semantic values in their native owner.
use super::*;
use crate::syntax::{self, wire};
use std::{ffi::c_void, mem::{self, MaybeUninit}, sync::Arc};

#[repr(C)]
struct BindingCursor {
    storage: wire::StorageView,
    offset: u64,
    arguments: wire::Slice,
    index: u64,
    mode: u32,
    checked: u32,
    stage: u32,
    shape: wire::Optional<u32>,
}
#[repr(u32)]
enum RequestKind { Complete, Type, Shape }
#[repr(C)]
struct BindingRequest { kind: RequestKind, reference: u64 }
#[repr(C)]
struct BindingHost {
    actual: unsafe extern "C" fn(*mut c_void,wire::Text)->*const c_void,
    value: unsafe extern "C" fn(*mut c_void,wire::Text)->wire::Optional<u32>,
    reserve: unsafe extern "C" fn(*mut c_void,u64),
    check: unsafe extern "C" fn(*mut c_void,wire::TypeRef)->u32,
    append_value: unsafe extern "C" fn(*mut c_void,u32),
    begin_contracts: unsafe extern "C" fn(*mut c_void,u64),
    append_predicate: unsafe extern "C" fn(*mut c_void,wire::PredicateRef,u64),
    construct: unsafe extern "C" fn(*mut c_void,wire::Text,u32,wire::Optional<u32>,u32),
    clone_actual: unsafe extern "C" fn(*mut c_void,*const c_void),
    wrap: unsafe extern "C" fn(*mut c_void,u32,u32,u32,wire::Optional<u32>),
}

unsafe extern "C" {
    #[link_name="bootstrap.source_bind::type_start"]
    fn type_start(source:wire::SignatureNode)->BindingCursor;
    #[link_name="bootstrap.source_bind::arguments_start"]
    fn arguments_start(storage:wire::StorageView,arguments:wire::Slice,checked:u32)->BindingCursor;
    #[link_name="bootstrap.source_bind::binding_step"]
    fn binding_step(state:*mut BindingCursor,context:*mut c_void,host:BindingHost)->BindingRequest;
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

enum TypeSource {
    Type(syntax::Type),
    Arguments(syntax::List<wire::ArgumentNode>),
}
impl TypeSource {
    fn view(&self)->syntax::SourceView<'_> {
        match self {Self::Type(source)=>source.source_view(),Self::Arguments(source)=>source.source_view()}
    }
}
struct TypeFrame<'a> {
    source:TypeSource,bindings:&'a Arc<Bindings>,world:&'a World,
    context:(&'a Source,&'a str,&'a [String]),span:Span,
    arguments:Vec<GenericArgument>,contracts:Vec<ContractPredicate>,
    result:MaybeUninit<TypeName>,error:Option<Diagnostic>,
}
struct BindingFrame<'a> {cursor:BindingCursor,data:TypeFrame<'a>}
impl<'a> BindingFrame<'a> {
    fn type_name(source:syntax::Type,bindings:&'a Arc<Bindings>,world:&'a World,
        context:(&'a Source,&'a str,&'a [String]),span:Span)->Self {
        let cursor=unsafe{type_start(source.signature_node())};
        Self{cursor,data:TypeFrame{source:TypeSource::Type(source),bindings,world,context,span,
            arguments:Vec::new(),contracts:Vec::new(),result:MaybeUninit::uninit(),error:None}}
    }
    fn arguments(source:syntax::List<wire::ArgumentNode>,bindings:&'a Arc<Bindings>,world:&'a World,
        context:(&'a Source,&'a str,&'a [String]),span:Span)->Self {
        let cursor=unsafe{arguments_start(source.source_view().storage(),source.source_slice(),1)};
        Self{cursor,data:TypeFrame{source:TypeSource::Arguments(source),bindings,world,context,span,
            arguments:Vec::new(),contracts:Vec::new(),result:MaybeUninit::uninit(),error:None}}
    }
}
unsafe fn type_frame<'a,'b>(raw:*mut c_void)->&'a mut TypeFrame<'b> {unsafe{&mut *raw.cast()}}
unsafe extern "C" fn type_value(raw:*mut c_void,name:wire::Text)->wire::Optional<u32> {
    let f=unsafe{type_frame(raw)};bound_value(f.source.view(),&f.bindings.values,name)
}
unsafe extern "C" fn actual_type(raw:*mut c_void,name:wire::Text)->*const c_void {
    let f=unsafe{type_frame(raw)};
    f.bindings.types.get(f.source.view().text(name)).map_or(std::ptr::null(),|ty|(ty as *const TypeName).cast())
}
unsafe extern "C" fn reserve_arguments(raw:*mut c_void,count:u64) {
    unsafe{type_frame(raw)}.arguments.reserve(count as usize);
}
unsafe extern "C" fn check_type(raw:*mut c_void,reference:wire::TypeRef)->u32 {
    let f=unsafe{type_frame(raw)};
    match f.bindings.check_type(&f.source.view().node(reference.offset),f.context.0,f.span) {
        Ok(())=>1,Err(error)=>{f.error=Some(error);0},
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
    f.contracts.push(predicate(f.source.view(),&f.bindings.values,reference,wire::Optional{present:1,value:parameters}));
}
unsafe extern "C" fn construct_type(raw:*mut c_void,name:wire::Text,canonical:u32,size:wire::Optional<u32>,contracts:u32) {
    let f=unsafe{type_frame(raw)};
    f.result.write(TypeName{name:f.source.view().text(name).into(),canonical:canonical!=0,
        arguments:mem::take(&mut f.arguments),dataset:false,view:false,tile:false,count:count(size),
        contracts:(contracts!=0).then(||Box::new(mem::take(&mut f.contracts)))});
}
unsafe extern "C" fn clone_actual(raw:*mut c_void,actual:*const c_void) {
    unsafe{type_frame(raw)}.result.write(unsafe{&*actual.cast::<TypeName>()}.clone());
}
unsafe extern "C" fn wrap_type(raw:*mut c_void,dataset:u32,view:u32,tile:u32,size:wire::Optional<u32>) {
    let result=unsafe{type_frame(raw).result.assume_init_mut()};
    result.dataset=dataset!=0;result.view=view!=0;result.tile=tile!=0;result.count=count(size);
}

// This is the original binding query's active state. Native stepping does not
// call World for a shape or recursively enter another native type binder.
pub(super) struct SourceBinding<'a> {frames:Vec<BindingFrame<'a>>}
pub(super) struct ShapeNeed<'a> {
    shape:syntax::Shape,bindings:&'a Arc<Bindings>,world:&'a World,
    context:(&'a Source,&'a str,&'a [String]),span:Span,
}
pub(super) enum BindingProgress<'a> {
    Complete(SourceBinding<'a>),
    Shape(SourceBinding<'a>,ShapeNeed<'a>),
}
impl ShapeNeed<'_> {
    fn evaluate(self)->Result<u32> {
        self.world.source_shape(&self.shape,self.bindings,self.context.0,self.span,self.context.1,self.context.2)
    }
}
impl<'a> SourceBinding<'a> {
    fn type_name(bindings:&'a Arc<Bindings>,world:&'a World,source:&syntax::Type,
        context:(&'a Source,&'a str,&'a [String]),span:Span)->Self {
        Self{frames:vec![BindingFrame::type_name(source.clone(),bindings,world,context,span)]}
    }
    fn arguments(bindings:&'a Arc<Bindings>,world:&'a World,source:&syntax::List<wire::ArgumentNode>,
        context:(&'a Source,&'a str,&'a [String]),span:Span)->Self {
        Self{frames:vec![BindingFrame::arguments(source.clone(),bindings,world,context,span)]}
    }
    pub(super) fn advance(mut self)->Result<BindingProgress<'a>> {
        loop {
            let frame=self.frames.last_mut().unwrap();
            let request=unsafe{binding_step(&mut frame.cursor,(&mut frame.data as *mut TypeFrame<'_>).cast(),BindingHost{
                actual:actual_type,value:type_value,reserve:reserve_arguments,check:check_type,append_value,
                begin_contracts,append_predicate,construct:construct_type,clone_actual,wrap:wrap_type,
            })};
            if let Some(error)=frame.data.error.take(){return Err(error);}
            match request.kind {
                RequestKind::Complete=>{
                    if self.frames.len()==1{return Ok(BindingProgress::Complete(self));}
                    let child=self.frames.pop().unwrap();
                    let value=unsafe{child.data.result.assume_init()};
                    self.frames.last_mut().unwrap().data.arguments.push(GenericArgument::Type(value));
                }
                RequestKind::Type=>{
                    let frame=&self.frames.last().unwrap().data;
                    let child=BindingFrame::type_name(frame.source.view().node(request.reference),
                        frame.bindings,frame.world,frame.context,frame.span);
                    self.frames.push(child);
                }
                RequestKind::Shape=>{
                    let frame=&self.frames.last().unwrap().data;
                    let need=ShapeNeed{shape:frame.source.view().node(request.reference),bindings:frame.bindings,
                        world:frame.world,context:frame.context,span:frame.span};
                    return Ok(BindingProgress::Shape(self,need));
                }
            }
        }
    }
    pub(super) fn complete_shape(mut self,value:u32)->Self {
        self.frames.last_mut().unwrap().cursor.shape=wire::Optional{present:1,value};self
    }
    fn finish_type(mut self)->TypeName {unsafe{self.frames.pop().unwrap().data.result.assume_init()}}
    fn finish_arguments(mut self)->Vec<GenericArgument> {self.frames.pop().unwrap().data.arguments}
}
fn drive<'a>(mut binding:SourceBinding<'a>)->Result<SourceBinding<'a>> {
    loop {
        match binding.advance()? {
            BindingProgress::Complete(done)=>return Ok(done),
            BindingProgress::Shape(pending,need)=>binding=pending.complete_shape(need.evaluate()?),
        }
    }
}
pub(super) fn type_name(bindings:&Arc<Bindings>,world:&World,source:&syntax::Type,
    context:(&Source,&str,&[String]),span:Span)->Result<TypeName> {
    drive(SourceBinding::type_name(bindings,world,source,context,span)).map(SourceBinding::finish_type)
}
pub(super) fn arguments(bindings:&Arc<Bindings>,world:&World,arguments:&syntax::List<wire::ArgumentNode>,
    context:(&Source,&str,&[String]),span:Span)->Result<Vec<GenericArgument>> {
    drive(SourceBinding::arguments(bindings,world,arguments,context,span)).map(SourceBinding::finish_arguments)
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
