//! The original Position, Arc, set and vector containers remain native owners.
use super::*;
use std::{ffi::c_void, collections::btree_set};
type Address = *mut c_void;
#[repr(C)]
#[derive(Clone,Copy)]
struct Index { kind:u32, value:u32, identity:Address, reg:u64 }
#[repr(C)]
#[derive(Clone,Copy)]
struct Node { kind:u32, owner:u64, member:u64, word:u64, index:Index, condition:Address, count:u64 }
#[repr(C)]
struct Item { node:Address, present:u32 }
fn pointer<T>(value:&T)->Address {(value as *const T).cast_mut().cast()}
fn index(value:Option<&ApplicationIndex>)->Index {
    match value {
        None=>Index{kind:0,value:0,identity:std::ptr::null_mut(),reg:0},
        Some(ApplicationIndex::Constant(value))=>Index{kind:1,value:*value,identity:std::ptr::null_mut(),reg:0},
        Some(ApplicationIndex::Value(identity,reg))=>Index{kind:2,value:0,identity:pointer(identity),reg:*reg as u64},
    }
}
unsafe fn required_index(value:Index)->ApplicationIndex {
    if value.kind==1 {ApplicationIndex::Constant(value.value)}else{ApplicationIndex::Value(unsafe{*value.identity.cast::<ValueIdentity>()},value.reg as Reg)}
}
unsafe fn actual_index(value:Index)->Option<ApplicationIndex> {if value.kind==0{None}else{Some(unsafe{required_index(value)})}}
unsafe extern "C" fn inspect(value:Address)->Node {
    let mut result=Node{kind:0,owner:0,member:0,word:0,index:index(None),condition:std::ptr::null_mut(),count:0};
    match unsafe{&*value.cast::<Position>()} {
        Position::Initialized=>{},
        Position::Local(_)=>result.kind=1,
        Position::Borrowed(..)=>result.kind=2,
        Position::Initialization(_)=>result.kind=3,
        Position::Element(..)=>result.kind=4,
        Position::ViewElement(..)=>result.kind=5,
        Position::TileElement(..)=>result.kind=6,
        Position::Parameter(owner,word)=>{result.kind=7;result.owner=*owner as u64;result.word=*word as u64;},
        Position::ApplicationElement(owner,at,word)=>{result.kind=8;result.owner=*owner as u64;result.index=index(Some(at));result.word=u64::from(*word);},
        Position::ApplicationInput(input,at,word)=>{result.kind=9;result.owner=input.level as u64;result.member=input.index as u64;result.index=index(at.as_ref());result.word=u64::from(*word);},
        Position::Indexed(choices,at)=>{result.kind=10;result.index=index(Some(at));result.count=choices.len() as u64;},
        Position::Conditional(when,_,_)=>{result.kind=11;result.condition=pointer(when.as_ref());result.count=2;},
        Position::Both(..)=>{result.kind=12;result.count=2;},
    }
    result
}
unsafe extern "C" fn child(value:Address,at:u64)->Address {
    match unsafe{&*value.cast::<Position>()} {
        Position::Both(a,b)|Position::Conditional(_,a,b)=>pointer(if at==0{a.as_ref()}else{b.as_ref()}),
        Position::Indexed(choices,_)=>pointer(&choices[at as usize]),
        _=>std::ptr::null_mut(),
    }
}
unsafe extern "C" fn equal(a:Address,b:Address)->u32 {u32::from(unsafe{&*a.cast::<Position>()==&*b.cast::<Position>()})}
unsafe extern "C" fn identity_equal(a:Address,b:Address)->u32 {u32::from(unsafe{*a.cast::<ValueIdentity>()==*b.cast::<ValueIdentity>()})}
unsafe extern "C" fn presence_kind(value:Address)->u32 {
    match unsafe{&*value.cast::<owned_presence::Presence>()} {owned_presence::Presence::All=>1,owned_presence::Presence::None=>0,_=>2}
}
unsafe extern "C" fn presence_equal(a:Address,b:Address)->u32 {u32::from(unsafe{&*a.cast::<owned_presence::Presence>()==&*b.cast::<owned_presence::Presence>()})}
unsafe extern "C" fn presence_uses_index(value:Address,at:Index)->u32 {
    u32::from(unsafe{&*value.cast::<owned_presence::Presence>()}.uses_index(unsafe{required_index(at)}))
}
unsafe extern "C" fn contains(set:Address,value:Address)->u32 {u32::from(unsafe{&*set.cast::<BTreeSet<Position>>()}.contains(unsafe{&*value.cast::<Position>()}))}
// These are scalar keys for the original BTreeSet, never projected trees.
unsafe extern "C" fn contains_parameter(set:Address,owner:u64,word:u64)->u32 {u32::from(unsafe{&*set.cast::<BTreeSet<Position>>()}.contains(&Position::Parameter(owner as usize,word as usize)))}
unsafe extern "C" fn contains_element(set:Address,owner:u64,at:Index,word:u32)->u32 {u32::from(unsafe{&*set.cast::<BTreeSet<Position>>()}.contains(&Position::ApplicationElement(owner as usize,unsafe{required_index(at)},word)))}
unsafe extern "C" fn contains_input(set:Address,owner:u64,member:u64,at:Index,word:u32)->u32 {u32::from(unsafe{&*set.cast::<BTreeSet<Position>>()}.contains(&Position::ApplicationInput(view::LoopInput{level:owner as usize,index:member as usize},unsafe{actual_index(at)},word)))}
enum Iter { One(btree_set::Iter<'static,Position>), Union(btree_set::Union<'static,Position>) }
unsafe extern "C" fn iterate(a:Address,b:Address)->Address {
    // The B traversal borrows the actual input sets and drops this iterator
    // before its synchronous caller returns or any owning set can be changed.
    let a:&'static BTreeSet<Position>=unsafe{&*a.cast::<BTreeSet<Position>>()};
    let iter=if b.is_null(){Iter::One(a.iter())}else{let b:&'static BTreeSet<Position>=unsafe{&*b.cast::<BTreeSet<Position>>()};Iter::Union(a.union(b))};
    Box::into_raw(Box::new(iter)).cast()
}
unsafe extern "C" fn next(value:Address)->Item {
    let item=match unsafe{&mut *value.cast::<Iter>()}{Iter::One(iter)=>iter.next(),Iter::Union(iter)=>iter.next()};
    match item{Some(value)=>Item{node:pointer(value),present:1},None=>Item{node:std::ptr::null_mut(),present:0}}
}
unsafe extern "C" fn drop_iterator(value:Address){drop(unsafe{Box::from_raw(value.cast::<Iter>())});}
unsafe extern "C" fn fresh_vector()->Address {Box::into_raw(Box::new(Vec::<Position>::new())).cast()}
unsafe extern "C" fn push_vector(vector:Address,value:Address) {unsafe{&mut *vector.cast::<Vec<Position>>()}.push(*unsafe{Box::from_raw(value.cast::<Position>())});}
unsafe extern "C" fn vector_count(value:Address)->u64 {unsafe{&*value.cast::<Vec<Position>>()}.len() as u64}
unsafe extern "C" fn vector_at(value:Address,at:u64)->Address {pointer(&unsafe{&*value.cast::<Vec<Position>>()}[at as usize])}
unsafe extern "C" fn make_position(shape:Node,a:Address,b:Address,choices:Address)->Address {
    let value=match shape.kind {
        0=>Position::Initialized,
        7=>Position::Parameter(shape.owner as usize,shape.word as usize),
        8=>Position::ApplicationElement(shape.owner as usize,unsafe{required_index(shape.index)},shape.word as u32),
        9=>Position::ApplicationInput(view::LoopInput{level:shape.owner as usize,index:shape.member as usize},unsafe{actual_index(shape.index)},shape.word as u32),
        10=>Position::Indexed((*unsafe{Box::from_raw(choices.cast::<Vec<Position>>())}).into(),unsafe{required_index(shape.index)}),
        11=>Position::Conditional(Arc::new(unsafe{&*shape.condition.cast::<owned_presence::Presence>()}.clone()),Arc::new(*unsafe{Box::from_raw(a.cast::<Position>())}),Arc::new(*unsafe{Box::from_raw(b.cast::<Position>())})),
        12=>Position::Both(Arc::new(*unsafe{Box::from_raw(a.cast::<Position>())}),Arc::new(*unsafe{Box::from_raw(b.cast::<Position>())})),
        // This is the original application_word unreachable shape, with the
        // same private caller contract; B chooses the actual output kind.
        _=>unreachable!(),
    };
    Box::into_raw(Box::new(value)).cast()
}
unsafe extern "C" fn clone_position(value:Address)->Address {Box::into_raw(Box::new(unsafe{&*value.cast::<Position>()}.clone())).cast()}
unsafe extern "C" fn fresh_set()->Address {Box::into_raw(Box::new(BTreeSet::<Position>::new())).cast()}
unsafe extern "C" fn insert_set(set:Address,value:Address) {unsafe{&mut *set.cast::<BTreeSet<Position>>()}.insert(unsafe{&*value.cast::<Position>()}.clone());}
#[repr(C)]
#[derive(Clone,Copy)]
struct Host {
    inspect:unsafe extern "C" fn(Address)->Node,
    child:unsafe extern "C" fn(Address,u64)->Address,
    equal:unsafe extern "C" fn(Address,Address)->u32,
    identity_equal:unsafe extern "C" fn(Address,Address)->u32,
    presence_kind:unsafe extern "C" fn(Address)->u32,
    presence_equal:unsafe extern "C" fn(Address,Address)->u32,
    presence_uses_index:unsafe extern "C" fn(Address,Index)->u32,
    contains:unsafe extern "C" fn(Address,Address)->u32,
    contains_parameter:unsafe extern "C" fn(Address,u64,u64)->u32,
    contains_element:unsafe extern "C" fn(Address,u64,Index,u32)->u32,
    contains_input:unsafe extern "C" fn(Address,u64,u64,Index,u32)->u32,
    iterate:unsafe extern "C" fn(Address,Address)->Address,
    next:unsafe extern "C" fn(Address)->Item,
    drop_iterator:unsafe extern "C" fn(Address),
    fresh_vector:unsafe extern "C" fn()->Address,
    push_vector:unsafe extern "C" fn(Address,Address),
    vector_count:unsafe extern "C" fn(Address)->u64,
    vector_at:unsafe extern "C" fn(Address,u64)->Address,
    make_position:unsafe extern "C" fn(Node,Address,Address,Address)->Address,
    clone_position:unsafe extern "C" fn(Address)->Address,
    fresh_set:unsafe extern "C" fn()->Address,
    insert_set:unsafe extern "C" fn(Address,Address),
}
const HOST:Host=Host{inspect,child,equal,identity_equal,presence_kind,presence_equal,presence_uses_index,contains,contains_parameter,contains_element,contains_input,iterate,next,drop_iterator,fresh_vector,push_vector,vector_count,vector_at,make_position,clone_position,fresh_set,insert_set};
unsafe extern "C" {
    #[link_name="bootstrap.ownership_position::select"] fn source_select(when:Address,a:Address,b:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_position::both"] fn source_both(a:Address,b:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_position::index"] fn source_index(values:Address,index:Index,width:u64,host:Host)->Address;
    #[link_name="bootstrap.ownership_position::word"] fn source_word(value:Address,index:Index,word:u32,host:Host)->Address;
    #[link_name="bootstrap.ownership_position::ready"] fn source_ready(value:Address,known:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_position::prefix_ready"] fn source_prefix(value:Address,length:u32,word:u32,known:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_position::uses_scope"] fn source_scope(value:Address,owner:u64,host:Host)->u32;
    #[link_name="bootstrap.ownership_position::depends_index"] fn source_depends(value:Address,index:Index,host:Host)->u32;
    #[link_name="bootstrap.ownership_position::intersect"] fn source_intersect(a:Address,b:Address,host:Host)->Address;
}
pub(super) fn selected(when:owned_presence::Presence,a:Position,b:Position)->Position {
    match unsafe{source_select(pointer(&when),pointer(&a),pointer(&b),HOST)} {0=>a,1=>b,_=>Position::Conditional(Arc::new(when),Arc::new(a),Arc::new(b))}
}
pub(super) fn both(a:Position,b:Position)->Position {match unsafe{source_both(pointer(&a),pointer(&b),HOST)} {0=>a,1=>b,_=>Position::Both(Arc::new(a),Arc::new(b))}}
pub(super) fn indexed(values:Vec<Position>,at:ApplicationIndex,width:usize)->Vec<Position> {*unsafe{Box::from_raw(source_index(pointer(&values),index(Some(&at)),width as u64,HOST).cast::<Vec<Position>>())}}
pub(super) fn word(value:&Position,at:Option<ApplicationIndex>,word:u32)->Position {*unsafe{Box::from_raw(source_word(pointer(value),index(at.as_ref()),word,HOST).cast::<Position>())}}
pub(super) fn initialized(value:&Position,known:&BTreeSet<Position>)->bool {unsafe{source_ready(pointer(value),pointer(known),HOST)!=0}}
pub(super) fn prefix(value:&Position,length:u32,word:u32,known:&BTreeSet<Position>)->bool {unsafe{source_prefix(pointer(value),length,word,pointer(known),HOST)!=0}}
pub(super) fn scope(value:&Position,owner:usize)->bool {unsafe{source_scope(pointer(value),owner as u64,HOST)!=0}}
pub(super) fn depends(value:&Position,at:ApplicationIndex)->bool {unsafe{source_depends(pointer(value),index(Some(&at)),HOST)!=0}}
pub(super) fn intersect(a:&BTreeSet<Position>,b:&BTreeSet<Position>)->BTreeSet<Position> {*unsafe{Box::from_raw(source_intersect(pointer(a),pointer(b),HOST).cast::<BTreeSet<Position>>())}}
