//! Dataset release and element-loop control comes from the B producer.
use super::*;
use crate::check::dataset_drop as d;
fn generated(reason:GenerationReason,trigger:&Option<GenerationAnchor>)->Arc<GeneratedOrigin>{Arc::new(GeneratedOrigin{reason,trigger:trigger.clone()})}
impl Builder<'_>{
 pub(in crate::check) async fn application_drop(&mut self,data:&Data,retained:&view::Borrow,span:Span)->Result<()>{
  unsafe{self.world.call(self.application_drop_with(data,retained,span,Self::drop_application_elements)).await}
 }
 pub(in crate::check) async fn drop_task_owned_dataset(&mut self,data:&Data,retained:&view::Borrow,span:Span)->Result<()>{
  unsafe{self.world.call(self.application_drop_with(data,retained,span,async |builder,data,_,span|{
   unsafe{builder.world.call(builder.drop_application_elements_with(data,span,async |builder,_,place|{
    unsafe{builder.world.call(builder.drop_task_completion_element(&place,span)).await}
   })).await}
  })).await}
 }
 async fn application_drop_with(&mut self,data:&Data,retained:&view::Borrow,span:Span,
  drop_elements:impl std::ops::AsyncFnOnce(&mut Self,&DataReference,&view::Borrow,Span)->Result<()>,
 )->Result<()>{
  let mut cursor=unsafe{d::start()};let mut facts=d::ParentFacts::default();
  let mut refreshed=None;let mut reference:Option<DataReference>=None;let mut handle=None;let mut drop_elements=Some(drop_elements);
  loop{let next=unsafe{d::parent_step(cursor,facts)};cursor=next.cursor;match next.need{
   d::ParentNeed::Borrow=>self.check_view_requirement(view::Requirement::Unborrowed{resource:view::resource_borrow(data),borrow:retained.clone(),kind:view::Exclusion::Resource,span})?,
   d::ParentNeed::Position=>{facts.borrowed=u32::from(self.application_position_borrowed(data.root.identity()));}
   d::ParentNeed::Borrowed=>return self.fail(span,"B227","dataset 的元素仍被当前游标或位置借用，不能结束父存储"),
   d::ParentNeed::Inspect=>{facts.children=u32::from(data.ty.owns_resources());facts.nonempty=u32::from(data.length.maximum()!=0);facts.placed=u32::from(data.place.is_some());}
   d::ParentNeed::Refresh=>{
    let place=unsafe{data.place.as_ref().unwrap_unchecked()};
    refreshed=Some(unsafe{self.world.call(self.read_cleanup_storage(place,async |builder|unsafe{builder.world.call(builder.refresh_application_data(data.clone(),span)).await})).await}?);
   }
   d::ParentNeed::Reference=>{reference=Some(DataReference::direct(refreshed.take().unwrap_or_else(||data.clone())));}
   d::ParentNeed::Elements=>{(unsafe{drop_elements.take().unwrap_unchecked()})(self,unsafe{reference.as_ref().unwrap_unchecked()},retained,span).await?;}
   d::ParentNeed::ReferenceHandle=>{handle=Some(unsafe{reference.as_ref().unwrap_unchecked()}.data.root.regs[0]);}
   d::ParentNeed::StoredHandle=>{
    let place=unsafe{data.place.as_ref().unwrap_unchecked()}.clone().project(Ty::scalar(Scalar::U32),&[0]);
    handle=Some(unsafe{self.world.call(self.read_cleanup_storage(&place,async |builder|unsafe{builder.world.call(builder.read_place(&place,span,None)).await})).await}?.regs[0]);
   }
   d::ParentNeed::Handle=>{handle=Some(data.root.regs[0]);}
   d::ParentNeed::Release=>{
    let origin=generated(GenerationReason::DatasetRelease,&self.source_site.clone().map(GenerationAnchor::Syntax));
    let call=self.application_release(unsafe{handle.take().unwrap_unchecked()});self.code.push(Inst::generated(call.kind,origin));
   }
   d::ParentNeed::Complete=>return Ok(()),
  }}
 }
 pub(in crate::check::application) async fn drop_application_elements(&mut self,data:&DataReference,retained:&view::Borrow,span:Span)->Result<()>{
  unsafe{self.world.call(self.drop_application_elements_with(data,span,async |builder,data,place|unsafe{builder.world.call(builder.drop_dataset_element(data,place,retained,span)).await})).await}
 }
 async fn drop_dataset_element(&mut self,data:&DataReference,place:Place,retained:&view::Borrow,span:Span)->Result<()>{
  let model=match &data.elements{ElementFacts::Known(_)=>d::Model::Known,ElementFacts::Common(..)=>d::Model::Common,ElementFacts::Selected(..)=>d::Model::Selected,ElementFacts::Unknown=>d::Model::Unknown,ElementFacts::Input(_)=>d::Model::Input,ElementFacts::Cleanup(_)=>d::Model::Cleanup};
  let mut cursor=unsafe{d::start()};let mut place=Some(place);let mut value=None;let mut element=None;let mut trigger=None;
  loop{let next=unsafe{d::element_step(cursor,model)};cursor=next.cursor;match next.need{
   d::ElementNeed::Read=>{
    trigger=self.source_site.clone().map(GenerationAnchor::Syntax);let origin=generated(GenerationReason::DatasetDropElementRead,&trigger);let place=unsafe{place.as_ref().unwrap_unchecked()};
    value=Some(unsafe{self.world.call(self.read_cleanup_storage(place,async |builder|unsafe{builder.world.call(builder.read_storage_value(place,span,Some(origin))).await})).await}?);
   }
   d::ElementNeed::NativeModel=>{element=Some(unsafe{self.world.call(self.materialize_element(data,value.take(),place.as_ref().unwrap_unchecked())).await}?);}
   d::ElementNeed::NativeDrop=>return unsafe{self.world.call(self.native_element_drop(element.take().unwrap_unchecked(),retained,span,generated(GenerationReason::DatasetElementDrop,&trigger))).await},
   d::ElementNeed::KnownModel=>{element=Some(unsafe{self.world.call(self.materialize_element(data,None,place.as_ref().unwrap_unchecked())).await}?);}
   d::ElementNeed::Storage=>{unsafe{owned_borrows::set_storage(element.as_mut().unwrap_unchecked(),place.take().unwrap_unchecked())};}
   d::ElementNeed::KnownDrop=>return unsafe{self.world.call(self.drop_owned_tree(element.take().unwrap_unchecked(),retained,span)).await},
  }}
 }
 pub(in crate::check::application) async fn drop_application_elements_with(&mut self,data:&DataReference,span:Span,
  drop_element:impl std::ops::AsyncFnOnce(&mut Self,&DataReference,Place)->Result<()>,
 )->Result<()>{
  let mut cursor=unsafe{d::start()};let mut trigger=None;let mut condition_origin=None;
  let mut scalar=None;let mut slot=None;let mut cursor_reg=None;let mut zero=None;let mut test=None;let mut one=None;let mut index=None;let mut place=None;
  let mut outer=Vec::new();let mut condition=Vec::new();let mut drop_element=Some(drop_element);
  loop{let next=unsafe{d::loop_step(cursor)};cursor=next.cursor;match next.need{
   d::LoopNeed::Cpu=>{self.application_cpu(&data.data,span)?;}
   d::LoopNeed::Initialize=>{
    trigger=self.source_site.clone().map(GenerationAnchor::Syntax);let ty=Ty::scalar(Scalar::U32);let local=self.local_slots(&ty).0[0];slot=Some(local);scalar=Some(ty);
    self.code.push(Inst::generated(InstKind::StoreLocal(local,data.length.regs[0]),generated(GenerationReason::DatasetDropCursor,&trigger)));
    condition_origin=Some(generated(GenerationReason::DatasetDropCondition,&trigger));
   }
   d::LoopNeed::Outer=>{outer=std::mem::take(&mut self.code);}
   d::LoopNeed::Load=>{
    let reg=self.reg(Scalar::U32);cursor_reg=Some(reg);self.code.push(Inst::generated(InstKind::LoadLocal(reg,unsafe{slot.unwrap_unchecked()}),unsafe{condition_origin.as_ref().unwrap_unchecked()}.clone()));
   }
   d::LoopNeed::Zero=>{zero=Some(self.constant_with_origin(Scalar::U32,0,condition_origin.clone()).regs[0]);}
   d::LoopNeed::Positive=>{
    let reg=self.reg(Scalar::Bool);test=Some(reg);
    self.code.push(Inst::generated(InstKind::Binary{dst:reg,op:ir::BinaryOp::Compare(ir::Comparison::Greater),a:unsafe{cursor_reg.unwrap_unchecked()},b:unsafe{zero.unwrap_unchecked()}},unsafe{condition_origin.take().unwrap_unchecked()}));
   }
   d::LoopNeed::Condition=>{condition=std::mem::take(&mut self.code);}
   d::LoopNeed::One=>{one=Some(self.constant_with_origin(Scalar::U32,1,Some(generated(GenerationReason::DatasetDropIndex,&trigger))).regs[0]);}
   d::LoopNeed::Previous=>{
    let reg=self.reg(Scalar::U32);index=Some(reg);
    self.code.push(Inst::generated(InstKind::Binary{dst:reg,op:ir::BinaryOp::ISub,a:unsafe{cursor_reg.unwrap_unchecked()},b:unsafe{one.unwrap_unchecked()}},generated(GenerationReason::DatasetDropIndex,&trigger)));
   }
   d::LoopNeed::Place=>{
    let value=Value{ty:unsafe{scalar.take().unwrap_unchecked()},regs:vec![unsafe{index.unwrap_unchecked()}],proof:bounds::Proof::unsigned(0,unsafe{d::index_maximum(data.length.maximum())}).with_domains(Domains::from([IndexDomain::Upper(data.length.identity(),1)])),uniform:false,literal:false,constant:None};
    let cursor=bounds::Clause::values([(Scalar::U32,value.identity(),&value.proof)]);
    place=Some((data.element(&value,(value.minimum(),value.maximum())),cursor));
   }
   d::LoopNeed::Drop=>{
    let(place,cursor)=unsafe{place.take().unwrap_unchecked()};
    let path=bounds::Clause::both(&self.path,&cursor);
    let outer_path=std::mem::replace(&mut self.path,path);
    let result=(unsafe{drop_element.take().unwrap_unchecked()})(self,data,place).await;
    self.path=outer_path;
    result?;
   }
   d::LoopNeed::Advance=>{self.code.push(Inst::generated(InstKind::StoreLocal(unsafe{slot.unwrap_unchecked()},unsafe{index.unwrap_unchecked()}),generated(GenerationReason::DatasetDropAdvance,&trigger)));}
   d::LoopNeed::Loop=>{
    let body=std::mem::replace(&mut self.code,std::mem::take(&mut outer));
    self.code.push(Inst::generated(InstKind::Loop{condition:std::mem::take(&mut condition),test:unsafe{test.unwrap_unchecked()},body},generated(GenerationReason::DatasetDropLoop,&trigger)));
   }
   d::LoopNeed::Complete=>return Ok(()),
  }}
 }
}
