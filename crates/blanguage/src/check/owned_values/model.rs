//! B traverses the original owned payloads; native leaves retain their containers.
use super::*;
use std::ffi::c_void;
use std::ops::ControlFlow;
type Address=*mut c_void;
fn pointer<T>(value:&T)->Address{(value as *const T).cast_mut().cast()}
fn pointer_mut<T>(value:&mut T)->Address{(value as *mut T).cast()}
#[repr(C)]
#[derive(Clone,Copy)]
struct Info{count:u64,flags:u32}
#[repr(C)]
#[derive(Clone, Copy)]
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
    returned_item(unsafe { &*owner.cast::<OwnedValuePayload>() }.payloads[at as usize].as_deref())
}
fn returned_item(value: Option<&Returned>) -> Item {
    match value {
        None=>Item{node:std::ptr::null_mut(),kind:0},
        Some(Returned::Unit)=>Item{node:std::ptr::null_mut(),kind:1},
        Some(Returned::Value(value))=>Item{node:pointer(value.as_ref()),kind:2},
        Some(Returned::Dataset(data))=>Item{node:pointer(data),kind:3},
        Some(Returned::OwnedValue(value))=>Item{node:pointer(value.as_ref()),kind:4},
    }
}
unsafe extern "C" fn child_mut(owner:Address,at:u64)->Item{
    mutable_item(unsafe { &mut *owner.cast::<OwnedValuePayload>() }.payloads[at as usize].as_deref_mut())
}
fn mutable_item(value: Option<&mut Returned>) -> Item {
    match value {
        None=>Item{node:std::ptr::null_mut(),kind:0},
        Some(Returned::Unit)=>Item{node:std::ptr::null_mut(),kind:1},
        Some(Returned::Value(value))=>Item{node:pointer_mut(value.as_mut()),kind:2},
        Some(Returned::Dataset(data))=>Item{node:pointer_mut(data),kind:3},
        Some(Returned::OwnedValue(value))=>Item{node:pointer_mut(value.as_mut()),kind:4},
    }
}
unsafe extern "C" fn path_child(owner: Address, at: u64) -> Item {
    let owner = unsafe { &*owner.cast::<OwnedValuePayload>() };
    returned_item(owner.payloads.get(at as usize).and_then(Option::as_deref))
}

unsafe extern "C" fn path_child_mut(owner: Address, at: u64) -> Item {
    let owner = unsafe { &mut *owner.cast::<OwnedValuePayload>() };
    mutable_item(owner.payloads.get_mut(at as usize).and_then(Option::as_deref_mut))
}

unsafe extern "C" fn path_moved(item: Item) -> u32 {
    // B asks only about a selected aggregate or dataset, using its original kind.
    let state = unsafe {
        if item.kind == 3 { data_ownership(item.node) } else { ownership(item.node) }
    };
    unsafe { state_moved(state) }
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
    v.proof.set_view_borrow(0,view::Borrow::source(view::Source::Data(identity)));
}
unsafe extern "C" fn join_child(a:Address,b:Address,at:u64,stored:u32,when:Address,presence:Address,merge_metadata:u32){
    let a=unsafe{&mut *a.cast::<OwnedValuePayload>()};let b=unsafe{&mut *b.cast::<OwnedValuePayload>()};
    let other=b.payloads[at as usize].take().unwrap();
    a.payloads[at as usize].as_mut().unwrap().join_returned(*other,stored!=0,unsafe{choice(when)},unsafe{presence.cast::<owned_presence::Presence>().as_ref()},merge_metadata);
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
    join_child:unsafe extern "C" fn(Address,Address,u64,u32,Address,Address,u32),
    join_ownership:unsafe extern "C" fn(Address,Address,Address),
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
const HOST:Host=Host{inspect,value,ownership,child,field,field_owned,aggregate_count,component_count,component_word,component_index,index_at,state_available,state_owned,state_moved,state_equal,state_set_owned,state_set_borrowed,state_set_moved,proof_join_stored,proof_join_common,uniform,set_uniform,constant_equal,clear_constant,proof_borrow,proof_clear,proof_assign,length_constant,view_empty,install_source,join_child,join_ownership,move_child,data_root,data_length,data_ownership,data_count,data_read,data_clear_residency,data_identity,ownership_loop,loop_path,path_possible,fresh_indices,push_index,indices_view,drop_indices};
const MUT_HOST:Host=Host{value:value_mut,ownership:ownership_mut,child:child_mut,data_root:data_root_mut,data_length:data_length_mut,data_ownership:data_ownership_mut,..HOST};

#[repr(C)]
struct DataPathHost {
    index_at: unsafe extern "C" fn(Indices, u64) -> u64,
    child: unsafe extern "C" fn(Address, u64) -> Item,
    moved: unsafe extern "C" fn(Item) -> u32,
}

#[repr(C)]
struct LivePathHost {
    inspect: unsafe extern "C" fn(Address) -> Info,
    child: unsafe extern "C" fn(Address, u64) -> Item,
    moved: unsafe extern "C" fn(Item) -> u32,
    push: unsafe extern "C" fn(Address, u64),
    pop: unsafe extern "C" fn(Address),
    visit: unsafe extern "C" fn(Address, Address, u32) -> u32,
}

unsafe extern "C" {
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
    #[link_name="bootstrap.ownership_payload::visit_values"] fn source_values(value:Item,context:Address,visit:unsafe extern "C" fn(Address,Address),host:Host);
    #[link_name = "bootstrap.ownership_payload::data_at"]
    fn source_data_at(value: Address, path: Indices, live: u32, host: DataPathHost) -> *mut Data;
    #[link_name = "bootstrap.ownership_payload::live_paths"]
    fn source_live_paths(value: Address, independent: u32, context: Address, host: LivePathHost) -> u32;
}
pub(super) fn visit_live_paths<F>(value: &OwnedValuePayload, visit: F) -> ControlFlow<()>
where F: FnMut(&[usize], &Data, bool) -> ControlFlow<()> {
    struct Visit<F> {
        path: Vec<usize>,
        visit: F,
    }

    unsafe extern "C" fn push<F>(context: Address, index: u64) {
        unsafe { &mut *context.cast::<Visit<F>>() }.path.push(index as usize);
    }

    unsafe extern "C" fn pop<F>(context: Address) {
        unsafe { &mut *context.cast::<Visit<F>>() }.path.pop();
    }

    unsafe extern "C" fn emit<F>(context: Address, data: Address, independent: u32) -> u32
    where F: FnMut(&[usize], &Data, bool) -> ControlFlow<()> {
        let context = unsafe { &mut *context.cast::<Visit<F>>() };
        let data = unsafe { &*data.cast::<Data>() };
        u32::from((context.visit)(
            &context.path, data, independent != 0 && data.ownership == Ownership::Owned,
        ).is_break())
    }

    let mut context = Visit { path: Vec::new(), visit };
    let stopped = unsafe {
        source_live_paths(pointer(value), 1, pointer_mut(&mut context), LivePathHost {
            inspect, child, moved: path_moved, push: push::<F>, pop: pop::<F>, visit: emit::<F>,
        })
    };
    if stopped == 0 { ControlFlow::Continue(()) } else { ControlFlow::Break(()) }
}

fn data_at<'a>(value: &'a OwnedValuePayload, path: &[usize], live: bool) -> Option<&'a Data> {
    // The result is borrowed from the original owner and never retained by B.
    unsafe {
        source_data_at(
            pointer(value), indices(path), u32::from(live),
            DataPathHost { index_at, child: path_child, moved: path_moved },
        ).as_ref()
    }
}

pub(in crate::check) fn live_data_at<'a>(value: &'a OwnedValuePayload, path: &[usize]) -> Option<&'a Data> {
    data_at(value, path, true)
}

pub(in crate::check) fn owned_data_at<'a>(value: &'a OwnedValuePayload, path: &[usize]) -> Option<&'a Data> {
    data_at(value, path, false)
}

pub(in crate::check) fn owned_data_at_mut<'a>(
    value: &'a mut OwnedValuePayload, path: &[usize],
) -> Option<&'a mut Data> {
    // B returns only the single leaf selected through the exclusive owner.
    unsafe {
        source_data_at(
            pointer_mut(value), indices(path), 0,
            DataPathHost { index_at, child: path_child_mut, moved: path_moved },
        ).as_mut()
    }
}

unsafe extern "C" fn join_ownership(left: Address, right: Address, presence: Address) {
    let a = unsafe { &mut *left.cast::<Ownership>() };
    let b = unsafe { &*right.cast::<Ownership>() };
    let condition = unsafe { &*presence.cast::<owned_presence::Presence>() };
    *a = Ownership::either(a, b, Some(condition));
}
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
unsafe extern "C" fn visit_value<F:FnMut(&mut Value)>(value:Address,context:Address){
    (unsafe{&mut *context.cast::<F>()})(unsafe{&mut *value.cast::<Value>()});
}
impl Returned {
    pub(in crate::check) fn visit_values_mut<F:FnMut(&mut Value)>(&mut self,visit:&mut F){
        unsafe{source_values(mutable_item(Some(self)),(visit as *mut F).cast(),visit_value::<F>,MUT_HOST)};
    }
}


#[repr(C)]
#[derive(Clone, Copy)]
struct Alternative { owner: Address, choice: Address }

#[derive(Clone, Copy)]
struct SelectionInput<'a, 'w> {
    when: &'a owned_presence::Presence,
    item: Item,
    path: Option<(&'a Builder<'w>, &'a bounds::Condition)>,
}

struct SelectionChoice<'a, 'w> {
    input: SelectionInput<'a, 'w>,
    owner: std::borrow::Cow<'a, Ownership>,
}

struct Selection<'a, 'w> {
    choices: &'a [SelectionChoice<'a, 'w>],
    proofs: Vec<(bounds::Condition, bounds::Proof)>,
    combined: owned_presence::Presence,
}

#[repr(C)]
struct SelectionHost {
    alternative: unsafe extern "C" fn(Address, usize) -> Alternative,
    mode: unsafe extern "C" fn(Address) -> u32,
    equal: unsafe extern "C" fn(Address, Address) -> u32,
    accumulate: unsafe extern "C" fn(Address, Alternative),
    commit: unsafe extern "C" fn(Item, Address, Address, u32),
    reset_residency: unsafe extern "C" fn(Item),
    join_residency: unsafe extern "C" fn(Item, Alternative),
    push_proof: unsafe extern "C" fn(Address, Alternative),
    finish_proofs: unsafe extern "C" fn(Item, Address),
    inspect: unsafe extern "C" fn(Address) -> Info,
    mutable_child: unsafe extern "C" fn(Address, u64) -> Item,
    select_child: unsafe extern "C" fn(Item, Address, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_payload::select_presence"]
    fn source_select_presence(target: Item, inputs: Address, count: usize, host: SelectionHost);
}

const SELECT: SelectionHost = SelectionHost {
    alternative, mode: selection_mode, equal: state_equal,
    accumulate: accumulate_presence, commit: commit_presence, reset_residency: reset_selection_residency,
    join_residency: join_selection_residency, push_proof: selection_proof, finish_proofs: install_selected_proofs,
    inspect, mutable_child: child_mut, select_child,
};

impl Returned {
    pub(in super::super) fn select_presence<'a>(
        &mut self,
        candidates: impl IntoIterator<Item = (&'a owned_presence::Presence, &'a Returned)>,
    ) {
        select_model(mutable_item(Some(self)), candidates.into_iter().map(|(when, value)| SelectionInput {
            when, item: returned_item(Some(value)), path: None,
        }));
    }

    pub(in crate::check) fn select_place_presence<'a, 'w>(
        &mut self,
        builder: &'a Builder<'w>,
        candidates: impl IntoIterator<Item = (&'a owned_presence::Presence, &'a bounds::Condition, &'a Returned)>,
    ) {
        select_model(mutable_item(Some(self)), candidates.into_iter().map(|(when, condition, value)| SelectionInput {
            when, item: returned_item(Some(value)), path: Some((builder, condition)),
        }));
    }
}

fn select_model<'a, 'w: 'a>(target: Item, inputs: impl IntoIterator<Item = SelectionInput<'a, 'w>>) {
    if !matches!(target.kind, 3 | 4) {
        return;
    }
    let choices: Vec<_> = inputs.into_iter().filter(|input| input.item.kind == target.kind).map(|input| {
        // The sources are borrowed through the synchronous selection, including its child calls.
        let owner = if target.kind == 3 {
            &unsafe { &*input.item.node.cast::<Data>() }.ownership
        } else {
            &unsafe { &*input.item.node.cast::<OwnedValuePayload>() }.ownership
        };
        let owner = match input.path {
            Some((builder, condition)) => std::borrow::Cow::Owned(
                owned_presence::path::ownership_in(builder, owner, Some(condition)),
            ),
            None => std::borrow::Cow::Borrowed(owner),
        };
        SelectionChoice { input, owner }
    }).collect();
    let mut selection = Selection {
        choices: &choices, proofs: Vec::new(), combined: owned_presence::Presence::None,
    };
    unsafe { source_select_presence(target, pointer_mut(&mut selection), choices.len(), SELECT); }
}

unsafe extern "C" fn alternative(selection: Address, index: usize) -> Alternative {
    let selection = unsafe { &*selection.cast::<Selection<'_, '_>>() };
    let choice = &selection.choices[index];
    Alternative { owner: pointer(choice.owner.as_ref()), choice: pointer(choice) }
}

unsafe extern "C" fn selection_mode(owner: Address) -> u32 {
    match unsafe { &*owner.cast::<Ownership>() } {
        Ownership::Moved => 0,
        Ownership::Owned => 1,
        Ownership::Borrowed => 2,
        Ownership::Conditional(_) => 3,
    }
}

unsafe extern "C" fn accumulate_presence(selection: Address, candidate: Alternative) {
    let selection = unsafe { &mut *selection.cast::<Selection<'_, '_>>() };
    let choice = unsafe { &*candidate.choice.cast::<SelectionChoice<'_, '_>>() };
    let current = std::mem::replace(&mut selection.combined, owned_presence::Presence::None);
    selection.combined = current.or(choice.input.when.clone().and(choice.owner.presence()));
}

unsafe extern "C" fn commit_presence(output: Item, first: Address, selection: Address, mode: u32) {
    let owner = match mode {
        0 => unsafe { &*first.cast::<Ownership>() }.clone(),
        1 => Ownership::Moved,
        _ => {
            let selection = unsafe { &mut *selection.cast::<Selection<'_, '_>>() };
            Ownership::from_presence(std::mem::replace(&mut selection.combined, owned_presence::Presence::None))
        }
    };
    let output = if output.kind == 3 {
        unsafe { data_ownership_mut(output.node) }
    } else {
        unsafe { ownership_mut(output.node) }
    };
    unsafe { *output.cast::<Ownership>() = owner; }
}

unsafe extern "C" fn reset_selection_residency(target: Item) {
    unsafe { &mut *target.node.cast::<Data>() }.residency = residency::Residency::Absent;
}

unsafe extern "C" fn join_selection_residency(target: Item, candidate: Alternative) {
    let target = unsafe { &mut *target.node.cast::<Data>() };
    let input = unsafe { &*candidate.choice.cast::<SelectionChoice<'_, '_>>() }.input;
    let data = unsafe { &*input.item.node.cast::<Data>() };
    let residency = match input.path {
        Some((builder, condition)) => owned_presence::path::residency_in(builder, &data.residency, Some(condition)),
        None => data.residency.clone(),
    };
    target.residency.join(&residency::Residency::selected(
        input.when.clone(), residency, residency::Residency::Absent,
    ));
}

unsafe extern "C" fn selection_proof(selection: Address, candidate: Alternative) {
    let selection = unsafe { &mut *selection.cast::<Selection<'_, '_>>() };
    let choice = unsafe { &*candidate.choice.cast::<SelectionChoice<'_, '_>>() };
    let input = choice.input;
    let proof = if input.item.kind == 3 {
        let data = unsafe { &*input.item.node.cast::<Data>() };
        (input.when.clone().and(choice.owner.presence()).condition(), view::resource_proof(data))
    } else {
        let value = unsafe { &*input.item.node.cast::<OwnedValuePayload>() };
        let condition = input.path.map_or_else(|| input.when.condition(), |(_, condition)| condition.clone());
        (condition, value.value.proof.borrow_state())
    };
    selection.proofs.push(proof);
}

unsafe extern "C" fn install_selected_proofs(target: Item, selection: Address) {
    let selection = unsafe { &mut *selection.cast::<Selection<'_, '_>>() };
    let value = if target.kind == 3 {
        &mut unsafe { &mut *target.node.cast::<Data>() }.root
    } else {
        &mut unsafe { &mut *target.node.cast::<OwnedValuePayload>() }.value
    };
    value.proof.select_view_borrows(&selection.proofs);
    selection.proofs.clear();
}

unsafe extern "C" fn select_child(target: Item, selection: Address, index: usize) {
    let selection = unsafe { &*selection.cast::<Selection<'_, '_>>() };
    let inputs = selection.choices.iter().map(|choice| {
        let mut input = choice.input;
        input.item = unsafe { child(input.item.node, index as u64) };
        input
    });
    select_model(target, inputs);
}

#[repr(C)]
struct LookupNode { item: Item, place: Address, payload: Address, ty: Address, projected: u32 }

#[repr(C)]
struct LookupHost {
    matches_place: unsafe extern "C" fn(Address, Address) -> u32,
    capture: unsafe extern "C" fn(Address, LookupNode),
    inspect: unsafe extern "C" fn(Address) -> Info,
    field: unsafe extern "C" fn(Address, u64) -> Field,
    visit_child: unsafe extern "C" fn(Address, LookupNode, usize, Field),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_payload::collect_owned_places"]
    fn source_collect_places(context: Address, node: LookupNode, host: LookupHost);
}

const LOOKUP: LookupHost = LookupHost {
    matches_place, capture: capture_owned_place, inspect: lookup_shape,
    field: lookup_field, visit_child: visit_lookup_child,
};

struct CandidateLookup<'a, 'w> {
    builder: &'a Builder<'w>,
    ty: &'a Ty,
    positions: &'a BTreeSet<Position>,
    name: &'a str,
    root: Item,
    path: Vec<usize>,
    output: &'a mut Vec<(String, Vec<usize>, Returned)>,
}

fn place_pointer(place: Option<&Place>) -> Address {
    place.map_or(std::ptr::null_mut(), pointer)
}

impl Builder<'_> {
    pub(in super::super) fn owned_place_candidates(&self, place: &Place) -> Vec<(String, Vec<usize>, Returned)> {
        let mut positions = BTreeSet::new();
        owned_borrows::positions(&place.owned_positions().unwrap()[0], &mut positions);
        let mut output = Vec::new();
        for (name, binding) in &self.env {
            let (item, storage, ty) = match binding {
                Binding::OwnedValue(value) => (
                    Item { node: pointer(value.as_ref()), kind: 4 }, value.place.as_ref(), pointer(&value.value.ty),
                ),
                Binding::ApplicationData(data) if data.place.is_some() =>
                    (Item { node: pointer(data), kind: 3 }, data.place.as_ref(), std::ptr::null_mut()),
                _ => continue,
            };
            let mut context = CandidateLookup {
                builder: self, ty: place.ty(), positions: &positions, name, root: item,
                path: Vec::new(), output: &mut output,
            };
            let node = LookupNode {
                item, place: place_pointer(storage), payload: std::ptr::null_mut(), ty, projected: 0,
            };
            unsafe { source_collect_places(pointer_mut(&mut context), node, LOOKUP); }
        }
        output
    }
}

unsafe extern "C" fn matches_place(context: Address, place: Address) -> u32 {
    let context = unsafe { &*context.cast::<CandidateLookup<'_, '_>>() };
    let place = unsafe { &*place.cast::<Place>() };
    u32::from(place.ty() == context.ty && context.positions.contains(&place.owned_positions().unwrap()[0]))
}

unsafe extern "C" fn capture_owned_place(context: Address, node: LookupNode) {
    let context = unsafe { &mut *context.cast::<CandidateLookup<'_, '_>>() };
    let model = if let Some((&first, rest)) = context.path.split_first() {
        let root = unsafe { &*context.root.node.cast::<OwnedValuePayload>() };
        let mut value = context.builder.record_component(&root.value, first);
        for &index in rest {
            value = context.builder.record_component(&value, index);
        }
        let payload = unsafe { &*node.payload.cast::<Returned>() };
        let place = (node.projected != 0).then(|| || unsafe { &*node.place.cast::<Place>() }.clone());
        storage::unpack(context.builder, value, || payload, place)
    } else if node.item.kind == 3 {
        Returned::Dataset(unsafe { &*node.item.node.cast::<Data>() }.clone())
    } else {
        let value = unsafe { &*node.item.node.cast::<OwnedValuePayload>() };
        Returned::OwnedValue(Box::new(value.clone()))
    };
    context.output.push((context.name.to_owned(), context.path.clone(), model));
}

unsafe extern "C" fn lookup_shape(ty: Address) -> Info {
    let ty = unsafe { &*ty.cast::<Ty>() };
    Info { count: ty.aggregate_len() as u64, flags: u32::from(ty.has_variants()) }
}

unsafe extern "C" fn lookup_field(ty: Address, index: u64) -> Field {
    let (offset, ty) = unsafe { &*ty.cast::<Ty>() }.aggregate_part(index as usize);
    Field { ty: pointer(ty), offset, owns: u32::from(ty.owns_resources()) }
}

unsafe extern "C" fn visit_lookup_child(context: Address, parent: LookupNode, index: usize, field: Field) {
    let owner = unsafe { &*parent.item.node.cast::<OwnedValuePayload>() };
    let payload = unsafe { owner.payloads[index].as_deref().unwrap_unchecked() };
    let item = returned_item(Some(payload));
    let projected = if item.kind == 3 || item.kind == 4 {
        let parent_ty = unsafe { &*parent.ty.cast::<Ty>() };
        let field_ty = unsafe { &*field.ty.cast::<Ty>() };
        unsafe { parent.place.cast::<Place>().as_ref() }
            .map(|place| owned_records::owned_field_place(parent_ty, place, field_ty, field.offset))
    } else {
        None
    };
    let place = projected.as_ref().or_else(|| match payload {
        Returned::Dataset(data) => data.place.as_ref(),
        _ => None,
    });
    let node = LookupNode {
        item, place: place_pointer(place), payload: pointer(payload), ty: field.ty,
        projected: u32::from(projected.is_some()),
    };
    unsafe { &mut *context.cast::<CandidateLookup<'_, '_>>() }.path.push(index);
    unsafe { source_collect_places(context, node, LOOKUP); }
    unsafe { &mut *context.cast::<CandidateLookup<'_, '_>>() }.path.pop();
}


#[repr(C)]
struct PayloadPair { left: Item, right: Item }

#[repr(u32)]
enum DataInitial { Left,Right,Selected }
#[repr(u32)]
enum DataJoin { Left,Right,Selected,Common }
#[derive(Clone,Copy)]
#[repr(C)]
struct DataPresenceHost {
    moved:unsafe extern "C" fn(Address)->u32,
    ownership:unsafe extern "C" fn(Address,Address,Address,u32),
    initialization:unsafe extern "C" fn(Address,Address,DataInitial,Address,u32),
    residency:unsafe extern "C" fn(Address,Address,DataJoin,Address,u32),
}
const DATA_PRESENCE:DataPresenceHost=DataPresenceHost {
    moved:data_moved,ownership:data_merge_ownership,
    initialization:data_merge_initialization,residency:data_merge_residency,
};
unsafe extern "C" fn data_moved(value:Address)->u32 {
    u32::from(unsafe { &*value.cast::<Data>() }.ownership==Ownership::Moved)
}
unsafe extern "C" fn data_merge_ownership(a:Address,b:Address,when:Address,take_right:u32) {
    let left=unsafe { &mut *a.cast::<Data>() };let right=unsafe { &mut *b.cast::<Data>() };
    let selected=unsafe { &*when.cast::<owned_presence::Presence>() };
    let ownership=Ownership::either(&left.ownership,&right.ownership,Some(selected));
    let target=if take_right!=0 {right}else{left};target.ownership=ownership;
}
unsafe extern "C" fn data_merge_initialization(a:Address,b:Address,mode:DataInitial,when:Address,take_right:u32) {
    let left=unsafe { &mut *a.cast::<Data>() };let right=unsafe { &mut *b.cast::<Data>() };
    let initialization=match mode {
        DataInitial::Left=>left.initialization.take().unwrap_or(Position::Initialized),
        DataInitial::Right=>right.initialization.take().unwrap_or(Position::Initialized),
        DataInitial::Selected=>Position::selected(unsafe { &*when.cast::<owned_presence::Presence>() }.clone(),
            left.initialization.take().unwrap_or(Position::Initialized),right.initialization.take().unwrap_or(Position::Initialized)),
    };
    let target=if take_right!=0 {right}else{left};
    target.initialization=match initialization {Position::Initialized=>None,position=>Some(position)};
}
unsafe extern "C" fn data_merge_residency(a:Address,b:Address,mode:DataJoin,when:Address,take_right:u32) {
    let left=unsafe { &mut *a.cast::<Data>() };let right=unsafe { &mut *b.cast::<Data>() };
    let residency=match mode {
        DataJoin::Left=>std::mem::take(&mut left.residency),
        DataJoin::Right=>std::mem::take(&mut right.residency),
        DataJoin::Selected=>residency::Residency::conditional(unsafe { &*when.cast::<bounds::Condition>() }.clone(),
            std::mem::take(&mut left.residency),std::mem::take(&mut right.residency)),
        DataJoin::Common=>{left.residency.join(&right.residency);return;},
    };
    let target=if take_right!=0 {right}else{left};target.residency=residency;
}
struct ReturnedJoin {borrows:Option<(view::Borrow,view::Borrow)>}
#[repr(C)]
struct ReturnedHost {
    context:Address,values:Host,data:DataPresenceHost,
    save:unsafe extern "C" fn(Address,Address,Address),
    restore:unsafe extern "C" fn(Address,Address,Address),
    maximum:unsafe extern "C" fn(Address)->u32,
    elements:unsafe extern "C" fn(Address,Address,DataJoin,Address,Address),
    prefixes:unsafe extern "C" fn(Address,Address,Address),
}
unsafe extern "C" fn save_root_borrows(context:Address,a:Address,b:Address) {
    let left=unsafe { &mut *a.cast::<Data>() };let right=unsafe { &mut *b.cast::<Data>() };
    let first=view::resource_borrow(left);let second=view::resource_borrow(right);
    left.root.proof.set_view_borrow(0,view::Borrow::default());right.root.proof.set_view_borrow(0,view::Borrow::default());
    unsafe { &mut *context.cast::<ReturnedJoin>() }.borrows=Some((first,second));
}
unsafe extern "C" fn restore_root_borrows(context:Address,a:Address,when:Address) {
    let (left,right)=unsafe { &mut *context.cast::<ReturnedJoin>() }.borrows.take().unwrap();
    unsafe { &mut *a.cast::<Data>() }.root.proof.set_view_borrow(0,left.selected(unsafe { &*when.cast::<bounds::Condition>() },&right));
}
unsafe extern "C" fn data_maximum(value:Address)->u32 {unsafe { &*value.cast::<Data>() }.length.maximum()}
unsafe extern "C" fn merge_elements(a:Address,b:Address,mode:DataJoin,when:Address,presence:Address) {
    let left=unsafe { &mut *a.cast::<Data>() };let right=unsafe { &mut *b.cast::<Data>() };
    let elements=std::mem::take(&mut left.elements);let other=std::mem::take(&mut right.elements);
    let presence=unsafe { presence.cast::<owned_presence::Presence>().as_ref() };
    left.elements=match mode {
        DataJoin::Left=>elements.retain_side(other,false,presence),
        DataJoin::Right=>elements.retain_side(other,true,presence),
        DataJoin::Selected=>elements.join_selected(other,unsafe { &*when.cast::<bounds::Condition>() },presence),
        DataJoin::Common=>elements.common(other),
    };
}
unsafe extern "C" fn merge_prefixes(a:Address,b:Address,when:Address) {
    let left=unsafe { &mut *a.cast::<Data>() };let right=unsafe { &*b.cast::<Data>() };
    left.prefixes=prefix::join_fields(&left.prefixes,&right.prefixes,unsafe { choice(when) });
}
unsafe extern "C" {
    #[link_name="bootstrap.ownership_payload::join_returned"]
    fn source_join_returned(left:Item,right:Item,stored:u32,choice:Address,presence:Address,merge_metadata:u32,host:ReturnedHost);
}
impl Returned {
    pub(in crate::check) fn join_with_presence(&mut self, other: Self, stored: bool,
        choice: Option<&bounds::Condition>, presence: Option<&owned_presence::Presence>) {
        self.join_returned(other, stored, choice, presence, 1);
    }

    pub(in crate::check) fn join_place_payload(&mut self, other: Self) {
        self.join_returned(other, false, None, None, 0);
    }

    fn join_returned(&mut self, mut other: Self, stored: bool, choice: Option<&bounds::Condition>,
        presence: Option<&owned_presence::Presence>, merge_metadata: u32) {
        let mut context=ReturnedJoin {borrows:None};
        unsafe { source_join_returned(mutable_item(Some(self)),mutable_item(Some(&mut other)),u32::from(stored),
            choice.map_or(std::ptr::null_mut(),pointer),presence.map_or(std::ptr::null_mut(),pointer),merge_metadata,ReturnedHost {
                context:pointer_mut(&mut context),values:MUT_HOST,data:DATA_PRESENCE,
                save:save_root_borrows,restore:restore_root_borrows,maximum:data_maximum,
                elements:merge_elements,prefixes:merge_prefixes,
            }); }
    }
}

#[repr(C)]
struct RetainedHost {
    inspect: unsafe extern "C" fn(Address) -> Info,
    child: unsafe extern "C" fn(Address, u64) -> Item,
    ownership: unsafe extern "C" fn(PayloadPair, Address, u32),
    dataset: unsafe extern "C" fn(PayloadPair) -> PayloadPair,
    data:DataPresenceHost,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_payload::retain_presence"]
    fn source_retain_presence(pair: PayloadPair, presence: Address, take_right: u32, host: RetainedHost);
}

impl Returned {
    pub(in super::super) fn merge_retained_presence(
        &mut self, other: &mut Self, presence: &owned_presence::Presence, take_right: bool,
    ) {
        let pair = PayloadPair { left: mutable_item(Some(self)), right: mutable_item(Some(other)) };
        unsafe {
            source_retain_presence(pair, pointer(presence), u32::from(take_right), RetainedHost {
                inspect, child: child_mut, ownership: retain_ownership, dataset: retain_dataset,data:DATA_PRESENCE,
            });
        }
    }
}

unsafe extern "C" fn retain_ownership(pair: PayloadPair, presence: Address, take_right: u32) {
    let left = unsafe { &mut *pair.left.node.cast::<OwnedValuePayload>() };
    let right = unsafe { &mut *pair.right.node.cast::<OwnedValuePayload>() };
    let condition = unsafe { &*presence.cast::<owned_presence::Presence>() };
    let ownership = Ownership::either(&left.ownership, &right.ownership, Some(condition));
    let output = if take_right != 0 { &mut right.ownership } else { &mut left.ownership };
    *output = ownership;
}

unsafe extern "C" fn retain_dataset(pair: PayloadPair) -> PayloadPair {
    let left = unsafe { &mut *pair.left.node.cast::<Data>() };
    let right = unsafe { &mut *pair.right.node.cast::<Data>() };
    PayloadPair { left: mutable_item(left.elements.known_mut()), right: mutable_item(right.elements.known_mut()) }
}
