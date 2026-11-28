//! Synchronous source queries borrow the original sealed syntax, env and Ty.
use super::*;
use crate::check::owned_values::{self, InitialWrite, LoopTarget};
use std::{ffi::c_void,ptr};
type Address=*const c_void;
#[derive(Clone,Copy)]
#[repr(C)]struct SyntaxSource{nodes:Address,source:Address,stored:Address,expression:crate::syntax::wire::ExprRef}
#[derive(Clone,Copy)]
#[repr(C)]struct Bytes{data:*const u8,count:u64}
impl Bytes{unsafe fn text<'a>(self)->&'a str{unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.data,self.count as usize))}}}

#[repr(u32)]
#[allow(dead_code)] // Produced by the B syntax query.
enum PlaceRootKind { Other, Named, Address, Temporary }

#[repr(C)]
struct RawPlaceRoot { name: Bytes, kind: PlaceRootKind }

#[derive(Clone, Copy)]
pub(in crate::check) enum PlaceRoot<'a> {
    Other,
    Name(&'a str),
    Address,
    Temporary,
}

impl<'a> PlaceRoot<'a> {
    pub(in crate::check) fn name(self) -> Option<&'a str> {
        match self { Self::Name(name) => Some(name), _ => None }
    }

    pub(in crate::check) fn is_address(self) -> bool { matches!(self, Self::Address) }
}

#[derive(Clone,Copy)]
#[repr(C)]struct TypeInfo{array:Address,length:u64,fields:u64,components:u64}
#[derive(Clone,Copy)]
#[repr(C)]struct Field{name:Bytes,offset:u32,ty:Address}
#[derive(Clone,Copy)]
#[repr(C)]struct Part{offset:u32,ty:Address}
#[derive(Clone,Copy,Default)]
#[repr(C)]struct KnownIndex{present:u32,value:u32}
#[derive(Clone,Copy)]
#[repr(C)]struct Projection{ty:Address,root:crate::syntax::wire::Text,offset:u32}
#[derive(Clone,Copy)]
#[repr(C)]struct Host{
 root:unsafe extern "C" fn(Address,Bytes)->Address,
 index:unsafe extern "C" fn(Address,Bytes)->KnownIndex,
 ty:unsafe extern "C" fn(Address)->TypeInfo,
 field:unsafe extern "C" fn(Address,u64)->Field,
 element:unsafe extern "C" fn(Address,u64)->Part,
 selected_field:unsafe extern "C" fn(Address,Bytes)->Address,
 word:unsafe extern "C" fn(Address,u64)->u32,
 push:unsafe extern "C" fn(*mut c_void,u64),
}
unsafe extern "C"{
 #[link_name="bootstrap.ownership_projection::owned"]
 fn b_owned(source:SyntaxSource,env:Address,path:*mut c_void,host:Host)->Projection;
 #[link_name="bootstrap.ownership_projection::owned_type"]
 fn b_type(source:SyntaxSource,env:Address,host:Host)->Address;
 #[link_name="bootstrap.ownership_projection::loop_projection"]
 fn b_loop_projection(source: SyntaxSource, context: *mut c_void, host: Host) -> LoopSelection;
 #[link_name="bootstrap.ownership_projection::place_root"]
 fn b_place_root(source: SyntaxSource) -> RawPlaceRoot;
 #[link_name="bootstrap.ownership_projection::field_layout"]
 fn b_field_layout(root: Address, fields: *const usize, count: u64, host: Host) -> Part;
 #[link_name="bootstrap.ownership_projection::components"]
 fn b_components(root:Address,selected:Address,offset:u32,output:*mut c_void,host:Host);
}
fn address<T>(value:&T)->Address{ptr::from_ref(value).cast()}
fn source(expr:&Expr)->SyntaxSource{
 let node=expr.signature_node();
 SyntaxSource{nodes:node.storage.nodes.cast_const(),source:node.storage.source.cast_const(),
  stored:node.storage.text.cast_const(),expression:crate::syntax::wire::ExprRef{offset:node.offset}}
}
unsafe extern "C" fn root(env:Address,name:Bytes)->Address{
 match unsafe{&*env.cast::<BTreeMap<String,Binding>>()}.get(unsafe{name.text()}){
  Some(Binding::OwnedValue(value))=>address(&value.value.ty),_=>ptr::null(),
 }
}
unsafe extern "C" fn known_index(env:Address,name:Bytes)->KnownIndex{
 let known=match unsafe{&*env.cast::<BTreeMap<String,Binding>>()}.get(unsafe{name.text()}){
  Some(Binding::Value(value))if value.ty==Ty::scalar(Scalar::U32)=>value.constant.as_ref().and_then(|v|v.first().copied()).map(|v|v as u32),
  Some(Binding::Owned(value))if value.place.ty()==&Ty::scalar(Scalar::U32)=>value.proof.known_value(value.place.ty()).and_then(|v|v.first().copied()).map(|v|v as u32),
  _=>None,
 };
 known.map_or_else(KnownIndex::default,|value|KnownIndex{present:1,value})
}
unsafe extern "C" fn type_info(value:Address)->TypeInfo{
 let ty=unsafe{&*value.cast::<Ty>()};
 TypeInfo{array:ty.array.as_deref().map_or(ptr::null(),address),length:if ty.array.is_some(){ty.aggregate_len()as u64}else{0},
  fields:ty.fields.len()as u64,components:ty.components.len()as u64}
}
unsafe extern "C" fn field(value:Address,index:u64)->Field{
 let(name,offset,ty)=&unsafe{&*value.cast::<Ty>()}.fields[index as usize];
 Field{name:Bytes{data:name.as_ptr(),count:name.len()as u64},offset:*offset,ty:address(ty.as_ref())}
}
unsafe extern "C" fn element(value:Address,index:u64)->Part{
 let(offset,ty)=unsafe{&*value.cast::<Ty>()}.aggregate_part(index as usize);
 Part{offset,ty:address(ty)}
}
unsafe extern "C" fn selected_field(value:Address,name:Bytes)->Address{
 unsafe{&*value.cast::<Ty>()}.field(unsafe{name.text()}).map_or(ptr::null(),|(_,ty)|address(ty))
}
unsafe extern "C" fn word(value:Address,index:u64)->u32{unsafe{&*value.cast::<Ty>()}.components[index as usize].1}
unsafe extern "C" fn push(output:*mut c_void,value:u64){unsafe{&mut *output.cast::<Vec<usize>>()}.push(value as usize);}
const HOST:Host=Host{root,index:known_index,ty:type_info,field,element,selected_field,word,push};
pub(super) fn owned<'a>(builder:&'a Builder<'_>,expr:&Expr)->Option<(OwnedProjection,&'a Ty)>{
 let mut path=Vec::new();
 let result=unsafe{b_owned(source(expr),address(&builder.env),ptr::from_mut(&mut path).cast(),HOST)};
 // Every returned Ty is a borrow from this unchanged env and its actual fields.
 let ty=unsafe{result.ty.cast::<Ty>().as_ref()}?;
 let root=expr.source_view().text(result.root).to_owned();
 Some((OwnedProjection{root,path,offset:result.offset},ty))
}
pub(super) fn ty<'a>(builder:&'a Builder<'_>,expr:&Expr)->Option<&'a Ty>{
 unsafe{b_type(source(expr),address(&builder.env),HOST).cast::<Ty>().as_ref()}
}

#[repr(u32)]
#[allow(dead_code)] // Produced by bootstrap.ownership_projection.
enum LoopPlace { Missing, Root, Child }

#[repr(C)]
struct LoopSelection { ty: Address, place: LoopPlace }

struct LoopProjection<'a> {
    bindings: &'a BTreeMap<String, Binding>,
    root: Option<&'a Binding>,
    selected: Option<LoopTarget<'a>>,
}

unsafe extern "C" fn loop_root(context: Address, name: Bytes) -> Address {
    let probe = unsafe { &mut *context.cast_mut().cast::<LoopProjection<'_>>() };
    probe.root = probe.bindings.get(unsafe { name.text() });
    match probe.root {
        Some(Binding::OwnedValue(owner)) => {
            probe.selected = Some(LoopTarget::Aggregate(owner));
            address(&owner.value.ty)
        }
        _ => ptr::null(),
    }
}

unsafe extern "C" fn loop_index(context: Address, name: Bytes) -> KnownIndex {
    let probe = unsafe { &*context.cast::<LoopProjection<'_>>() };
    unsafe { known_index(address(probe.bindings), name) }
}

unsafe extern "C" fn loop_payload(context: *mut c_void, index: u64) {
    let probe = unsafe { &mut *context.cast::<LoopProjection<'_>>() };
    let owner = match probe.selected.take() {
        Some(LoopTarget::Aggregate(owner)) => Some(owner),
        Some(LoopTarget::Projected(Returned::OwnedValue(owner))) => Some(owner.as_ref()),
        _ => None,
    };
    // A known prefix may still select an ordinary field or precede a dynamic index.
    probe.selected = owner.and_then(|owner| owner.payloads.get(index as usize))
        .and_then(Option::as_deref).map(LoopTarget::Projected);
}

const LOOP_HOST: Host = Host { root: loop_root, index: loop_index, push: loop_payload, ..HOST };

pub(in crate::check) fn initial_loop_write(builder: &Builder<'_>, expr: &Expr, shape: bool) -> InitialWrite {
    let mut probe = LoopProjection { bindings: &builder.env, root: None, selected: None };
    let selection = unsafe { b_loop_projection(source(expr), ptr::from_mut(&mut probe).cast(), LOOP_HOST) };
    if let Some(ty) = unsafe { selection.ty.cast::<Ty>().as_ref() } {
        if !ty.owns_resources() {
            return InitialWrite::Ready;
        }
        if matches!(selection.place, LoopPlace::Child) {
            return owned_values::initial_write(builder, probe.selected.unwrap(), shape);
        }
    }
    let root = match probe.root {
        Some(Binding::OwnedValue(owner)) => LoopTarget::Aggregate(owner),
        Some(Binding::ApplicationData(data)) => LoopTarget::Dataset(data),
        _ => return InitialWrite::Ready,
    };
    if matches!(selection.place, LoopPlace::Root) {
        return owned_values::initial_write(builder, root, shape);
    }
    if shape && owned_values::initial_root_incomplete(root) {
        InitialWrite::IncompleteShape
    } else {
        InitialWrite::Ready
    }
}

pub(in crate::check) fn place_root(expr: &Expr) -> PlaceRoot<'_> {
    let root = unsafe { b_place_root(source(expr)) };
    match root.kind {
        PlaceRootKind::Other => PlaceRoot::Other,
        // The name borrows the sealed syntax held by expr.
        PlaceRootKind::Named => PlaceRoot::Name(unsafe { root.name.text() }),
        PlaceRootKind::Address => PlaceRoot::Address,
        PlaceRootKind::Temporary => PlaceRoot::Temporary,
    }
}

pub(in crate::check) fn root_name(expr: &Expr) -> Option<&str> { place_root(expr).name() }
pub(in crate::check) fn field_layout<'a>(root: &'a Ty, fields: &[usize]) -> (u32, &'a Ty) {
    let selected = unsafe { b_field_layout(address(root), fields.as_ptr(), fields.len() as u64, HOST) };
    // The checked field path selects a Ty borrowed from the original layout.
    (selected.offset, unsafe { &*selected.ty.cast::<Ty>() })
}
pub(in crate::check) fn components(root:&Ty,selected:&Ty,offset:u32)->Vec<usize>{
 let mut output=Vec::new();unsafe{b_components(address(root),address(selected),offset,ptr::from_mut(&mut output).cast(),HOST)};output
}
pub(in crate::check) fn path_components(root: &Ty, fields: &[usize]) -> Vec<usize> {
    if fields.is_empty() {
        return (0..root.components.len()).collect();
    }
    let (offset, selected) = field_layout(root, fields);
    components(root, selected, offset)
}
