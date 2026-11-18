//! B selects the original release sequence; typed storage and IR stay with Builder.
use super::*;
#[derive(Clone,Copy,Default)]
#[repr(u32)] enum Kind{#[default]Other,Owned,Data}
#[derive(Clone,Copy,Default)]
#[repr(u32)] enum State{#[default]Inactive,Owned,Conditional}
#[derive(Clone,Copy)]
#[repr(C)] struct Cursor{phase:u32,index:u64}
#[repr(C)] struct Step<T>{cursor:Cursor,need:T}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct Facts{kind:Kind,ownership:State,presence:u32,placed:u32,task:u32,cleanup:u32,variants:u32,field_owns:u32,fields:u64}
#[repr(u32)]enum Entry{Present,ErrorPosition}
#[repr(u32)]enum Need{Conditional,Frame,Data,Inspect,Load,Task,User,InspectField,Unpack,Forget,Borrow,Recurse,ClearPlace,Variant,Complete,Next}
#[derive(Clone,Copy)]
#[repr(C)]struct VariantFacts{count:u64,known:u32}
#[repr(u32)]enum VariantNeed{Condition,Outer,Unpack,Drop,Yes,Rest,Emit,Complete,Next}
#[repr(u32)]enum UserNeed{Payload,MovableData,MovableTree,Argument,Instance,Invoke,Complete,Next}
unsafe extern "C"{
 #[link_name="bootstrap.ownership_release::start"]fn start()->Cursor;
 #[link_name="bootstrap.ownership_release::entry"]fn entry(may_own:u32,pending:u32,stored_words:u32)->Entry;
 #[link_name="bootstrap.ownership_release::step"]fn step(state:Cursor,facts:Facts)->Step<Need>;
 #[link_name="bootstrap.ownership_release::variant_step"]fn variant_step(state:Cursor,facts:VariantFacts)->Step<VariantNeed>;
 #[link_name="bootstrap.ownership_release::user_step"]fn user_step(state:Cursor,payload:Kind)->Step<UserNeed>;
 #[link_name="bootstrap.ownership_release::resource_step"]fn resource_step(state:Cursor,count:u64)->Step<u32>;
}
fn kind(value:&Returned)->Kind{match value{Returned::OwnedValue(_)=>Kind::Owned,Returned::Dataset(_)=>Kind::Data,_=>Kind::Other}}
fn ownership(value:&Ownership)->State{match value{Ownership::Owned=>State::Owned,Ownership::Conditional(_)=>State::Conditional,_=>State::Inactive}}
fn owner(value:&Returned)->&OwnedValue{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};value}
fn owner_mut(value:&mut Returned)->&mut OwnedValue{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};value}
impl Builder<'_>{
 pub(in crate::check) async fn drop_owned_tree(&mut self,value:Returned,retained:&view::Borrow,span:Span)->Result<()>{
  unsafe{self.world.call(self.drop_owned_tree_with(value,retained,span,Self::application_drop)).await}
 }
 pub(in crate::check) async fn drop_owned_tree_with(
  &mut self,value:Returned,retained:&view::Borrow,span:Span,
  drop_dataset:impl std::ops::AsyncFn(&mut Self,&application::Data,&view::Borrow,Span)->Result<()>+Copy,
 )->Result<()>{
  let (may_own,place)=match &value{
   Returned::Dataset(data)=>(data.ownership.may_own(),data.place.clone()),
   Returned::OwnedValue(value)=>(value.ownership.may_own(),value.place()),
   _=>(false,None),
  };
  match unsafe{entry(u32::from(may_own),u32::from(self.pending_error.is_some()),u32::from(place.as_ref().is_some_and(|place|!place.ty().components.is_empty())))}{
   Entry::Present=>unsafe{self.world.call(self.drop_present_owned_tree(value,retained,span,drop_dataset)).await},
   Entry::ErrorPosition=>{
    let error=unsafe{self.pending_error.unwrap_unchecked()};let place=unsafe{place.unwrap_unchecked()};
    let status=self.reg(Scalar::U64);self.emit(InstKind::LoadLocal(status,error.slot()));
    let arguments=self.error_position_arguments(&place,status,span)?;let consumed=self.reg(Scalar::Bool);
    self.emit(application::runtime_call("error_consumed",arguments,Some(Ty::scalar(Scalar::Bool)),vec![consumed],None,None).kind);
    let no_error=bounds::Condition::guard(bounds::Clause::zero_results([status]));
    let consumed_proof=bounds::Proof::condition(no_error.negate().logical(bounds::Condition::default(),false));
    let retained_owner=owned_presence::Presence::input(&Value{ty:Ty::scalar(Scalar::Bool),regs:vec![consumed],proof:consumed_proof,uniform:false,literal:false,constant:None}).not();
    unsafe{self.world.call(self.owned_when(&retained_owner,span,None,None,async |builder|unsafe{builder.world.call(builder.drop_present_owned_tree(value,retained,span,drop_dataset)).await})).await}
   }
  }
 }
 async fn drop_present_owned_tree(
  &mut self,mut value:Returned,retained:&view::Borrow,span:Span,
  drop_dataset:impl std::ops::AsyncFn(&mut Self,&application::Data,&view::Borrow,Span)->Result<()>+Copy,
 )->Result<()>{
  let state=match &value{Returned::Dataset(data)=>ownership(&data.ownership),Returned::OwnedValue(value)=>ownership(&value.ownership),_=>State::Inactive};
  let mut facts=Facts{kind:kind(&value),ownership:state,..Facts::default()};let mut cursor=unsafe{start()};
  let mut field=None;let mut remaining=view::Borrow::default();
  loop{let next=unsafe{step(cursor,facts)};cursor=next.cursor;match next.need{
   Need::Conditional=>{
    let state=match &mut value{Returned::Dataset(data)=>std::mem::replace(&mut data.ownership,Ownership::Owned),Returned::OwnedValue(value)=>std::mem::replace(&mut value.ownership,Ownership::Owned),_=>unsafe{std::hint::unreachable_unchecked()}};
    let Ownership::Conditional(when)=state else{unsafe{std::hint::unreachable_unchecked()}};
    let trigger=self.source_site.clone().map(ir::GenerationAnchor::Syntax);
    let branch=Arc::new(GeneratedOrigin{reason:ir::GenerationReason::ConditionalDropBranch,trigger:trigger.clone()});
    let condition=Arc::new(GeneratedOrigin{reason:ir::GenerationReason::ConditionalDropCondition,trigger});
    return unsafe{self.world.call(self.owned_when(&when,span,Some(branch),Some(condition),async |builder|unsafe{builder.world.call(builder.drop_present_owned_tree(value,retained,span,drop_dataset)).await})).await};
   }
   Need::Frame=>{self.application_frame(span)?;}
   Need::Data=>{let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};return drop_dataset(self,&data,retained,span).await;}
   Need::Inspect=>{
    let result=owner(&value);let ty=&result.value.ty;
    facts.presence=u32::from(ty.has_owned_presence());facts.placed=u32::from(result.place.is_some());facts.task=u32::from(ty.task_result().is_some());facts.cleanup=u32::from(ty.cleanup.is_some());facts.variants=u32::from(ty.has_variants());facts.fields=ty.aggregate_len()as u64;
   }
   Need::Load=>{
    let result=owner_mut(&mut value);let place=unsafe{result.place().unwrap_unchecked()};
    unsafe{self.world.call(self.read_cleanup_storage(&place,async |builder|unsafe{builder.world.call(builder.load_owned_value(result,span)).await})).await}?;
   }
   Need::Task=>{
    let Returned::OwnedValue(result)=value else{unsafe{std::hint::unreachable_unchecked()}};
    return unsafe{self.world.call(self.wait_task_value(result.value,span,async |builder,value|unsafe{builder.world.call(builder.drop_owned_tree_with(value,retained,span,drop_dataset)).await})).await};
   }
   Need::User=>{let Returned::OwnedValue(result)=value else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.drop_user_owned(result,retained,span)).await};}
   Need::InspectField=>{facts.field_owns=u32::from(owner(&value).value.ty.aggregate_part(cursor.index as usize).1.owns_resources());}
   Need::Unpack=>{field=Some(self.unpack_record(owner(&value),cursor.index as usize));}
   Need::Forget=>{unsafe{owner_mut(&mut value).payloads[cursor.index as usize].as_deref_mut().unwrap_unchecked()}.restrict_owned(&owned_presence::Presence::None);}
   Need::Borrow=>{remaining=retained.clone();unsafe{self.world.call(owner(&value).cleanup_borrow(self,&mut remaining)).await}?;}
   Need::Recurse=>{unsafe{self.world.call(self.drop_owned_tree_with(field.take().unwrap_unchecked(),&remaining,span,drop_dataset)).await}?;}
   Need::ClearPlace=>{owner_mut(&mut value).place=None;}
   Need::Variant=>return unsafe{self.world.call(self.drop_variant(owner(&value),0,retained,span,drop_dataset)).await},
   Need::Complete=>return Ok(()),
   Need::Next=>{},
  }}
 }
 async fn drop_variant(
  &mut self,value:&OwnedValue,index:usize,retained:&view::Borrow,span:Span,
  drop_dataset:impl std::ops::AsyncFn(&mut Self,&application::Data,&view::Borrow,Span)->Result<()>+Copy,
 )->Result<()>{
  let mut cursor=Cursor{phase:0,index:index as u64};let mut facts=VariantFacts{count:value.payloads.len()as u64,known:0};
  let mut branch=None;let mut condition=None;let mut outer=Vec::new();let mut yes=Vec::new();let mut item=None;
  loop{let next=unsafe{variant_step(cursor,facts)};cursor=next.cursor;match next.need{
   VariantNeed::Condition=>{
    let trigger=self.source_site.clone().map(ir::GenerationAnchor::Syntax);
    branch=Some(Arc::new(GeneratedOrigin{reason:ir::GenerationReason::VariantDropBranch,trigger:trigger.clone()}));
    let selection=Arc::new(GeneratedOrigin{reason:ir::GenerationReason::VariantDropCondition,trigger});
    let result=self.owned_variant_condition(value,index,span,Some(selection))?;facts.known=match &result.constant{None=>0,Some(bits)=>if bits[0]==0{1}else{2}};condition=Some(result);
   }
   VariantNeed::Outer=>{outer=std::mem::take(&mut self.code);}
   VariantNeed::Unpack=>{item=Some(unsafe{self.world.call(self.unpack_variant(value,index)).await}?);}
   VariantNeed::Drop=>{unsafe{self.world.call(self.drop_owned_tree_with(item.take().unwrap_unchecked(),retained,span,drop_dataset)).await}?;}
   VariantNeed::Yes=>{yes=std::mem::take(&mut self.code);}
   VariantNeed::Rest=>{unsafe{self.world.call(self.drop_variant(value,index+1,retained,span,drop_dataset)).await}?;}
   VariantNeed::Emit=>{
    let no=std::mem::replace(&mut self.code,std::mem::take(&mut outer));
    self.code.push(Inst::generated(InstKind::If{condition:unsafe{condition.as_ref().unwrap_unchecked()}.regs[0],yes:std::mem::take(&mut yes),no},unsafe{branch.take().unwrap_unchecked()}));
   }
   VariantNeed::Complete=>return Ok(()),
   VariantNeed::Next=>{},
  }}
 }
 pub(in crate::check) async fn drop_user_owned(&mut self,value:OwnedValue,retained:&view::Borrow,span:Span)->Result<()>{
  let mut cursor=unsafe{start()};let mut payload=None;let mut payload_kind=Kind::Other;let mut argument=None;let mut instance=None;
  loop{let next=unsafe{user_step(cursor,payload_kind)};cursor=next.cursor;match next.need{
   UserNeed::Payload=>{let item=self.unpack_record(&value,0);payload_kind=kind(&item);payload=Some(item);}
   UserNeed::MovableData=>{
    let Some(Returned::Dataset(data))=&payload else{unsafe{std::hint::unreachable_unchecked()}};self.application_movable(data,retained,span)?;
   }
   UserNeed::MovableTree=>{
    let Some(Returned::OwnedValue(value))=&payload else{unsafe{std::hint::unreachable_unchecked()}};let resources=value.resources();let mut at=unsafe{start()};
    loop{let next=unsafe{resource_step(at,resources.len()as u64)};at=next.cursor;if next.need==0{break;}self.application_movable(resources[(at.index-1)as usize],retained,span)?;}
   }
   UserNeed::Argument=>{argument=Some(cooperative::Argument::resource(unsafe{payload.take().unwrap_unchecked()}));}
   UserNeed::Instance=>{instance=Some(unsafe{self.world.call(self.specialized_function(value.value.ty.cleanup.as_ref().unwrap_unchecked().clone(),std::slice::from_ref(argument.as_ref().unwrap_unchecked()),span)).await}?);}
   UserNeed::Invoke=>{
    let origin=Arc::new(GeneratedOrigin{reason:ir::GenerationReason::UserOwnedDrop,trigger:self.source_site.clone().map(ir::GenerationAnchor::Syntax)});
    unsafe{self.world.call(self.invoke_returned(instance.take().unwrap_unchecked(),vec![argument.take().unwrap_unchecked()],span,Some(origin),None)).await}?;
   }
   UserNeed::Complete=>return Ok(()),
   UserNeed::Next=>{},
  }}
 }
}
