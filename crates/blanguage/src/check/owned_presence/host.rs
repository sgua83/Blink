//! Native storage operations for the original Presence and Position owners.
use super::*;
use ownership::{
    PositionIndex, PositionView, POSITION_VIEW,
    position_equal, position_index, required_index, retain_shared_positions,
};
use std::ffi::c_void;

type Address = *mut c_void;
#[repr(C)]
#[derive(Clone, Copy)]
struct Selection { operation: u32, a: PositionIndex, b: PositionIndex }
#[repr(C)]
pub(in crate::check) struct PresenceView {
    payload_offset: usize,
    first_index_offset: usize,
    ownership_payload_offset: usize,
    positions: &'static PositionView,
    width: usize,
}

pub(in crate::check) const PRESENCE_VIEW: PresenceView = PresenceView {
    payload_offset: size_of::<u32>().next_multiple_of(align_of::<Presence>()),
    first_index_offset: size_of::<Comparison>().next_multiple_of(align_of::<ownership::ApplicationIndex>()),
    ownership_payload_offset: size_of::<u32>().next_multiple_of(align_of::<Ownership>()),
    positions: &POSITION_VIEW,
    width: size_of::<Presence>(),
};

fn pointer<T>(value: &T) -> Address { (value as *const T).cast_mut().cast() }
fn comparison(value: u32) -> Comparison {
    [Comparison::Equal, Comparison::NotEqual, Comparison::Less,
     Comparison::Greater, Comparison::LessEqual, Comparison::GreaterEqual][value as usize]
}
unsafe fn take(value: Address) -> Presence { std::mem::replace(unsafe { &mut *value.cast::<Presence>() }, Presence::None) }
unsafe extern "C" fn input_mentions(value: Address, identity: Address) -> u32 {
    let value = unsafe { &*value.cast::<PresenceInput>() };
    u32::from(value.condition().mentions(unsafe { *identity.cast::<ValueIdentity>() }))
}

unsafe extern "C" fn capture_input(output: Address, value: Address) {
    let value = PresenceInput::capture(unsafe { &*value.cast::<Value>() });
    unsafe { *output.cast::<Presence>() = Presence::Input(value); }
}

unsafe extern "C" fn clone_input(output: Address, value: Address) {
    let value = unsafe { &*value.cast::<PresenceInput>() }.clone();
    unsafe { *output.cast::<Presence>() = Presence::Input(value); }
}

#[repr(u32)]
pub(in crate::check) enum SharedNodeKind { Position = 0, Presence = 1 }

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_position::publish_presence"]
    fn publish_presence(value: Address, layout: &PresenceView) -> Address;
    #[link_name = "bootstrap.ownership_position::release_shared_presence"]
    fn release_shared_presence(value: Address, layout: &PresenceView,
        drop_payload: unsafe extern "C" fn(Address, SharedNodeKind));
    #[link_name = "bootstrap.ownership_position::take_unique_shared_position"]
    fn take_unique_shared_position(value: Address, output: Address, width: usize) -> u32;
}

pub(in crate::check) unsafe extern "C" fn drop_payload(value: Address, kind: SharedNodeKind) {
    unsafe {
        match kind {
            SharedNodeKind::Position => value.cast::<Position>().drop_in_place(),
            SharedNodeKind::Presence => value.cast::<PresenceInput>().drop_in_place(),
        }
    }
}

#[repr(transparent)]
pub(in crate::check) struct PresenceNode(std::ptr::NonNull<Presence>);

// A published node is immutable. The shared storage counts ownership atomically.
unsafe impl Send for PresenceNode {}
unsafe impl Sync for PresenceNode {}

impl PresenceNode {
    pub(in crate::check) fn new(mut value: Presence) -> Self {
        const { assert!(align_of::<Presence>() <= 16); }
        let node = unsafe { publish_presence(std::ptr::from_mut(&mut value).cast(), &PRESENCE_VIEW) };
        Self(unsafe { std::ptr::NonNull::new_unchecked(node.cast()) })
    }

    fn into_value(self) -> Presence {
        let mut result = std::mem::MaybeUninit::<Presence>::uninit();
        let moved = unsafe {
            take_unique_shared_position(self.0.cast().as_ptr(), result.as_mut_ptr().cast(), size_of::<Presence>())
        };
        if moved == 0 {
            return self.as_ref().clone();
        }
        std::mem::forget(self);
        unsafe { result.assume_init() }
    }
}

impl std::ops::Deref for PresenceNode {
    type Target = Presence;
    fn deref(&self) -> &Presence { unsafe { self.0.as_ref() } }
}
impl AsRef<Presence> for PresenceNode {
    fn as_ref(&self) -> &Presence { self }
}
impl Clone for PresenceNode {
    fn clone(&self) -> Self {
        unsafe { retain_shared_positions(self.0.cast().as_ptr()) };
        Self(self.0)
    }
}
impl Drop for PresenceNode {
    fn drop(&mut self) {
        unsafe { release_shared_presence(self.0.cast().as_ptr(), &PRESENCE_VIEW, drop_payload) };
    }
}
impl PartialEq for PresenceNode {
    fn eq(&self, other: &Self) -> bool { self.cmp(other).is_eq() }
}
impl Eq for PresenceNode {}
impl PartialOrd for PresenceNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}
impl Ord for PresenceNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.0 == other.0 { return std::cmp::Ordering::Equal; }
        self.as_ref().cmp(other.as_ref())
    }
}

type Children = unsafe extern "C" fn(Address, Address, Address, Address, Host) -> u32;
type Scoped = unsafe extern "C" fn(Address, Address, Host) -> u32;
unsafe extern "C" fn children_scope(value: Address, context: Address, host: Host, apply: Children) -> u32 {
    let (mut left, mut right) = match unsafe { take(value) } {
        Presence::Not(left) => (left.into_value(), Presence::None),
        Presence::Both(left, right) | Presence::Either(left, right) => (left.into_value(), right.into_value()),
        leaf => (leaf, Presence::None),
    };
    unsafe { apply(value, pointer(&mut left), pointer(&mut right), context, host) }
}
unsafe extern "C" fn clone_scope(value: Address, context: Address, host: Host, apply: Scoped) -> u32 {
    let mut value = unsafe { &*value.cast::<Presence>() }.clone();
    unsafe { apply(pointer(&mut value), context, host) }
}
unsafe extern "C" fn truth_scope(value: u32, context: Address, host: Host, apply: Scoped) -> u32 {
    let mut value = if value == 0 { Presence::None } else { Presence::All };
    unsafe { apply(pointer(&mut value), context, host) }
}
unsafe extern "C" fn condition_input(output: Address, value: Address) {
    let value = unsafe { &*value.cast::<PresenceInput>() }.condition();
    unsafe { output.cast::<bounds::Condition>().write(value); }
}
unsafe extern "C" fn condition_index(output: Address, kind: u32, a: PositionIndex, b: PositionIndex) {
    let operand = |value| match unsafe { required_index(value) } {
        ownership::ApplicationIndex::Constant(value) => bounds::Operand::Constant(Scalar::U32, Bits::from(value)),
        ownership::ApplicationIndex::Value(value, _) => bounds::Operand::Value(value),
    };
    let a = operand(a); let b = operand(b); let proof = bounds::Proof::default();
    let value = if kind == 0 { bounds::Condition::index_equal(a, b) }
        else { bounds::Condition::index_below((a, &proof), (b, &proof), kind == 1) };
    unsafe { output.cast::<bounds::Condition>().write(value); }
}
unsafe extern "C" fn condition_substitute(output: Address, value: Address, inputs: Address, a: PositionIndex, b: PositionIndex) {
    let inputs = unsafe { &*inputs.cast::<BTreeMap<ValueIdentity, bounds::Proof>>() };
    let omitted = |index: PositionIndex| if index.kind == 2 { Some(unsafe { *index.identity.cast::<ValueIdentity>() }) } else { None };
    let a = omitted(a); let b = omitted(b);
    let condition = unsafe { &*value.cast::<bounds::Condition>() };
    let substituted = condition.substitute_values_with(|id| {
        if Some(id) == a || Some(id) == b { None } else { inputs.get(&id) }
    });
    unsafe { output.cast::<bounds::Condition>().write(substituted); }
}
unsafe extern "C" fn input_offset(inputs: Address, index: PositionIndex) -> u32 {
    let inputs = unsafe { &*inputs.cast::<BTreeMap<ValueIdentity, bounds::Proof>>() };
    let found = match unsafe { required_index(index) } {
        ownership::ApplicationIndex::Constant(_) => false,
        ownership::ApplicationIndex::Value(id, _) => inputs.get(&id).is_some_and(|proof| proof.domains(0).contains(&IndexDomain::Offset(id, 1))),
    };
    u32::from(found)
}
unsafe extern "C" fn condition_truth_in(value: Address, path: Address) -> u32 {
    match unsafe { &*value.cast::<bounds::Condition>() }.truth_in(unsafe { &*path.cast::<bounds::Clause>() }) {
        None => 0, Some(false) => 1, Some(true) => 2,
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Host {
    layout: &'static PresenceView,
    input_mentions: unsafe extern "C" fn(Address, Address) -> u32,
    clone_input: unsafe extern "C" fn(Address, Address),
    drop_payload: unsafe extern "C" fn(Address, SharedNodeKind),
    children_scope: unsafe extern "C" fn(Address, Address, Host, Children) -> u32,
    clone_scope: unsafe extern "C" fn(Address, Address, Host, Scoped) -> u32,
    truth_scope: unsafe extern "C" fn(u32, Address, Host, Scoped) -> u32,
    position_equal: unsafe extern "C" fn(Address, Address) -> u32,
    condition_conflict: unsafe extern "C" fn(*const c_void) -> u32,
    condition_input: unsafe extern "C" fn(Address, Address),
    condition_index: unsafe extern "C" fn(Address, u32, PositionIndex, PositionIndex),
    condition_substitute: unsafe extern "C" fn(Address, Address, Address, PositionIndex, PositionIndex),
    input_offset: unsafe extern "C" fn(Address, PositionIndex) -> u32,
    condition_truth_in: unsafe extern "C" fn(Address, Address) -> u32,
}
const HOST: Host = Host {
    layout: &PRESENCE_VIEW, input_mentions, clone_input, drop_payload,
    children_scope, clone_scope, truth_scope,
    position_equal,
    condition_conflict: bounds::condition_conflict,
    condition_input, condition_index, condition_substitute,
    input_offset, condition_truth_in,
};
unsafe extern "C" {
    #[link_name = "bootstrap.ownership_presence::relation"] fn source_relation(output: Address, value: Selection, host: Host);
    #[link_name = "bootstrap.ownership_presence::compare"] fn source_compare(a: Address, b: Address, host: Host) -> i32;
    #[link_name = "bootstrap.ownership_presence::uses_index"] fn source_uses_index(value: Address, wanted: PositionIndex, host: Host) -> u32;
    #[link_name = "bootstrap.ownership_presence::from_value"]
    fn source_from_value(output: Address, value: Address, constant: u32, layout: &PresenceView,
        capture: unsafe extern "C" fn(Address, Address), drop_payload: unsafe extern "C" fn(Address, SharedNodeKind));
    #[link_name = "bootstrap.ownership_presence::combine"] fn source_combine(left: Address, right: Address, any: u32, host: Host);
    #[link_name = "bootstrap.ownership_presence::negate"] fn source_negate(value: Address, host: Host);
    #[link_name = "bootstrap.ownership_presence::selected"] fn source_selected(output: Address, position: Address, wanted: Address, host: Host);
    #[link_name = "bootstrap.ownership_presence::condition"] fn source_condition(output: Address, value: Address, host: Host);
    #[link_name = "bootstrap.ownership_presence::loop_condition"] fn source_loop_condition(output: Address, value: Address, inputs: Address, host: Host);
    #[link_name = "bootstrap.ownership_presence::ownership_may_own"] fn source_may_own(value: Address, host: Host) -> u32;
    #[link_name = "bootstrap.ownership_presence::ownership_available"] fn source_available(value: Address, host: Host) -> u32;
    #[link_name = "bootstrap.ownership_presence::ownership_presence"] fn source_presence(output: Address, value: Address, host: Host);
    #[link_name = "bootstrap.ownership_presence::ownership_from_presence"] fn source_from_presence(output: Address, value: Address, host: Host);
    #[link_name = "bootstrap.ownership_presence::ownership_either"] fn source_either(output: Address, a: Address, b: Address, choice: Address, has_choice: u32, host: Host);
    #[link_name = "bootstrap.ownership_presence::ownership_restricted"] fn source_restricted(output: Address, value: Address, when: Address, host: Host);
    #[link_name = "bootstrap.ownership_presence::ownership_compatible"] fn source_compatible(a: Address, b: Address, inputs: Address, path: Address, host: Host) -> u32;
}
unsafe extern "C" {
    #[link_name = "bootstrap.ownership_presence::clone_presence"]
    fn source_clone_presence(output: Address, source: Address, host: Host);
    #[link_name = "bootstrap.ownership_position::clone_ownership"]
    fn source_clone_ownership(output: Address, source: Address, layout: &PresenceView,
        drop_payload: unsafe extern "C" fn(Address, SharedNodeKind));
}

pub(super) fn clone_presence(value: &Presence) -> Presence {
    let mut output = Presence::None;
    unsafe { source_clone_presence(std::ptr::from_mut(&mut output).cast(), pointer(value), HOST); }
    output
}

pub(super) fn clone_ownership(value: &Ownership) -> Ownership {
    let mut output = Ownership::Moved;
    unsafe { source_clone_ownership(std::ptr::from_mut(&mut output).cast(), pointer(value), &PRESENCE_VIEW, drop_payload); }
    output
}

pub(super) fn relation(operation: Comparison, a: ownership::ApplicationIndex, b: ownership::ApplicationIndex) -> Presence {
    let mut output = Presence::None;
    unsafe { source_relation(pointer(&mut output), Selection { operation: operation as u32, a: position_index(Some(&a)), b: position_index(Some(&b)) }, HOST); }
    output
}
pub(super) fn compare(a: &Presence, b: &Presence) -> std::cmp::Ordering {
    unsafe { source_compare(pointer(a), pointer(b), HOST) }.cmp(&0)
}
pub(super) fn uses_index(value: &Presence, at: ownership::ApplicationIndex) -> bool {
    unsafe { source_uses_index(pointer(value), position_index(Some(&at)), HOST) != 0 }
}
pub(super) fn from_value(value: &Value) -> Presence {
    let constant = match value.constant.as_deref() { Some([0]) => 1, Some([_]) => 2, _ => 0 };
    let mut output = Presence::None;
    unsafe {
        source_from_value(std::ptr::from_mut(&mut output).cast(), pointer(value), constant,
            &PRESENCE_VIEW, capture_input, drop_payload);
    }
    output
}
pub(super) fn combine(mut left: Presence, mut right: Presence, any: bool) -> Presence {
    unsafe { source_combine(pointer(&mut left), pointer(&mut right), u32::from(any), HOST); } left
}
pub(super) fn negate(mut value: Presence) -> Presence { unsafe { source_negate(pointer(&mut value), HOST); } value }
pub(super) fn selected_presence(position: &Position, wanted: &Position) -> Presence {
    let mut output = Presence::None; unsafe { source_selected(pointer(&mut output), pointer(position), pointer(wanted), HOST); } output
}
pub(super) fn condition(value: &Presence) -> bounds::Condition {
    let mut output = std::mem::MaybeUninit::<bounds::Condition>::uninit();
    // The B projection writes both owned clauses before returning.
    unsafe { source_condition(output.as_mut_ptr().cast(), pointer(value), HOST); output.assume_init() }
}
pub(super) fn loop_condition(value: &Presence, inputs: &BTreeMap<ValueIdentity, bounds::Proof>) -> bounds::Condition {
    let mut output = std::mem::MaybeUninit::<bounds::Condition>::uninit();
    unsafe { source_loop_condition(output.as_mut_ptr().cast(), pointer(value), pointer(inputs), HOST); output.assume_init() }
}
pub(super) fn may_own(value: &Ownership) -> bool { unsafe { source_may_own(pointer(value), HOST) != 0 } }
pub(super) fn available(value: &Ownership) -> bool { unsafe { source_available(pointer(value), HOST) != 0 } }
pub(super) fn presence(value: &Ownership) -> Presence {
    let mut output = Presence::None; unsafe { source_presence(pointer(&mut output), pointer(value), HOST); } output
}
pub(super) fn from_presence(mut value: Presence) -> Ownership {
    let mut output = Ownership::Moved; unsafe { source_from_presence(pointer(&mut output), pointer(&mut value), HOST); } output
}
pub(super) fn either(a: &Ownership, b: &Ownership, choice: Option<&Presence>) -> Ownership {
    let mut output = Ownership::Moved;
    let selected = choice.map_or(std::ptr::null_mut(), pointer);
    unsafe { source_either(pointer(&mut output), pointer(a), pointer(b), selected, u32::from(choice.is_some()), HOST); } output
}
pub(super) fn restricted(value: &Ownership, when: &Presence) -> Ownership {
    let mut output = Ownership::Moved; unsafe { source_restricted(pointer(&mut output), pointer(value), pointer(when), HOST); } output
}
pub(super) fn compatible(a: &Ownership, b: &Ownership, inputs: &BTreeMap<ValueIdentity, bounds::Proof>, path: &bounds::Clause) -> bool {
    unsafe { source_compatible(pointer(a), pointer(b), pointer(inputs), pointer(path), HOST) != 0 }
}


#[repr(C)]
struct EmitBranch { value: Address, slot: usize, condition: Reg, either: u32 }

#[repr(C)]
struct EmitHost {
    layout: &'static PresenceView,
    constant: unsafe extern "C" fn(Address, u32, u32) -> Reg,
    compare: unsafe extern "C" fn(Address, u32, Reg, Reg) -> Reg,
    invert: unsafe extern "C" fn(Address, Reg) -> Reg,
    slot: unsafe extern "C" fn(Address) -> usize,
    store: unsafe extern "C" fn(Address, usize, Reg),
    load: unsafe extern "C" fn(Address, usize) -> Reg,
    scoped_branch: unsafe extern "C" fn(Address, EmitBranch),
    swap_code: unsafe extern "C" fn(Address, Address),
    fixed_store: unsafe extern "C" fn(Address, Address, usize, Reg),
    branch: unsafe extern "C" fn(Address, Reg, Address, Address),
}

const EMITTER: EmitHost = EmitHost {
    layout: &PRESENCE_VIEW, constant: emit_constant, compare: emit_compare,
    invert: emit_invert, slot: bool_slot, store: store_bool, load: load_bool,
    scoped_branch, swap_code, fixed_store, branch: emit_branch,
};

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_presence::emit_presence"]
    fn source_emit_presence(context: Address, value: Address, host: EmitHost) -> Reg;
    #[link_name = "bootstrap.ownership_presence::emit_presence_branch"]
    fn source_emit_branch(context: Address, work: EmitBranch, right: Address,
        fixed: Address, host: EmitHost);
}

struct Emission<'a, 'w> {
    builder: &'a mut Builder<'w>,
    origin: Option<&'a Arc<GeneratedOrigin>>,
}

impl Emission<'_, '_> {
    fn emit(&mut self, kind: InstKind) {
        match self.origin {
            Some(origin) => self.builder.code.push(Inst::generated(kind, origin.clone())),
            None => self.builder.emit(kind),
        }
    }

    fn value(&mut self, scalar: Scalar, instruction: impl FnOnce(Reg) -> InstKind) -> Reg {
        let dst = self.builder.reg(scalar);
        self.emit(instruction(dst));
        dst
    }
}

pub(super) fn emit_presence(builder: &mut Builder<'_>, value: &Presence, origin: Option<&Arc<GeneratedOrigin>>) -> Reg {
    let mut emission = Emission { builder, origin };
    unsafe { source_emit_presence(std::ptr::from_mut(&mut emission).cast(), pointer(value), EMITTER) }
}

unsafe extern "C" fn emit_constant(context: Address, value: u32, index: u32) -> Reg {
    let state = unsafe { &mut *context.cast::<Emission<'_, '_>>() };
    let scalar = if index == 0 { Scalar::Bool } else { Scalar::U32 };
    state.value(scalar, |dst| InstKind::Constant(dst, scalar, Bits::from(value)))
}

unsafe extern "C" fn emit_compare(context: Address, operation: u32, a: Reg, b: Reg) -> Reg {
    let state = unsafe { &mut *context.cast::<Emission<'_, '_>>() };
    let op = BinaryOp::Compare(comparison(operation));
    state.value(Scalar::Bool, |dst| InstKind::Binary { dst, op, a, b })
}

unsafe extern "C" fn emit_invert(context: Address, a: Reg) -> Reg {
    let state = unsafe { &mut *context.cast::<Emission<'_, '_>>() };
    state.value(Scalar::Bool, |dst| InstKind::Unary { dst, op: UnaryOp::LogicalNot, a })
}

unsafe extern "C" fn bool_slot(context: Address) -> usize {
    let state = unsafe { &mut *context.cast::<Emission<'_, '_>>() };
    state.builder.local_slots(&Ty::scalar(Scalar::Bool)).0[0]
}

unsafe extern "C" fn store_bool(context: Address, slot: usize, value: Reg) {
    let state = unsafe { &mut *context.cast::<Emission<'_, '_>>() };
    state.emit(InstKind::StoreLocal(slot, value));
}

unsafe extern "C" fn load_bool(context: Address, slot: usize) -> Reg {
    let state = unsafe { &mut *context.cast::<Emission<'_, '_>>() };
    state.value(Scalar::Bool, |dst| InstKind::LoadLocal(dst, slot))
}

unsafe extern "C" fn scoped_branch(context: Address, work: EmitBranch) {
    let mut right = Vec::<Inst>::new();
    let mut fixed = Vec::<Inst>::new();
    unsafe {
        source_emit_branch(context, work, std::ptr::from_mut(&mut right).cast(),
            std::ptr::from_mut(&mut fixed).cast(), EMITTER);
    }
}

unsafe extern "C" fn swap_code(context: Address, code: Address) {
    let state = unsafe { &mut *context.cast::<Emission<'_, '_>>() };
    std::mem::swap(&mut state.builder.code, unsafe { &mut *code.cast::<Vec<Inst>>() });
}

unsafe extern "C" fn fixed_store(context: Address, code: Address, slot: usize, value: Reg) {
    let state = unsafe { &*context.cast::<Emission<'_, '_>>() };
    let kind = InstKind::StoreLocal(slot, value);
    let store = match state.origin {
        Some(origin) => Inst::generated(kind, origin.clone()),
        None => Inst::new(kind),
    };
    unsafe { &mut *code.cast::<Vec<Inst>>() }.push(store);
}

unsafe extern "C" fn emit_branch(context: Address, condition: Reg, yes: Address, no: Address) {
    let state = unsafe { &mut *context.cast::<Emission<'_, '_>>() };
    let yes = std::mem::take(unsafe { &mut *yes.cast::<Vec<Inst>>() });
    let no = std::mem::take(unsafe { &mut *no.cast::<Vec<Inst>>() });
    state.emit(InstKind::If { condition, yes, no });
}
