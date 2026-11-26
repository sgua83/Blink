//! 四个独立元素的 SSE2 降低；计划只包含已证明的访问与可表达指令，尾部共用标量 IR。
use super::*;
use allocation::{PacketAllocation, PacketPlace};
use super::operands::ScalarValueHost;
use crate::builtin_types::ScalarEncoding;
use crate::ir::dataflow::{HOST, Span};
use super::structure::{self, ScalarCursor};
#[path="packet/control.rs"]
mod control;
struct Step<'a> {
    instruction: &'a Inst,
    decision: control::Decision,
    first: Vec<Step<'a>>,
    second: Vec<Step<'a>>,
}

pub(super) struct Plan<'a> {
    code: Vec<Step<'a>>,
}
impl<'a> Plan<'a> {
    pub fn new(
        program: &Program, stage: &Stage, code: &'a [Inst], registers: &'a [Scalar],
        tile_scope: bool,
    ) -> Option<(Self, Vec<u8>)> {
        control::run(program, stage, code, registers, tile_scope)
    }
}

impl From<PacketPlace> for operands::MoveLocation {
    fn from(place: PacketPlace) -> Self {
        match place {
            PacketPlace::Register(reg) => allocation::Location::Xmm(reg).into(),
            PacketPlace::Stack(offset) => operands::Memory::new(4, offset).into(),
        }
    }
}
unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::emit_packet_body"]
    fn emit_packet_body(context: Slot, emitter: Slot, code: Span, targets: structure::ControlTargets, layout: PacketLayout, host: PacketBodyHost);
    #[link_name = "bootstrap.x64_instruction::packet_lane"]
    fn encode_packet_lane(src: operands::MoveLocation, lane: u8, gpr: u8, output: *mut u8) -> u64;
}

type Slot = *mut std::ffi::c_void;

#[repr(C)]
struct PacketBodyStep {
    instruction: *const Inst,
    first: Span,
    second: Span,
    decision: control::Decision,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PacketLayout {
    sparse: u32,
    slots: u32,
    buffer_bytes: u32,
    stride_field: u32,
}

#[repr(C)]
struct PacketBodyHost {
    control: structure::Host,
    step: unsafe extern "C" fn(*const Step<'_>) -> PacketBodyStep,
    local: unsafe extern "C" fn(Slot, usize) -> operands::MoveLocation,
    mark: unsafe extern "C" fn(Slot) -> ScalarCursor,
    publish: unsafe extern "C" fn(Slot, *const Inst, ScalarCursor),
    reserve: unsafe extern "C" fn(Slot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(Slot, usize),
    values: ScalarValueHost,
}

struct PacketBody<'a> {
    allocation: &'a PacketAllocation,
    registers: &'a [Scalar],
    layout: PacketLayout,
}

unsafe extern "C" fn packet_location(context: *const std::ffi::c_void, reg: Reg) -> operands::MoveLocation {
    let body = unsafe { &*context.cast::<PacketBody<'_>>() };
    body.allocation.location(PrivateSlot::Register(reg)).into()
}

unsafe extern "C" fn packet_scalar(context: *const std::ffi::c_void, reg: Reg) -> ScalarEncoding {
    let body = unsafe { &*context.cast::<PacketBody<'_>>() };
    body.registers[reg].into()
}

unsafe extern "C" fn packet_local(context: Slot, slot: usize) -> operands::MoveLocation {
    let body = unsafe { &*context.cast::<PacketBody<'_>>() };
    body.allocation.location(PrivateSlot::Local(slot)).into()
}

fn packet_host(context: Slot) -> PacketBodyHost {
    PacketBodyHost {
        control: structure::control_host(emit_packet_child, test_packet),
        step: packet_body_step,
        local: packet_local,
        mark: structure::mark_scalar,
        publish: structure::publish_scalar,
        reserve: structure::reserve_scalar,
        commit: structure::commit_scalar,
        values: ScalarValueHost { ir: HOST, context, location: packet_location, scalar: packet_scalar },
    }
}

unsafe extern "C" fn emit_packet_child(context: Slot, emitter: Slot, code: Span, targets: structure::ControlTargets) -> u32 {
    let layout = unsafe { &*context.cast::<PacketBody<'_>>() }.layout;
    unsafe { emit_packet_body(context, emitter, code, targets, layout, packet_host(context)) };
    0
}

unsafe extern "C" fn test_packet(context: Slot, emitter: Slot, reg: Reg) {
    let source = unsafe { packet_location(context, reg) };
    let emitter = unsafe { &mut *emitter.cast::<Emitter<'_>>() };
    emitter.emit_bytes(16, |output| unsafe { encode_packet_lane(source, 0, 0, output) });
}

unsafe extern "C" fn packet_body_step(step: *const Step<'_>) -> PacketBodyStep {
    let step = unsafe { &*step };
    PacketBodyStep {
        instruction: step.instruction,
        first: Span::slice(&step.first),
        second: Span::slice(&step.second),
        decision: step.decision,
    }
}

impl Emitter<'_> {
    pub(super) fn emit_packet_plan(&mut self, plan: &Plan<'_>, allocation: &PacketAllocation, sparse: u32, targets: structure::ControlTargets) {
        let layout = PacketLayout { sparse, slots: self.frame.end as u32, buffer_bytes: BUFFER_BYTES,
            stride_field: BUFFER_STRIDE };
        let mut body = PacketBody { allocation, registers: self.registers, layout };
        let context = std::ptr::from_mut(&mut body).cast();
        let emitter = std::ptr::from_mut(self).cast();
        unsafe { emit_packet_body(context, emitter, Span::slice(&plan.code), targets, layout, packet_host(context)) };
    }
}
