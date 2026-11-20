//! Native storage operations for the original Presence and Position owners.
use super::*;
use std::ffi::c_void;

type Address = *mut c_void;
#[repr(C)]
#[derive(Clone, Copy)]
struct Index { kind: u32, value: u32, identity: Address, reg: u64 }
#[repr(C)]
#[derive(Clone, Copy)]
struct Selection { operation: u32, a: Index, b: Index }
#[repr(C)]
struct Node {
    kind: u32, operation: u32, a: Index, b: Index,
    first: Address, second: Address, value: Address,
}
#[repr(C)]
struct PositionNode { indexed: u32, count: u64, index: Index }
fn pointer<T>(value: &T) -> Address { (value as *const T).cast_mut().cast() }
fn absent_index() -> Index { Index { kind: 0, value: 0, identity: std::ptr::null_mut(), reg: 0 } }
fn index(value: &ownership::ApplicationIndex) -> Index {
    match value {
        ownership::ApplicationIndex::Constant(value) => Index { kind: 1, value: *value, ..absent_index() },
        ownership::ApplicationIndex::Value(identity, reg) => Index { kind: 2, identity: pointer(identity), reg: *reg as u64, ..absent_index() },
    }
}
unsafe fn actual_index(value: Index) -> ownership::ApplicationIndex {
    if value.kind == 1 { ownership::ApplicationIndex::Constant(value.value) }
    else { ownership::ApplicationIndex::Value(unsafe { *value.identity.cast::<ValueIdentity>() }, value.reg as Reg) }
}
fn comparison(value: u32) -> Comparison {
    [Comparison::Equal, Comparison::NotEqual, Comparison::Less,
     Comparison::Greater, Comparison::LessEqual, Comparison::GreaterEqual][value as usize]
}
unsafe fn take(value: Address) -> Presence { std::mem::replace(unsafe { &mut *value.cast::<Presence>() }, Presence::None) }
unsafe extern "C" fn inspect(value: Address) -> Node {
    let mut node = Node {
        kind: 0, operation: 0, a: absent_index(), b: absent_index(),
        first: std::ptr::null_mut(), second: std::ptr::null_mut(), value: std::ptr::null_mut(),
    };
    match unsafe { &*value.cast::<Presence>() } {
        Presence::None => {}, Presence::All => node.kind = 1,
        Presence::Input(value) => { node.kind = 2; node.value = pointer(value); },
        Presence::Selected(operation, a, b) => { node.kind = 3; node.operation = *operation as u32; node.a = index(a); node.b = index(b); },
        Presence::Not(value) => { node.kind = 4; node.first = pointer(value.as_ref()); },
        Presence::Both(a, b) => { node.kind = 5; node.first = pointer(a.as_ref()); node.second = pointer(b.as_ref()); },
        Presence::Either(a, b) => { node.kind = 6; node.first = pointer(a.as_ref()); node.second = pointer(b.as_ref()); },
    }
    node
}
unsafe extern "C" fn compare_index(a: Index, b: Index) -> i32 {
    unsafe { actual_index(a) }.cmp(&unsafe { actual_index(b) }) as i32
}
unsafe extern "C" fn compare_input(a: Address, b: Address) -> i32 {
    unsafe { &*a.cast::<Value>() }.identity().cmp(&unsafe { &*b.cast::<Value>() }.identity()) as i32
}
unsafe extern "C" fn constant_input(value: Address) -> u32 {
    match unsafe { &*value.cast::<Value>() }.constant.as_deref() { Some([0]) => 1, Some([_]) => 2, _ => 0 }
}
unsafe extern "C" fn input_uses_index(value: Address, at: Index) -> u32 {
    let value = unsafe { &*value.cast::<Value>() };
    u32::from(match unsafe { actual_index(at) } {
        ownership::ApplicationIndex::Constant(_) => false,
        ownership::ApplicationIndex::Value(id, _) => value.identity() == id || bounds::Condition::of(value).mentions(id),
    })
}
unsafe extern "C" fn truth(output: Address, value: u32) {
    unsafe { *output.cast::<Presence>() = if value == 0 { Presence::None } else { Presence::All }; }
}
unsafe extern "C" fn selected(output: Address, value: Selection) {
    // Read both original indices before replacing the owner that may contain them.
    let a = unsafe { actual_index(value.a) }; let b = unsafe { actual_index(value.b) };
    unsafe { *output.cast::<Presence>() = Presence::Selected(comparison(value.operation), a, b); }
}
unsafe extern "C" fn input(output: Address, value: Address) {
    let value = unsafe { &*value.cast::<Value>() }.clone();
    unsafe { *output.cast::<Presence>() = Presence::Input(value); }
}
unsafe extern "C" fn move_value(output: Address, source: Address) {
    let value = unsafe { take(source) }; unsafe { *output.cast::<Presence>() = value; }
}
unsafe extern "C" fn pair(output: Address, left: Address, right: Address, any: u32) {
    let left = Box::new(unsafe { take(left) }); let right = Box::new(unsafe { take(right) });
    unsafe { *output.cast::<Presence>() = if any == 0 { Presence::Both(left, right) } else { Presence::Either(left, right) }; }
}
unsafe extern "C" fn wrap_not(value: Address) {
    let original = unsafe { take(value) }; unsafe { *value.cast::<Presence>() = Presence::Not(Box::new(original)); }
}

type Children = unsafe extern "C" fn(Address, Address, Address, Address, Host) -> u32;
type Selected = unsafe extern "C" fn(Selection, Address, Host) -> u32;
type Scoped = unsafe extern "C" fn(Address, Address, Host) -> u32;
unsafe extern "C" fn children_scope(value: Address, context: Address, host: Host, apply: Children) -> u32 {
    let (mut left, mut right) = match unsafe { take(value) } {
        Presence::Not(left) => (*left, Presence::None),
        Presence::Both(left, right) | Presence::Either(left, right) => (*left, *right),
        leaf => (leaf, Presence::None),
    };
    unsafe { apply(value, pointer(&mut left), pointer(&mut right), context, host) }
}
unsafe extern "C" fn selected_scope(value: Selection, context: Address, host: Host, apply: Selected) -> u32 {
    // The mutable target can replace its selected node during the callback.
    // These actual Copy indices stay on this original Rust stack until return.
    let a = unsafe { actual_index(value.a) }; let b = unsafe { actual_index(value.b) };
    let saved = Selection { operation: value.operation, a: index(&a), b: index(&b) };
    unsafe { apply(saved, context, host) }
}
unsafe extern "C" fn clone_scope(value: Address, context: Address, host: Host, apply: Scoped) -> u32 {
    let mut value = unsafe { &*value.cast::<Presence>() }.clone();
    unsafe { apply(pointer(&mut value), context, host) }
}
unsafe extern "C" fn truth_scope(value: u32, context: Address, host: Host, apply: Scoped) -> u32 {
    let mut value = if value == 0 { Presence::None } else { Presence::All };
    unsafe { apply(pointer(&mut value), context, host) }
}
unsafe extern "C" fn position(value: Address) -> PositionNode {
    match unsafe { &*value.cast::<Position>() } {
        Position::Indexed(choices, at) => PositionNode { indexed: 1, count: choices.len() as u64, index: index(at) },
        _ => PositionNode { indexed: 0, count: 0, index: absent_index() },
    }
}
unsafe extern "C" fn position_child(value: Address, at: u64) -> Address {
    match unsafe { &*value.cast::<Position>() } {
        Position::Indexed(choices, _) => pointer(&choices[at as usize]),
        _ => std::ptr::null_mut(),
    }
}
unsafe extern "C" fn position_equal(a: Address, b: Address) -> u32 {
    u32::from(unsafe { &*a.cast::<Position>() == &*b.cast::<Position>() })
}
type ConditionSlot = Option<bounds::Condition>;
unsafe extern "C" fn condition_scope(context: Address, host: Host, apply: Scoped) -> u32 {
    let mut value: ConditionSlot = None;
    unsafe { apply(pointer(&mut value), context, host) }
}
unsafe extern "C" fn condition_clone_scope(value: Address, context: Address, host: Host, apply: Scoped) -> u32 {
    let mut value = unsafe { &*value.cast::<ConditionSlot>() }.clone();
    unsafe { apply(pointer(&mut value), context, host) }
}
unsafe extern "C" fn condition_truth(output: Address, value: u32) {
    unsafe { *output.cast::<ConditionSlot>() = Some(bounds::Condition::constant(value != 0)); }
}
unsafe extern "C" fn condition_input(output: Address, value: Address) {
    let value = bounds::Condition::of(unsafe { &*value.cast::<Value>() });
    unsafe { *output.cast::<ConditionSlot>() = Some(value); }
}
unsafe extern "C" fn condition_index(output: Address, kind: u32, a: Index, b: Index) {
    let operand = |value| match unsafe { actual_index(value) } {
        ownership::ApplicationIndex::Constant(value) => bounds::Operand::Constant(Scalar::U32, Bits::from(value)),
        ownership::ApplicationIndex::Value(value, _) => bounds::Operand::Value(value),
    };
    let a = operand(a); let b = operand(b); let proof = bounds::Proof::default();
    let value = if kind == 0 { bounds::Condition::index_equal(a, b) }
        else { bounds::Condition::index_below((a, &proof), (b, &proof), kind == 1) };
    unsafe { *output.cast::<ConditionSlot>() = Some(value); }
}
unsafe extern "C" fn condition_negate(value: Address) {
    let value = unsafe { &mut *value.cast::<ConditionSlot>() };
    *value = Some(value.take().unwrap().negate());
}
unsafe extern "C" fn condition_logical(left: Address, right: Address, any: u32) {
    let a = unsafe { &mut *left.cast::<ConditionSlot>() }.take().unwrap();
    let b = unsafe { &mut *right.cast::<ConditionSlot>() }.take().unwrap();
    unsafe { *left.cast::<ConditionSlot>() = Some(a.logical(b, any != 0)); }
}
unsafe extern "C" fn condition_substitute(value: Address, inputs: Address, a: Index, b: Index) {
    let inputs = unsafe { &*inputs.cast::<BTreeMap<ValueIdentity, bounds::Proof>>() };
    let omitted = |index: Index| if index.kind == 2 { Some(unsafe { *index.identity.cast::<ValueIdentity>() }) } else { None };
    let a = omitted(a); let b = omitted(b);
    let slot = unsafe { &mut *value.cast::<ConditionSlot>() };
    let condition = slot.as_ref().unwrap();
    let bindings = condition.value_bindings(|id| if Some(id) == a || Some(id) == b { None } else { inputs.get(&id) });
    *slot = Some(condition.at_call(&bindings));
}
unsafe extern "C" fn input_offset(inputs: Address, index: Index) -> u32 {
    let inputs = unsafe { &*inputs.cast::<BTreeMap<ValueIdentity, bounds::Proof>>() };
    let found = match unsafe { actual_index(index) } {
        ownership::ApplicationIndex::Constant(_) => false,
        ownership::ApplicationIndex::Value(id, _) => inputs.get(&id).is_some_and(|proof| proof.domains(0).contains(&IndexDomain::Offset(id, 1))),
    };
    u32::from(found)
}
unsafe extern "C" fn condition_truth_in(value: Address, path: Address) -> u32 {
    match unsafe { &*value.cast::<ConditionSlot>() }.as_ref().unwrap().truth_in(unsafe { &*path.cast::<bounds::Clause>() }) {
        None => 0, Some(false) => 1, Some(true) => 2,
    }
}
#[repr(C)]
struct OwnershipNode { kind: u32, presence: Address }
unsafe extern "C" fn ownership_read(value: Address) -> OwnershipNode {
    let (kind, presence) = match unsafe { &*value.cast::<Ownership>() } {
        Ownership::Moved => (0, std::ptr::null_mut()),
        Ownership::Owned => (1, std::ptr::null_mut()),
        Ownership::Borrowed => (2, std::ptr::null_mut()),
        Ownership::Conditional(value) => (3, pointer(value.as_ref())),
    };
    OwnershipNode { kind, presence }
}
unsafe extern "C" fn ownership_copy(output: Address, source: Address) {
    let value = unsafe { &*source.cast::<Ownership>() }.clone();
    unsafe { *output.cast::<Ownership>() = value; }
}
unsafe extern "C" fn ownership_constant(output: Address, kind: u32) {
    let value = match kind { 0 => Ownership::Moved, 1 => Ownership::Owned, _ => Ownership::Borrowed };
    unsafe { *output.cast::<Ownership>() = value; }
}
unsafe extern "C" fn ownership_conditional(output: Address, value: Address) {
    let value = unsafe { take(value) };
    unsafe { *output.cast::<Ownership>() = Ownership::Conditional(Box::new(value)); }
}
unsafe extern "C" fn copy_value(output: Address, source: Address) {
    let value = unsafe { &*source.cast::<Presence>() }.clone();
    unsafe { *output.cast::<Presence>() = value; }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Host {
    inspect: unsafe extern "C" fn(Address) -> Node,
    compare_index: unsafe extern "C" fn(Index, Index) -> i32,
    compare_input: unsafe extern "C" fn(Address, Address) -> i32,
    constant_input: unsafe extern "C" fn(Address) -> u32,
    input_uses_index: unsafe extern "C" fn(Address, Index) -> u32,
    truth: unsafe extern "C" fn(Address, u32),
    selected: unsafe extern "C" fn(Address, Selection),
    input: unsafe extern "C" fn(Address, Address),
    move_value: unsafe extern "C" fn(Address, Address),
    pair: unsafe extern "C" fn(Address, Address, Address, u32),
    wrap_not: unsafe extern "C" fn(Address),
    children_scope: unsafe extern "C" fn(Address, Address, Host, Children) -> u32,
    selected_scope: unsafe extern "C" fn(Selection, Address, Host, Selected) -> u32,
    clone_scope: unsafe extern "C" fn(Address, Address, Host, Scoped) -> u32,
    truth_scope: unsafe extern "C" fn(u32, Address, Host, Scoped) -> u32,
    position: unsafe extern "C" fn(Address) -> PositionNode,
    position_child: unsafe extern "C" fn(Address, u64) -> Address,
    position_equal: unsafe extern "C" fn(Address, Address) -> u32,
    condition_scope: unsafe extern "C" fn(Address, Host, Scoped) -> u32,
    condition_clone_scope: unsafe extern "C" fn(Address, Address, Host, Scoped) -> u32,
    condition_truth: unsafe extern "C" fn(Address, u32),
    condition_input: unsafe extern "C" fn(Address, Address),
    condition_index: unsafe extern "C" fn(Address, u32, Index, Index),
    condition_negate: unsafe extern "C" fn(Address),
    condition_logical: unsafe extern "C" fn(Address, Address, u32),
    condition_substitute: unsafe extern "C" fn(Address, Address, Index, Index),
    input_offset: unsafe extern "C" fn(Address, Index) -> u32,
    condition_truth_in: unsafe extern "C" fn(Address, Address) -> u32,
    ownership_read: unsafe extern "C" fn(Address) -> OwnershipNode,
    ownership_copy: unsafe extern "C" fn(Address, Address),
    ownership_constant: unsafe extern "C" fn(Address, u32),
    ownership_conditional: unsafe extern "C" fn(Address, Address),
    copy_value: unsafe extern "C" fn(Address, Address),
}
const HOST: Host = Host {
    inspect, compare_index, compare_input, constant_input, input_uses_index,
    truth, selected, input, move_value, pair, wrap_not,
    children_scope, selected_scope, clone_scope, truth_scope,
    position, position_child, position_equal,
    condition_scope, condition_clone_scope, condition_truth, condition_input,
    condition_index, condition_negate, condition_logical, condition_substitute,
    input_offset, condition_truth_in, ownership_read, ownership_copy,
    ownership_constant, ownership_conditional, copy_value,
};
unsafe extern "C" {
    #[link_name = "bootstrap.ownership_presence::relation"] fn source_relation(output: Address, value: Selection, host: Host);
    #[link_name = "bootstrap.ownership_presence::compare"] fn source_compare(a: Address, b: Address, host: Host) -> i32;
    #[link_name = "bootstrap.ownership_presence::uses_index"] fn source_uses_index(value: Address, wanted: Index, host: Host) -> u32;
    #[link_name = "bootstrap.ownership_presence::from_value"] fn source_from_value(output: Address, value: Address, host: Host);
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
pub(super) fn relation(operation: Comparison, a: ownership::ApplicationIndex, b: ownership::ApplicationIndex) -> Presence {
    let mut output = Presence::None;
    unsafe { source_relation(pointer(&mut output), Selection { operation: operation as u32, a: index(&a), b: index(&b) }, HOST); }
    output
}
pub(super) fn compare(a: &Presence, b: &Presence) -> std::cmp::Ordering {
    unsafe { source_compare(pointer(a), pointer(b), HOST) }.cmp(&0)
}
pub(super) fn uses_index(value: &Presence, at: ownership::ApplicationIndex) -> bool {
    unsafe { source_uses_index(pointer(value), index(&at), HOST) != 0 }
}
pub(super) fn from_value(value: &Value) -> Presence {
    let mut output = Presence::None; unsafe { source_from_value(pointer(&mut output), pointer(value), HOST); } output
}
pub(super) fn combine(mut left: Presence, mut right: Presence, any: bool) -> Presence {
    unsafe { source_combine(pointer(&mut left), pointer(&mut right), u32::from(any), HOST); } left
}
pub(super) fn negate(mut value: Presence) -> Presence { unsafe { source_negate(pointer(&mut value), HOST); } value }
pub(super) fn selected_presence(position: &Position, wanted: &Position) -> Presence {
    let mut output = Presence::None; unsafe { source_selected(pointer(&mut output), pointer(position), pointer(wanted), HOST); } output
}
pub(super) fn condition(value: &Presence) -> bounds::Condition {
    let mut output: ConditionSlot = None; unsafe { source_condition(pointer(&mut output), pointer(value), HOST); } output.unwrap()
}
pub(super) fn loop_condition(value: &Presence, inputs: &BTreeMap<ValueIdentity, bounds::Proof>) -> bounds::Condition {
    let mut output: ConditionSlot = None; unsafe { source_loop_condition(pointer(&mut output), pointer(value), pointer(inputs), HOST); } output.unwrap()
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
