//! Call compatibility consumes the original arguments and original location owners.
use super::*;
use cooperative::Argument;
use std::ffi::c_void;
type Address=*const c_void;
#[repr(C)]
struct ArgumentInfo{exclusive:u32,value:u32,atomic:u32,declared_dataset:u32,stage_dataset:u32,reference:u32,owned_value:u32}
#[repr(C)]
struct StageInfo{present:u32,binding:u64,count:u64}
#[repr(C)]
struct StageWord{start:u32,end:u32}
#[repr(C)]
struct RegionInfo{present:u32,sources:u64,words:u64}
#[repr(C)]
struct HandleValues{values:*const ValueIdentity,count:u64}
type Equal=unsafe extern "C" fn(Address,u64,Address,u64)->u32;
type HandleQuery=unsafe extern "C" fn(HandleValues,HandleValues,Equal)->u32;
#[repr(C)]
struct AliasHost{
 argument:unsafe extern "C" fn(Address,u64)->ArgumentInfo,
 stage:unsafe extern "C" fn(Address,u64)->StageInfo,
 stage_word:unsafe extern "C" fn(Address,u64,u64)->StageWord,
 place_overlap:unsafe extern "C" fn(Address,u64,u64)->u32,
 region:unsafe extern "C" fn(Address,u64)->RegionInfo,
 source_equal:unsafe extern "C" fn(Address,u64,u64,u64,u64)->u32,
 region_word:unsafe extern "C" fn(Address,u64,u64)->u32,
 with_handles:unsafe extern "C" fn(Address,u64,u64,HandleQuery)->u32,
}
unsafe extern "C"{
 #[link_name="bootstrap.call_aliases::first_conflict"]
 fn first_conflict(context:Address,count:u64,host:AliasHost)->u64;
}
struct Context<'a,'b,P>{
 builder:&'a Builder<'b>,
 arguments:&'a[Argument],
 parameter:P,
}
pub(super) fn conflict<P:Fn(usize)->(Access,bool)>(builder:&Builder<'_>,parameter:P,arguments:&[Argument])->Option<usize>{
 let context=Context{builder,arguments,parameter};
 let index=unsafe{first_conflict((&context as *const Context<'_,'_,P>).cast(),arguments.len()as u64,AliasHost{
  argument:argument::<P>,stage:stage_info::<P>,stage_word:stage_word::<P>,place_overlap:place_overlap::<P>,
  region:region_info::<P>,source_equal:source_equal::<P>,region_word:region_word::<P>,with_handles:with_handles::<P>,
 })}as usize;
 (index<arguments.len()).then_some(index)
}
unsafe fn context<'a,'b,P>(value:Address)->&'a Context<'a,'b,P>{unsafe{&*value.cast::<Context<'a,'b,P>>()}}
unsafe extern "C" fn argument<P:Fn(usize)->(Access,bool)>(value:Address,index:u64)->ArgumentInfo{
 let cx=unsafe{context::<P>(value)};let argument=&cx.arguments[index as usize];let(access,dataset)=(cx.parameter)(index as usize);
 ArgumentInfo{exclusive:u32::from(matches!(access,Access::Write|Access::InOut)),value:u32::from(access==Access::Value),
  atomic:u32::from(access==Access::Atomic),declared_dataset:u32::from(dataset),stage_dataset:u32::from(matches!(argument,Argument::Dataset(_))),
  reference:u32::from(matches!(argument,Argument::Reference(_)|Argument::Atomic(_)|Argument::OwnedReference{..})),
  owned_value:u32::from(matches!(argument,Argument::OwnedValue(_)))}
}
struct Stage<'a>{binding:usize,offset:u32,ty:&'a Ty,words:Option<&'a[u32]>}
impl Stage<'_>{
 fn len(&self)->usize{self.words.map_or(self.ty.components.len(),|words|words.len().min(self.ty.components.len()))}
 fn word(&self,index:usize)->StageWord{
  let(scalar,field)=self.ty.components[index];
  let start=self.words.map_or(self.offset+field,|words|words[index]);StageWord{start,end:start+scalar.size()}
 }
}
fn stage<'a>(builder:&'a Builder<'_>,argument:&'a Argument)->Option<Stage<'a>>{
 match argument{
  Argument::Dataset(view)=>{let(offset,ty)=builder.dataset_layout(view);Some(Stage{binding:view.binding,offset,ty,words:None})},
  Argument::Reference(OwnedBinding{place,..})|Argument::Atomic(place)|Argument::OwnedReference{place,..}=>match place.storage(){
   Place::Element{binding,ty,words,..}=>Some(Stage{binding:*binding,offset:0,ty,words:Some(words)}),_=>None,
  },
  _=>None,
 }
}
unsafe extern "C" fn stage_info<P>(value:Address,index:u64)->StageInfo{
 let cx=unsafe{context::<P>(value)};
 stage(cx.builder,&cx.arguments[index as usize]).map_or(StageInfo{present:0,binding:0,count:0},|view|
  StageInfo{present:1,binding:view.binding as u64,count:view.len()as u64})
}
unsafe extern "C" fn stage_word<P>(value:Address,index:u64,word:u64)->StageWord{
 let cx=unsafe{context::<P>(value)};
 unsafe{stage(cx.builder,&cx.arguments[index as usize]).unwrap_unchecked()}.word(word as usize)
}
fn place<'a>(builder:&'a Builder<'_>,argument:&'a Argument)->Option<&'a Place>{
 match argument{
  Argument::Reference(owner)=>Some(&owner.place),Argument::Atomic(place)=>Some(place),
  Argument::OwnedReference{place,..}=>Some(place),Argument::OwnedValue(value)=>value.place.as_ref(),
  Argument::Scoped{owner,..}=>match builder.env.get(owner){
   Some(Binding::OwnedValue(value))=>value.place.as_ref(),Some(Binding::Owned(value))=>Some(&value.place),_=>None,
  },
  _=>None,
 }
}
unsafe extern "C" fn place_overlap<P>(value:Address,first:u64,second:u64)->u32{
 let cx=unsafe{context::<P>(value)};
 u32::from(place(cx.builder,&cx.arguments[first as usize]).zip(place(cx.builder,&cx.arguments[second as usize]))
  .is_some_and(|(a,b)|a.overlaps(b)))
}
enum Words<'a>{Layout(u32,&'a Ty),Stored(&'a[u32])}
impl Words<'_>{
 fn len(&self)->usize{match self{Self::Layout(_,ty)=>ty.components.len(),Self::Stored(words)=>words.len()}}
 fn at(&self,index:usize)->u32{match self{Self::Layout(offset,ty)=>offset+ty.components[index].1,Self::Stored(words)=>words[index]}}
}
struct Region<'a>{sources:&'a[ValueIdentity],words:Words<'a>}
fn region(argument:&Argument)->Option<Region<'_>>{
 match argument{
  Argument::BorrowedData(data)=>{let(offset,ty)=data.element_layout();Some(Region{sources:&data.sources,words:Words::Layout(offset,ty)})},
  Argument::Reference(OwnedBinding{place,..})|Argument::Atomic(place)|Argument::OwnedReference{place,..}=>match place.storage(){
   Place::ApplicationElement{sources,words,..}=>Some(Region{sources,words:Words::Stored(words)}),_=>None,
  },
  _=>None,
 }
}
unsafe extern "C" fn region_info<P>(value:Address,index:u64)->RegionInfo{
 let cx=unsafe{context::<P>(value)};
 region(&cx.arguments[index as usize]).map_or(RegionInfo{present:0,sources:0,words:0},|view|
  RegionInfo{present:1,sources:view.sources.len()as u64,words:view.words.len()as u64})
}
unsafe extern "C" fn source_equal<P>(value:Address,a:u64,i:u64,b:u64,j:u64)->u32{
 let cx=unsafe{context::<P>(value)};
 let first=unsafe{region(&cx.arguments[a as usize]).unwrap_unchecked()};
 let second=unsafe{region(&cx.arguments[b as usize]).unwrap_unchecked()};
 u32::from(first.sources[i as usize]==second.sources[j as usize])
}
unsafe extern "C" fn region_word<P>(value:Address,index:u64,word:u64)->u32{
 let cx=unsafe{context::<P>(value)};unsafe{region(&cx.arguments[index as usize]).unwrap_unchecked()}.words.at(word as usize)
}
// These are the original semantic owner-query products, not a second argument
// or resource state. B only borrows their identities during the final comparison.
fn owners(borrow:view::Borrow)->Vec<ValueIdentity>{
 borrow.owners().sources.into_iter().filter_map(|source|if let view::Source::Data(id)=source{Some(id)}else{None}).collect()
}
fn handles(argument:&Argument)->Vec<ValueIdentity>{
 match argument{
  Argument::BorrowedData(reference)=>{
   let mut handles=reference.sources.clone();handles.extend(owners(view::resource_borrow(&reference.data)));handles
  },
  Argument::ApplicationData(data)=>vec![data.root.identity()],
  Argument::Scoped{value,..}=>match value{
   returns::Returned::Dataset(data)=>vec![data.root.identity()],returns::Returned::OwnedValue(value)=>value.handles(),
   returns::Returned::Value(value)=>owners(value.proof.typed_view_borrow(&value.ty)),_=>Vec::new(),
  },
  Argument::OwnedValue(value)=>value.handles(),
  Argument::OwnedReference{value,place,..}=>{
   let mut handles=value.handles();if let Place::ApplicationElement{sources,resource,..}=place.storage(){
    handles.extend(sources);handles.extend(owners(resource.clone()));
   }handles
  },
  Argument::Value(value)=>owners(value.proof.typed_view_borrow(&value.ty)),
  Argument::Reference(owner)=>{
   let mut handles=owners(owner.proof.typed_view_borrow(owner.place.ty()));
   if let Place::ApplicationElement{sources,resource,..}=owner.place.storage(){handles.extend(sources);handles.extend(owners(resource.clone()));}
   handles
  },
  Argument::Atomic(place)=>match place.storage(){
   Place::ApplicationElement{sources,resource,..}=>{let mut handles=sources.clone();handles.extend(owners(resource.clone()));handles},
   _=>Vec::new(),
  },
  _=>Vec::new(),
 }
}
unsafe extern "C" fn equal_handles(a:Address,i:u64,b:Address,j:u64)->u32{
 u32::from(unsafe{&*a.cast::<ValueIdentity>().add(i as usize)}==unsafe{&*b.cast::<ValueIdentity>().add(j as usize)})
}
unsafe extern "C" fn with_handles<P>(value:Address,a:u64,b:u64,query:HandleQuery)->u32{
 let cx=unsafe{context::<P>(value)};
 let first=handles(&cx.arguments[a as usize]);let second=handles(&cx.arguments[b as usize]);
 unsafe{query(HandleValues{values:first.as_ptr(),count:first.len()as u64},HandleValues{values:second.as_ptr(),count:second.len()as u64},equal_handles)}
}
