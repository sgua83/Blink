//! Synchronous access to the original x64 emitter and its scalar or packet blocks.
use super::*;
use std::ffi::c_void;
use crate::ir::dataflow::{self, Span, View};

type Slot = *mut c_void;

#[repr(u32)]
#[allow(dead_code)]
pub(super) enum ControlKind { If, Loop, Scope, Exit, Break, Continue, Raise }

#[repr(C)]
pub(super) struct Control {
    pub(super) first: Span,
    pub(super) second: Span,
    pub(super) register: Reg,
    pub(super) kind: ControlKind,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ControlTargets {
    function: *mut usize,
    block: *mut usize,
    loop_exit: *mut usize,
    loop_begin: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Host {
    emit: unsafe extern "C" fn(Slot, Slot, Span, ControlTargets) -> u32,
    load: unsafe extern "C" fn(Slot, Slot, Reg),
    buffer: unsafe extern "C" fn(Slot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(Slot, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::emit_scalar_body"]
    fn emit_scalar_body(context: Slot, emitter: Slot, code: Span, targets: ControlTargets, host: ScalarHost) -> ScalarBodyStatus;
}

unsafe fn emitter<'a, 'b>(slot: Slot) -> &'a mut Emitter<'b> {
    unsafe { &mut *slot.cast() }
}

pub(super) fn control_host(
    emit: unsafe extern "C" fn(Slot, Slot, Span, ControlTargets) -> u32,
    load: unsafe extern "C" fn(Slot, Slot, Reg),
) -> Host {
    Host { emit, load, buffer: reserve_scalar, commit: commit_scalar }
}

#[repr(u32)]
#[allow(dead_code)]
pub(super) enum ScalarLeaf {
    Call, Print, Texture, Publish, Collective, TileLength, TileLane, FunctionAddress,
}

#[repr(u32)]
#[allow(dead_code)]
pub(super) enum ScalarBodyStatus { Done, Unlowered, Subgroup, RayQuery }

impl ScalarBodyStatus {
    pub(super) fn result(self) -> Result<(), &'static str> {
        match self {
            Self::Done => Ok(()),
            Self::Unlowered => Err("原生操作尚未降低到CPU执行IR"),
            Self::Subgroup => Err("CPU 后端不能执行 GPU subgroup 协作"),
            Self::RayQuery => Err("CPU 后端不能执行硬件光线查询"),
        }
    }
}

#[repr(C)]
pub(super) struct ScalarCursor { start: usize, ranges: usize }

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ScalarHost {
    control: Host,
    mark: unsafe extern "C" fn(Slot) -> ScalarCursor,
    publish: unsafe extern "C" fn(Slot, *const Inst, ScalarCursor),
    emit: unsafe extern "C" fn(Slot, ScalarLeaf, View),
    addresses: operands::IndexedAddressHost,
    local: unsafe extern "C" fn(*const c_void, usize) -> MoveLocation,
    reserve: unsafe extern "C" fn(Slot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(Slot, usize),
    literal: unsafe extern "C" fn(Slot, usize, Bits, u32),
    math_symbol: unsafe extern "C" fn(Slot, usize, ScalarMathReference),
}

#[repr(C)]
struct ScalarMathReference { field: u64, length: u32, name: [u8; 6] }

pub(super) unsafe extern "C" fn reserve_scalar(slot: Slot, count: usize) -> NativeCodeBuffer {
    unsafe { emitter(slot) }.reserve_native_code(count)
}

pub(super) unsafe extern "C" fn commit_scalar(slot: Slot, end: usize) {
    unsafe { emitter(slot).commit_native_code(end) };
}

unsafe extern "C" fn scalar_literal(slot: Slot, at: usize, bits: Bits, wide: u32) {
    unsafe { emitter(slot) }.record_literal_at(at, (bits, wide != 0));
}

unsafe extern "C" fn scalar_math_symbol(slot: Slot, start: usize, reference: ScalarMathReference) {
    let symbol = unsafe { std::str::from_utf8_unchecked(&reference.name[..reference.length as usize]) };
    let target = NativeSymbol::External(crate::ast::ExternalName {
        library: "ucrtbase.dll".into(), symbol: symbol.into(),
    });
    unsafe { emitter(slot) }.record_symbol_target(start + reference.field as usize, target, true);
}

unsafe extern "C" fn scalar_child(context: Slot, slot: Slot, code: Span, targets: ControlTargets) -> u32 {
    let host = unsafe { *context.cast::<ScalarHost>() };
    unsafe { emit_scalar_body(context, slot, code, targets, host) as u32 }
}

unsafe extern "C" fn scalar_load(_: Slot, slot: Slot, register: Reg) {
    unsafe { emitter(slot) }.load(register);
}

pub(super) unsafe extern "C" fn mark_scalar(slot: Slot) -> ScalarCursor {
    let e = unsafe { emitter(slot) };
    ScalarCursor { start: e.code.len(), ranges: e.operation_ranges.len() }
}

pub(super) unsafe extern "C" fn publish_scalar(slot: Slot, inst: *const Inst, cursor: ScalarCursor) {
    unsafe { emitter(slot) }.operation_range(unsafe { &*inst }, cursor.start, cursor.ranges);
}

pub(super) unsafe extern "C" fn emit_scalar(slot: Slot, leaf: ScalarLeaf, value: View) {
    let e = unsafe { emitter(slot) };
    match leaf {
        ScalarLeaf::Call => e.native_call(unsafe { value.left_as() }),
        ScalarLeaf::Print => e.print_call(value.a, value.b),
        ScalarLeaf::Texture => e.texture_access(value),
        ScalarLeaf::Publish => e.publish_values(value),
        ScalarLeaf::Collective => e.tile_collective(value),
        ScalarLeaf::TileLength => e.tile_length(value.a),
        ScalarLeaf::TileLane => e.tile_lane(value.a),
        ScalarLeaf::FunctionAddress => {
            match unsafe { value.left_as() } {
                CallTarget::Defined(name) => e.emit_symbol_target(NativeSymbol::Function(name.clone()), false),
                CallTarget::External(name) => e.emit_symbol_target(NativeSymbol::External(name.clone()), false),
                CallTarget::Indirect(reg) => e.load(*reg),
            }
            e.store(value.a);
        }
    }
}

impl ScalarHost {
    pub(super) fn new(slot: Slot, workgroup: u32, tiles: operands::TileSlots) -> Self {
        Self {
            control: control_host(scalar_child, scalar_load),
            mark: mark_scalar,
            publish: publish_scalar,
            emit: emit_scalar,
            addresses: operands::scalar_body_addresses(slot, workgroup, tiles),
            local: operands::scalar_body_local,
            reserve: reserve_scalar,
            commit: commit_scalar,
            literal: scalar_literal,
            math_symbol: scalar_math_symbol,
        }
    }
}

impl Emitter<'_> {
    pub(super) fn block(&mut self, code: &[Inst], targets: ControlTargets) -> Result<(), &'static str> {
        let workgroup = self.workgroup;
        let tiles = operands::TileSlots::new(self.tiles, &self.frame);
        let slot = std::ptr::from_mut(self).cast();
        let mut host = ScalarHost::new(slot, workgroup, tiles);
        unsafe { emit_scalar_body(std::ptr::from_mut(&mut host).cast(), slot, Span::slice(code), targets, host) }.result()
    }
}
