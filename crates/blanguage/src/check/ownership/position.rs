//! The original Position, Arc, set and vector containers remain native owners.
use super::*;
use owned_presence::Presence;
use std::{ffi::c_void, collections::btree_set};
type Address = *mut c_void;
type EqualPosition = unsafe extern "C" fn(Address, Address) -> u32;
type PresenceKind = unsafe extern "C" fn(Address) -> u32;
#[repr(C)]
struct SharedPositionSpan {
    positions: Address,
    count: usize,
}

impl SharedPositionSpan {
    fn empty() -> Self {
        Self { positions: std::ptr::null_mut(), count: 0 }
    }
}

#[repr(C)]
struct SharedPositionChildren {
    first: SharedPositionSpan,
    second: SharedPositionSpan,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_position::allocate_shared_positions"]
    fn allocate_shared_positions(bytes: usize) -> Address;
    #[link_name = "bootstrap.ownership_position::retain_shared_positions"]
    fn retain_shared_positions(positions: Address);
    #[link_name = "bootstrap.ownership_position::release_shared_positions"]
    fn release_shared_positions(
        span: SharedPositionSpan, width: usize,
        take: unsafe extern "C" fn(Address) -> SharedPositionChildren,
    );
}

#[repr(C)]
pub(in crate::check) struct SharedPositions {
    positions: std::ptr::NonNull<Position>,
    count: usize,
}

// B publishes immutable typed values and holds the shared references atomically.
unsafe impl Send for SharedPositions {}
unsafe impl Sync for SharedPositions {}

impl SharedPositions {
    fn span(&self) -> SharedPositionSpan {
        SharedPositionSpan { positions: self.positions.cast::<c_void>().as_ptr(), count: self.count }
    }

    fn into_span(self) -> SharedPositionSpan {
        std::mem::ManuallyDrop::new(self).span()
    }
}

fn allocate_shared<T>(count: usize) -> std::ptr::NonNull<T> {
    const { assert!(size_of::<T>() != 0 && align_of::<T>() <= 16); }
    let result = unsafe { allocate_shared_positions(count * size_of::<T>()) };
    unsafe { std::ptr::NonNull::new_unchecked(result.cast()) }
}

impl Default for SharedPositions {
    fn default() -> Self {
        Self { positions: std::ptr::NonNull::dangling(), count: 0 }
    }
}

impl From<Vec<Position>> for SharedPositions {
    fn from(mut values: Vec<Position>) -> Self {
        let count = values.len();
        if count == 0 {
            return Self::default();
        }
        let positions = allocate_shared::<Position>(count);
        unsafe {
            std::ptr::copy_nonoverlapping(values.as_ptr(), positions.as_ptr(), count);
            values.set_len(0);
        }
        Self { positions, count }
    }
}

impl FromIterator<Position> for SharedPositions {
    fn from_iter<I: IntoIterator<Item = Position>>(values: I) -> Self {
        Self::from(values.into_iter().collect::<Vec<_>>())
    }
}

impl std::ops::Deref for SharedPositions {
    type Target = [Position];

    fn deref(&self) -> &[Position] {
        unsafe { std::slice::from_raw_parts(self.positions.as_ptr(), self.count) }
    }
}

impl Clone for SharedPositions {
    fn clone(&self) -> Self {
        let span = self.span();
        if span.count != 0 {
            unsafe { retain_shared_positions(span.positions) };
        }
        Self { positions: self.positions, count: self.count }
    }
}

impl PartialEq for SharedPositions {
    fn eq(&self, other: &Self) -> bool { self.cmp(other).is_eq() }
}
impl Eq for SharedPositions {}
impl PartialOrd for SharedPositions {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}
impl Ord for SharedPositions {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.positions == other.positions && self.count == other.count {
            return std::cmp::Ordering::Equal;
        }
        self[..].cmp(&other[..])
    }
}

impl Drop for SharedPositions {
    fn drop(&mut self) {
        unsafe extern "C" fn take(value: Address) -> SharedPositionChildren {
            // B calls this once for an initialized slot after the final reference.
            match unsafe { value.cast::<Position>().read() } {
                Position::Both(left, right) | Position::Conditional(_, left, right) => {
                    SharedPositionChildren { first: left.0.into_span(), second: right.0.into_span() }
                }
                Position::Indexed(choices, _) => {
                    SharedPositionChildren { first: choices.into_span(), second: SharedPositionSpan::empty() }
                }
                value => {
                    drop(value);
                    SharedPositionChildren { first: SharedPositionSpan::empty(), second: SharedPositionSpan::empty() }
                }
            }
        }

        let span = self.span();
        if span.count != 0 {
            unsafe { release_shared_positions(span, size_of::<Position>(), take) };
        }
    }
}

#[repr(transparent)]
pub(in crate::check) struct PositionCondition(std::ptr::NonNull<Presence>);

// Published conditions are immutable. B releases the allocation on its last reference.
unsafe impl Send for PositionCondition {}
unsafe impl Sync for PositionCondition {}

impl PositionCondition {
    fn new(value: Presence) -> Self {
        let condition = allocate_shared::<Presence>(1);
        unsafe { condition.as_ptr().write(value) };
        Self(condition)
    }
}

impl AsRef<Presence> for PositionCondition {
    fn as_ref(&self) -> &Presence {
        unsafe { self.0.as_ref() }
    }
}

impl std::ops::Deref for PositionCondition {
    type Target = Presence;

    fn deref(&self) -> &Presence { self.as_ref() }
}

impl Clone for PositionCondition {
    fn clone(&self) -> Self {
        unsafe { retain_shared_positions(self.0.cast().as_ptr()) };
        Self(self.0)
    }
}

impl PartialEq for PositionCondition {
    fn eq(&self, other: &Self) -> bool { self.0 == other.0 || self.as_ref() == other.as_ref() }
}
impl Eq for PositionCondition {}
impl PartialOrd for PositionCondition {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}
impl Ord for PositionCondition {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl Drop for PositionCondition {
    fn drop(&mut self) {
        unsafe extern "C" fn take(value: Address) -> SharedPositionChildren {
            unsafe { value.cast::<Presence>().drop_in_place() };
            SharedPositionChildren { first: SharedPositionSpan::empty(), second: SharedPositionSpan::empty() }
        }

        let span = SharedPositionSpan { positions: self.0.cast().as_ptr(), count: 1 };
        unsafe { release_shared_positions(span, size_of::<Presence>(), take) };
    }
}

#[repr(transparent)]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::check) struct PositionChild(SharedPositions);

impl PositionChild {
    fn new(value: Position) -> Self {
        let position = allocate_shared::<Position>(1);
        unsafe { position.as_ptr().write(value) };
        Self(SharedPositions { positions: position, count: 1 })
    }
}

impl AsRef<Position> for PositionChild {
    fn as_ref(&self) -> &Position { self }
}

impl std::ops::Deref for PositionChild {
    type Target = Position;

    fn deref(&self) -> &Position {
        unsafe { self.0.positions.as_ref() }
    }
}

#[repr(C)]
#[derive(Clone,Copy)]
pub(in crate::check) struct PositionIndex {
    pub(in crate::check) kind: u32,
    value: u32,
    pub(in crate::check) identity: Address,
    reg: u64,
}
#[repr(C)]
#[derive(Clone,Copy)]
struct Node { kind:u32, owner:u64, member:u64, word:u64, index:PositionIndex, condition:Address, count:u64 }
#[repr(C)]
struct Item { node:Address, present:u32 }
fn pointer<T>(value:&T)->Address {(value as *const T).cast_mut().cast()}
pub(in crate::check) fn position_index(value:Option<&ApplicationIndex>)->PositionIndex {
    match value {
        None=>PositionIndex{kind:0,value:0,identity:std::ptr::null_mut(),reg:0},
        Some(ApplicationIndex::Constant(value))=>PositionIndex{kind:1,value:*value,identity:std::ptr::null_mut(),reg:0},
        Some(ApplicationIndex::Value(identity,reg))=>PositionIndex{kind:2,value:0,identity:pointer(identity),reg:*reg as u64},
    }
}
pub(in crate::check) unsafe fn required_index(value:PositionIndex)->ApplicationIndex {
    if value.kind==1 {ApplicationIndex::Constant(value.value)}else{ApplicationIndex::Value(unsafe{*value.identity.cast::<ValueIdentity>()},value.reg as Reg)}
}
unsafe fn actual_index(value:PositionIndex)->Option<ApplicationIndex> {if value.kind==0{None}else{Some(unsafe{required_index(value)})}}
#[repr(C)]
#[derive(Clone, Copy)]
pub(in crate::check) struct PositionView {
    payload_offset: usize,
    stride: usize,
    index_payload_offset: usize,
    index_register_offset: usize,
    index_width: usize,
    selection_payload_offset: usize,
    selection_width: usize,
}

pub(in crate::check) const POSITION_VIEW: PositionView = PositionView {
    // repr(C, u32) puts each tag before its aligned union of variant payloads.
    payload_offset: size_of::<u32>().next_multiple_of(align_of::<Position>()),
    stride: size_of::<Position>(),
    index_payload_offset: size_of::<u32>().next_multiple_of(align_of::<ApplicationIndex>()),
    index_register_offset: size_of::<ValueIdentity>().next_multiple_of(align_of::<Reg>()),
    index_width: size_of::<ApplicationIndex>(),
    selection_payload_offset: size_of::<u32>().next_multiple_of(align_of::<ApplicationSelection>()),
    selection_width: size_of::<ApplicationSelection>(),
};

pub(in crate::check) unsafe extern "C" fn equal(a:Address,b:Address)->u32 {u32::from(unsafe{&*a.cast::<Position>()==&*b.cast::<Position>()})}
unsafe extern "C" fn identity_equal(a:Address,b:Address)->u32 {u32::from(unsafe{*a.cast::<ValueIdentity>()==*b.cast::<ValueIdentity>()})}
unsafe extern "C" fn presence_kind(value:Address)->u32 {
    match unsafe{&*value.cast::<owned_presence::Presence>()} {owned_presence::Presence::All=>1,owned_presence::Presence::None=>0,_=>2}
}
unsafe extern "C" fn presence_equal(a:Address,b:Address)->u32 {u32::from(unsafe{&*a.cast::<owned_presence::Presence>()==&*b.cast::<owned_presence::Presence>()})}
unsafe extern "C" fn presence_uses_index(value:Address,at:PositionIndex)->u32 {
    u32::from(unsafe{&*value.cast::<owned_presence::Presence>()}.uses_index(unsafe{required_index(at)}))
}
unsafe extern "C" fn contains(set:Address,value:Address)->u32 {u32::from(unsafe{&*set.cast::<BTreeSet<Position>>()}.contains(unsafe{&*value.cast::<Position>()}))}
// These are scalar keys for the original BTreeSet, never projected trees.
unsafe extern "C" fn contains_parameter(set:Address,owner:u64,word:u64)->u32 {u32::from(unsafe{&*set.cast::<BTreeSet<Position>>()}.contains(&Position::Parameter(owner as usize,word as usize)))}
unsafe extern "C" fn contains_element(set:Address,owner:u64,at:PositionIndex,word:u32)->u32 {u32::from(unsafe{&*set.cast::<BTreeSet<Position>>()}.contains(&Position::ApplicationElement(owner as usize,unsafe{required_index(at)},word)))}
unsafe extern "C" fn contains_input(set:Address,owner:u64,member:u64,at:PositionIndex,word:u32)->u32 {u32::from(unsafe{&*set.cast::<BTreeSet<Position>>()}.contains(&Position::application_input(view::LoopInput{level:owner as usize,index:member as usize},unsafe{actual_index(at)},word)))}
enum Iter { One(btree_set::Iter<'static,Position>), Difference(btree_set::Difference<'static,Position>) }
unsafe extern "C" fn iterate(a:Address,b:Address)->Address {
    // The B traversal borrows the actual input sets and drops this iterator
    // before its synchronous caller returns or any owning set can be changed.
    let a:&'static BTreeSet<Position>=unsafe{&*a.cast::<BTreeSet<Position>>()};
    let iter=if b.is_null(){Iter::One(a.iter())}else{let b:&'static BTreeSet<Position>=unsafe{&*b.cast::<BTreeSet<Position>>()};Iter::Difference(a.difference(b))};
    Box::into_raw(Box::new(iter)).cast()
}
unsafe extern "C" fn next(value:Address)->Item {
    let item=match unsafe{&mut *value.cast::<Iter>()}{Iter::One(iter)=>iter.next(),Iter::Difference(iter)=>iter.next()};
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
        9=>Position::application_input(view::LoopInput{level:shape.owner as usize,index:shape.member as usize},unsafe{actual_index(shape.index)},shape.word as u32),
        10=>Position::Indexed((*unsafe{Box::from_raw(choices.cast::<Vec<Position>>())}).into(),unsafe{required_index(shape.index)}),
        11=>Position::Conditional(PositionCondition::new(unsafe{&*shape.condition.cast::<owned_presence::Presence>()}.clone()),PositionChild::new(*unsafe{Box::from_raw(a.cast::<Position>())}),PositionChild::new(*unsafe{Box::from_raw(b.cast::<Position>())})),
        12=>Position::Both(PositionChild::new(*unsafe{Box::from_raw(a.cast::<Position>())}),PositionChild::new(*unsafe{Box::from_raw(b.cast::<Position>())})),
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
    positions: PositionView,
    equal:unsafe extern "C" fn(Address,Address)->u32,
    identity_equal:unsafe extern "C" fn(Address,Address)->u32,
    presence_kind:unsafe extern "C" fn(Address)->u32,
    presence_equal:unsafe extern "C" fn(Address,Address)->u32,
    presence_uses_index:unsafe extern "C" fn(Address,PositionIndex)->u32,
    contains:unsafe extern "C" fn(Address,Address)->u32,
    contains_parameter:unsafe extern "C" fn(Address,u64,u64)->u32,
    contains_element:unsafe extern "C" fn(Address,u64,PositionIndex,u32)->u32,
    contains_input:unsafe extern "C" fn(Address,u64,u64,PositionIndex,u32)->u32,
    field_coverage: unsafe extern "C" fn(LoopFieldState, Node) -> u32,
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
const HOST:Host=Host{positions:POSITION_VIEW,equal,identity_equal,presence_kind,presence_equal,presence_uses_index,contains,contains_parameter,contains_element,contains_input,field_coverage,iterate,next,drop_iterator,fresh_vector,push_vector,vector_count,vector_at,make_position,clone_position,fresh_set,insert_set};
#[repr(C)]
struct DatasetPositionCleanup {
    position: unsafe extern "C" fn(Address),
    vector: unsafe extern "C" fn(Address),
}

unsafe extern "C" fn drop_position(value: Address) { drop(unsafe { Box::from_raw(value.cast::<Position>()) }); }
unsafe extern "C" fn drop_position_vector(value: Address) { drop(unsafe { Box::from_raw(value.cast::<Vec<Position>>()) }); }

unsafe extern "C" {
    #[link_name="bootstrap.ownership_position::select"] fn source_select(when:Address,a:Address,b:Address,equal:EqualPosition,presence_kind:PresenceKind)->u32;
    #[link_name="bootstrap.ownership_position::both"] fn source_both(a:Address,b:Address,equal:EqualPosition)->u32;
    #[link_name="bootstrap.ownership_position::index"] fn source_index(values:Address,index:PositionIndex,width:u64,host:Host)->Address;
    #[link_name="bootstrap.ownership_position::word"] fn source_word(value:Address,index:PositionIndex,word:u32,host:Host)->Address;
    #[link_name = "bootstrap.ownership_position::dataset_position"]
    fn source_dataset_position(value: Address, selected: PositionIndex, host: Host, cleanup: DatasetPositionCleanup) -> Address;
    #[link_name="bootstrap.ownership_position::ready"] fn source_ready(value:Address,known:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_position::word_ready"] fn source_word_ready(value:Address,at:PositionIndex,word:u32,known:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_position::prefix_ready"] fn source_prefix(value:Address,length:u32,word:u32,known:Address,host:Host)->u32;
    #[link_name="bootstrap.ownership_position::uses_scope"] fn source_scope(value:Address,owner:u64,host:Host)->u32;
    #[link_name = "bootstrap.ownership_position::merge_initialization"]
    fn source_merge_initialization(
        exits: Address, count: usize, condition_count: usize, host: Host,
        initialization: InitializationHost,
    ) -> Address;
}
pub(super) fn selected(when:owned_presence::Presence,a:Position,b:Position)->Position {
    match unsafe{source_select(pointer(&when),pointer(&a),pointer(&b),equal,presence_kind)} {0=>a,1=>b,_=>Position::Conditional(PositionCondition::new(when),PositionChild::new(a),PositionChild::new(b))}
}
pub(super) fn both(a:Position,b:Position)->Position {match unsafe{source_both(pointer(&a),pointer(&b),equal)} {0=>a,1=>b,_=>Position::Both(PositionChild::new(a),PositionChild::new(b))}}
pub(super) fn indexed(values:Vec<Position>,at:ApplicationIndex,width:usize)->Vec<Position> {*unsafe{Box::from_raw(source_index(pointer(&values),position_index(Some(&at)),width as u64,HOST).cast::<Vec<Position>>())}}
pub(super) fn word(value:&Position,at:Option<ApplicationIndex>,word:u32)->Position {*unsafe{Box::from_raw(source_word(pointer(value),position_index(at.as_ref()),word,HOST).cast::<Position>())}}
pub(super) fn dataset_position(value: &Position, operand: bounds::Operand) -> Option<Position> {
    let selected = match operand {
        bounds::Operand::Constant(Scalar::U32, value) => Some(ApplicationIndex::Constant(value as u32)),
        bounds::Operand::Value(id @ ValueIdentity::Register(reg)) => Some(ApplicationIndex::Value(id, reg)),
        _ => None,
    };
    let cleanup = DatasetPositionCleanup { position: drop_position, vector: drop_position_vector };
    let result = unsafe { source_dataset_position(pointer(value), position_index(selected.as_ref()), HOST, cleanup) };
    (!result.is_null()).then(|| *unsafe { Box::from_raw(result.cast::<Position>()) })
}
#[repr(C)]
struct DatasetMappedLeaf { shape: Node, position: Address, action: u32 }

#[repr(C)]
struct DatasetOutputMapping {
    context: Address,
    scope: u64,
    field: unsafe extern "C" fn(Address, Node) -> DatasetMappedLeaf,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_position::dataset_output_position"]
    fn source_dataset_output_position(value: Address, output: DatasetOutputMapping, host: Host,
        cleanup: DatasetPositionCleanup) -> Address;
}

pub(in crate::check) fn project_dataset_output(
    value: Position, scope: usize, output: &address::DatasetOutput,
) -> Position {
    unsafe extern "C" fn make_field(context: Address, node: Node) -> DatasetMappedLeaf {
        let output = unsafe { &*context.cast::<address::DatasetOutput>() };
        let index = unsafe { actual_index(node.index) }.map(|index| match index {
            ApplicationIndex::Constant(value) => bounds::Operand::Constant(Scalar::U32, Bits::from(value)),
            ApplicationIndex::Value(identity, _) => bounds::Operand::Value(identity),
        });
        let position = Box::into_raw(Box::new(output.field(index, node.word as u32))).cast();
        DatasetMappedLeaf { shape: node, position, action: 3 }
    }

    let mapping = DatasetOutputMapping { context: pointer(output), scope: scope as u64, field: make_field };
    let cleanup = DatasetPositionCleanup { position: drop_position, vector: drop_position_vector };
    let result = unsafe { source_dataset_output_position(pointer(&value), mapping, HOST, cleanup) };
    if result.is_null() { value } else { *unsafe { Box::from_raw(result.cast::<Position>()) } }
}

#[repr(C)]
struct InitializationReadSpan {
    positions: Address,
    count: usize,
    stride: usize,
}

impl InitializationReadSpan {
    fn from_positions(positions: &[Position]) -> Self {
        Self {
            positions: positions.as_ptr().cast_mut().cast(),
            count: positions.len(),
            stride: size_of::<Position>(),
        }
    }
}

#[repr(u32)]
enum InitializationReadMode { Complete, Borrow }

#[repr(C)]
struct InitializationReadHost {
    input: unsafe extern "C" fn(Address) -> u32,
    emit: unsafe extern "C" fn(Address, Address),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_position::initialization_read"]
    fn source_initialization_read(
        positions: InitializationReadSpan, known: Address, output: Address, mode: InitializationReadMode,
        host: Host, read: InitializationReadHost,
    ) -> u32;
}

unsafe extern "C" fn input_initialization_read(value: Address) -> u32 {
    let Position::Borrowed(borrow, ..) = (unsafe { &*value.cast::<Position>() }) else { unreachable!() };
    u32::from(!borrow.empty() && borrow.sources.iter().all(|source| match source {
        view::Source::Parameter(ValueIdentity::Register(_) | ValueIdentity::ParameterContent(..), _) => true,
        view::Source::Position(origin) => origin.symbolic_input() && !origin.has_local(),
        _ => false,
    }) && borrow.inputs.is_empty() && borrow.calls.is_empty())
}

unsafe extern "C" fn emit_initialization_read(output: Address, value: Address) {
    let output = unsafe { &mut *output.cast::<view::Borrow>() };
    match unsafe { &*value.cast::<Position>() } {
        Position::ApplicationInput(input, ..) => output.inputs.insert(*input),
        Position::Borrowed(borrow, layout, word) => {
            let (scalar, _) = layout.parts.iter().find(|(_, at)| at == word).unwrap();
            let origin = address::Origin {
                root: address::Root::Derived(borrow.as_ref().clone()),
                layout: address::Layout { size: scalar.size(), align: scalar.size(), parts: Arc::from([(*scalar, 0)]) },
                base: layout.clone(),
                words: Arc::from([*word]),
                positions: Default::default(),
                writable: false,
                initialized: Arc::from([false]),
            };
            output.union(&origin.normalize());
        }
        _ => unreachable!(),
    }
}

// The original read snapshot remains borrowed throughout the B traversal.
const READ_HOST: InitializationReadHost = InitializationReadHost {
    input: input_initialization_read,
    emit: emit_initialization_read,
};

pub(in crate::check) fn initialization_read_complete(read: &address::InitializationRead) -> bool {
    unsafe {
        source_initialization_read(InitializationReadSpan::from_positions(&read.positions), pointer(&read.initialized),
            std::ptr::null_mut(), InitializationReadMode::Complete, HOST, READ_HOST) != 0
    }
}

pub(in crate::check) fn initialization_read_borrow(read: &address::InitializationRead) -> Option<view::Borrow> {
    let mut result = view::Borrow::default();
    let missing = unsafe {
        source_initialization_read(InitializationReadSpan::from_positions(&read.positions), pointer(&read.initialized),
            std::ptr::from_mut(&mut result).cast(), InitializationReadMode::Borrow, HOST, READ_HOST) != 0
    };
    missing.then_some(result)
}

pub(super) fn initialized(value:&Position,known:&BTreeSet<Position>)->bool {unsafe{source_ready(pointer(value),pointer(known),HOST)!=0}}
pub(super) fn word_initialized(value:&Position,at:Option<ApplicationIndex>,word:u32,known:&BTreeSet<Position>)->bool {
    unsafe{source_word_ready(pointer(value),position_index(at.as_ref()),word,pointer(known),HOST)!=0}
}
pub(super) fn prefix(value:&Position,length:u32,word:u32,known:&BTreeSet<Position>)->bool {unsafe{source_prefix(pointer(value),length,word,pointer(known),HOST)!=0}}
pub(super) fn scope(value:&Position,owner:usize)->bool {unsafe{source_scope(pointer(value),owner as u64,HOST)!=0}}
#[repr(C)]
struct InitializationHost {
    positions: unsafe extern "C" fn(Address, usize) -> Address,
    retain_conditional: unsafe extern "C" fn(Address, usize, Address),
    append_conditional: unsafe extern "C" fn(Address, usize, Address),
    drop_positions: unsafe extern "C" fn(Address),
}

struct InitializationExits<'a, T> {
    states: *mut T,
    conditions: &'a [owned_presence::Presence],
    positions: fn(&mut T) -> &mut BTreeSet<Position>,
}

impl<T> InitializationExits<'_, T> {
    unsafe fn positions(&self, index: usize) -> &mut BTreeSet<Position> {
        // B borrows distinct exits during each intersection and ends those
        // borrows before retaining or moving any exit's positions.
        (self.positions)(unsafe { &mut *self.states.add(index) })
    }
}

pub(in crate::check) fn merge_initialization<T>(
    states: &mut [T],
    conditions: &[owned_presence::Presence],
    positions: fn(&mut T) -> &mut BTreeSet<Position>,
) -> BTreeSet<Position> {
    unsafe extern "C" fn get<T>(exits: Address, index: usize) -> Address {
        let exits = unsafe { &*exits.cast::<InitializationExits<'_, T>>() };
        pointer(unsafe { exits.positions(index) })
    }
    unsafe extern "C" fn retain<T>(exits: Address, index: usize, common: Address) {
        let exits = unsafe { &*exits.cast::<InitializationExits<'_, T>>() };
        let common = unsafe { &*common.cast::<BTreeSet<Position>>() };
        unsafe { exits.positions(index) }.retain(|position| !common.contains(position));
    }
    unsafe extern "C" fn append<T>(exits: Address, index: usize, common: Address) {
        let exits = unsafe { &*exits.cast::<InitializationExits<'_, T>>() };
        let common = unsafe { &mut *common.cast::<BTreeSet<Position>>() };
        let positions = std::mem::take(unsafe { exits.positions(index) });
        let when = &exits.conditions[index];
        common.extend(positions.into_iter().map(|position| {
            Position::selected(when.clone(), position, Position::Initialized)
        }));
    }
    unsafe extern "C" fn release(positions: Address) {
        drop(unsafe { Box::from_raw(positions.cast::<BTreeSet<Position>>()) });
    }

    let exits = InitializationExits { states: states.as_mut_ptr(), conditions, positions };
    let host = InitializationHost {
        positions: get::<T>, retain_conditional: retain::<T>, append_conditional: append::<T>,
        drop_positions: release,
    };
    let result = unsafe {
        source_merge_initialization(
            pointer(&exits), states.len(), conditions.len().min(states.len()), HOST, host,
        )
    };
    *unsafe { Box::from_raw(result.cast::<BTreeSet<Position>>()) }
}

#[repr(C)]
struct LoopFieldRow { position: Address, words: *const u32, count: usize }

#[repr(C)]
struct LoopFieldState { known: Address, path: Address, effects: Address }

#[repr(C)]
struct LoopFieldHost {
    row: unsafe extern "C" fn(Address, usize) -> LoopFieldRow,
    edge: unsafe extern "C" fn(Address, usize) -> LoopFieldState,
    publish: unsafe extern "C" fn(Address, Address),
}

struct LoopFieldExit<'a, R, T> {
    rows: &'a [R],
    fields: fn(&R) -> (&Position, &[u32]),
    states: &'a [T],
    edge: fn(&T) -> (&BTreeSet<Position>, &bounds::Clause, Option<&view_contracts::InitializedEffects>),
}

unsafe extern "C" fn loop_field_row<R, T>(context: Address, at: usize) -> LoopFieldRow {
    let context = unsafe { &*context.cast::<LoopFieldExit<'_, R, T>>() };
    let (position, words) = (context.fields)(&context.rows[at]);
    LoopFieldRow { position: pointer(position), words: words.as_ptr(), count: words.len() }
}

unsafe extern "C" fn loop_field_state<R, T>(context: Address, at: usize) -> LoopFieldState {
    let context = unsafe { &*context.cast::<LoopFieldExit<'_, R, T>>() };
    let (known, path, effects) = (context.edge)(&context.states[at]);
    LoopFieldState {
        known: pointer(known),
        path: pointer(path),
        effects: effects.map_or(std::ptr::null_mut(), pointer),
    }
}

unsafe extern "C" fn publish_loop_field(output: Address, value: Address) {
    let position = *unsafe { Box::from_raw(value.cast::<Position>()) };
    unsafe { &mut *output.cast::<BTreeSet<Position>>() }.insert(position);
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_position::loop_field_initialization"]
    fn source_loop_fields(
        context: Address, output: Address, rows: usize, states: usize,
        selected: PositionIndex, host: Host, exit: LoopFieldHost,
    );
}

#[repr(C)]
struct ArrayRange { begin: u32, limit: u32, nonempty: u32 }

#[repr(C)]
struct ArrayState { known: Address, ready: u32 }

#[repr(C)]
struct ArrayExitHost {
    next: unsafe extern "C" fn(Address) -> Item,
    state: unsafe extern "C" fn(Address, usize, Address) -> ArrayState,
}

type LivePosition<'a, T> = dyn Fn(&T, &Position) -> bool + 'a;

struct ArrayExit<'a, T> {
    states: &'a [T],
    positions: fn(&T) -> &BTreeSet<Position>,
    live: Option<&'a LivePosition<'a, T>>,
    candidates: btree_set::IntoIter<&'a Position>,
}

unsafe extern "C" fn next_array_position<T>(context: Address) -> Item {
    let exit = unsafe { &mut *context.cast::<ArrayExit<'_, T>>() };
    match exit.candidates.next() {
        Some(value) => Item { node: pointer(value), present: 1 },
        None => Item { node: std::ptr::null_mut(), present: 0 },
    }
}

unsafe extern "C" fn array_state<T>(context: Address, at: usize, item: Address) -> ArrayState {
    let exit = unsafe { &*context.cast::<ArrayExit<'_, T>>() };
    let state = &exit.states[at];
    match exit.live {
        Some(ready) => ArrayState {
            known: std::ptr::null_mut(),
            ready: u32::from(ready(state, unsafe { &*item.cast::<Position>() })),
        },
        None => ArrayState { known: pointer((exit.positions)(state)), ready: 0 },
    }
}


unsafe extern "C" {
    #[link_name = "bootstrap.ownership_position::array_initialization"]
    fn source_array_initialization(
        context: Address, output: Address, count: usize, range: ArrayRange,
        index: PositionIndex, host: Host, exits: ArrayExitHost,
    );
}

impl Position {
    pub(in crate::check) fn initialize_loop_fields<R, T>(
        rows: &[R],
        fields: fn(&R) -> (&Self, &[u32]),
        states: &[T],
        edge: fn(&T) -> (&BTreeSet<Self>, &bounds::Clause, Option<&view_contracts::InitializedEffects>),
        output: &mut BTreeSet<Self>,
        at: ApplicationIndex,
    ) {
        let context = LoopFieldExit { rows, fields, states, edge };
        let exit = LoopFieldHost {
            row: loop_field_row::<R, T>,
            edge: loop_field_state::<R, T>,
            publish: publish_loop_field,
        };
        unsafe {
            source_loop_fields(
                pointer(&context), std::ptr::from_mut(output).cast(),
                rows.len(), states.len(), position_index(Some(&at)), HOST, exit,
            );
        }
    }

    pub(in crate::check) fn initialize_array_exits<T>(
        states: &[T],
        positions: fn(&T) -> &BTreeSet<Self>,
        live: Option<&dyn Fn(&T, &Self) -> bool>,
        output: &mut BTreeSet<Self>,
        at: ApplicationIndex,
        range: Option<(u32, u32)>,
        nonempty: bool,
    ) {
        // Keep the original union's order and duplicate elimination while
        // lending the keys. Only facts published to the exit own a value.
        let candidates: BTreeSet<_> = states.iter().flat_map(positions).collect();
        let mut context = ArrayExit { states, positions, live, candidates: candidates.into_iter() };
        let (begin, limit) = range.unwrap_or((0, 0));
        let range = ArrayRange { begin, limit, nonempty: u32::from(nonempty) };
        unsafe {
            source_array_initialization(
                std::ptr::from_mut(&mut context).cast(), std::ptr::from_mut(output).cast(),
                states.len(), range, position_index(Some(&at)), HOST,
                ArrayExitHost { next: next_array_position::<T>, state: array_state::<T> },
            );
        }
    }
}


#[repr(C)]
struct InitializationEffectsHost {
    enter: unsafe extern "C" fn(Address, Node, u64),
    leave: unsafe extern "C" fn(Address),
    effect: unsafe extern "C" fn(Address, Address, PositionIndex) -> u32,
}

struct InitializationEffects<F> {
    root: bounds::Condition,
    branches: Vec<bounds::Condition>,
    effect: F,
}

unsafe extern "C" fn enter_effect<F>(context: Address, shape: Node, at: u64) {
    let state = unsafe { &mut *context.cast::<InitializationEffects<F>>() };
    let condition = if shape.kind == 11 {
        let when = unsafe { &*shape.condition.cast::<owned_presence::Presence>() }.condition();
        if at == 0 { when } else { when.negate() }
    } else {
        bounds::Condition::selected(unsafe { required_index(shape.index) }, at as usize)
    };
    let outer = state.branches.last().unwrap_or(&state.root);
    state.branches.push(outer.clone().logical(condition, false));
}

unsafe extern "C" fn leave_effect<F>(context: Address) {
    let state = unsafe { &mut *context.cast::<InitializationEffects<F>>() };
    state.branches.pop();
}

unsafe extern "C" fn initialization_effect<F>(context: Address, value: Address, index: PositionIndex) -> u32
where F: FnMut(&Position, &bounds::Condition, PositionIndex) -> bool {
    let state = unsafe { &mut *context.cast::<InitializationEffects<F>>() };
    let position = unsafe { &*value.cast::<Position>() };
    let when = state.branches.last().unwrap_or(&state.root);
    u32::from((state.effect)(position, when, index))
}


#[repr(C)]
#[derive(Clone, Copy, Default)]
struct InitializationFilter {
    kind: u32,
    indexed: u32,
    owner: u64,
    member: u64,
    word: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_position::visit_initialization_effects"]
    fn source_initialization_effects(
        initialized: Address, context: Address, filter: InitializationFilter,
        host: Host, sink: InitializationEffectsHost,
    ) -> u32;
    #[link_name = "bootstrap.ownership_position::visit_initialization_position"]
    fn source_initialization_position(
        position: Address, context: Address, filter: InitializationFilter,
        host: Host, sink: InitializationEffectsHost,
    ) -> u32;
}

unsafe extern "C" fn field_coverage(edge: LoopFieldState, field: Node) -> u32 {
    let path = unsafe { &*edge.path.cast::<bounds::Clause>() };
    let mut remaining = path.clone();
    let known = unsafe { &*edge.known.cast::<BTreeSet<Position>>() };
    let wanted = unsafe { actual_index(field.index) };
    let filter = InitializationFilter {
        kind: field.kind, indexed: field.index.kind, owner: field.owner, member: field.member, word: field.word,
    };
    let mut cover = |_: &Position, when: &bounds::Condition, index: PositionIndex| {
        let actual = unsafe { actual_index(index) };
        remaining = match (wanted, actual) {
            (Some(wanted), Some(actual)) if wanted != actual => {
                let equality = Presence::indices(Comparison::Equal, wanted, actual).condition();
                when.clone().logical(equality, false).assume(&remaining, false)
            }
            _ => when.assume(&remaining, false),
        };
        !remaining.possible()
    };
    let complete = visit_initializations(
        InitializationInput::Set(known), bounds::Condition::constant(true), filter, &mut cover,
    ) || unsafe { edge.effects.cast::<view_contracts::InitializedEffects>().as_ref() }.is_some_and(|effects| {
        effects.visit_guarded_positions(path, |position, when| {
            visit_initializations(InitializationInput::Position(position), when.clone(), filter, &mut cover)
        })
    });
    u32::from(complete || !remaining.possible())
}

enum InitializationInput<'a> {
    Set(&'a BTreeSet<Position>),
    Position(&'a Position),
}

fn visit_initializations<F>(
    input: InitializationInput<'_>, root: bounds::Condition, filter: InitializationFilter, effect: F,
) -> bool
where F: FnMut(&Position, &bounds::Condition, PositionIndex) -> bool {
    let mut context = InitializationEffects { root, branches: Vec::new(), effect };
    let sink = InitializationEffectsHost {
        enter: enter_effect::<F>, leave: leave_effect::<F>, effect: initialization_effect::<F>,
    };
    let context = std::ptr::from_mut(&mut context).cast();
    unsafe {
        (match input {
            InitializationInput::Set(set) => source_initialization_effects(pointer(set), context, filter, HOST, sink),
            InitializationInput::Position(position) => source_initialization_position(pointer(position), context, filter, HOST, sink),
        }) != 0
    }
}

impl Position {
    pub(in crate::check) fn visit_initialization_effects<F>(
        initialized: &BTreeSet<Self>, path: &bounds::Clause, mut effect: F,
    ) -> bool
    where F: FnMut(&Self, &bounds::Condition) -> bool {
        visit_initializations(
            InitializationInput::Set(initialized), bounds::Condition::execution(path),
            InitializationFilter::default(), |position, when, _| effect(position, when),
        )
    }
}


struct Resolution<'a,'w> {
    builder: &'a Builder<'w>,
    initial: Option<&'a Presence>,
    initial_condition: Option<&'a bounds::Condition>,
    assumptions: Vec<(Presence,bounds::Condition)>,
    choices: Vec<Option<Presence>>,
    results: Vec<Position>,
}

impl Resolution<'_,'_> {
    fn truth(&self, when: &Presence) -> u32 {
        let assuming = self.assumptions.last().map(|(_, condition)| condition).or(self.initial_condition);
        match self.builder.condition_truth(&when.condition(),assuming) {
            Some(false) => 0,
            Some(true) => 1,
            None => 2,
        }
    }

    fn assume(&mut self, when: Presence) {
        let prior = self.assumptions.last().map(|(presence, _)| presence).or(self.initial);
        let presence = prior.cloned().unwrap_or(Presence::All).and(when);
        let condition = presence.condition();
        self.assumptions.push((presence,condition));
    }
}

#[repr(C)]
struct ResolutionBase { choices: u64, results: u64 }
#[repr(C)]
struct ResolutionPair { left: Address, right: Address }

unsafe fn resolution<'a,'w>(context: Address) -> &'a mut Resolution<'a,'w> {
    unsafe { &mut *context.cast() }
}

unsafe extern "C" fn resolution_truth(context: Address, when: Address) -> u32 {
    let state = unsafe { &*context.cast::<Resolution<'_,'_>>() };
    state.truth(unsafe { &*when.cast::<Presence>() })
}

unsafe extern "C" fn resolution_assume(context: Address, when: Address, negate: u32) {
    let when = unsafe { &*when.cast::<Presence>() }.clone();
    unsafe { resolution(context) }.assume(if negate == 0 { when } else { when.not() });
}

unsafe extern "C" fn resolution_leave(context: Address) {
    unsafe { resolution(context) }.assumptions.pop();
}

unsafe extern "C" fn resolution_mark(context: Address) -> ResolutionBase {
    let state = unsafe { &*context.cast::<Resolution<'_,'_>>() };
    ResolutionBase { choices: state.choices.len() as u64, results: state.results.len() as u64 }
}

unsafe extern "C" fn resolution_query_choice(context: Address, index: PositionIndex, at: u64) -> u32 {
    let state = unsafe { resolution(context) };
    let when = Presence::indices(
        Comparison::Equal, unsafe { required_index(index) }, ApplicationIndex::Constant(at as u32),
    );
    let truth = state.truth(&when);
    if truth != 1 {
        state.choices.push((truth == 2).then_some(when));
    }
    truth
}

unsafe extern "C" fn resolution_assume_choice(context: Address, at: u64) -> u32 {
    let state = unsafe { resolution(context) };
    match state.choices[at as usize].take() {
        Some(when) => { state.assume(when); 1 }
        None => 0,
    }
}

unsafe extern "C" fn resolution_release_choices(context: Address, base: u64) {
    unsafe { resolution(context) }.choices.truncate(base as usize);
}

unsafe extern "C" fn resolution_emit(context: Address, value: Address) {
    let position = if value.is_null() {
        Position::Initialized
    } else {
        unsafe { &*value.cast::<Position>() }.clone()
    };
    unsafe { resolution(context) }.results.push(position);
}

unsafe extern "C" fn resolution_pair(context: Address) -> ResolutionPair {
    let state = unsafe { &*context.cast::<Resolution<'_,'_>>() };
    let count = state.results.len();
    ResolutionPair { left: pointer(&state.results[count-2]), right: pointer(&state.results[count-1]) }
}

unsafe extern "C" fn resolution_finish_pair(context: Address, source: Address, selected: u32) {
    let results = &mut unsafe { resolution(context) }.results;
    let right = results.pop().unwrap();
    let left = results.pop().unwrap();
    let selected = match selected {
        0 => left,
        1 => right,
        _ => match unsafe { &*source.cast::<Position>() } {
            Position::Conditional(when, ..) =>
                Position::Conditional(when.clone(),PositionChild::new(left),PositionChild::new(right)),
            Position::Both(..) => Position::Both(PositionChild::new(left),PositionChild::new(right)),
            _ => unsafe { std::hint::unreachable_unchecked() },
        },
    };
    results.push(selected);
}

unsafe extern "C" fn resolution_result(context: Address, at: u64) -> Address {
    let state = unsafe { &*context.cast::<Resolution<'_,'_>>() };
    pointer(&state.results[at as usize])
}

unsafe extern "C" fn resolution_finish_indexed(context: Address, index: PositionIndex, base: u64, uniform: u32) {
    let results = &mut unsafe { resolution(context) }.results;
    let mut choices = results.drain(base as usize..);
    let position = if uniform != 0 {
        choices.next().unwrap()
    } else {
        Position::Indexed(choices.by_ref().collect(),unsafe { required_index(index) })
    };
    drop(choices);
    results.push(position);
}

#[repr(C)]
struct ResolutionHost {
    positions: PositionView,
    equal: EqualPosition,
    presence_kind: PresenceKind,
    truth: unsafe extern "C" fn(Address,Address) -> u32,
    assume: unsafe extern "C" fn(Address,Address,u32),
    leave: unsafe extern "C" fn(Address),
    mark: unsafe extern "C" fn(Address) -> ResolutionBase,
    query_choice: unsafe extern "C" fn(Address,PositionIndex,u64) -> u32,
    assume_choice: unsafe extern "C" fn(Address,u64) -> u32,
    release_choices: unsafe extern "C" fn(Address,u64),
    emit: unsafe extern "C" fn(Address,Address),
    pair: unsafe extern "C" fn(Address) -> ResolutionPair,
    finish_pair: unsafe extern "C" fn(Address,Address,u32),
    result: unsafe extern "C" fn(Address,u64) -> Address,
    finish_indexed: unsafe extern "C" fn(Address,PositionIndex,u64,u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_position::resolve_position"]
    fn source_resolve_position(value: Address, context: Address, resolver: ResolutionHost);
}

pub(in crate::check) fn resolve_initialization(
    builder: &Builder<'_>,
    value: &Position,
    initial: Option<&Presence>,
    initial_condition: Option<&bounds::Condition>,
) -> Position {
    let mut state = Resolution {
        builder, initial, initial_condition,
        assumptions: Vec::new(), choices: Vec::new(), results: Vec::new(),
    };
    let resolver = ResolutionHost {
        positions: POSITION_VIEW, equal, presence_kind,
        truth: resolution_truth, assume: resolution_assume, leave: resolution_leave,
        mark: resolution_mark, query_choice: resolution_query_choice, assume_choice: resolution_assume_choice,
        release_choices: resolution_release_choices, emit: resolution_emit, pair: resolution_pair,
        finish_pair: resolution_finish_pair, result: resolution_result, finish_indexed: resolution_finish_indexed,
    };
    unsafe { source_resolve_position(pointer(value),(&mut state as *mut Resolution<'_,'_>).cast(),resolver) };
    state.results.pop().unwrap()
}
