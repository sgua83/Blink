//! Native fields and storage for the original Place owner.
use super::*;
use std::ffi::c_void;
type Address=*mut c_void;
fn pointer<T>(value:&T)->Address{(value as *const T).cast_mut().cast()}
#[repr(C)]
struct Node{
 kind:u32,flags:u32,first:u64,second:u64,words:u64,data:u64,owned:u64,sources:u64,
 word_values:Address,primary:Address,secondary:Address,source_values:Address,
 data_values:Address,owned_values:Address,layout:Address,ty:Address,low:u32,high:u32,
}
unsafe extern "C" fn inspect(value:Address)->Node{
 let mut n=Node{kind:0,flags:0,first:0,second:0,words:0,data:0,owned:0,sources:0,word_values:std::ptr::null_mut(),primary:std::ptr::null_mut(),secondary:std::ptr::null_mut(),source_values:std::ptr::null_mut(),data_values:std::ptr::null_mut(),owned_values:std::ptr::null_mut(),layout:std::ptr::null_mut(),ty:std::ptr::null_mut(),low:0,high:0};
 let words=match unsafe{&*value.cast::<Place>()}{
  Place::Reference{ty,address,scope,words,..}=>{n.primary=pointer(address);n.ty=pointer(ty);if let Some(scope)=scope{n.flags|=2;n.first=*scope as u64;}if let ReferenceAddress::Borrowed{borrow,layout,..}=address{n.flags|=1;n.data_values=pointer(borrow);n.layout=pointer(layout);}words},
  Place::ApplicationElement{identity,initialization,index,words,sources,index_range,element_layout,..}=>{n.kind=1;n.primary=pointer(identity);n.layout=pointer(element_layout);if let Some(value)=initialization{n.flags=4;n.secondary=pointer(value);}n.first=*index as u64;n.sources=sources.len() as u64;n.source_values=sources.as_ptr().cast_mut().cast();n.low=index_range.0;n.high=index_range.1;words},
  Place::AtomicParameter(_)=>{n.kind=2;return n;},
  Place::ViewElement{source,index,stride,words,..}=>{n.kind=3;n.primary=pointer(source);n.secondary=pointer(stride);n.first=*index as u64;words},
  Place::TileElement{tile,index,words,..}=>{n.kind=4;n.first=*tile as u64;n.second=*index as u64;words},
  Place::Local(_,slots,..)|Place::ReadOnlyLocal(_,slots,..)=>{n.kind=if matches!(unsafe{&*value.cast::<Place>()},Place::Local(..)){5}else{6};n.flags=8;n.words=slots.len() as u64;n.word_values=slots.as_ptr().cast_mut().cast();return n;},
  Place::Indexed{base,offset,words,locations,initialization,..}=>{n.kind=7;n.primary=pointer(base.as_ref());n.first=*offset as u64;n.data=locations.len() as u64;n.owned=initialization.len() as u64;n.data_values=pointer(locations);n.owned_values=pointer(initialization);words},
  Place::Element{binding,index,words,..}=>{n.kind=8;n.first=*binding as u64;n.second=*index as u64;words},
 };
 n.words=words.len() as u64;n.word_values=words.as_ptr().cast_mut().cast();n
}
unsafe extern "C" fn word(value:Address,at:u64,wide:u32)->u64{if wide==0{u64::from(unsafe{*value.cast::<u32>().add(at as usize)})}else{(unsafe{*value.cast::<usize>().add(at as usize)}) as u64}}
unsafe extern "C" fn equal(kind:u32,a:Address,b:Address)->u32{
 u32::from(match kind{
  0=>unsafe{*a.cast::<ReferenceAddress>()==*b.cast::<ReferenceAddress>()},
  1=>unsafe{*a.cast::<ValueIdentity>()==*b.cast::<ValueIdentity>()},
  2=>unsafe{*a.cast::<(Position,ownership::ApplicationIndex)>()==*b.cast::<(Position,ownership::ApplicationIndex)>()},
  3=>unsafe{*a.cast::<[ValueIdentity;3]>()==*b.cast::<[ValueIdentity;3]>()},
  4=>unsafe{*a.cast::<ElementStride>()==*b.cast::<ElementStride>()},
  _=>unsafe{*a.cast::<Place>()==*b.cast::<Place>()},
 })
}
unsafe extern "C" fn source_equal(a:Address,i:u64,b:Address,j:u64)->u32{u32::from(unsafe{*a.cast::<ValueIdentity>().add(i as usize)==*b.cast::<ValueIdentity>().add(j as usize)})}
unsafe extern "C" fn reference_overlap(a:Node,b:Node)->u32{
 let a_address=unsafe{&*a.primary.cast::<ReferenceAddress>()};let b_address=unsafe{&*b.primary.cast::<ReferenceAddress>()};
 let a_ty=unsafe{&*a.ty.cast::<Ty>()};let b_ty=unsafe{&*b.ty.cast::<Ty>()};
 let a_words=unsafe{std::slice::from_raw_parts(a.word_values.cast::<u32>(),a.words as usize)};let b_words=unsafe{std::slice::from_raw_parts(b.word_values.cast::<u32>(),b.words as usize)};
 u32::from(a_address.overlaps(a_ty,a_words,b_address,b_ty,b_words))
}
unsafe extern "C" fn storage(value:Address)->Address{pointer(unsafe{&*value.cast::<Place>()}.storage())}
unsafe extern "C" fn append_component(output:Address,at:u64){unsafe{&mut *output.cast::<Vec<usize>>()}.push(at as usize);}
unsafe extern "C" fn append_element(output:Address,binding:u64,word:u32){unsafe{&mut *output.cast::<Vec<Position>>()}.push(Position::Element(binding as usize,word));}
unsafe extern "C" fn append_view(output:Address,source:Address,index:u64,stride:Address,word:u32){unsafe{&mut *output.cast::<Vec<Position>>()}.push(Position::ViewElement(unsafe{*source.cast::<[ValueIdentity;3]>()},index as Reg,unsafe{*stride.cast::<ElementStride>()},word));}
unsafe extern "C" fn append_tile(output:Address,tile:u64,index:u64,word:u32){unsafe{&mut *output.cast::<Vec<Position>>()}.push(Position::TileElement(tile as usize,index as Reg,word));}
unsafe extern "C" fn append_existing(output:Address,values:Address,at:u64){let value=unsafe{&*values.cast::<Vec<Position>>()}[at as usize].clone();unsafe{&mut *output.cast::<Vec<Position>>()}.push(value);}
unsafe extern "C" fn append_application(output:Address,value:Address,word:u32){let(scope,index)=unsafe{&*value.cast::<(Position,ownership::ApplicationIndex)>()};let value=scope.application_word(Some(*index),word);unsafe{&mut *output.cast::<Vec<Position>>()}.push(value);}
unsafe extern "C" fn append_parameter(output:Address,scope:u64,word:u32){unsafe{&mut *output.cast::<Vec<Position>>()}.push(Position::Parameter(scope as usize,word as usize));}
unsafe extern "C" fn append_local(output:Address,slot:u64){unsafe{&mut *output.cast::<Vec<Position>>()}.push(Position::Local(slot as usize));}
type Scoped=unsafe extern "C" fn(Address,Address,Host)->u32;
unsafe extern "C" fn borrow_scope(identity:Address,context:Address,host:Host,apply:Scoped)->u32{
 let value=Arc::new(view::Borrow::source(view::Source::Data(unsafe{*identity.cast::<ValueIdentity>()})));unsafe{apply(pointer(&value),context,host)}
}
unsafe extern "C" fn append_borrowed(output:Address,borrow:Address,layout:Address,word:u32){let value=Position::Borrowed(unsafe{&*borrow.cast::<Arc<view::Borrow>>()}.clone(),unsafe{&*layout.cast::<address::Layout>()}.clone(),word);unsafe{&mut *output.cast::<Vec<Position>>()}.push(value);}
unsafe extern "C" fn reference_positions(output:Address,borrow:Address,layout:Address,words:Address,count:u64){
 let borrow=unsafe{&*borrow.cast::<view::Borrow>()};let layout=unsafe{&*layout.cast::<address::Layout>()};let words=unsafe{std::slice::from_raw_parts(words.cast::<u32>(),count as usize)};
 unsafe{*output.cast::<Vec<Position>>()=address::borrowed_positions(borrow,layout,words)}
}
unsafe extern "C" fn owned_scope(place:Address,context:Address,host:Host,apply:Scoped)->u32{match owned(unsafe{&*place.cast::<Place>()}){Some(values)=>unsafe{apply(pointer(&values),context,host)},None=>0}}
unsafe extern "C" fn position_count(values:Address)->u64{unsafe{&*values.cast::<Vec<Position>>()}.len() as u64}
unsafe extern "C" fn position_equal(a:Address,i:u64,b:Address,j:u64)->u32{u32::from(unsafe{&*a.cast::<Vec<Position>>()}[i as usize]==unsafe{&*b.cast::<Vec<Position>>()}[j as usize])}
#[repr(C)]
#[derive(Clone,Copy)]
struct Host{
 inspect:unsafe extern "C" fn(Address)->Node,
 word:unsafe extern "C" fn(Address,u64,u32)->u64,
 equal:unsafe extern "C" fn(u32,Address,Address)->u32,
 source_equal:unsafe extern "C" fn(Address,u64,Address,u64)->u32,
 reference_overlap:unsafe extern "C" fn(Node,Node)->u32,
 storage:unsafe extern "C" fn(Address)->Address,
 append_component:unsafe extern "C" fn(Address,u64),
 append_element:unsafe extern "C" fn(Address,u64,u32),
 append_view:unsafe extern "C" fn(Address,Address,u64,Address,u32),
 append_tile:unsafe extern "C" fn(Address,u64,u64,u32),
 append_existing:unsafe extern "C" fn(Address,Address,u64),
 append_application:unsafe extern "C" fn(Address,Address,u32),
 append_parameter:unsafe extern "C" fn(Address,u64,u32),
 append_local:unsafe extern "C" fn(Address,u64),
 borrow_scope:unsafe extern "C" fn(Address,Address,Host,Scoped)->u32,
 append_borrowed:unsafe extern "C" fn(Address,Address,Address,u32),
 reference_positions:unsafe extern "C" fn(Address,Address,Address,Address,u64),
 owned_scope:unsafe extern "C" fn(Address,Address,Host,Scoped)->u32,
 position_count:unsafe extern "C" fn(Address)->u64,
 position_equal:unsafe extern "C" fn(Address,u64,Address,u64)->u32,
}
const HOST:Host=Host{inspect,word,equal,source_equal,reference_overlap,storage,append_component,append_element,append_view,append_tile,append_existing,append_application,append_parameter,append_local,borrow_scope,append_borrowed,reference_positions,owned_scope,position_count,position_equal};
unsafe extern "C"{
 #[link_name="bootstrap.ownership_place::has_owned"]fn source_has_owned(value:Address,host:Host)->u32;
 #[link_name="bootstrap.ownership_place::positions"]fn source_positions(output:Address,value:Address,owned:u32,host:Host)->u32;
 #[link_name="bootstrap.ownership_place::components"]fn source_components(output:Address,value:Address,owner:Address,host:Host)->u32;
 #[link_name="bootstrap.ownership_place::overlaps"]fn source_overlaps(a:Address,b:Address,host:Host)->u32;
}
pub(super) fn has_owned(value:&Place)->bool{unsafe{source_has_owned(pointer(value),HOST)!=0}}
pub(super) fn data(value:&Place)->Vec<Position>{let mut output=Vec::new();unsafe{source_positions(pointer(&mut output),pointer(value),0,HOST)};output}
pub(super) fn owned(value:&Place)->Option<Vec<Position>>{let mut output=Vec::new();if unsafe{source_positions(pointer(&mut output),pointer(value),1,HOST)}!=0{Some(output)}else{None}}
pub(super) fn components(value:&Place,owner:&Place)->Option<Vec<usize>>{let mut output=Vec::new();if unsafe{source_components(pointer(&mut output),pointer(value),pointer(owner),HOST)}!=0{Some(output)}else{None}}
pub(super) fn overlaps(a:&Place,b:&Place)->bool{unsafe{source_overlaps(pointer(a),pointer(b),HOST)!=0}}
