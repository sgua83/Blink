//! Windows x64 原生机器码后端。每个阶段编译为独立原生范围函数。
use crate::builtin_types::ScalarEncoding;
use crate::ir::*;
use super::{BufferAddress,NativeCode,NativeSymbol,Relocation,ContinuationCode,ProgramImage,RangeResult,SortCode,StageCode};
#[path="allocation.rs"]
mod allocation;
use allocation::{Allocation, Frame, PacketAllocation};
#[path="application.rs"]
mod application;
#[path="unwind.rs"]
mod unwind;
pub(crate) use unwind::{encode as unwind_info, Operation as UnwindOperation};
#[path="compact.rs"]
mod compact;
#[path="custom_reduce.rs"]
mod custom_reduce;
#[path="external.rs"]
mod external;
#[path="operands.rs"]
mod operands;
use operands::{AddressEncoding, Memory, MoveLocation};
#[path="packet.rs"]
mod packet;
#[path="program.rs"]
pub(crate) mod program;
#[path="reads.rs"]
mod reads;
pub use program::{compile_program, emit};
#[path="reduce.rs"]
mod reduce;
#[path="scan.rs"]
mod scan;
#[path="segmented_reduce.rs"]
mod segmented_reduce;
#[path="sort.rs"]
mod sort;
#[path="structure.rs"]
mod structure;
use structure::{Flow, ScalarLeaf};
use crate::ir::dataflow::{self, Node, Span, View};
use std::collections::{btree_map, btree_set};
#[path="texture.rs"]
mod texture;
pub(crate) use application::{
    emit as emit_application, emit_function,
};
#[path="tile.rs"]
mod tile;
pub use compact::emit_compact_scatter;
pub use custom_reduce::{
    emit_custom_reduce_tail, emit_custom_scan_add, emit_custom_scan_prefix,
    emit_custom_segmented_finish, emit_custom_segmented_prefix,
};
pub use scan::emit_scan_add;
pub use segmented_reduce::emit_segmented_reduce_finish;
pub use sort::{emit_sort_gather, emit_sort_histogram, emit_sort_key, emit_sort_scatter};

const BUFFER_BYTES: u32 = std::mem::size_of::<BufferAddress>() as u32;
const BUFFER_STRIDE: u32 = std::mem::offset_of!(BufferAddress, stride) as u32;

#[repr(C)]
struct RangeEnd {
    back_edge: usize,
    end: usize,
}

#[repr(C)]
struct RangePrologue {
    length: u64,
    probe_field: u64,
    operations: usize,
    stack_bytes: u32,
    prolog: u8,
}

#[repr(C)]
struct TailLiteral {
    bits: Bits,
    references: *const usize,
    reference_count: usize,
    wide: u64,
    target: usize,
}

#[repr(C)]
struct CodeTail {
    end: usize,
    region_count: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::stack_register"]
    fn encode_stack_register(register: u8, restore: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tail_capacity"]
    fn tail_capacity(probe: usize, literal_count: usize) -> usize;
    #[link_name = "bootstrap.x64_instruction::write_tail"]
    fn write_tail(image: *mut u8, end: usize, probe: usize, literals: *mut TailLiteral,
        literal_count: usize, regions: *mut super::Region) -> CodeTail;
    #[link_name = "bootstrap.x64_instruction::patch_relative"]
    fn patch_relative(field: *mut u8, offset: u64, target: u64);
    #[link_name = "bootstrap.x64_instruction::encode_branch"]
    fn encode_branch(condition: u8, test_bits: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::jump_to"]
    fn encode_jump_to(offset: u64, target: u64, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::patch_branches"]
    fn patch_branches(image: *mut u8, target: u64, fields: *const usize, count: u64);
    #[link_name = "bootstrap.frame_layout::range_prologue"]
    fn range_prologue(frame: u32, saved: *const allocation::SavedRegister, count: usize,
        operations: *mut UnwindOperation, output: *mut u8) -> RangePrologue;
    #[link_name = "bootstrap.frame_layout::range_epilogue"]
    fn range_epilogue(frame: u32, saved: *const allocation::SavedRegister, count: usize,
        image: *mut u8, end: usize, exits: *const usize, exit_count: usize) -> usize;
    #[link_name = "bootstrap.x64_instruction::range_bounds"]
    fn encode_range_bounds(packet: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::emit_map_range"]
    fn emit_map_range(input: MapRangeInput, host: MapRangeHost) -> u32;
    #[link_name = "bootstrap.x64_instruction::finish_range"]
    fn encode_range_end(image: *mut u8, at: usize, begin: usize, exit_field: usize,
        skip_field: usize, step: u8) -> RangeEnd;

}

enum Branch {
    Always,
    Flags(u8),
    Zero32,
    Nonzero32,
    Nonzero64,
}

static EMPTY_TILE_WORDS: std::collections::BTreeSet<SharedWord> = std::collections::BTreeSet::new();

struct Emitter<'a> {
    used_tile_words: &'a std::collections::BTreeSet<SharedWord>,
    code: Vec<u8>,
    unwind: Vec<UnwindOperation>,
    prolog: u8,
    probe: Option<usize>,
    relocations: Vec<Relocation>,
    calls: Vec<Option<usize>>,
    operation_ranges: Vec<super::OperationRange>,
    function_abi: Option<super::FunctionAbi>,
    literals: std::collections::BTreeMap<(Bits, bool), Vec<usize>>,
    stage: &'a Stage,
    workgroup: u32,
    buffers: &'a [Buffer],
    textures: &'a [TextureBinding],
    breaks: Vec<Vec<usize>>,
    exits: Vec<Vec<usize>>,
    continues: Vec<usize>,
    allocation: Allocation,
    frame: Frame,
}
impl Emitter<'_> {
    // Literal bytes belong to this code image, after all returning instructions
    // and any local helpers. Their references move with the same image.
    fn finish(mut self, stack_bytes: u32, include_operation_ranges: bool) -> NativeCode {
        let function_end = self.code.len();
        let unwind = unwind::encode(self.prolog, &self.unwind);
        let mut literals: Vec<_> = self.literals.iter().map(|(&(bits, wide), references)| TailLiteral {
            bits,
            references: references.as_ptr(),
            reference_count: references.len(),
            wide: u64::from(wide),
            target: 0,
        }).collect();
        let probe = self.probe.unwrap_or(0);
        let count = literals.len();
        let has_probe = usize::from(self.probe.is_some());
        let mut tail_regions = Vec::with_capacity(count + usize::from(count != 0) + has_probe);
        self.code.reserve(unsafe { tail_capacity(probe, count) });
        // B writes the tail and its regions while the original image and references are live.
        unsafe {
            let tail = write_tail(self.code.as_mut_ptr(), function_end, probe,
                literals.as_mut_ptr(), count, tail_regions.as_mut_ptr());
            self.code.set_len(tail.end);
            tail_regions.set_len(tail.region_count);
        }
        let mut tail_references = Vec::with_capacity(count + has_probe);
        if let Some(at) = self.probe {
            tail_references.push(super::TailReference { target: function_end, rel32_fields: vec![at] });
        }
        tail_references.extend(literals.into_iter().zip(self.literals.into_values())
            .map(|(literal, rel32_fields)| super::TailReference { target: literal.target, rel32_fields }));
        NativeCode {
            bytes: self.code,
            relocations: self.relocations,
            stack_bytes,
            calls: self.calls,
            function_end,
            unwind,
            tail_regions: Some(tail_regions),
            tail_references: Some(tail_references),
            sources: super::CodeSources::Emitted {
                operation_ranges: include_operation_ranges.then_some(self.operation_ranges),
                function_abi: self.function_abi,
            },
        }
    }
    fn push_registers(&mut self, registers: &[u8]) {
        for &register in registers {
            self.emit_bytes(2, |output| unsafe { encode_stack_register(register, 0, output) });
            let at = self.code.len() as u8;
            self.unwind.push(UnwindOperation::push(at, register));
        }
        self.prolog = self.code.len() as u8;
    }
    fn enter(&mut self, frame: u32) -> u32 {
        let saved = &self.frame.saved;
        self.code.reserve(39 + 9 * saved.len());
        self.unwind.reserve(6 + saved.len());
        let entry = unsafe {
            let entry = range_prologue(
                frame, saved.as_ptr(), saved.len(), self.unwind.as_mut_ptr(), self.code.as_mut_ptr(),
            );
            self.code.set_len(entry.length as usize);
            self.unwind.set_len(entry.operations);
            entry
        };
        self.probe = (entry.probe_field != 0).then_some(entry.probe_field as usize);
        self.prolog = entry.prolog;
        entry.stack_bytes
    }
    fn leave(&mut self, frame: u32) {
        let exits = self.exits.pop().unwrap();
        let saved = &self.frame.saved;
        let end = self.code.len();
        self.code.reserve(20 + 9 * saved.len());
        unsafe {
            let bytes = range_epilogue(
                frame, saved.as_ptr(), saved.len(), self.code.as_mut_ptr(), end,
                exits.as_ptr(), exits.len(),
            );
            self.code.set_len(end + bytes);
        }
    }
    fn bytes(&mut self, b: &[u8]) {
        self.code.extend_from_slice(b);
    }
    fn jump(&mut self, branch: Branch) -> usize {
        let (condition, test_bits) = match branch {
            Branch::Always => (0, 0),
            Branch::Flags(condition) => (condition, 0),
            Branch::Zero32 => (0x84, 32),
            Branch::Nonzero32 => (0x85, 32),
            Branch::Nonzero64 => (0x85, 64),
        };
        self.emit_bytes(9, |output| unsafe { encode_branch(condition, test_bits, output) });
        self.code.len() - 4
    }
    fn range_bounds(&mut self, packet: bool) -> usize {
        self.emit_bytes(16, |output| unsafe { encode_range_bounds(u32::from(packet), output) });
        self.code.len() - 4
    }
    fn finish_range(&mut self, begin: usize, exit_field: usize, skip_field: usize, step: u8) -> usize {
        let at = self.code.len();
        self.code.reserve(8);
        unsafe {
            let range = encode_range_end(self.code.as_mut_ptr(), at, begin, exit_field, skip_field, step);
            self.code.set_len(range.end);
            range.back_edge
        }
    }
    fn patch(&mut self, at: usize, target: usize) {
        let field = &mut self.code[at..at + 4];
        unsafe { patch_relative(field.as_mut_ptr(), at as u64, target as u64); }
    }
    fn jump_to(&mut self, target: usize) {
        let start = self.code.len();
        self.emit_bytes(5, |output| unsafe { encode_jump_to(start as u64, target as u64, output) });
    }

    fn patch_scope(&mut self, fields: &[usize]) {
        if !fields.is_empty() {
            let target = self.code.len() as u64;
            unsafe { patch_branches(self.code.as_mut_ptr(), target, fields.as_ptr(), fields.len() as u64) };
        }
    }

    fn close_exits(&mut self) {
        let fields = self.exits.pop().unwrap();
        self.patch_scope(&fields);
    }

    fn close_breaks(&mut self) {
        let fields = self.breaks.pop().unwrap();
        self.patch_scope(&fields);
    }

    fn abi_range(&mut self, start: usize, kind: super::AbiRangeKind) {
        self.record_abi_range(start..self.code.len(), kind);
    }
    fn record_abi_range(&mut self, range: std::ops::Range<usize>, kind: super::AbiRangeKind) {
        if let Some(abi) = &mut self.function_abi {
            abi.ranges.push(super::AbiRange { start: range.start, end: range.end, kind });
        }
    }
    fn parallel_abi(&mut self, start: usize, part: super::ParallelRangePart) {
        self.parallel_abi_range(start..self.code.len(), part);
    }
    fn parallel_abi_range(&mut self, range: std::ops::Range<usize>, part: super::ParallelRangePart) {
        self.record_abi_range(range, super::AbiRangeKind::ParallelRange { part, origin: OperationOrigin {
            source_site: self.stage.operation_source.clone(), generated_origin: self.stage.generated_origin.clone(),
        } });
    }
    fn operation_range(&mut self, inst: &Inst, start: usize, first: usize) {
        let includes_nested_operations = self.operation_ranges.len() != first;
        self.operation_ranges.push(super::OperationRange {
            start, end: self.code.len(), includes_nested_operations, origin: OperationOrigin::from_instruction(inst),
        });
    }
}
/// 所有范围共用RangeEntry：buffers、uniforms、end、begin、output，返回执行状态。
/// 算法输出（包括计数）只写第五参数，map及无摘要续阶段不读取该指针。
#[repr(C)]
struct MapRangeInput {
    code: Span,
    packet: *const std::ffi::c_void,
    phase_count: usize,
    binding: usize,
    word: u32,
    selected: u32,
    frame: u32,
    slots: u32,
    buffer_bytes: u32,
    stride_field: u32,
    tile_scope: u32,
    workgroup: u32,
}

#[repr(u32)]
enum MapRangePart {
    Enter, PacketBounds, PacketCollect, PacketAdvance, PacketBackEdge, PacketTail,
    ScalarBounds, ScalarSelection, ScalarAdvance, ScalarBackEdge, Exit,
    TileBase, TilePhaseEnter, TilePhaseAdvance, TileInputBoundary, TilePrivatePointer,
}

#[repr(C)]
pub(super) struct NativeCodeBuffer { image: *mut u8, end: usize }

impl NativeCodeBuffer {
    fn reserve(code: &mut Vec<u8>, count: usize) -> Self {
        code.reserve(count);
        Self { image: code.as_mut_ptr(), end: code.len() }
    }
}

#[repr(C)]
struct MapRangeEntry { end: usize, stack_bytes: u32 }

#[repr(C)]
struct MapRangeObservation {
    start: usize,
    end: usize,
    index: usize,
    value: u32,
    part: MapRangePart,
}

#[repr(C)]
struct TilePhaseView { code: Span, step: Node }

#[repr(C)]
#[derive(Default)]
struct TileInputFragment { binding: usize, source_offset: u32, storage_base: u32, unit_bytes: u32 }

#[repr(C)]
struct TilePrivateSlot {
    location: operands::MoveLocation,
    scalar: ScalarEncoding,
    index: usize,
    scalar_origin: *const Scalar,
    storage_offset: u32,
    local: u32,
}

struct TileInputCursor<'a> {
    groups: btree_map::Iter<'a, (usize, u32), std::collections::BTreeSet<(u32, Scalar)>>,
    fragments: btree_set::Iter<'a, (u32, Scalar)>,
    binding: usize,
    storage_base: u32,
}

type MapRangeSlot = *mut std::ffi::c_void;

#[repr(C)]
struct MapRangeHost {
    context: MapRangeSlot,
    enter: unsafe extern "C" fn(MapRangeSlot, u32) -> MapRangeEntry,
    leave: unsafe extern "C" fn(MapRangeSlot, u32) -> usize,
    reserve: unsafe extern "C" fn(MapRangeSlot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(MapRangeSlot, usize),
    packet: unsafe extern "C" fn(MapRangeSlot, *const std::ffi::c_void) -> u32,
    scalar: unsafe extern "C" fn(MapRangeSlot, Span) -> u32,
    next_input: unsafe extern "C" fn(MapRangeSlot) -> TileInputFragment,
    observe_input: unsafe extern "C" fn(MapRangeSlot, usize, usize, TileInputFragment, u32),
    ir: dataflow::Host,
    phase: unsafe extern "C" fn(MapRangeSlot, usize) -> TilePhaseView,
    next_private: unsafe extern "C" fn(MapRangeSlot, u32) -> TilePrivateSlot,
    observe_private: unsafe extern "C" fn(MapRangeSlot, usize, usize, usize, u32, TilePrivateSlot),
    tile_step: unsafe extern "C" fn(MapRangeSlot, *const Inst, ScalarLeaf, View),
    observe: unsafe extern "C" fn(MapRangeSlot, MapRangeObservation),
}

struct MapRange<'a, 'b> {
    emitter: &'a mut Emitter<'b>,
    phases: &'a [TilePhase<'b>],
    input: TileInputCursor<'a>,
    live_in: btree_set::Iter<'a, PrivateSlot>,
    saved: btree_set::Iter<'a, PrivateSlot>,
    result: Result<(), &'static str>,
}

unsafe extern "C" fn map_range_enter(slot: MapRangeSlot, frame: u32) -> MapRangeEntry {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let stack_bytes = range.emitter.enter(frame);
    MapRangeEntry { end: range.emitter.code.len(), stack_bytes }
}

unsafe extern "C" fn map_range_leave(slot: MapRangeSlot, frame: u32) -> usize {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    range.emitter.leave(frame);
    range.emitter.code.len()
}

impl Emitter<'_> {
    pub(super) fn reserve_native_code(&mut self, count: usize) -> NativeCodeBuffer {
        NativeCodeBuffer::reserve(&mut self.code, count)
    }

    pub(super) unsafe fn commit_native_code(&mut self, end: usize) {
        unsafe { self.code.set_len(end) };
    }
}

unsafe extern "C" fn map_range_reserve(slot: MapRangeSlot, count: usize) -> NativeCodeBuffer {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    range.emitter.reserve_native_code(count)
}

unsafe extern "C" fn map_range_commit(slot: MapRangeSlot, end: usize) {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    unsafe { range.emitter.commit_native_code(end) };
}

unsafe extern "C" fn map_range_packet(slot: MapRangeSlot, packet: *const std::ffi::c_void) -> u32 {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let (plan, allocation) = unsafe { &*packet.cast::<(packet::Plan<'_>, PacketAllocation)>() };
    range.result = range.emitter.emit_packet_plan(plan, allocation);
    u32::from(range.result.is_ok())
}

unsafe extern "C" fn map_range_scalar(slot: MapRangeSlot, code: Span) -> u32 {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    range.result = range.emitter.block(unsafe { code.as_slice() });
    u32::from(range.result.is_ok())
}

unsafe extern "C" fn map_range_next_input(slot: MapRangeSlot) -> TileInputFragment {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let input = &mut range.input;
    loop {
        if let Some(&(source_offset, unit)) = input.fragments.next() {
            return TileInputFragment {
                binding: input.binding, source_offset,
                storage_base: input.storage_base, unit_bytes: unit.size(),
            };
        }
        let Some((&(binding, word), fragments)) = input.groups.next() else {
            return TileInputFragment::default();
        };
        input.binding = binding;
        input.storage_base = range.emitter.shared_stream(SharedWord::Input { binding, word });
        input.fragments = fragments.iter();
    }
}

unsafe extern "C" fn map_range_observe_input(
    slot: MapRangeSlot, start: usize, end: usize, item: TileInputFragment, storage_offset: u32,
) {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    range.emitter.parallel_abi_range(start..end, super::ParallelRangePart::TileInputCopy {
        binding: item.binding, source_offset: item.source_offset,
        storage_offset, unit_bytes: item.unit_bytes,
    });
}

unsafe extern "C" fn map_range_phase(slot: MapRangeSlot, index: usize) -> TilePhaseView {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let phase = &range.phases[index];
    range.live_in = phase.live_in.iter();
    range.saved = phase.saved.iter();
    TilePhaseView { code: Span::slice(phase.code), step: phase.step.map_or(Node::EMPTY, Node::instruction) }
}

unsafe extern "C" fn map_range_next_private(slot: MapRangeSlot, restore: u32) -> TilePrivateSlot {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let slots = if restore != 0 { &mut range.live_in } else { &mut range.saved };
    let Some(&slot) = slots.next() else {
        return TilePrivateSlot {
            location: allocation::Location::Immediate(0).into(), scalar: Scalar::U32.into(),
            index: 0, scalar_origin: std::ptr::null(), storage_offset: 0, local: 0,
        };
    };
    let scalar = range.emitter.stage.private_scalar_ref(slot);
    let (local, index) = match slot {
        PrivateSlot::Register(index) => (0, index),
        PrivateSlot::Local(index) => (1, index),
    };
    TilePrivateSlot {
        location: range.emitter.allocation.location(slot).into(), scalar: (*scalar).into(),
        index, scalar_origin: scalar,
        storage_offset: range.emitter.private_stream(slot), local,
    }
}

unsafe extern "C" fn map_range_observe_private(
    slot: MapRangeSlot, start: usize, end: usize, phase: usize, restore: u32, item: TilePrivateSlot,
) {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    range.emitter.parallel_abi_range(start..end, super::ParallelRangePart::TilePrivate {
        phase, restore: restore != 0, local: item.local != 0, slot: item.index,
        storage_offset: item.storage_offset, scalar: unsafe { *item.scalar_origin },
    });
}

unsafe extern "C" fn map_range_tile_step(slot: MapRangeSlot, inst: *const Inst, leaf: ScalarLeaf, value: View) {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let emitter = &mut *range.emitter;
    let start = emitter.code.len();
    let first = emitter.operation_ranges.len();
    // The current phase lends its original step to the same leaf emitter as scalar code.
    unsafe { structure::emit_scalar(std::ptr::from_mut(emitter).cast(), leaf, value) };
    emitter.operation_range(unsafe { &*inst }, start, first);
}

unsafe extern "C" fn map_range_observe(slot: MapRangeSlot, event: MapRangeObservation) {
    use super::ParallelRangePart as Part;
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let part = match event.part {
        MapRangePart::Enter => Part::Enter { frame_bytes: event.value },
        MapRangePart::Exit => Part::Exit { frame_bytes: event.value },
        MapRangePart::PacketBounds => Part::PacketBounds,
        MapRangePart::PacketCollect => Part::PacketCollect { binding: event.index, offset: event.value },
        MapRangePart::PacketAdvance => Part::PacketAdvance,
        MapRangePart::PacketBackEdge => Part::PacketBackEdge,
        MapRangePart::PacketTail => Part::PacketTail,
        MapRangePart::ScalarBounds => Part::ScalarBounds,
        MapRangePart::ScalarSelection => Part::ScalarSelection { binding: event.index, offset: event.value },
        MapRangePart::ScalarAdvance => Part::ScalarAdvance,
        MapRangePart::ScalarBackEdge => Part::ScalarBackEdge,
        MapRangePart::TileBase => Part::TileBase { storage_offset: event.value },
        MapRangePart::TilePhaseEnter => Part::TilePhaseEnter { phase: event.index },
        MapRangePart::TilePhaseAdvance => Part::TilePhaseAdvance { phase: event.index },
        MapRangePart::TileInputBoundary => Part::TileInputBoundary { width: event.value },
        MapRangePart::TilePrivatePointer => Part::TilePrivatePointer { phase: event.index, restore: event.value != 0 },
    };
    range.emitter.parallel_abi_range(event.start..event.end, part);
}


fn emit_map(
    program: &Program, index: usize, inputs: &crate::ir::dataflow::InputFragments,
    used: &std::collections::BTreeSet<SharedWord>, layout: crate::ir::dataflow::TileLayout,
) -> Result<NativeCode, &'static str> {
    let mut lowered = program.stages[index].clone();
    reads::place(&mut lowered.code, &mut lowered.registers, &lowered.writes);
    let stage = &lowered;
    let tile_width = stage.tile_workgroup_width();
    let tile_scope = tile_width.is_some();
    let workgroup = tile_width.unwrap_or(Stage::DEFAULT_WORKGROUP_WIDTH);
    let (mut graph, phases) = allocation::Graph::with_live(stage, Default::default(), tile_scope);
    let allocation = Allocation::new(stage, &mut graph, tile_scope);
    let packet = packet::Plan::new(program, stage, tile_scope).map(|plan| {
        let allocation = PacketAllocation::new(stage, &mut graph, &plan.address_only);
        (plan, allocation)
    });
    drop(graph);
    let mut frame_layout = Frame::new(
        &allocation,
        packet.as_ref().map(|(_, allocation)| allocation),
    );
    let data_end = if tile_scope {
        frame_layout.layout_tile(stage, workgroup, &phases, layout)
    } else {
        frame_layout.end
    };
    let bytes = data_end + if packet.is_some() && stage.selection.is_some() { 16 } else { 0 };
    let frame = i32::try_from(bytes.div_ceil(16) * 16)
        .map_err(|_| "原生 tile 栈帧超过 x64 范围函数 ABI")? as u32;
    let mut e = Emitter {
        used_tile_words: used,
        unwind: Vec::new(),
        prolog: 0,
        probe: None,
        code: Vec::new(),
        relocations: Vec::new(),
        calls: Vec::new(),
        operation_ranges: Vec::new(),
        function_abi: Some(super::FunctionAbi { definition: None, instance: None, ranges: Vec::new() }),
        literals: std::collections::BTreeMap::new(),
        stage,
        workgroup,
        buffers: &program.buffers,
        textures: &program.textures,
        breaks: Vec::new(),
        exits: vec![Vec::new()],
        continues: Vec::new(),
        allocation,
        frame: frame_layout,
    };
    let (binding, word, selected) = stage.selection.map_or((0, 0, 0), |selection| {
        (selection.binding, selection.word, 1)
    });
    let input = MapRangeInput {
        code: Span::slice(&stage.code),
        phase_count: phases.len(),
        packet: packet.as_ref().map_or(std::ptr::null(), |packet| std::ptr::from_ref(packet).cast()),
        binding, word, selected, frame, slots: e.frame.end as u32,
        buffer_bytes: BUFFER_BYTES, stride_field: BUFFER_STRIDE,
        tile_scope: u32::from(tile_scope), workgroup,
    };
    let mut range = MapRange {
        emitter: &mut e,
        phases: &phases,
        input: TileInputCursor {
            groups: inputs.iter(), fragments: btree_set::Iter::default(), binding: 0, storage_base: 0,
        },
        live_in: btree_set::Iter::default(),
        saved: btree_set::Iter::default(),
        result: Ok(()),
    };
    let stack_bytes = unsafe {
        emit_map_range(input, MapRangeHost {
            context: std::ptr::from_mut(&mut range).cast(),
            enter: map_range_enter, leave: map_range_leave,
            reserve: map_range_reserve, commit: map_range_commit,
            packet: map_range_packet, scalar: map_range_scalar,
            next_input: map_range_next_input, observe_input: map_range_observe_input,
            ir: dataflow::HOST, phase: map_range_phase,
            next_private: map_range_next_private, observe_private: map_range_observe_private,
            tile_step: map_range_tile_step, observe: map_range_observe,
        })
    };
    range.result?;
    Ok(e.finish(stack_bytes, true))
}
