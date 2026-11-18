//! B owns cleanup-borrow production; the original typed facts and output stay borrowed.
use super::*;
use std::{ffi::c_void,ptr};
type Address=*const c_void;
#[derive(Clone,Copy,Default)]
#[repr(u32)]enum Kind{#[default]Unknown,Unit,Value,Owned,Data}
#[derive(Clone,Copy)]
#[repr(C)]struct Info{flags:u32,count:u64}
#[derive(Clone,Copy)]
#[repr(C)]struct Field{word:u32,ty:Address}
#[derive(Clone,Copy)]
#[repr(C)]struct Host{
 inspect:unsafe extern "C" fn(Address)->Info,
 selected:unsafe extern "C" fn(Address,u32,Address,*mut c_void,Info),
 fields:unsafe extern "C" fn(Address,u32,Address,*mut c_void),
 child:unsafe extern "C" fn(Address,u64)->Address,
 offset:unsafe extern "C" fn(Address,u64,u32)->u32,
 word:unsafe extern "C" fn(Address,u64)->u32,
 field:unsafe extern "C" fn(Address,u64)->Field,
 emit:unsafe extern "C" fn(*mut c_void,u64,Address,Address),
}
#[derive(Clone,Copy)]#[repr(C)]struct Cursor{phase:u32,index:u64}
#[repr(C)]struct Step<T>{cursor:Cursor,need:T}
#[repr(u32)]enum OwnerNeed{Views,Children,Restrict,Union,Complete}
#[repr(u32)]enum ChildNeed{Inspect,Data,Owned,Complete,Next}
#[derive(Clone,Copy)]#[repr(C)]struct ChildFacts{count:u64,kind:Kind,active:u32}
#[repr(u32)]enum DataNeed{Inspect,Owned,Data,Stored,Unresolved,Restrict,Union,Complete,Next}
#[derive(Clone,Copy,Default)]#[repr(C)]struct DataFacts{active:u32,nonempty:u32,kind:Kind,conditional:u32,stored:u32}
type ViewEntry=unsafe extern "C" fn(Address,u32,Address,*mut c_void,Info,Host);
unsafe extern "C"{
 #[link_name="bootstrap.ownership_cleanup::start"]fn start()->Cursor;
 #[link_name="bootstrap.ownership_cleanup::binding"]fn binding(kind:Kind)->Kind;
 #[link_name="bootstrap.ownership_cleanup::owner_step"]fn owner_step(state:Cursor,info:Info)->Step<OwnerNeed>;
 #[link_name="bootstrap.ownership_cleanup::child_step"]fn child_step(state:Cursor,facts:ChildFacts)->Step<ChildNeed>;
 #[link_name="bootstrap.ownership_cleanup::data_step"]fn data_step(state:Cursor,facts:DataFacts)->Step<DataNeed>;
 #[link_name="bootstrap.ownership_cleanup::views"]fn views(node:Address,offset:u32,path:Address,context:*mut c_void,info:Info,host:Host);
 #[link_name="bootstrap.ownership_cleanup::views_active"]fn views_active(node:Address,offset:u32,path:Address,context:*mut c_void,info:Info,host:Host);
 #[link_name="bootstrap.ownership_cleanup::views_selected"]fn views_selected(node:Address,offset:u32,path:Address,context:*mut c_void,info:Info,host:Host);
 #[link_name="bootstrap.ownership_cleanup::fields"]fn fields(list:Address,count:u64,offset:u32,path:Address,context:*mut c_void,host:Host);
}
fn kind(value:Option<&Returned>)->Kind{match value{None=>Kind::Unknown,Some(Returned::Unit)=>Kind::Unit,Some(Returned::Value(_))=>Kind::Value,Some(Returned::OwnedValue(_))=>Kind::Owned,Some(Returned::Dataset(_))=>Kind::Data}}
fn info(value:&OwnedValuePayload)->Info{
 let ty=&value.value.ty;
 Info{flags:u32::from(value.ownership.may_own())|(u32::from(matches!(value.ownership,Ownership::Conditional(_)))<<1)|(u32::from(ty.cleanup.is_some())<<2)|(u32::from(ty.task_result().is_some())<<3)|(u32::from(ty.has_variants())<<4),count:value.payloads.len()as u64}
}
unsafe extern "C" fn inspect(node:Address)->Info{info(unsafe{&*node.cast::<OwnedValuePayload>()})}
unsafe extern "C" fn child(node:Address,index:u64)->Address{
 match unsafe{&*node.cast::<OwnedValuePayload>()}.payloads[index as usize].as_deref(){Some(Returned::OwnedValue(value))=>ptr::from_ref(value.as_ref()).cast(),_=>ptr::null()}
}
unsafe extern "C" fn offset(node:Address,index:u64,variant:u32)->u32{
 let ty=&unsafe{&*node.cast::<OwnedValuePayload>()}.value.ty;
 if variant!=0{ty.variant_payload_offset()}else{ty.aggregate_part(index as usize).0}
}
struct Views<'a,F>{root:&'a Value,into:&'a mut view::Borrow,map:&'a F}
fn host<F:Fn(usize,&Ty,view::Borrow)->view::Borrow>()->Host{Host{inspect,selected:selected::<F>,fields:borrow_fields::<F>,child,offset,word:word::<F>,field,emit:emit::<F>}}
unsafe extern "C" fn selected<F:Fn(usize,&Ty,view::Borrow)->view::Borrow>(node:Address,offset:u32,path:Address,context:*mut c_void,info:Info){
 let value=unsafe{&*node.cast::<OwnedValuePayload>()};let Ownership::Conditional(when)=&value.ownership else{unsafe{std::hint::unreachable_unchecked()}};
 let selected=when.condition().assume(unsafe{&*path.cast::<bounds::Clause>()},true);
 unsafe{views_selected(node,offset,ptr::from_ref(&selected).cast(),context,info,host::<F>())};
}
unsafe extern "C" fn borrow_fields<F:Fn(usize,&Ty,view::Borrow)->view::Borrow>(node:Address,offset:u32,path:Address,context:*mut c_void){
 let value=unsafe{&*node.cast::<OwnedValuePayload>()};let list=value.value.ty.borrow_fields();
 unsafe{fields(ptr::from_ref(&list).cast(),list.len()as u64,offset,path,context,host::<F>())};
}
unsafe extern "C" fn field(list:Address,index:u64)->Field{
 let list=unsafe{&*list.cast::<Vec<(u32,&Ty)>>()};let(word,ty)=list[index as usize];Field{word,ty:ptr::from_ref(ty).cast()}
}
unsafe extern "C" fn word<F>(context:Address,index:u64)->u32{unsafe{&*context.cast::<Views<'_,F>>()}.root.ty.components[index as usize].1}
unsafe extern "C" fn emit<F:Fn(usize,&Ty,view::Borrow)->view::Borrow>(context:*mut c_void,index:u64,ty:Address,path:Address){
 let context=unsafe{&mut *context.cast::<Views<'_,F>>()};let borrow=context.root.proof.view_borrow(index as usize).restricted(unsafe{&*path.cast::<bounds::Clause>()});
 context.into.union(&(context.map)(index as usize,unsafe{&*ty.cast::<Ty>()},borrow));
}
fn accumulate_views<F:Fn(usize,&Ty,view::Borrow)->view::Borrow>(value:&OwnedValuePayload,into:&mut view::Borrow,map:&F,info:Info,entry:ViewEntry){
 let mut context=Views{root:&value.value,into,map};let path=bounds::Clause::default();
 unsafe{entry(ptr::from_ref(value).cast(),0,ptr::from_ref(&path).cast(),ptr::from_mut(&mut context).cast(),info,host::<F>())};
}
pub(in crate::check) async fn cleanup_borrow(builder:&Builder<'_>,value:&Binding,into:&mut view::Borrow)->Result<()>{
 let source=match value{Binding::OwnedValue(_)=>Kind::Owned,Binding::ApplicationData(_)=>Kind::Data,_=>Kind::Unknown};
 match unsafe{binding(source)}{
  Kind::Owned=>{let Binding::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};unsafe{builder.world.call(value.cleanup_borrow(builder,into)).await}}
  Kind::Data=>{let Binding::ApplicationData(value)=value else{unsafe{std::hint::unreachable_unchecked()}};unsafe{builder.world.call(data_contents(builder,value,into)).await}}
  _=>Ok(()),
 }
}
async fn data_contents(builder:&Builder<'_>,data:&application::Data,into:&mut view::Borrow)->Result<()>{
 let mut cursor=unsafe{start()};let mut facts=DataFacts{active:u32::from(data.ownership.may_own()),nonempty:u32::from(data.length.maximum()!=0),conditional:u32::from(matches!(data.ownership,Ownership::Conditional(_))),..DataFacts::default()};let mut borrowed=view::Borrow::default();
 loop{let next=unsafe{data_step(cursor,facts)};cursor=next.cursor;match next.need{
  DataNeed::Inspect=>{facts.kind=kind(data.elements.known());}
  DataNeed::Owned=>{let Some(Returned::OwnedValue(value))=data.elements.known()else{unsafe{std::hint::unreachable_unchecked()}};unsafe{builder.world.call(value.cleanup_borrow(builder,&mut borrowed)).await}?;}
  DataNeed::Data=>{let Some(Returned::Dataset(value))=data.elements.known()else{unsafe{std::hint::unreachable_unchecked()}};unsafe{builder.world.call(data_contents(builder,value,&mut borrowed)).await}?;}
  DataNeed::Stored=>{facts.stored=u32::from(unsafe{builder.world.call(builder.world.stored_borrows(&data.ty,builder.scope.source(),builder.scope.span(),builder.scope.module(),builder.scope.imports(),&mut BTreeSet::new(),true)).await}?);}
  DataNeed::Unresolved=>{borrowed=view::Borrow::source(view::Source::Unresolved);}
  DataNeed::Restrict=>{let Ownership::Conditional(when)=&data.ownership else{unsafe{std::hint::unreachable_unchecked()}};borrowed=borrowed.restricted_by(&when.condition().assume(&bounds::Clause::default(),true));}
  DataNeed::Union=>{into.union(&borrowed);}
  DataNeed::Complete=>return Ok(()),
  DataNeed::Next=>{},
 }}
}
async fn children(builder:&Builder<'_>,value:&OwnedValuePayload,into:&mut view::Borrow)->Result<()>{
 let mut cursor=unsafe{start()};let mut facts=ChildFacts{count:value.payloads.len()as u64,kind:Kind::Unknown,active:0};
 loop{let next=unsafe{child_step(cursor,facts)};cursor=next.cursor;match next.need{
  ChildNeed::Inspect=>{let child=value.payloads[cursor.index as usize].as_deref();facts.kind=kind(child);facts.active=u32::from(matches!(child,Some(Returned::OwnedValue(child))if child.ownership.may_own()));}
  ChildNeed::Data=>{let Some(Returned::Dataset(child))=value.payloads[cursor.index as usize].as_deref()else{unsafe{std::hint::unreachable_unchecked()}};unsafe{builder.world.call(data_contents(builder,child,into)).await}?;}
  ChildNeed::Owned=>{let Some(Returned::OwnedValue(child))=value.payloads[cursor.index as usize].as_deref()else{unsafe{std::hint::unreachable_unchecked()}};unsafe{builder.world.call(children(builder,child,into)).await}?;}
  ChildNeed::Complete=>return Ok(()),
  ChildNeed::Next=>{},
 }}
}
impl OwnedValuePayload{
 pub(in crate::check) fn cleanup_views(&self,into:&mut view::Borrow,map:&impl Fn(usize,&Ty,view::Borrow)->view::Borrow){accumulate_views(self,into,map,info(self),views);}
 pub(in crate::check) async fn cleanup_borrow(&self,builder:&Builder<'_>,into:&mut view::Borrow)->Result<()>{
  let info=info(self);let mut cursor=unsafe{start()};let mut nested=view::Borrow::default();
  loop{let next=unsafe{owner_step(cursor,info)};cursor=next.cursor;match next.need{
   OwnerNeed::Views=>{accumulate_views(self,into,&|_,_,borrow|borrow,info,views_active);}
   OwnerNeed::Children=>{unsafe{builder.world.call(children(builder,self,&mut nested)).await}?;}
   OwnerNeed::Restrict=>{let Ownership::Conditional(when)=&self.ownership else{unsafe{std::hint::unreachable_unchecked()}};nested=nested.restricted_by(&when.condition().assume(&bounds::Clause::default(),true));}
   OwnerNeed::Union=>{into.union(&nested);}
   OwnerNeed::Complete=>return Ok(()),
  }}
 }
}
