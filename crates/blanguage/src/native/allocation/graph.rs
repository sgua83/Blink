//! Borrowed IR and primitive access to the one native allocation graph.
use super::*;
use crate::builtin_types::ScalarEncoding;
use crate::ir::dataflow::Span;
use crate::ir::{PRIVATE_LIVE_HOST, PrivateLiveHost};
use super::super::external;

type Slots = BTreeSet<PrivateSlot>;
#[repr(u32)]
#[derive(Clone, Copy)]
enum Space { Register, Local }
#[repr(C)]
#[derive(Clone, Copy)]
struct Event { anchor: usize, skip: usize, kind: u32, skip_present: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct Core {
    visit_set: unsafe extern "C" fn(*const Slots, usize, *mut c_void, Event),
}
#[repr(C)]
struct Host {
    core: Core,
    live: PrivateLiveHost,
    scalar: unsafe extern "C" fn(Span, Reg) -> ScalarEncoding,
    call_abi: unsafe extern "C" fn(*const Call) -> external::CallAbi,
    call_argument: external::ArgumentType,
    register_operand: unsafe extern "C" fn(*mut c_void, Reg) -> u32,
}
#[repr(C)]
struct Request {
    code: Span,
    registers: Span,
    places: *mut ColorPlace,
    count: usize,
    live: *mut Slots,
    empty: *const Slots,
}
#[repr(C)]
struct Built {
    state: *mut c_void,
    uses_r15: u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.coloring::graph_create"]
    fn graph_create(request: *const Request, host: Host) -> Built;
    #[link_name = "bootstrap.coloring::graph_together"]
    fn graph_together(graph: *mut c_void, live: *const Slots, core: Core);
    #[link_name = "bootstrap.coloring::graph_inputs"]
    fn graph_inputs(graph: *mut c_void, inputs: *const Reg, count: usize);
    #[link_name = "bootstrap.coloring::graph_live_event"]
    fn graph_live_event(graph: *mut c_void, live: *const Slots, space: Space, index: usize,
        ordinal: usize, event: Event, core: Core);
    #[link_name = "bootstrap.coloring::graph_register_read"]
    fn graph_register_read(graph: *mut c_void, index: usize, weight: u64);
}
pub(super) fn build(places: &mut [ColorPlace], count: usize, code: &[Inst],
    registers: &[Scalar], live: &mut Slots, empty: &Slots) -> (*mut c_void, bool) {
    let request = Request {
        code: Span::slice(code), registers: Span::slice(registers),
        places: places.as_mut_ptr(), count, live, empty,
    };
    let built = unsafe { graph_create(&request, HOST) };
    (built.state, built.uses_r15 != 0)
}
pub(super) fn together(graph: &mut Graph, live: &Slots) { unsafe { graph_together(graph.state, live, CORE) } }
pub(super) fn inputs(graph: &mut Graph, inputs: &[Reg]) { unsafe { graph_inputs(graph.state, inputs.as_ptr(), inputs.len()) } }

unsafe extern "C" fn scalar(registers: Span, register: Reg) -> ScalarEncoding {
    let registers = unsafe { registers.as_slice::<Scalar>() };
    registers[register].into()
}
unsafe extern "C" fn visit_set(live: *const Slots, limit: usize, graph: *mut c_void, event: Event) {
    for (ordinal, slot) in unsafe { &*live }.iter().take(limit).enumerate() {
        let (space, index) = match slot { PrivateSlot::Register(index) => (Space::Register, *index), PrivateSlot::Local(index) => (Space::Local, *index) };
        unsafe { graph_live_event(graph, live, space, index, ordinal, event, CORE); }
    }
}
#[repr(C)]
struct Operand {
    graph: *mut c_void,
    live: *mut Slots,
    weight: u64,
}

unsafe extern "C" fn register_operand(context: *mut c_void, register: Reg) -> u32 {
    let operand = unsafe { &*context.cast::<Operand>() };
    unsafe {
        graph_register_read(operand.graph, register, operand.weight);
        (PRIVATE_LIVE_HOST.insert_register)(operand.live, register)
    }
}

const CORE: Core = Core { visit_set };
const HOST: Host = Host {
    core: CORE, live: PRIVATE_LIVE_HOST, scalar,
    call_abi: external::call_abi, call_argument: external::argument_type, register_operand,
};
