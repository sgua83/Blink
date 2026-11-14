//! Borrowed IR and primitive access to the one native allocation graph.
use super::*;
type Slots = BTreeSet<PrivateSlot>;
#[repr(C)]
#[derive(Clone, Copy)]
struct Block { instructions: *const Inst, count: usize }
impl Block {
    fn from(code: &[Inst]) -> Self { Self { instructions: code.as_ptr(), count: code.len() } }
    const EMPTY: Self = Self { instructions: std::ptr::NonNull::<Inst>::dangling().as_ptr(), count: 0 };
    unsafe fn slice<'a>(self) -> &'a [Inst] { unsafe { std::slice::from_raw_parts(self.instructions, self.count) } }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Scalars { data: *const Scalar, count: usize }
#[repr(u32)]
enum Kind { Ordinary, If, Loop, Scope, Raise, Exit, Break, Continue, LocalLoad, LocalStore, Cast, Unary, Binary, Select, Math, Constant, Print, Call }
#[repr(C)]
struct Node {
    first: Block, second: Block, instruction: *const Inst, arguments: *const Reg, count: usize,
    dst: Reg, a: Reg, b: Reg, bits: u64, outgoing: u64,
    kind: Kind, flags: u32, operation: u32,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum Space { Register, Local }
#[repr(u32)]
enum Relation { Edge, Affinity }
#[repr(u32)]
enum Counter { Registers, Outgoing }
#[repr(u32)]
enum SlotAction { Insert, Remove }
#[repr(u32)]
enum SetAction { Copy, Union, DrainUnion, Swap, Clear }
#[repr(C)]
#[derive(Clone, Copy)]
struct Event { anchor: usize, skip: usize, kind: u32, skip_present: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct Definition { weight: u64, transfer_dst: usize, transfer_src: usize, bits: u64, transfer: u32, literal: u32, pass: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct Core {
    addressable: unsafe extern "C" fn(*const Graph, usize) -> u32,
    relation: unsafe extern "C" fn(*mut Graph, Relation, usize, usize),
    weight: unsafe extern "C" fn(*const Graph, usize) -> u64,
    set_weight: unsafe extern "C" fn(*mut Graph, usize, u64),
    across: unsafe extern "C" fn(*mut Graph, usize),
    counter: unsafe extern "C" fn(*const Graph, Counter) -> u64,
    set_outgoing: unsafe extern "C" fn(*mut Graph, u64),
    location: unsafe extern "C" fn(*mut Graph, usize) -> *mut ColorPlace,
    visit_set: unsafe extern "C" fn(*const Slots, usize, *mut Graph, Event),
    slot: unsafe extern "C" fn(*mut Slots, SlotAction, Space, usize),
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Host {
    core: Core,
    read: unsafe extern "C" fn(Block, usize, Scalars) -> Node,
    clone_set: unsafe extern "C" fn(*const Slots) -> *mut Slots,
    drop_set: unsafe extern "C" fn(*mut Slots),
    change: unsafe extern "C" fn(SetAction, *mut Slots, *mut Slots),
    loop_live: unsafe extern "C" fn(Block, Reg, Block, *const Slots, *const Slots, *const Slots, *const Slots) -> *mut Slots,
    definitions: unsafe extern "C" fn(*const Inst, *mut Graph, *mut Slots, Definition),
    reads: unsafe extern "C" fn(*const Inst, *mut Graph, *mut Slots, u64),
}
unsafe extern "C" {
    #[link_name = "bootstrap.coloring::graph_block"]
    fn graph_block(graph: *mut Graph, code: Block, registers: Scalars, live: *mut Slots,
        break_live: *const Slots, continue_live: *const Slots, exit_live: *const Slots, weight: u64, host: Host);
    #[link_name = "bootstrap.coloring::graph_together"]
    fn graph_together(graph: *mut Graph, live: *const Slots, core: Core);
    #[link_name = "bootstrap.coloring::graph_inputs"]
    fn graph_inputs(graph: *mut Graph, inputs: *const Reg, count: usize, core: Core);
    #[link_name = "bootstrap.coloring::graph_live_event"]
    fn graph_live_event(graph: *mut Graph, live: *const Slots, space: Space, index: usize,
        ordinal: usize, event: Event, core: Core);
    #[link_name = "bootstrap.coloring::graph_definition"]
    fn graph_definition(graph: *mut Graph, live: *mut Slots, space: Space, index: usize, definition: Definition, core: Core);
    #[link_name = "bootstrap.coloring::graph_read"]
    fn graph_read(graph: *mut Graph, live: *mut Slots, space: Space, index: usize, weight: u64, core: Core);
}
pub(super) fn analyze(graph: &mut Graph, code: &[Inst], registers: &[Scalar], live: &mut Slots,
    break_live: &Slots, continue_live: &Slots, exit_live: &Slots, weight: u64) {
    unsafe { graph_block(graph, Block::from(code), Scalars { data: registers.as_ptr(), count: registers.len() },
        live, break_live, continue_live, exit_live, weight, HOST); }
}
pub(super) fn together(graph: &mut Graph, live: &Slots) { unsafe { graph_together(graph, live, CORE) } }
pub(super) fn inputs(graph: &mut Graph, inputs: &[Reg]) { unsafe { graph_inputs(graph, inputs.as_ptr(), inputs.len(), CORE) } }

unsafe extern "C" fn read(code: Block, index: usize, registers: Scalars) -> Node {
    let instruction = unsafe { code.instructions.add(index) }; let inst = unsafe { &*instruction };
    let registers = unsafe { std::slice::from_raw_parts(registers.data, registers.count) };
    let mut node = Node { first: Block::EMPTY, second: Block::EMPTY, instruction,
        arguments: std::ptr::NonNull::<Reg>::dangling().as_ptr(), count: 0, dst: 0, a: 0, b: 0,
        bits: 0, outgoing: 0, kind: Kind::Ordinary, flags: 0, operation: 0 };
    match &inst.kind {
        InstKind::If { condition, yes, no } => { node.kind = Kind::If; node.a = *condition; node.first = Block::from(yes); node.second = Block::from(no); }
        InstKind::Loop { condition, test, body } => { node.kind = Kind::Loop; node.a = *test; node.first = Block::from(condition); node.second = Block::from(body); }
        InstKind::Scope(body) => { node.kind = Kind::Scope; node.first = Block::from(body); }
        InstKind::Raise(error) => { node.kind = Kind::Raise; node.a = *error; }
        InstKind::Exit => node.kind = Kind::Exit,
        InstKind::Break => node.kind = Kind::Break,
        InstKind::Continue => node.kind = Kind::Continue,
        InstKind::LoadLocal(dst, slot) => { node.kind = Kind::LocalLoad; node.dst = *dst; node.a = *slot; }
        InstKind::StoreLocal(slot, src) => { node.kind = Kind::LocalStore; node.dst = *slot; node.a = *src; }
        InstKind::Cast { dst, src } => { node.kind = Kind::Cast; node.dst = *dst; node.a = *src;
            node.flags = u32::from(super::super::operands::bit_copy(registers[*src], registers[*dst])); }
        InstKind::Unary { dst, a, .. } => { node.kind = Kind::Unary; node.dst = *dst; node.a = *a; }
        InstKind::Binary { dst, op, a, b } => { node.kind = Kind::Binary; node.dst = *dst; node.a = *a; node.b = *b;
            if let BinaryOp::Compare(cmp) = op { node.flags = 4 | (u32::from(registers[*a].is_float()) << 1); node.operation = *cmp as u32; } }
        InstKind::Select { dst, b, .. } => { node.kind = Kind::Select; node.dst = *dst; node.a = *b; }
        InstKind::Math { dst, op, args } => { node.kind = Kind::Math; node.dst = *dst; node.operation = *op as u32; node.arguments = args.as_ptr(); node.count = args.len(); }
        InstKind::Constant(_, _, bits) => { node.kind = Kind::Constant; node.bits = *bits; }
        InstKind::Print { .. } => node.kind = Kind::Print,
        InstKind::Call(call) => { node.kind = Kind::Call; node.outgoing = super::super::external::outgoing_bytes(call); }
        _ => {}
    }
    node
}
unsafe extern "C" fn addressable(graph: *const Graph, id: usize) -> u32 { u32::from(unsafe { &*graph }.addressable.contains(&id)) }
unsafe extern "C" fn relation(graph: *mut Graph, kind: Relation, from: usize, to: usize) {
    let graph = unsafe { &mut *graph }; match kind { Relation::Edge => { graph.edges[from].insert(to); }, Relation::Affinity => { graph.affinities[from].insert(to); } }
}
unsafe extern "C" fn weight(graph: *const Graph, id: usize) -> u64 { unsafe { &*graph }.weights[id] }
unsafe extern "C" fn set_weight(graph: *mut Graph, id: usize, value: u64) { unsafe { &mut *graph }.weights[id] = value; }
unsafe extern "C" fn across(graph: *mut Graph, id: usize) { unsafe { &mut *graph }.across_call.insert(id); }
unsafe extern "C" fn counter(graph: *const Graph, kind: Counter) -> u64 {
    let graph = unsafe { &*graph }; match kind { Counter::Registers => graph.registers as u64, Counter::Outgoing => graph.outgoing_bytes }
}
unsafe extern "C" fn set_outgoing(graph: *mut Graph, bytes: u64) { unsafe { &mut *graph }.outgoing_bytes = bytes; }
unsafe extern "C" fn location(graph: *mut Graph, id: usize) -> *mut ColorPlace { &mut unsafe { &mut *graph }.locations[id] }
unsafe extern "C" fn visit_set(live: *const Slots, limit: usize, graph: *mut Graph, event: Event) {
    for (ordinal, slot) in unsafe { &*live }.iter().take(limit).enumerate() {
        let (space, index) = match slot { PrivateSlot::Register(index) => (Space::Register, *index), PrivateSlot::Local(index) => (Space::Local, *index) };
        unsafe { graph_live_event(graph, live, space, index, ordinal, event, CORE); }
    }
}
unsafe extern "C" fn slot(live: *mut Slots, action: SlotAction, space: Space, index: usize) {
    let value = match space { Space::Register => PrivateSlot::Register(index), Space::Local => PrivateSlot::Local(index) };
    let live = unsafe { &mut *live }; match action { SlotAction::Insert => { live.insert(value); }, SlotAction::Remove => { live.remove(&value); } }
}
unsafe extern "C" fn clone_set(live: *const Slots) -> *mut Slots { Box::into_raw(Box::new(unsafe { &*live }.clone())) }
unsafe extern "C" fn drop_set(live: *mut Slots) { drop(unsafe { Box::from_raw(live) }); }
unsafe extern "C" fn change(action: SetAction, target: *mut Slots, source: *mut Slots) {
    let target = unsafe { &mut *target }; match action {
        SetAction::Copy => *target = unsafe { &*source }.clone(),
        SetAction::Union => target.extend(unsafe { &*source }.iter().copied()),
        SetAction::DrainUnion => target.append(unsafe { &mut *source }),
        SetAction::Swap => std::mem::swap(target, unsafe { &mut *source }),
        SetAction::Clear => target.clear(),
    }
}
unsafe extern "C" fn header(condition: Block, test: Reg, body: Block, after: *const Slots,
    break_live: *const Slots, continue_live: *const Slots, exit_live: *const Slots) -> *mut Slots {
    Box::into_raw(Box::new(loop_live(unsafe { condition.slice() }, test, unsafe { body.slice() },
        unsafe { &*after }, unsafe { &*break_live }, unsafe { &*continue_live }, unsafe { &*exit_live })))
}
unsafe extern "C" fn definitions(instruction: *const Inst, graph: *mut Graph, live: *mut Slots, definition: Definition) {
    let inst = unsafe { &*instruction };
    for reg in inst.results() { unsafe { graph_definition(graph, live, Space::Register, reg, definition, CORE); } }
    if let InstKind::StoreLocal(slot, _) = &inst.kind { unsafe { graph_definition(graph, live, Space::Local, *slot, definition, CORE); } }
}
unsafe extern "C" fn reads(instruction: *const Inst, graph: *mut Graph, live: *mut Slots, weight: u64) {
    let inst = unsafe { &*instruction };
    inst.all_operands(&mut |reg| { unsafe { graph_read(graph, live, Space::Register, reg, weight, CORE) }; true });
    for index in inst.local_reads() { unsafe { graph_read(graph, live, Space::Local, index, weight, CORE); } }
}
const CORE: Core = Core { addressable, relation, weight, set_weight, across, counter, set_outgoing, location, visit_set, slot };
const HOST: Host = Host { core: CORE, read, clone_set, drop_set, change, loop_live: header, definitions, reads };
