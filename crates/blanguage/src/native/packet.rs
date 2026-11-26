//! 四个独立元素的 SSE2 降低；计划只包含已证明的访问与可表达指令，尾部共用标量 IR。
use super::*;
use allocation::{ColorLayout, Graph, PacketAllocation, PacketPositions};
use crate::ir::dataflow;
use crate::builtin_types::ScalarEncoding;
use crate::ir::dataflow::{HOST, Span};
use super::structure::{self, ScalarCursor};
#[path="packet/control.rs"]
mod control;
pub(super) struct Plan<'a> {
    code: Span,
    _instructions: std::marker::PhantomData<&'a [Inst]>,
}

impl Drop for Plan<'_> {
    fn drop(&mut self) {
        unsafe { packet_plan_drop(self.code) };
    }
}
impl<'a> Plan<'a> {
    pub fn new(
        program: &Program, stage: &Stage, code: &'a [Inst], registers: &'a [Scalar],
        tile_scope: bool, graph: &Graph,
    ) -> Option<(Self, PacketAllocation)> {
        control::run(program, stage, code, registers, tile_scope, graph)
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::packet_plan_drop"]
    fn packet_plan_drop(code: Span);
    #[link_name = "bootstrap.x64_instruction::emit_packet_body"]
    fn emit_packet_body(context: Slot, emitter: Slot, code: Span, targets: structure::ControlTargets, layout: PacketLayout, host: PacketBodyHost);
    #[link_name = "bootstrap.x64_instruction::packet_condition"]
    fn packet_condition(context: Slot, register: Reg, output: *mut u8) -> u64;
}

type Slot = *mut std::ffi::c_void;

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
    mark: unsafe extern "C" fn(Slot) -> ScalarCursor,
    publish: unsafe extern "C" fn(Slot, *const Inst, ScalarCursor),
    reserve: unsafe extern "C" fn(Slot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(Slot, usize),
    ir: dataflow::Host,
    scalar: unsafe extern "C" fn(*const std::ffi::c_void, Reg) -> ScalarEncoding,
}

#[repr(C)]
struct PacketBody<'a> {
    positions: PacketPositions,
    registers: &'a [Scalar],
    layout: PacketLayout,
}

unsafe extern "C" fn packet_scalar(context: *const std::ffi::c_void, reg: Reg) -> ScalarEncoding {
    let body = unsafe { &*context.cast::<PacketBody<'_>>() };
    body.registers[reg].into()
}

fn packet_host() -> PacketBodyHost {
    PacketBodyHost {
        control: structure::control_host(emit_packet_child, test_packet),
        mark: structure::mark_scalar,
        publish: structure::publish_scalar,
        reserve: structure::reserve_scalar,
        commit: structure::commit_scalar,
        ir: HOST,
        scalar: packet_scalar,
    }
}

unsafe extern "C" fn emit_packet_child(context: Slot, emitter: Slot, code: Span, targets: structure::ControlTargets) -> u32 {
    let layout = unsafe { &*context.cast::<PacketBody<'_>>() }.layout;
    unsafe { emit_packet_body(context, emitter, code, targets, layout, packet_host()) };
    0
}

unsafe extern "C" fn test_packet(context: Slot, emitter: Slot, reg: Reg) {
    let emitter = unsafe { &mut *emitter.cast::<Emitter<'_>>() };
    emitter.emit_bytes(16, |output| unsafe { packet_condition(context, reg, output) });
}

impl Emitter<'_> {
    pub(super) fn emit_packet_plan(&mut self, plan: &Plan<'_>, allocation: &PacketAllocation, sparse: u32, targets: structure::ControlTargets) {
        let layout = PacketLayout { sparse, slots: self.frame.end as u32, buffer_bytes: BUFFER_BYTES,
            stride_field: BUFFER_STRIDE };
        let mut body = PacketBody { positions: allocation.positions, registers: self.registers, layout };
        let context = std::ptr::from_mut(&mut body).cast();
        let emitter = std::ptr::from_mut(self).cast();
        unsafe { emit_packet_body(context, emitter, plan.code, targets, layout, packet_host()) };
    }
}
