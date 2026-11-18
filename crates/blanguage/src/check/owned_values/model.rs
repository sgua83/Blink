//! B traverses the original owned payloads; native leaves retain their containers.
use super::*;
use std::ffi::c_void;
type Address=*mut c_void;
fn pointer<T>(value:&T)->Address{(value as *const T).cast_mut().cast()}
fn pointer_mut<T>(value:&mut T)->Address{(value as *mut T).cast()}
#[repr(C)]
#[derive(Clone,Copy)]
struct Info{count:u64,flags:u32}
#[repr(C)]
struct Item{node:Address,kind:u32}
#[repr(C)]
#[derive(Clone,Copy)]
struct Field{ty:Address,offset:u32,owns:u32}
#[repr(C)]
#[derive(Clone,Copy)]
struct Indices{values:Address,count:u64}
#[repr(C)]
struct Count{known:u32,value:u32}
fn indices(values:&[usize])->Indices{Indices{values:values.as_ptr().cast_mut().cast(),count:values.len() as u64}}
unsafe fn slice<'a>(values:Indices)->&'a [usize]{unsafe{std::slice::from_raw_parts(values.values.cast::<usize>(),values.count as usize)}}
unsafe fn choice<'a>(value:Address)->Option<&'a bounds::Condition>{unsafe{value.cast::<bounds::Condition>().as_ref()}}
unsafe extern "C" fn inspect(value:Address)->Info{
    let value=unsafe{&*value.cast::<OwnedValuePayload>()};
    Info{count:value.payloads.len() as u64,flags:u32::from(value.value.ty.has_variants())|u32::from(value.value.ty.has_owned_presence())*2}
}
unsafe extern "C" fn value(owner:Address)->Address{pointer(&unsafe{&*owner.cast::<OwnedValuePayload>()}.value)}
unsafe extern "C" fn ownership(owner:Address)->Address{pointer(&unsafe{&*owner.cast::<OwnedValuePayload>()}.ownership)}
unsafe extern "C" fn value_mut(owner:Address)->Address{pointer_mut(&mut unsafe{&mut *owner.cast::<OwnedValuePayload>()}.value)}
unsafe extern "C" fn ownership_mut(owner:Address)->Address{pointer_mut(&mut unsafe{&mut *owner.cast::<OwnedValuePayload>()}.ownership)}
unsafe extern "C" fn child(owner:Address,at:u64)->Item{
    match unsafe{&*owner.cast::<OwnedValuePayload>()}.payloads[at as usize].as_deref(){
        None=>Item{node:std::ptr::null_mut(),kind:0},
        Some(Returned::Unit)=>Item{node:std::ptr::null_mut(),kind:1},
        Some(Returned::Value(value))=>Item{node:pointer(value.as_ref()),kind:2},
        Some(Returned::Dataset(data))=>Item{node:pointer(data),kind:3},
        Some(Returned::OwnedValue(value))=>Item{node:pointer(value.as_ref()),kind:4},
    }
}
unsafe extern "C" fn child_mut(owner:Address,at:u64)->Item{
    match unsafe{&mut *owner.cast::<OwnedValuePayload>()}.payloads[at as usize].as_deref_mut(){
        None=>Item{node:std::ptr::null_mut(),kind:0},
        Some(Returned::Unit)=>Item{node:std::ptr::null_mut(),kind:1},
        Some(Returned::Value(value))=>Item{node:pointer_mut(value.as_mut()),kind:2},
        Some(Returned::Dataset(data))=>Item{node:pointer_mut(data),kind:3},
        Some(Returned::OwnedValue(value))=>Item{node:pointer_mut(value.as_mut()),kind:4},
    }
}
unsafe extern "C" fn field(owner:Address,at:u64)->Field{
    let (offset,ty)=unsafe{&*owner.cast::<OwnedValuePayload>()}.value.ty.aggregate_part(at as usize);
    Field{ty:pointer(ty),offset,owns:u32::from(ty.owns_resources())}
}
unsafe extern "C" fn field_owned(owner:Address,at:u64)->u32{u32::from(unsafe{&*owner.cast::<OwnedValuePayload>()}.value.ty.aggregate_part(at as usize).1.owns_resources())}
unsafe extern "C" fn aggregate_count(owner:Address)->u64{unsafe{&*owner.cast::<OwnedValuePayload>()}.value.ty.aggregate_len() as u64}
unsafe extern "C" fn component_count(ty:Address)->u64{unsafe{&*ty.cast::<Ty>()}.components.len() as u64}
unsafe extern "C" fn component_word(ty:Address,at:u64)->u32{unsafe{&*ty.cast::<Ty>()}.components[at as usize].1}
unsafe extern "C" fn component_index(value:Address,word:u32)->u64{unsafe{&*value.cast::<Value>()}.ty.components.iter().position(|(_,at)|*at==word).unwrap() as u64}
unsafe extern "C" fn index_at(values:Indices,at:u64)->u64{(unsafe{slice(values)})[at as usize] as u64}
unsafe extern "C" fn state_available(value:Address)->u32{u32::from(unsafe{&*value.cast::<Ownership>()}.available())}
unsafe extern "C" fn state_owned(value:Address)->u32{u32::from(unsafe{&*value.cast::<Ownership>()}==&Ownership::Owned)}
unsafe extern "C" fn state_moved(value:Address)->u32{u32::from(unsafe{&*value.cast::<Ownership>()}==&Ownership::Moved)}
unsafe extern "C" fn state_equal(a:Address,b:Address)->u32{u32::from(unsafe{&*a.cast::<Ownership>()==&*b.cast::<Ownership>()})}
unsafe extern "C" fn state_set_owned(value:Address){unsafe{*value.cast::<Ownership>()=Ownership::Owned};}
unsafe extern "C" fn state_set_borrowed(value:Address){unsafe{*value.cast::<Ownership>()=Ownership::Borrowed};}
unsafe extern "C" fn state_set_moved(value:Address){unsafe{*value.cast::<Ownership>()=Ownership::Moved};}
unsafe extern "C" fn proof_join_stored(a:Address,b:Address,when:Address){
    let a=unsafe{&mut *a.cast::<Value>()};let b=unsafe{&*b.cast::<Value>()};
    a.proof.join_condition(&b.proof,a.regs.len(),unsafe{choice(when)});
}
unsafe extern "C" fn proof_join_common(a:Address,b:Address,when:Address){
    let a=unsafe{&mut *a.cast::<Value>()};let b=unsafe{&*b.cast::<Value>()};
    a.proof=bounds::Proof::common(a,b,unsafe{choice(when)},None);
}
unsafe extern "C" fn uniform(value:Address)->u32{u32::from(unsafe{&*value.cast::<Value>()}.uniform)}
unsafe extern "C" fn set_uniform(value:Address,uniform:u32){unsafe{&mut *value.cast::<Value>()}.uniform=uniform!=0;}
unsafe extern "C" fn constant_equal(a:Address,b:Address)->u32{u32::from(unsafe{&*a.cast::<Value>()}.constant==unsafe{&*b.cast::<Value>()}.constant)}
unsafe extern "C" fn clear_constant(value:Address){unsafe{&mut *value.cast::<Value>()}.constant=None;}
unsafe extern "C" fn proof_borrow(value:Address){let v=unsafe{&mut *value.cast::<Value>()};v.proof=v.proof.borrow_state();}
unsafe extern "C" fn proof_clear(value:Address){unsafe{&mut *value.cast::<Value>()}.proof=bounds::Proof::default();}
unsafe extern "C" fn proof_assign(value:Address,components:Indices){let v=unsafe{&mut *value.cast::<Value>()};v.proof.assign(unsafe{slice(components)},v.ty.components.len(),bounds::Proof::default());}
unsafe extern "C" fn length_constant(value:Address,count:u32){unsafe{&mut *value.cast::<Value>()}.proof=bounds::Proof::unsigned(count,count);}
unsafe extern "C" fn view_empty(value:Address)->u32{u32::from(unsafe{&*value.cast::<Value>()}.proof.view_borrow(0).empty())}
unsafe extern "C" fn install_source(value:Address){
    let v=unsafe{&mut *value.cast::<Value>()};let identity=v.identity();
    v.proof.set_view_sources(0,BTreeSet::from([view::Source::Data(identity)]));
}
unsafe extern "C" fn join_child(a:Address,b:Address,at:u64,stored:u32,when:Address){
    let a=unsafe{&mut *a.cast::<OwnedValuePayload>()};let b=unsafe{&mut *b.cast::<OwnedValuePayload>()};
    let other=b.payloads[at as usize].take().unwrap();
    a.payloads[at as usize].as_mut().unwrap().join(*other,stored!=0,unsafe{choice(when)});
}
unsafe extern "C" fn move_child(a:Address,b:Address,at:u64){
    let a=unsafe{&mut *a.cast::<OwnedValuePayload>()};let b=unsafe{&mut *b.cast::<OwnedValuePayload>()};
    a.payloads[at as usize]=b.payloads[at as usize].take();
}
unsafe extern "C" fn data_root(data:Address)->Address{pointer(&unsafe{&*data.cast::<Data>()}.root)}
unsafe extern "C" fn data_length(data:Address)->Address{pointer(&unsafe{&*data.cast::<Data>()}.length)}
unsafe extern "C" fn data_ownership(data:Address)->Address{pointer(&unsafe{&*data.cast::<Data>()}.ownership)}
unsafe extern "C" fn data_root_mut(data:Address)->Address{pointer_mut(&mut unsafe{&mut *data.cast::<Data>()}.root)}
unsafe extern "C" fn data_length_mut(data:Address)->Address{pointer_mut(&mut unsafe{&mut *data.cast::<Data>()}.length)}
unsafe extern "C" fn data_ownership_mut(data:Address)->Address{pointer_mut(&mut unsafe{&mut *data.cast::<Data>()}.ownership)}
unsafe extern "C" fn data_count(data:Address)->Count{match unsafe{&*data.cast::<Data>()}.count{Some(value)=>Count{known:1,value},None=>Count{known:0,value:0}}}
unsafe extern "C" fn data_read(data:Address){unsafe{&mut *data.cast::<Data>()}.access=Access::Read;}
unsafe extern "C" fn data_clear_residency(data:Address){unsafe{&mut *data.cast::<Data>()}.residency=residency::Residency::default();}
unsafe extern "C" fn data_identity(data:Address,id:Address)->u32{u32::from(unsafe{&*data.cast::<Data>()}.root.proof.known_identity(0)==Some(unsafe{*id.cast::<ValueIdentity>()}))}
struct Loop<'a>{inputs:&'a BTreeMap<ValueIdentity,bounds::Proof>,path:Option<Box<bounds::Clause>>}
unsafe extern "C" fn ownership_loop(a:Address,b:Address,context:Address,path:Address)->u32{
    let context=unsafe{&*context.cast::<Loop<'_>>()};
    u32::from(unsafe{&*a.cast::<Ownership>()}.loop_compatible(unsafe{&*b.cast::<Ownership>()},context.inputs,unsafe{&*path.cast::<bounds::Clause>()}))
}
unsafe extern "C" fn loop_path(value:Address,context:Address,path:Address)->Address{
    let path=Box::new(unsafe{&*value.cast::<Ownership>()}.presence().condition().assume(unsafe{&*path.cast::<bounds::Clause>()},true));
    let address=pointer(path.as_ref());
    unsafe{&mut *context.cast::<Loop<'_>>()}.path=Some(path);
    address
}
unsafe extern "C" fn path_possible(path:Address)->u32{u32::from(unsafe{&*path.cast::<bounds::Clause>()}.possible())}
#[repr(C)]
struct LoopCursor{before:Address,after:Address,path:Address,count:u64,index:u64}
#[repr(C)]
struct LoopNeed{kind:u32,value:u32,before:Address,after:Address}
unsafe extern "C" fn fresh_indices()->Address{Box::into_raw(Box::new(Vec::<usize>::new())).cast()}
unsafe extern "C" fn push_index(value:Address,index:u64){unsafe{&mut *value.cast::<Vec<usize>>()}.push(index as usize);}
unsafe extern "C" fn indices_view(value:Address)->Indices{indices(unsafe{&*value.cast::<Vec<usize>>()})}
unsafe extern "C" fn drop_indices(value:Address){drop(unsafe{Box::from_raw(value.cast::<Vec<usize>>())});}

#[repr(C)]
#[derive(Clone,Copy)]
struct Host{
    inspect:unsafe extern "C" fn(Address)->Info,
    value:unsafe extern "C" fn(Address)->Address,
    ownership:unsafe extern "C" fn(Address)->Address,
    child:unsafe extern "C" fn(Address,u64)->Item,
    field:unsafe extern "C" fn(Address,u64)->Field,
    field_owned:unsafe extern "C" fn(Address,u64)->u32,
    aggregate_count:unsafe extern "C" fn(Address)->u64,
    component_count:unsafe extern "C" fn(Address)->u64,
    component_word:unsafe extern "C" fn(Address,u64)->u32,
    component_index:unsafe extern "C" fn(Address,u32)->u64,
    index_at:unsafe extern "C" fn(Indices,u64)->u64,
    state_available:unsafe extern "C" fn(Address)->u32,
    state_owned:unsafe extern "C" fn(Address)->u32,
    state_moved:unsafe extern "C" fn(Address)->u32,
    state_equal:unsafe extern "C" fn(Address,Address)->u32,
    state_set_owned:unsafe extern "C" fn(Address),
    state_set_borrowed:unsafe extern "C" fn(Address),
    state_set_moved:unsafe extern "C" fn(Address),
    proof_join_stored:unsafe extern "C" fn(Address,Address,Address),
    proof_join_common:unsafe extern "C" fn(Address,Address,Address),
    uniform:unsafe extern "C" fn(Address)->u32,
    set_uniform:unsafe extern "C" fn(Address,u32),
    constant_equal:unsafe extern "C" fn(Address,Address)->u32,
    clear_constant:unsafe extern "C" fn(Address),
    proof_borrow:unsafe extern "C" fn(Address),
    proof_clear:unsafe extern "C" fn(Address),
    proof_assign:unsafe extern "C" fn(Address,Indices),
    length_constant:unsafe extern "C" fn(Address,u32),
    view_empty:unsafe extern "C" fn(Address)->u32,
    install_source:unsafe extern "C" fn(Address),
    join_child:unsafe extern "C" fn(Address,Address,u64,u32,Address),
    move_child:unsafe extern "C" fn(Address,Address,u64),
    data_root:unsafe extern "C" fn(Address)->Address,
    data_length:unsafe extern "C" fn(Address)->Address,
    data_ownership:unsafe extern "C" fn(Address)->Address,
    data_count:unsafe extern "C" fn(Address)->Count,
    data_read:unsafe extern "C" fn(Address),
    data_clear_residency:unsafe extern "C" fn(Address),
    data_identity:unsafe extern "C" fn(Address,Address)->u32,
    ownership_loop:unsafe extern "C" fn(Address,Address,Address,Address)->u32,
    loop_path:unsafe extern "C" fn(Address,Address,Address)->Address,
    path_possible:unsafe extern "C" fn(Address)->u32,
    fresh_indices:unsafe extern "C" fn()->Address,
    push_index:unsafe extern "C" fn(Address,u64),
    indices_view:unsafe extern "C" fn(Address)->Indices,
    drop_indices:unsafe extern "C" fn(Address),
}
const HOST:Host=Host{inspect,value,ownership,child,field,field_owned,aggregate_count,component_count,component_word,component_index,index_at,state_available,state_owned,state_moved,state_equal,state_set_owned,state_set_borrowed,state_set_moved,proof_join_stored,proof_join_common,uniform,set_uniform,constant_equal,clear_constant,proof_borrow,proof_clear,proof_assign,length_constant,view_empty,install_source,join_child,move_child,data_root,data_length,data_ownership,data_count,data_read,data_clear_residency,data_identity,ownership_loop,loop_path,path_possible,fresh_indices,push_index,indices_view,drop_indices};
const MUT_HOST:Host=Host{value:value_mut,ownership:ownership_mut,child:child_mut,data_root:data_root_mut,data_length:data_length_mut,data_ownership:data_ownership_mut,..HOST};

unsafe extern "C" {
    #[link_name="bootstrap.ownership_payload::join"] fn source_join(a:Address,b:Address,stored:u32,choice:Address,host:Host);
    #[link_name="bootstrap.ownership_payload::available"] fn source_available(value:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_payload::all_available"] fn source_all_available(value:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_payload::borrow"] fn source_borrow(value:Address,host:Host);
    #[link_name="bootstrap.ownership_payload::incomplete"] fn source_incomplete(value:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_payload::loop_start"] fn source_loop_start(state:*mut LoopCursor,a:Address,b:Address,context:Address,path:Address,host:Host)->LoopNeed;
    #[link_name="bootstrap.ownership_payload::loop_step"] fn source_loop_step(state:*mut LoopCursor,compatible:u32,host:Host)->LoopNeed;
    #[link_name="bootstrap.ownership_payload::invalidate"] fn source_invalidate(value:Address,host:Host);
    #[link_name="bootstrap.ownership_payload::invalidate_components"] fn source_components(value:Address,components:Indices,borrowed:Address,host:Host);
    #[link_name="bootstrap.ownership_payload::mark_moved"] fn source_moved(value:Address,host:Host);
    #[link_name="bootstrap.ownership_payload::handles"] fn source_handles(value:Address,context:Address,visit:unsafe extern "C" fn(Address,Address),host:Host);
    #[link_name="bootstrap.ownership_payload::resources"] fn source_resources(value:Address,context:Address,visit:unsafe extern "C" fn(Address,Address),host:Host);
}
pub(super) fn join(value:&mut OwnedValuePayload,mut other:OwnedValue,stored:bool,choice:Option<&bounds::Condition>){unsafe{source_join(pointer_mut(value),pointer_mut(other.as_mut()),u32::from(stored),choice.map_or(std::ptr::null_mut(),pointer),MUT_HOST)};}
pub(super) fn available(value:&OwnedValuePayload)->bool{unsafe{source_available(pointer(value),HOST)!=0}}
pub(super) fn all_available(value:&OwnedValuePayload)->bool{unsafe{source_all_available(pointer(value),HOST)!=0}}
pub(super) fn borrow(value:&mut OwnedValuePayload){unsafe{source_borrow(pointer_mut(value),MUT_HOST)};}
pub(super) fn incomplete(value:&OwnedValuePayload)->bool{unsafe{source_incomplete(pointer(value),HOST)!=0}}
pub(super) async fn loop_compatible(value:&OwnedValuePayload,other:&OwnedValuePayload,builder:&mut Builder<'_>,span:Span,path:&bounds::Clause,inputs:&BTreeMap<ValueIdentity,bounds::Proof>)->Result<bool>{
    let world=builder.world;
    let mut context=Loop{inputs,path:None};
    let mut cursor=std::mem::MaybeUninit::<LoopCursor>::uninit();
    let mut need=unsafe{source_loop_start(cursor.as_mut_ptr(),pointer(value),pointer(other),pointer_mut(&mut context),pointer(path),HOST)};
    while need.kind!=0{
        let path=unsafe{&*(*cursor.as_ptr()).path.cast::<bounds::Clause>()};
        let compatible=if need.kind==3{
            let before=unsafe{&*need.before.cast::<Data>()};let after=unsafe{&*need.after.cast::<Data>()};
            unsafe{world.call(before.loop_compatible(after,builder,span,path,inputs)).await}?
        }else{
            let before=unsafe{&*need.before.cast::<OwnedValuePayload>()};let after=unsafe{&*need.after.cast::<OwnedValuePayload>()};
            unsafe{world.call(loop_compatible(before,after,builder,span,path,inputs)).await}?
        };
        need=unsafe{source_loop_step(cursor.as_mut_ptr(),u32::from(compatible),HOST)};
    }
    Ok(need.value!=0)
}
pub(super) fn invalidate(value:&mut OwnedValuePayload){unsafe{source_invalidate(pointer_mut(value),MUT_HOST)};}
pub(super) fn invalidate_components(value:&mut OwnedValuePayload,components:&[usize],borrowed:Option<ValueIdentity>){unsafe{source_components(pointer_mut(value),indices(components),borrowed.as_ref().map_or(std::ptr::null_mut(),pointer),MUT_HOST)};}
pub(super) fn mark_moved(value:&mut OwnedValuePayload){unsafe{source_moved(pointer_mut(value),MUT_HOST)};}
unsafe extern "C" fn push_handle(data:Address,context:Address){unsafe{&mut *context.cast::<Vec<ValueIdentity>>()}.push(unsafe{&*data.cast::<Data>()}.root.identity());}
pub(super) fn handles(value:&OwnedValuePayload)->Vec<ValueIdentity>{let mut values=Vec::<ValueIdentity>::new();unsafe{source_handles(pointer(value),(&mut values as *mut Vec<ValueIdentity>).cast(),push_handle,HOST)};values}
unsafe extern "C" fn push_resource(data:Address,context:Address){unsafe{&mut *context.cast::<Vec<&Data>>()}.push(unsafe{&*data.cast::<Data>()});}
pub(super) fn resources(value:&OwnedValuePayload)->Vec<&Data>{let mut values=Vec::<&Data>::new();unsafe{source_resources(pointer(value),(&mut values as *mut Vec<&Data>).cast(),push_resource,HOST)};values}
unsafe extern "C" fn visit_resource<F:FnMut(&Data)>(data:Address,context:Address){(unsafe{&mut *context.cast::<F>()})(unsafe{&*data.cast::<Data>()});}
pub(super) fn visit_resources<F:FnMut(&Data)>(value:&OwnedValuePayload,visit:&mut F){unsafe{source_resources(pointer(value),(visit as *mut F).cast(),visit_resource::<F>,HOST)};}
unsafe extern "C" fn visit_mut<F:FnMut(&mut Data)>(data:Address,context:Address){(unsafe{&mut *context.cast::<F>()})(unsafe{&mut *data.cast::<Data>()});}
pub(super) fn resources_mut<F:FnMut(&mut Data)>(value:&mut OwnedValuePayload,visit:&mut F){unsafe{source_resources(pointer_mut(value),(visit as *mut F).cast(),visit_mut::<F>,MUT_HOST)};}
