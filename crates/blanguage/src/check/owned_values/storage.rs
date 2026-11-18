//! B owns model production; the original typed products stay in these callers.
use super::*;
use std::{ffi::c_void,ptr};
#[derive(Clone,Copy,Default)]
#[repr(u32)]enum Kind{#[default]Unit,Value,Owned,Data}
fn kind(value:&Returned)->Kind{match value{Returned::Unit=>Kind::Unit,Returned::Value(_)=>Kind::Value,Returned::OwnedValue(_)=>Kind::Owned,Returned::Dataset(_)=>Kind::Data}}
#[derive(Clone,Copy)]
#[repr(C)]struct Cursor{phase:u32,index:u64}
#[repr(C)]struct Step<T>{cursor:Cursor,need:T}
#[repr(C)]struct Layout{length_offset:u32,alignment:u32,size:u32}
#[repr(u32)]enum StorageNeed{Value,Owned,Unit,RootProof,LengthProof,Data,Complete}
#[repr(u32)]enum RebindNeed{Value,Owned,OwnedPlace,Root,Length,Complete}
#[repr(u32)]enum UnpackNeed{Value,Clone,Inspect,ValueRegs,Owned,Root,Length,Count,Place,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)]struct UnpackFacts{owns:u32,kind:Kind,count:u32,place:u32}
#[derive(Clone,Copy)]
#[repr(u32)]enum TemplateNeed{Retain,Element,Handle,Length,FixedLength,LengthProof,DataOwned,DataMoved,InspectField,Field,Recurse,AggregateEmpty,AggregateStore,AggregateOwned,AggregateMoved,VariantSlots,VariantField,VariantStore,VariantOwned,VariantMoved,Unit,Value,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)]struct TemplateFacts{dataset:u32,owns:u32,variant:u32,cleanup:u32,task:u32,available:u32,void_type:u32,child_owns:u32,count:u32,fields:u64,variants:u64}
#[repr(u32)]enum ConstructNeed{Variant,Target,Count,Evaluate,AbandonInspect,AbandonRemove,Empty,Missing,InspectArray,ReturnItem,Copy,Check,InspectHold,Hold,Store,Finish,Take,Unhold,InspectItem,MoveValue,Storage,Payload,EmptyPayload,Pack,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)]struct ConstructFacts{variant:u32,cpu:u32,application:u32,array:u32,continues:u32,present:u32,kind:Kind,same_array:u32,borrows:u32,held:u32,field_owns:u32,arguments:u64,fields:u64,prepared:u64}
#[repr(C)]struct Component{scalar:u32,word:u32,size:u32}
#[repr(C)]struct VariantHost{
 count:unsafe extern "C" fn(*const c_void)->u64,
 component:unsafe extern "C" fn(*const c_void,u64)->Component,
 select:unsafe extern "C" fn(*mut c_void,u64),
 proof:unsafe extern "C" fn(*mut c_void),
 selected:unsafe extern "C" fn(*const c_void,u64)->u64,
 forget:unsafe extern "C" fn(*mut c_void,u64,u64),
}
unsafe extern "C"{
 #[link_name="bootstrap.ownership_storage::start"]fn start()->Cursor;
 #[link_name="bootstrap.ownership_storage::dataset_layout"]fn dataset_layout(root_size:u32,root_align:u32,length_size:u32,length_align:u32)->Layout;
 #[link_name="bootstrap.ownership_storage::storage_step"]fn storage_step(cursor:Cursor,kind:Kind)->Step<StorageNeed>;
 #[link_name="bootstrap.ownership_storage::rebind_step"]fn rebind_step(cursor:Cursor,kind:Kind)->Step<RebindNeed>;
 #[link_name="bootstrap.ownership_storage::unpack_step"]fn unpack_step(cursor:Cursor,facts:UnpackFacts)->Step<UnpackNeed>;
 #[link_name="bootstrap.ownership_storage::template_step"]fn template_step(cursor:Cursor,facts:TemplateFacts)->Step<TemplateNeed>;
 #[link_name="bootstrap.ownership_storage::construct_step"]fn construct_step(cursor:Cursor,facts:ConstructFacts)->Step<ConstructNeed>;
 #[link_name="bootstrap.ownership_storage::variant_projection"]fn project_variant(root:*const c_void,child:*const c_void,offset:u32,output:*mut c_void,host:VariantHost);
}
struct VariantProjection<'a>{value:&'a Value,components:Vec<usize>,proof:Option<bounds::Proof>}
unsafe extern "C" fn component_count(ty:*const c_void)->u64{unsafe{&*ty.cast::<Ty>()}.components.len()as u64}
unsafe extern "C" fn component(ty:*const c_void,index:u64)->Component{let(scalar,word)=unsafe{&*ty.cast::<Ty>()}.components[index as usize];Component{scalar:scalar as u32,word,size:scalar.size()}}
unsafe extern "C" fn select(output:*mut c_void,index:u64){unsafe{&mut *output.cast::<VariantProjection<'_>>()}.components.push(index as usize);}
unsafe extern "C" fn project_proof(output:*mut c_void){let p=unsafe{&mut *output.cast::<VariantProjection<'_>>()};p.proof=Some(p.value.projected_proof(&p.components));}
unsafe extern "C" fn selected(output:*const c_void,index:u64)->u64{unsafe{&*output.cast::<VariantProjection<'_>>()}.components[index as usize]as u64}
unsafe extern "C" fn forget(output:*mut c_void,index:u64,count:u64){unsafe{(&mut *output.cast::<VariantProjection<'_>>()).proof.as_mut().unwrap_unchecked().forget_value(&[index as usize],count as usize)};}
fn variant_value(value:&Value,ty:&Ty,offset:u32)->Value{
 let mut output=VariantProjection{value,components:Vec::new(),proof:None};
 unsafe{project_variant(ptr::from_ref(&value.ty).cast(),ptr::from_ref(ty).cast(),offset,ptr::from_mut(&mut output).cast(),VariantHost{count:component_count,component,select,proof:project_proof,selected,forget})};
 Value{ty:ty.clone(),regs:output.components.iter().map(|i|value.regs[*i]).collect(),proof:unsafe{output.proof.take().unwrap_unchecked()},uniform:value.uniform,literal:false,constant:None}
}
impl Builder<'_>{
 pub(in crate::check) fn returned_storage(&self,value:&Returned)->Value{
  let mut cursor=unsafe{start()};let k=kind(value);let mut output=None;let mut proof:Option<bounds::Proof>=None;
  loop{let step=unsafe{storage_step(cursor,k)};cursor=step.cursor;match step.need{
   StorageNeed::Value=>{let Returned::Value(value)=value else{unsafe{std::hint::unreachable_unchecked()}};output=Some((**value).clone());}
   StorageNeed::Owned=>{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};output=Some(value.value.clone());}
   StorageNeed::Unit=>{output=Some(Value{ty:Ty::unit(),regs:Vec::new(),proof:bounds::Proof::default(),uniform:true,literal:false,constant:Some(Vec::new())});}
   StorageNeed::RootProof=>{let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};let mut p=bounds::Proof::default();p.append(&data.root);proof=Some(p);}
   StorageNeed::LengthProof=>{let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};unsafe{proof.as_mut().unwrap_unchecked()}.append(&data.length);}
   StorageNeed::Data=>{
    let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};
    let layout=unsafe{dataset_layout(data.root.ty.size,data.root.ty.align,data.length.ty.size,data.length.ty.align)};
    let mut ty=Ty::owned_dataset(data.ty.syntax(),data.count);ty.components=vec![(data.root.ty.components[0].0,0),(Scalar::U32,layout.length_offset)];ty.align=layout.alignment;ty.size=layout.size;
    output=Some(Value{ty,regs:vec![data.root.regs[0],data.length.regs[0]],proof:unsafe{proof.take().unwrap_unchecked()},uniform:data.root.uniform&&data.length.uniform,literal:false,constant:None});
   }
   StorageNeed::Complete=>return unsafe{output.take().unwrap_unchecked()},
  }}
 }
 pub(in crate::check) fn rebind_returned(&mut self,value:&mut Returned,regs:Vec<Reg>){
  let mut cursor=unsafe{start()};let k=kind(value);let mut regs=Some(regs);
  loop{let step=unsafe{rebind_step(cursor,k)};cursor=step.cursor;match step.need{
   RebindNeed::Value=>{let Returned::Value(value)=value else{unsafe{std::hint::unreachable_unchecked()}};value.regs=unsafe{regs.take().unwrap_unchecked()};}
   RebindNeed::Owned=>{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};value.value.regs=unsafe{regs.take().unwrap_unchecked()};}
   RebindNeed::OwnedPlace=>{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};value.place=None;}
   RebindNeed::Root=>{let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};data.root.regs=vec![unsafe{regs.as_ref().unwrap_unchecked()}[0]];data.root.proof=bounds::Proof::capture(&data.root);}
   RebindNeed::Length=>{let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};data.length.regs=vec![unsafe{regs.as_ref().unwrap_unchecked()}[1]];data.length.proof=bounds::Proof::capture(&data.length).with_domains(Domains::from([IndexDomain::ApplicationLength(data.root.identity())]));}
   RebindNeed::Complete=>return,
  }}
 }
 pub(in crate::check) async fn template_returned(&self,mut value:Value,available:bool)->Result<Returned>{
  let mut cursor=unsafe{start()};let mut facts=TemplateFacts{dataset:u32::from(value.ty.dataset.is_some()),owns:u32::from(value.ty.owns_resources()),variant:u32::from(value.ty.has_variants()),cleanup:u32::from(value.ty.cleanup.is_some()),task:u32::from(value.ty.task_result().is_some()),available:u32::from(available),void_type:u32::from(value.ty.name=="void"),count:u32::from(!value.ty.values.is_empty()),fields:value.ty.aggregate_len()as u64,variants:value.ty.variants.len()as u64,..TemplateFacts::default()};
  let mut payloads:Vec<Option<Box<Returned>>>=Vec::new();let mut element=None;let mut handle:Option<Value>=None;let mut length:Option<Value>=None;let mut field=None;let mut returned=None;
  loop{let step=unsafe{template_step(cursor,facts)};cursor=step.cursor;let need=step.need;match need{
   TemplateNeed::Retain=>value.proof.retain_view_borrows(&value.ty),
   TemplateNeed::Element=>{element=Some(unsafe{self.world.call(self.expected_storage(value.ty.dataset.as_deref().unwrap_unchecked(),self.scope.span())).await}?);}
   TemplateNeed::Handle=>{let mut v=self.value_component(&value,0);v.proof=bounds::Proof::capture(&v);handle=Some(v);}
   TemplateNeed::Length=>{length=Some(self.value_component(&value,1));}
   TemplateNeed::FixedLength=>{let n=unsafe{*value.ty.values.first().unwrap_unchecked()};let v=unsafe{length.as_mut().unwrap_unchecked()};v.proof=bounds::Proof::unsigned(n,n).with_domains(Domains::from([IndexDomain::ApplicationLength(unsafe{handle.as_ref().unwrap_unchecked()}.identity())]));v.constant=Some(vec![Bits::from(n)]);}
   TemplateNeed::LengthProof=>{let v=unsafe{length.as_mut().unwrap_unchecked()};v.proof=bounds::Proof::capture(v).with_domains(Domains::from([IndexDomain::ApplicationLength(unsafe{handle.as_ref().unwrap_unchecked()}.identity())]));}
   TemplateNeed::DataOwned|TemplateNeed::DataMoved=>{
    let ownership=if matches!(need,TemplateNeed::DataOwned){Ownership::Owned}else{Ownership::Moved};
    return Ok(Returned::Dataset(Box::new(DataPayload{prefixes:BTreeMap::new(),elements:application::ElementFacts::Unknown,root:unsafe{handle.take().unwrap_unchecked()},length:unsafe{length.take().unwrap_unchecked()},ty:unsafe{element.take().unwrap_unchecked()},count:value.ty.values.first().copied(),layout:application::DataLayout::Declared,residency:residency::Residency::default(),ownership,access:Access::InOut,place:None,initialization:None})));
   }
   TemplateNeed::InspectField=>{facts.child_owns=u32::from(value.ty.aggregate_part(cursor.index as usize).1.owns_resources());}
   TemplateNeed::Field=>{field=Some(self.record_component(&value,cursor.index as usize));}
   TemplateNeed::Recurse=>{returned=Some(unsafe{self.world.call(self.template_returned(field.take().unwrap_unchecked(),available)).await}?);}
   TemplateNeed::AggregateEmpty=>payloads.push(None),
   TemplateNeed::AggregateStore=>payloads.push(Some(Box::new(unsafe{returned.take().unwrap_unchecked()}))),
   TemplateNeed::VariantSlots=>{payloads=vec![None;value.ty.variants.len()];}
   TemplateNeed::VariantField=>{field=Some(variant_value(&value,&value.ty.variants[cursor.index as usize].ty,value.ty.variant_payload_offset()));}
   TemplateNeed::VariantStore=>{payloads[cursor.index as usize]=Some(Box::new(unsafe{returned.take().unwrap_unchecked()}));}
   TemplateNeed::AggregateOwned|TemplateNeed::AggregateMoved|TemplateNeed::VariantOwned|TemplateNeed::VariantMoved=>{
    let ownership=if matches!(need,TemplateNeed::AggregateOwned|TemplateNeed::VariantOwned){Ownership::Owned}else{Ownership::Moved};
    return Ok(Returned::OwnedValue(Box::new(OwnedValuePayload{value,payloads,ownership,place:None,declaration:0})));
   }
   TemplateNeed::Unit=>return Ok(Returned::Unit),
   TemplateNeed::Value=>return Ok(Returned::value(value)),
   TemplateNeed::Next=>{},
  }}
 }
}

enum PreparedField{Value(Returned),Held(String)}
impl Builder<'_>{
 pub(in crate::check) async fn make_owned_record(&mut self,ty:Ty,args:&Expressions,span:Span)->Result<Option<Returned>>{
  let mut cursor=unsafe{start()};let mut facts=ConstructFacts{variant:u32::from(ty.has_variants()),cpu:u32::from(self.program.target==Target::Cpu),application:u32::from(self.application.is_some()),array:u32::from(ty.array.is_some()),arguments:args.len()as u64,fields:ty.aggregate_len()as u64,..ConstructFacts::default()};
  let mut fields=Vec::new();let mut pending=Vec::new().into_iter();let mut item:Option<Returned>=None;let mut arg:Option<Expr>=None;let mut held:Option<String>=None;let mut payloads=Vec::new();let mut values=Vec::new();
  loop{let step=unsafe{construct_step(cursor,facts)};cursor=step.cursor;match step.need{
   ConstructNeed::Variant=>return self.fail(span,"B269","带载荷变体通过声明的分支构造"),
   ConstructNeed::Target=>return self.fail(span,"B224","拥有资源的记录由普通CPU应用函数组织"),
   ConstructNeed::Count=>return self.fail(span,"B157","记录构造需要按声明顺序提供每个字段"),
   ConstructNeed::Evaluate=>{
    arg=Some(args.at(cursor.index as usize));let field=ty.aggregate_part(cursor.index as usize).1;
    item=unsafe{self.world.call(self.evaluate_returned(arg.as_ref().unwrap_unchecked(),Some(field))).await}?;
    facts.continues=u32::from(self.flow.continues());facts.present=u32::from(item.is_some());facts.prepared=fields.len()as u64;
   }
   ConstructNeed::AbandonInspect=>{facts.held=u32::from(matches!(fields[cursor.index as usize],PreparedField::Held(_)));}
   ConstructNeed::AbandonRemove=>{let PreparedField::Held(name)=&fields[cursor.index as usize]else{unsafe{std::hint::unreachable_unchecked()}};self.env.remove(name);}
   ConstructNeed::Empty=>return Ok(None),
   ConstructNeed::Missing=>return self.fail(unsafe{arg.as_ref().unwrap_unchecked()}.span(),"B228","记录字段需要值或拥有资源"),
   ConstructNeed::InspectArray=>{facts.same_array=u32::from(matches!(item.as_ref(),Some(Returned::OwnedValue(value))if value.value.ty==ty));}
   ConstructNeed::ReturnItem=>return Ok(item),
   ConstructNeed::Copy=>return self.fail(span,"B157","拥有数组构造需要逐项交付全部元素，不能复制一个拥有值填充"),
   ConstructNeed::Check=>{
    let field=ty.aggregate_part(cursor.index as usize).1;let at=unsafe{arg.as_ref().unwrap_unchecked()}.span();
    unsafe{self.world.call(self.check_returned_type(item.as_mut().unwrap_unchecked(),field,at)).await}?;arg.take();
   }
   ConstructNeed::InspectHold=>{
    let value=unsafe{item.as_ref().unwrap_unchecked()};facts.kind=kind(value);
    facts.borrows=u32::from(matches!(value,Returned::Value(value)if value.ty.contains_borrows()));
   }
   ConstructNeed::Hold=>{
    let binding=match unsafe{item.take().unwrap_unchecked()}{Returned::Dataset(data)=>Binding::ApplicationData(data),Returned::OwnedValue(value)=>Binding::OwnedValue(value),Returned::Value(value)=>Binding::Value(*value),Returned::Unit=>unsafe{std::hint::unreachable_unchecked()}};
    fields.push(PreparedField::Held(self.hold_binding(binding)));
   }
   ConstructNeed::Store=>fields.push(PreparedField::Value(unsafe{item.take().unwrap_unchecked()})),
   ConstructNeed::Finish=>{pending=std::mem::take(&mut fields).into_iter();}
   ConstructNeed::Take=>{match unsafe{pending.next().unwrap_unchecked()}{PreparedField::Value(value)=>{item=Some(value);facts.held=0;},PreparedField::Held(name)=>{held=Some(name);facts.held=1;}}}
   ConstructNeed::Unhold=>{
    let binding=unsafe{self.env.remove(&held.take().unwrap_unchecked()).unwrap_unchecked()};
    item=Some(match binding{Binding::ApplicationData(data)=>Returned::Dataset(data),Binding::OwnedValue(value)=>Returned::OwnedValue(value),Binding::Value(value)=>Returned::value(value),_=>unsafe{std::hint::unreachable_unchecked()}});
   }
   ConstructNeed::InspectItem=>{facts.kind=kind(unsafe{item.as_ref().unwrap_unchecked()});facts.field_owns=u32::from(ty.aggregate_part(cursor.index as usize).1.owns_resources());}
   ConstructNeed::MoveValue=>{let Returned::Value(value)=(unsafe{item.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};values.push(*value);}
   ConstructNeed::Storage=>{values.push(self.returned_storage(unsafe{item.as_ref().unwrap_unchecked()}));}
   ConstructNeed::Payload=>payloads.push(Some(Box::new(unsafe{item.take().unwrap_unchecked()}))),
   ConstructNeed::EmptyPayload=>{item.take();payloads.push(None);}
   ConstructNeed::Pack=>{
    let mut value=self.pack_record(ty,values,span)?;value.proof.retain_view_borrows(&value.ty);
    return Ok(Some(Returned::OwnedValue(Box::new(OwnedValuePayload{value,payloads,ownership:Ownership::Owned,place:None,declaration:0}))));
   }
   ConstructNeed::Next=>{},
  }}
 }
}
pub(in crate::check) fn unpack(builder:&mut Builder<'_>,owner:&OwnedValue,index:usize,value:Value,ty:&Ty,components:Vec<usize>)->Returned{
 let mut cursor=unsafe{start()};let mut facts=UnpackFacts{owns:u32::from(ty.owns_resources()),place:u32::from(owner.place.is_some()),..UnpackFacts::default()};let mut value=Some(value);let mut item:Option<Returned>=None;
 loop{let step=unsafe{unpack_step(cursor,facts)};cursor=step.cursor;match step.need{
  UnpackNeed::Value=>return Returned::value(unsafe{value.take().unwrap_unchecked()}),
  UnpackNeed::Clone=>{item=Some(unsafe{(**owner.payloads[index].as_ref().unwrap_unchecked()).clone()});}
  UnpackNeed::Inspect=>{let v=unsafe{item.as_ref().unwrap_unchecked()};facts.kind=kind(v);facts.count=u32::from(matches!(v,Returned::Dataset(data)if data.count.is_some()));}
  UnpackNeed::ValueRegs=>{let Returned::Value(v)=(unsafe{item.as_mut().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};v.regs=std::mem::take(&mut unsafe{value.as_mut().unwrap_unchecked()}.regs);}
  UnpackNeed::Owned=>{let Returned::OwnedValue(v)=(unsafe{item.as_mut().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};v.value=unsafe{value.take().unwrap_unchecked()};v.place=None;}
  UnpackNeed::Root=>{let Returned::Dataset(data)=(unsafe{item.as_mut().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};data.root.regs=vec![unsafe{value.as_ref().unwrap_unchecked()}.regs[0]];data.root.proof=bounds::Proof::capture(&data.root);}
  UnpackNeed::Length=>{let Returned::Dataset(data)=(unsafe{item.as_mut().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};data.length=builder.value_component(unsafe{value.as_ref().unwrap_unchecked()},1);}
  UnpackNeed::Count=>{let Returned::Dataset(data)=(unsafe{item.as_mut().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};data.length.proof.assume_same_length(&bounds::Proof::unsigned(unsafe{data.count.unwrap_unchecked()},unsafe{data.count.unwrap_unchecked()}));}
  UnpackNeed::Place=>{
   let place=unsafe{owner.place().unwrap_unchecked()}.project(ty.clone(),&components);
   match unsafe{item.as_mut().unwrap_unchecked()}{Returned::Dataset(data)=>data.place=Some(place),Returned::OwnedValue(child)=>child.place=Some(place),_=>unsafe{std::hint::unreachable_unchecked()}}
  }
  UnpackNeed::Complete=>return unsafe{item.take().unwrap_unchecked()},
  UnpackNeed::Next=>{},
 }}
}
