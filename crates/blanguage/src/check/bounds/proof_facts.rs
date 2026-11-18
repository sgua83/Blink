//! B projects facts directly from original proof/value fields into the result.
use super::*;
use std::{collections::btree_set,ffi::c_void,ptr};
type Slot=*mut c_void;
type Borrowed=*const c_void;
#[repr(C)]
struct Input {
    scalar:fact_fields::ScalarView,scalar_ref:Borrowed,primary:Borrowed,
    origins:usize,ranges:usize,mode:u32,nonzero:u32,constant:u32,bits:u64,numeric_only:u32,
}
#[repr(C)]
struct Bounds {lower:u64,upper:u64}
#[repr(C)]
struct Domain {fields:index_domains::Fields,known_bits:u32}
struct Context<'a,'b> {
    proof:&'a Proof,value:Option<&'a Value>,component:usize,
    domains:btree_set::Iter<'a,IndexDomain>,facts:&'b mut Facts,
}
unsafe fn frame<'a,'b,'c>(slot:Slot)->&'a mut Context<'b,'c>{unsafe{&mut *slot.cast()}}
unsafe extern "C" fn origin(slot:Slot,index:usize)->Borrowed {
    let c=unsafe{frame(slot)};ptr::from_ref(&c.proof.origins[c.component][index]).cast()
}
unsafe extern "C" fn range(slot:Slot,index:usize)->Bounds {
    let c=unsafe{frame(slot)};let(lower,upper)=c.proof.ranges.get(c.component).map_or((0,u64::MAX),|v|v.0[index]);Bounds{lower,upper}
}
unsafe extern "C" fn rewind(slot:Slot) {
    let c=unsafe{frame(slot)};c.domains=c.proof.domains.get(c.component).map(|d|d.iter()).unwrap_or_default();
}
unsafe extern "C" fn next(slot:Slot)->Domain {
    unsafe{frame(slot)}.domains.next().map(|d|Domain{fields:index_domains::fields(d),known_bits:u32::from(matches!(d,IndexDomain::KnownBits(..)))}).unwrap_or(Domain{fields:index_domains::Fields::default(),known_bits:0})
}
unsafe extern "C" fn boolean_cases(slot:Slot)->u32 {
    let c=unsafe{frame(slot)};
    let produced;
    let condition=if let Some(value)=c.value {produced=Condition::component(value,c.component);Some(&produced)}else{c.proof.component_condition(c.component)};
    condition.map(|v|v.cases.iter().enumerate().fold(0,|mask,(truth,case)|mask|if matches!(&**case.0,ClauseNode::Impossible){1<<truth}else{0})).unwrap_or(0)
}
unsafe extern "C" fn emit(slot:Slot,fields:fact_fields::Fields) {
    unsafe{frame(slot)}.facts.insert(unsafe{fact_fields::key(fields)});
}
#[repr(C)]
struct Host {
    origin:unsafe extern "C" fn(Slot,usize)->Borrowed,
    range:unsafe extern "C" fn(Slot,usize)->Bounds,
    rewind:unsafe extern "C" fn(Slot),next:unsafe extern "C" fn(Slot)->Domain,
    boolean_cases:unsafe extern "C" fn(Slot)->u32,
    emit:unsafe extern "C" fn(Slot,fact_fields::Fields),
    compare:unsafe extern "C" fn(Borrowed,Borrowed)->i32,
}
unsafe extern "C" {
    #[link_name="bootstrap.condition_graph::proof_facts"]
    fn project(context:Slot,input:Input,host:Host);
}
fn append(facts:&mut Facts,proof:&Proof,value:Option<&Value>,component:usize,scalar:&Scalar,primary:Option<&ValueIdentity>,mode:u32,numeric_only:bool) {
    let scalar_ref=ptr::from_ref(scalar).cast();
    let constant=value.and_then(|v|v.constant.as_ref()).map(|v|v[component]);
    let input=Input{scalar:unsafe{fact_fields::scalar(scalar_ref)},scalar_ref,
        primary:primary.map(|id|ptr::from_ref(id).cast()).unwrap_or(ptr::null()),
        origins:proof.origins.get(component).map_or(0,Vec::len),ranges:proof.ranges.get(component).map_or(1,|r|r.0.len()),
        mode,nonzero:u32::from(proof.nonzero.get(component)==Some(&true)),constant:u32::from(constant.is_some()),bits:constant.unwrap_or(0),numeric_only:u32::from(numeric_only)};
    let mut context=Context{proof,value,component,domains:btree_set::Iter::default(),facts};
    unsafe{project((&mut context as *mut Context<'_,'_>).cast(),input,Host{origin,range,rewind,next,boolean_cases,emit,compare:fact_fields::compare})};
}
pub(super) fn collect(proof:&Proof,ty:&Ty)->Facts {
    let mut facts=Facts::new();
    append_proof(&mut facts,proof,ty);
    facts
}
pub(super) fn append_proof(facts:&mut Facts,proof:&Proof,ty:&Ty) {
    for (component,(scalar,_)) in ty.components.iter().enumerate(){append(facts,proof,None,component,scalar,None,0,false);}
}
pub(super) fn append_value(facts:&mut Facts,value:&Value,numeric_only:bool) {
    for component in 0..value.regs.len(){
        let fallback=ValueIdentity::Register(value.regs[component]);
        append(facts,&value.proof,Some(value),component,&value.ty.components[component].0,Some(&fallback),1,numeric_only);
    }
}
pub(super) fn value(value:&Value)->Facts {
    let mut facts=Facts::new();append_value(&mut facts,value,false);facts
}
pub(super) fn append_identity(facts:&mut Facts,proof:&Proof,scalar:Scalar,identity:ValueIdentity) {
    append(facts,proof,None,0,&scalar,Some(&identity),2,false);
}
pub(super) fn append_component(facts:&mut Facts,proof:&Proof,component:usize,scalar:Scalar) {
    append(facts,proof,None,component,&scalar,None,0,false);
}
