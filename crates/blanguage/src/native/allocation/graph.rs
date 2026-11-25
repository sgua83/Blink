//! Borrowed IR and primitive access to the one native allocation graph.
use super::*;
use crate::ir::dataflow::{REGISTER_HOST, RegisterHost, Span};
use super::super::{external, operands};

type Slots = BTreeSet<PrivateSlot>;
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
struct Core {
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
struct Host {
    core: Core,
    values: RegisterHost,
    cast_bits: unsafe extern "C" fn(Span, Reg, Reg) -> u32,
    call_outgoing: unsafe extern "C" fn(*const Call) -> u64,
    math_outgoing: unsafe extern "C" fn(u32) -> u64,
    clone_set: unsafe extern "C" fn(*const Slots) -> *mut Slots,
    drop_set: unsafe extern "C" fn(*mut Slots),
    change: unsafe extern "C" fn(SetAction, *mut Slots, *mut Slots),
    loop_live: unsafe extern "C" fn(Span, Reg, Span, *const Slots, *const Slots, *const Slots, *const Slots) -> *mut Slots,
    register_reads: unsafe extern "C" fn(*const Inst, *mut Graph, *mut Slots, u64),
}
unsafe extern "C" {
    #[link_name = "bootstrap.coloring::graph_block"]
    fn graph_block(graph: *mut Graph, code: Span, registers: Span, live: *mut Slots,
        break_live: *const Slots, continue_live: *const Slots, exit_live: *const Slots, weight: u64, host: Host) -> u32;
    #[link_name = "bootstrap.coloring::graph_together"]
    fn graph_together(graph: *mut Graph, live: *const Slots, core: Core);
    #[link_name = "bootstrap.coloring::graph_inputs"]
    fn graph_inputs(graph: *mut Graph, inputs: *const Reg, count: usize, core: Core);
    #[link_name = "bootstrap.coloring::graph_live_event"]
    fn graph_live_event(graph: *mut Graph, live: *const Slots, space: Space, index: usize,
        ordinal: usize, event: Event, core: Core);
    #[link_name = "bootstrap.coloring::graph_read"]
    fn graph_read(graph: *mut Graph, live: *mut Slots, space: Space, index: usize, weight: u64, core: Core);
}
pub(super) fn analyze(graph: &mut Graph, code: &[Inst], registers: &[Scalar], live: &mut Slots,
    break_live: &Slots, continue_live: &Slots, exit_live: &Slots, weight: u64) -> bool {
    let uses_r15 = unsafe {
        graph_block(graph, Span::slice(code), Span::slice(registers),
            live, break_live, continue_live, exit_live, weight, HOST)
    };
    uses_r15 != 0
}
pub(super) fn together(graph: &mut Graph, live: &Slots) { unsafe { graph_together(graph, live, CORE) } }
pub(super) fn inputs(graph: &mut Graph, inputs: &[Reg]) { unsafe { graph_inputs(graph, inputs.as_ptr(), inputs.len(), CORE) } }

unsafe extern "C" fn cast_bits(registers: Span, from: Reg, into: Reg) -> u32 {
    let registers = unsafe { registers.as_slice::<Scalar>() };
    u32::from(operands::bit_copy(registers[from], registers[into]))
}
unsafe extern "C" fn call_outgoing(call: *const Call) -> u64 {
    external::outgoing_bytes(unsafe { &*call })
}
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
unsafe extern "C" fn header(condition: Span, test: Reg, body: Span, after: *const Slots,
    break_live: *const Slots, continue_live: *const Slots, exit_live: *const Slots) -> *mut Slots {
    Box::into_raw(Box::new(loop_live(unsafe { condition.as_slice() }, test, unsafe { body.as_slice() },
        unsafe { &*after }, unsafe { &*break_live }, unsafe { &*continue_live }, unsafe { &*exit_live })))
}
unsafe extern "C" fn register_reads(instruction: *const Inst, graph: *mut Graph, live: *mut Slots, weight: u64) {
    let inst = unsafe { &*instruction };
    inst.all_operands(&mut |reg| { unsafe { graph_read(graph, live, Space::Register, reg, weight, CORE) }; true });
}
const CORE: Core = Core { relation, weight, set_weight, across, counter, set_outgoing, location, visit_set, slot };
const HOST: Host = Host {
    core: CORE, values: REGISTER_HOST, cast_bits, call_outgoing,
    math_outgoing: operands::scalar_math_outgoing,
    clone_set, drop_set, change, loop_live: header, register_reads,
};
