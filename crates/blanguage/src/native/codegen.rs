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
use structure::ScalarLeaf;
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
struct FrameOutput {
    code: *mut u8,
    operations: *mut UnwindOperation,
    saved: allocation::SavedLayout,
    count: usize,
}

#[repr(C)]
struct FrameEntry {
    length: usize,
    probe_field: usize,
    operations: usize,
    prolog: u8,
}

#[repr(C)]
struct RangeFrameHost {
    reserve: unsafe extern "C" fn(*mut std::ffi::c_void, usize, usize) -> FrameOutput,
    publish: unsafe extern "C" fn(*mut std::ffi::c_void, FrameEntry),
}

#[repr(C)]
struct TailLiteral {
    bits: Bits,
    references: *const usize,
    reference_count: usize,
    wide: u64,
}

struct TailSource<'a> {
    literals: std::collections::btree_map::IntoIter<(Bits, bool), Vec<usize>>,
    references: &'a mut Vec<super::TailReference>,
}

unsafe extern "C" fn next_tail_literal(context: *mut std::ffi::c_void, target: usize) -> TailLiteral {
    let source = unsafe { &mut *context.cast::<TailSource<'_>>() };
    // B calls once per entry using the original iterator's length.
    let ((bits, wide), rel32_fields) = unsafe { source.literals.next().unwrap_unchecked() };
    let literal = TailLiteral {
        bits,
        references: rel32_fields.as_ptr(),
        reference_count: rel32_fields.len(),
        wide: u64::from(wide),
    };
    source.references.push(super::TailReference { target, rel32_fields });
    literal
}

#[repr(C)]
struct CodeTail {
    end: usize,
    region_count: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::tail_capacity"]
    fn tail_capacity(probe: usize, literal_count: usize) -> usize;
    #[link_name = "bootstrap.x64_instruction::write_tail"]
    fn write_tail(image: *mut u8, end: usize, probe: usize, literal_count: usize,
        regions: *mut super::Region, context: *mut std::ffi::c_void,
        next_literal: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> TailLiteral) -> CodeTail;
    #[link_name = "bootstrap.x64_instruction::emit_map_range"]
    fn emit_map_range(input: MapRangeInput, host: MapRangeHost) -> u32;

}

#[repr(C)]
struct LeafOutput {
    image: *mut u8,
    unwind: *mut u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LeafRange {
    end: usize,
    unwind_bytes: usize,
    stack_bytes: u32,
}

#[repr(C)]
struct CollectiveLeafOutput {
    range: LeafOutput,
    references: *mut usize,
    regions: *mut super::Region,
}

#[repr(C)]
struct CollectiveLeaf {
    range: LeafRange,
    function_end: usize,
    literal_target: usize,
    reference_count: usize,
    region_count: usize,
}

fn leaf_buffers<T>(body_capacity: usize, emit: impl FnOnce(LeafOutput) -> (LeafRange, T)) -> (NativeCode, T) {
    let mut bytes = Vec::with_capacity(body_capacity + 16);
    let mut unwind = Vec::with_capacity(20);
    let (range, facts) = emit(LeafOutput { image: bytes.as_mut_ptr(), unwind: unwind.as_mut_ptr() });
    // The B encoder supplies the initialized lengths of the two original buffers.
    unsafe {
        bytes.set_len(range.end);
        unwind.set_len(range.unwind_bytes);
    }
    let code = NativeCode {
        bytes,
        relocations: Vec::new(),
        stack_bytes: range.stack_bytes,
        calls: Vec::new(),
        function_end: range.end,
        unwind,
        tail_regions: Some(Vec::new()),
        tail_references: Some(Vec::new()),
        sources: super::CodeSources::Emitted { operation_ranges: None, function_abi: None },
    };
    (code, facts)
}

fn emit_leaf(body_capacity: usize, emit: impl FnOnce(LeafOutput) -> LeafRange) -> NativeCode {
    leaf_buffers(body_capacity, |output| (emit(output), ())).0
}

fn emit_collective_leaf(body_capacity: usize, emit: impl FnOnce(CollectiveLeafOutput) -> CollectiveLeaf) -> NativeCode {
    let mut references = [std::mem::MaybeUninit::<usize>::uninit(); 4];
    let mut regions = [std::mem::MaybeUninit::<super::Region>::uninit(); 2];
    let (mut code, facts) = leaf_buffers(body_capacity + 7, |range| {
        let facts = emit(CollectiveLeafOutput {
            range,
            references: references.as_mut_ptr().cast(),
            regions: regions.as_mut_ptr().cast(),
        });
        (facts.range, facts)
    });
    code.function_end = facts.function_end;
    if facts.reference_count != 0 {
        // Only the reported prefixes of these output rows have been written.
        let fields = unsafe { std::slice::from_raw_parts(references.as_ptr().cast(), facts.reference_count) };
        let tail = unsafe { std::slice::from_raw_parts(regions.as_ptr().cast(), facts.region_count) };
        code.tail_regions = Some(tail.to_vec());
        code.tail_references = Some(vec![super::TailReference {
            target: facts.literal_target,
            rel32_fields: fields.to_vec(),
        }]);
    }
    code
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
    registers: &'a [Scalar],
    tiles: &'a [Tile],
    range_origin: OperationOrigin,
    workgroup: u32,
    buffer_count: usize,
    textures: &'a [TextureBinding],
    allocation: Allocation,
    frame: Frame,
}
impl Emitter<'_> {
    // Literal bytes belong to this code image, after all returning instructions
    // and any local helpers. Their references move with the same image.
    fn finish(mut self, stack_bytes: u32, include_operation_ranges: bool) -> NativeCode {
        let function_end = self.code.len();
        let unwind = unwind::encode(self.prolog, &self.unwind);
        let probe = self.probe.unwrap_or(0);
        let count = self.literals.len();
        let has_probe = usize::from(self.probe.is_some());
        let mut tail_regions = Vec::with_capacity(count + usize::from(count != 0) + has_probe);
        let mut tail_references = Vec::with_capacity(count + has_probe);
        if let Some(at) = self.probe {
            tail_references.push(super::TailReference { target: function_end, rel32_fields: vec![at] });
        }
        self.code.reserve(unsafe { tail_capacity(probe, count) });
        // The observation owner keeps the original reference storage live while B patches the image.
        unsafe {
            let mut source = TailSource { literals: self.literals.into_iter(), references: &mut tail_references };
            let tail = write_tail(
                self.code.as_mut_ptr(), function_end, probe, count, tail_regions.as_mut_ptr(),
                std::ptr::from_mut(&mut source).cast(), next_tail_literal,
            );
            self.code.set_len(tail.end);
            tail_regions.set_len(tail.region_count);
        }
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
    fn reserve_frame(&mut self, code_bytes: usize, operations: usize) -> FrameOutput {
        self.code.reserve(code_bytes);
        self.unwind.reserve(operations);
        FrameOutput {
            code: self.code.as_mut_ptr(), operations: self.unwind.as_mut_ptr(),
            saved: self.frame.saved, count: self.frame.saved_count,
        }
    }
    unsafe fn publish_frame(&mut self, entry: FrameEntry) {
        unsafe {
            self.code.set_len(entry.length);
            self.unwind.set_len(entry.operations);
        }
        self.probe = (entry.probe_field != 0).then_some(entry.probe_field);
        self.prolog = entry.prolog;
    }
    fn bytes(&mut self, b: &[u8]) {
        self.code.extend_from_slice(b);
    }
    fn abi_range(&mut self, start: usize, kind: super::AbiRangeKind) {
        self.record_abi_range(start..self.code.len(), kind);
    }
    fn record_abi_range(&mut self, range: std::ops::Range<usize>, kind: super::AbiRangeKind) {
        if let Some(abi) = &mut self.function_abi {
            abi.ranges.push(super::AbiRange { start: range.start, end: range.end, kind });
        }
    }
    fn parallel_abi_range(&mut self, range: std::ops::Range<usize>, part: super::ParallelRangePart) {
        self.record_abi_range(range, super::AbiRangeKind::ParallelRange { part, origin: self.range_origin.clone() });
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
    saved: usize,
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
    frame: RangeFrameHost,
    reserve: unsafe extern "C" fn(MapRangeSlot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(MapRangeSlot, usize),
    packet: unsafe extern "C" fn(MapRangeSlot, *const std::ffi::c_void, structure::ControlTargets),
    scalar: unsafe extern "C" fn(MapRangeSlot, Span, structure::ControlTargets) -> u32,
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
    stage: &'b Stage,
    phases: &'a [TilePhase<'b>],
    input: TileInputCursor<'a>,
    live_in: btree_set::Iter<'a, PrivateSlot>,
    saved: btree_set::Iter<'a, PrivateSlot>,
    result: Result<(), &'static str>,
}

unsafe extern "C" fn map_range_frame(slot: MapRangeSlot, code_bytes: usize, operations: usize) -> FrameOutput {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    range.emitter.reserve_frame(code_bytes, operations)
}

unsafe extern "C" fn map_range_publish_frame(slot: MapRangeSlot, entry: FrameEntry) {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    unsafe { range.emitter.publish_frame(entry) };
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

unsafe extern "C" fn map_range_packet(slot: MapRangeSlot, packet: *const std::ffi::c_void, targets: structure::ControlTargets) {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let (plan, allocation) = unsafe { &*packet.cast::<(packet::Plan<'_>, PacketAllocation)>() };
    range.emitter.emit_packet_plan(plan, allocation, u32::from(range.stage.selection.is_some()), targets);
}

unsafe extern "C" fn map_range_scalar(slot: MapRangeSlot, code: Span, targets: structure::ControlTargets) -> u32 {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    range.result = range.emitter.block(unsafe { code.as_slice() }, targets);
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
    let scalar = slot.scalar(range.emitter.registers, &range.stage.locals);
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
    let stage = &program.stages[index];
    let (code, registers) = reads::place(&stage.code, &stage.registers, &stage.writes);
    let tile_width = stage.tile_workgroup_width_for(&code);
    let tile_scope = tile_width.is_some();
    let workgroup = tile_width.unwrap_or(Stage::DEFAULT_WORKGROUP_WIDTH);
    let mut graph = allocation::Graph::with_live(
        &code, &registers, &stage.locals, Default::default(),
    );
    let phases = if tile_scope { TilePhase::from_code(&code) } else { Vec::new() };
    graph.tile_interference(&phases);
    let allocation = Allocation::new(&stage.locals, &mut graph, tile_scope);
    let packet = packet::Plan::new(program, stage, &code, &registers, tile_scope, &graph);
    drop(graph);
    let mut frame_layout = Frame::new(
        &allocation,
        packet.as_ref().map(|(_, allocation)| allocation),
    );
    let data_end = if tile_scope {
        frame_layout.layout_tile(&registers, &stage.locals, workgroup, &phases, layout)
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
        registers: &registers,
        tiles: &stage.tiles,
        range_origin: OperationOrigin {
            source_site: stage.operation_source.clone(),
            generated_origin: stage.generated_origin.clone(),
        },
        workgroup,
        buffer_count: program.buffers.len(),
        textures: &program.textures,
        allocation,
        frame: frame_layout,
    };
    let (binding, word, selected) = stage.selection.map_or((0, 0, 0), |selection| {
        (selection.binding, selection.word, 1)
    });
    let input = MapRangeInput {
        code: Span::slice(&code),
        phase_count: phases.len(),
        packet: packet.as_ref().map_or(std::ptr::null(), |packet| std::ptr::from_ref(packet).cast()),
        binding, word, selected, frame, slots: e.frame.end as u32,
        buffer_bytes: BUFFER_BYTES, stride_field: BUFFER_STRIDE,
        tile_scope: u32::from(tile_scope), workgroup, saved: e.frame.saved_count,
    };
    let mut range = MapRange {
        emitter: &mut e,
        stage,
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
            frame: RangeFrameHost { reserve: map_range_frame, publish: map_range_publish_frame },
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
