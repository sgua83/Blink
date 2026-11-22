//! B owns model rebinding and contract translation; these callers own the actual typed values.
use super::*;
use std::{ffi::c_void,ptr};
#[derive(Clone,Copy,PartialEq,Eq)]
#[repr(u32)]enum Kind{Unit,Value,Owned,Data}
fn kind(value:&Returned)->Kind{match value{Returned::Unit=>Kind::Unit,Returned::Value(_)=>Kind::Value,Returned::OwnedValue(_)=>Kind::Owned,Returned::Dataset(_)=>Kind::Data}}
#[derive(Clone,Copy)]#[repr(C)]struct Cursor{phase:u32,index:u64}
#[repr(C)]struct Step<T>{cursor:Cursor,need:T}
#[repr(u32)]enum RebindNeed{Value,Owned,OwnedPlace,Root,Length,DataPlace,Inspect,Aggregate,Variant,Next,Complete}
#[derive(Clone,Copy)]#[repr(C)]struct RebindFacts{kind:Kind,variant:u32,present:u32,payloads:u64}
#[repr(u32)]enum ValueNeed{Proof,Constant,Known,Uniform,Complete}
#[repr(u32)]enum TranslateNeed{Scalar,Root,Length,RootProof,LengthProof,Residency,Prefixes,Count,Elements,Inspect,Payload,Clear,Next,Complete}
#[derive(Clone,Copy)]#[repr(C)]struct TranslateFacts{kind:Kind,model:Kind,present:u32,model_present:u32,payloads:u64,model_payloads:u64}
#[repr(C)]struct Component{word:u32,size:u32}
#[repr(C)]struct ProjectionHost{
 count:unsafe extern "C" fn(*const c_void)->u64,
 component:unsafe extern "C" fn(*const c_void,u64)->Component,
 reg:unsafe extern "C" fn(*const c_void,u64)->u64,
 append:unsafe extern "C" fn(*mut c_void,u64),
}
unsafe extern "C"{
 #[link_name="bootstrap.native_model::start"]fn start()->Cursor;
 #[link_name="bootstrap.native_model::rebind_step"]fn rebind_step(cursor:Cursor,facts:RebindFacts)->Step<RebindNeed>;
 #[link_name="bootstrap.native_model::value_step"]fn value_step(cursor:Cursor,constant:u32)->Step<ValueNeed>;
 #[link_name="bootstrap.native_model::translate_step"]fn translate_step(cursor:Cursor,facts:TranslateFacts)->Step<TranslateNeed>;
 #[link_name="bootstrap.native_model::project"]fn project(parent:*const c_void,child:*const c_void,value:*const c_void,offset:u32,output:*mut c_void,host:ProjectionHost);
}
unsafe extern "C" fn component_count(ty:*const c_void)->u64{unsafe{&*ty.cast::<Ty>()}.components.len()as u64}
unsafe extern "C" fn component(ty:*const c_void,index:u64)->Component{let(scalar,word)=unsafe{&*ty.cast::<Ty>()}.components[index as usize];Component{word,size:scalar.size()}}
unsafe extern "C" fn register(value:*const c_void,index:u64)->u64{unsafe{&*value.cast::<Value>()}.regs[index as usize]as u64}
unsafe extern "C" fn append_register(output:*mut c_void,reg:u64){unsafe{&mut *output.cast::<Vec<Reg>>()}.push(reg as usize);}
fn owned(value:&Returned)->&owned_values::OwnedValuePayload{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};value}
fn owned_mut(value:&mut Returned)->&mut owned_values::OwnedValuePayload{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};value}
fn data(value:&Returned)->&application::DataPayload{let Returned::Dataset(value)=value else{unsafe{std::hint::unreachable_unchecked()}};value}
fn data_mut(value:&mut Returned)->&mut application::DataPayload{let Returned::Dataset(value)=value else{unsafe{std::hint::unreachable_unchecked()}};value}
fn scalar(value:&Returned)->&Value{match value{Returned::Value(value)=>value,Returned::OwnedValue(value)=>&value.value,_=>unsafe{std::hint::unreachable_unchecked()}}}
fn scalar_mut(value:&mut Returned)->&mut Value{match value{Returned::Value(value)=>value,Returned::OwnedValue(value)=>&mut value.value,_=>unsafe{std::hint::unreachable_unchecked()}}}
// Interface numbers do not allocate real IR registers or emit conversion instructions.
pub(in crate::check) fn rebind_model(value:&mut Returned,registers:Vec<Reg>){
 let mut cursor=unsafe{start()};let mut facts=RebindFacts{kind:kind(value),variant:0,present:0,payloads:match value{Returned::OwnedValue(value)=>value.payloads.len()as u64,_=>0}};
 let mut registers=Some(registers);
 loop{let step=unsafe{rebind_step(cursor,facts)};cursor=step.cursor;match step.need{
  RebindNeed::Value=>scalar_mut(value).regs=unsafe{registers.take().unwrap_unchecked()},
  RebindNeed::Owned=>owned_mut(value).value.regs=unsafe{registers.take().unwrap_unchecked()},
  RebindNeed::OwnedPlace=>owned_mut(value).place=None,
  RebindNeed::Root=>data_mut(value).root.regs=vec![unsafe{registers.as_ref().unwrap_unchecked()}[0]],
  RebindNeed::Length=>data_mut(value).length.regs=vec![unsafe{registers.as_ref().unwrap_unchecked()}[1]],
  RebindNeed::DataPlace=>{let data=data_mut(value);data.place=None;data.initialization=None;}
  RebindNeed::Inspect=>{let value=owned(value);facts.present=u32::from(value.payloads[cursor.index as usize].is_some());facts.variant=u32::from(value.value.ty.has_variants());}
  RebindNeed::Aggregate|RebindNeed::Variant=>{
   let owner=owned(value);let index=cursor.index as usize;
   let(offset,ty)=match step.need{RebindNeed::Variant=>(owner.value.ty.variant_payload_offset(),owner.value.ty.variants[index].ty.as_ref()),_=>owner.value.ty.aggregate_part(index)};
   let mut registers=Vec::<Reg>::new();
   unsafe{project(ptr::from_ref(&owner.value.ty).cast(),ptr::from_ref(ty).cast(),ptr::from_ref(&owner.value).cast(),offset,ptr::from_mut(&mut registers).cast(),ProjectionHost{count:component_count,component,reg:register,append:append_register})};
   rebind_model(unsafe{owned_mut(value).payloads[index].as_mut().unwrap_unchecked()},registers);
  }
  RebindNeed::Next=>{},
  RebindNeed::Complete=>return,
 }}
}
fn translate_value(value:&mut Value,model:Option<&Value>,bindings:&BTreeMap<ValueIdentity,bounds::Proof>){
 let mut cursor=unsafe{start()};
 loop{let step=unsafe{value_step(cursor,u32::from(value.constant.is_some()))};cursor=step.cursor;match step.need{
  // Unknown contract sources stay unknown; model slots are not actual return values.
  ValueNeed::Proof=>{let proof=match model{Some(model)=>model.proof.clone(),None=>std::mem::take(&mut value.proof)};value.proof=proof.at_call(bindings);}
  ValueNeed::Constant=>{if let Some(model)=model{value.constant=model.constant.clone();}}
  ValueNeed::Known=>value.constant=value.proof.known_value(&value.ty),
  ValueNeed::Uniform=>value.uniform=false,
  ValueNeed::Complete=>return,
 }}
}
type ResidencyBinding = fn(&residency::Residency, &BTreeMap<ValueIdentity, bounds::Proof>) -> residency::Residency;

fn translate_model(
    value: &mut Returned,
    model: Option<&Returned>,
    bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
    stored: bool,
    bind_residency: ResidencyBinding,
) {
 let k=kind(value);let mut cursor=unsafe{start()};
 let mut facts=TranslateFacts{kind:k,model:model.map(kind).unwrap_or(k),present:0,model_present:0,payloads:match value{Returned::OwnedValue(value)=>value.payloads.len()as u64,_=>0},model_payloads:0};
 facts.model_payloads=model.map(|model|match model{Returned::OwnedValue(value)=>value.payloads.len()as u64,_=>0}).unwrap_or(facts.payloads);
 loop{let step=unsafe{translate_step(cursor,facts)};cursor=step.cursor;match step.need{
  TranslateNeed::Scalar=>translate_value(scalar_mut(value),model.map(scalar),bindings),
  TranslateNeed::Root=>translate_value(&mut data_mut(value).root,model.map(|model|&data(model).root),bindings),
  TranslateNeed::Length=>translate_value(&mut data_mut(value).length,model.map(|model|&data(model).length),bindings),
  TranslateNeed::RootProof=>{let value=data_mut(value);value.root.proof=bounds::Proof::capture(&value.root);}
  TranslateNeed::LengthProof=>{let value=data_mut(value);value.length.proof=bounds::Proof::capture(&value.length).with_domains(Domains::from([IndexDomain::ApplicationLength(value.root.identity())]));}
  TranslateNeed::Residency=>{
   let value=data_mut(value);
   value.residency=bind_residency(&model.map(data).unwrap_or(value).residency,bindings);
  }
  TranslateNeed::Prefixes=>{
   let value=data_mut(value);
   value.prefixes=match model{Some(model)=>data(model).prefixes.iter().map(|(word,fact)|(*word,fact.at_call(bindings))).collect(),None=>std::mem::take(&mut value.prefixes).into_iter().map(|(word,fact)|(word,fact.at_call(bindings))).collect()};
  }
  TranslateNeed::Count=>{if let Some(model)=model{data_mut(value).count=data(model).count;}}
  TranslateNeed::Elements=>{
   let value=data_mut(value);let elements=match model{Some(model)=>data(model).elements.clone(),None=>std::mem::take(&mut value.elements)};
   value.elements=elements.map(&mut|mut model|{
    let element_bindings=application::element_scope_bindings(bindings,model.registers().len(),stored);
    translate_model(&mut model,None,&element_bindings,true,bind_residency);model
   },&mut|when|when.at_call(bindings));
  }
  TranslateNeed::Inspect=>{
   let index=cursor.index as usize;facts.present=u32::from(owned(value).payloads[index].is_some());
   facts.model_present=model.map(|model|u32::from(owned(model).payloads[index].is_some())).unwrap_or(facts.present);
  }
  TranslateNeed::Payload=>{
   let index=cursor.index as usize;let source=model.map(|model|unsafe{owned(model).payloads[index].as_ref().unwrap_unchecked().as_ref()});
   translate_model(unsafe{owned_mut(value).payloads[index].as_mut().unwrap_unchecked()},source,bindings,stored,bind_residency);
  }
  TranslateNeed::Clear=>owned_mut(value).payloads[cursor.index as usize]=None,
  TranslateNeed::Next=>{},
  TranslateNeed::Complete=>return,
 }}
}
pub(in crate::check) fn translate_returned(
    value: &mut Returned,
    model: &Returned,
    bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
    stored: bool,
) {
    // A local element rebind still owns the original call snapshots.
    let bind_residency: ResidencyBinding = if stored {
        residency::Residency::element_indices
    } else {
        residency::Residency::at_call
    };
    translate_model(value, Some(model), bindings, stored, bind_residency);
}
pub(in crate::check) fn translate_in_place(
    value: &mut Returned,
    bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
    stored: bool,
) {
    translate_model(value, None, bindings, stored, residency::Residency::at_call);
}
