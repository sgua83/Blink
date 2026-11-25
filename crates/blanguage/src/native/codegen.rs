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
use structure::Flow;
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
struct FrameEncoding {
    length: u64,
    probe_field: u64,
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
    #[link_name = "bootstrap.x64_instruction::allocate_frame"]
    fn encode_stack_frame(frame: u32, output: *mut u8) -> FrameEncoding;
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
    fn allocate_frame(&mut self, frame: u32) {
        if frame == 0 {
            return;
        }
        // Optional mov/call (10 bytes) followed by sub rsp, imm32 (7 bytes).
        self.code.reserve(17);
        let start = self.code.len();
        let encoding = unsafe {
            let encoding = encode_stack_frame(frame, self.code.as_mut_ptr().add(start));
            self.code.set_len(start + encoding.length as usize);
            encoding
        };
        if encoding.probe_field != 0 {
            self.probe = Some(start + encoding.probe_field as usize);
        }
        self.prolog = self.code.len() as u8;
        self.unwind.push(UnwindOperation::allocate(self.prolog, frame));
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
    fn block(&mut self, code: &[Inst]) -> Result<(), &'static str> {
        for inst in code {
            let start = self.code.len();
            let first = self.operation_ranges.len();
            match &(inst).kind {
                InstKind::Call(call) => self.native_call(call),
                InstKind::Raise(error) => self.lower_control(Flow::Raise(*error), Self::block, Self::load)?,
                InstKind::Print { dst, value } => self.print_call(*dst, *value),
                InstKind::TextureSample { .. } | InstKind::Cleanup { .. } => return Err("原生操作尚未降低到CPU执行IR"),
                InstKind::TextureBindingAddress { .. } | InstKind::TextureSize { .. }
                | InstKind::TextureLevels { .. } | InstKind::TextureAddress { .. }
                | InstKind::TextureFormat { .. } | InstKind::TextureRead { .. }
                | InstKind::TextureFetch { .. } => self.texture_access(inst),
                InstKind::Scope(body) => self.lower_control(Flow::Scope(body), Self::block, Self::load)?,
                InstKind::Exit => self.lower_control(Flow::Exit, Self::block, Self::load)?,
                InstKind::Append(append) => self.append(inst, append.values.len()),
                InstKind::Atomic(..) => self.atomic(inst),
                // tile_phases 先完成全部有效 lane，再进入下一块内阶段。
                InstKind::TileBarrier => {}
                InstKind::TilePublish(publication) => self.publish_values(publication),
                InstKind::TileCollective(collective) => self.tile_collective(collective),
                InstKind::TileLength(dst) => self.tile_length(*dst),
                InstKind::TileLane(dst) => self.tile_lane(*dst),
                InstKind::Subgroup { .. } => return Err("CPU 后端不能执行 GPU subgroup 协作"),
                InstKind::RayQuery { .. } => return Err("CPU 后端不能执行硬件光线查询"),
                InstKind::ViewLoad { .. } => self.view_load(inst),
                InstKind::LoadIndexed { .. } | InstKind::StoreIndexed { .. }
                | InstKind::TileLoad { .. } | InstKind::PositionAddress { .. } => self.indexed_storage(inst),
                InstKind::FunctionAddress(dst, target) => {
                    match target {
                        CallTarget::Defined(name) => self.emit_symbol_target(NativeSymbol::Function(name.clone()), false),
                        CallTarget::External(name) => self.emit_symbol_target(NativeSymbol::External(name.clone()), false),
                        CallTarget::Indirect(reg) => self.load(*reg),
                    }
                    self.store(*dst);
                }
                InstKind::Constant(..) | InstKind::Index(..) | InstKind::Uniform(..)
                | InstKind::BufferStride(..) | InstKind::LoadLocal(..) | InstKind::StoreLocal(..)
                | InstKind::LoadLocalIndexed { .. } | InstKind::StoreLocalIndexed { .. }
                | InstKind::Load { .. } | InstKind::Store { .. } => self.scalar_storage(inst),
                InstKind::Unary { .. } | InstKind::Binary { .. } | InstKind::Cast { .. }
                | InstKind::Math { .. } | InstKind::Select { .. } | InstKind::IntegerDivide { .. } => self.scalar_value(inst),
                InstKind::If { condition, yes, no } => {
                    self.lower_control(Flow::If(*condition, yes, no), Self::block, Self::load)?;
                }
                InstKind::Loop { condition, test, body } => {
                    self.lower_control(Flow::Loop(condition, *test, body), Self::block, Self::load)?;
                }
                InstKind::Break => self.lower_control(Flow::Break, Self::block, Self::load)?,
                InstKind::Continue => self.lower_control(Flow::Continue, Self::block, Self::load)?,
            }
            self.operation_range(inst, start, first);
        }
        Ok(())
    }
}
/// 所有范围共用RangeEntry：buffers、uniforms、end、begin、output，返回执行状态。
/// 算法输出（包括计数）只写第五参数，map及无摘要续阶段不读取该指针。
#[repr(C)]
struct MapRangeInput {
    packet: *const std::ffi::c_void,
    binding: usize,
    word: u32,
    selected: u32,
    frame: u32,
    slots: u32,
    buffer_bytes: u32,
    stride_field: u32,
    tile_scope: u32,
    has_phases: u32,
}

#[repr(u32)]
enum MapRangePart {
    Enter, PacketBounds, PacketCollect, PacketAdvance, PacketBackEdge, PacketTail,
    ScalarBounds, ScalarSelection, ScalarAdvance, ScalarBackEdge, Exit,
}

#[repr(u32)]
enum MapRangeBody { Packet, Scalar, TileInput, TilePhases }

#[repr(C)]
struct MapRangeBuffer { image: *mut u8, end: usize }

#[repr(C)]
struct MapRangeEntry { end: usize, stack_bytes: u32 }

#[repr(C)]
struct MapRangeObservation {
    start: usize,
    end: usize,
    binding: usize,
    value: u32,
    part: MapRangePart,
}

type MapRangeSlot = *mut std::ffi::c_void;

#[repr(C)]
struct MapRangeHost {
    context: MapRangeSlot,
    enter: unsafe extern "C" fn(MapRangeSlot, u32) -> MapRangeEntry,
    leave: unsafe extern "C" fn(MapRangeSlot, u32) -> usize,
    reserve: unsafe extern "C" fn(MapRangeSlot, usize) -> MapRangeBuffer,
    commit: unsafe extern "C" fn(MapRangeSlot, usize),
    body: unsafe extern "C" fn(MapRangeSlot, MapRangeBody, *const std::ffi::c_void) -> u32,
    observe: unsafe extern "C" fn(MapRangeSlot, MapRangeObservation),
}

struct MapRange<'a, 'b> {
    emitter: &'a mut Emitter<'b>,
    phases: &'a [TilePhase<'b>],
    inputs: &'a crate::ir::dataflow::InputFragments,
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

unsafe extern "C" fn map_range_reserve(slot: MapRangeSlot, count: usize) -> MapRangeBuffer {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let code = &mut range.emitter.code;
    code.reserve(count);
    MapRangeBuffer { image: code.as_mut_ptr(), end: code.len() }
}

unsafe extern "C" fn map_range_commit(slot: MapRangeSlot, end: usize) {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    unsafe { range.emitter.code.set_len(end) };
}

unsafe extern "C" fn map_range_body(slot: MapRangeSlot, body: MapRangeBody, packet: *const std::ffi::c_void) -> u32 {
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    range.result = match body {
        MapRangeBody::Packet => {
            let (plan, allocation) = unsafe { &*packet.cast::<(packet::Plan<'_>, PacketAllocation)>() };
            range.emitter.emit_packet_plan(plan, allocation)
        }
        MapRangeBody::Scalar => {
            let stage = range.emitter.stage;
            range.emitter.block(&stage.code)
        },
        MapRangeBody::TileInput => {
            range.emitter.tile_input(range.inputs);
            Ok(())
        }
        MapRangeBody::TilePhases => range.emitter.tile_phases(range.phases, range.inputs),
    };
    u32::from(range.result.is_ok())
}

unsafe extern "C" fn map_range_observe(slot: MapRangeSlot, event: MapRangeObservation) {
    use super::ParallelRangePart as Part;
    let range = unsafe { &mut *slot.cast::<MapRange<'_, '_>>() };
    let part = match event.part {
        MapRangePart::Enter => Part::Enter { frame_bytes: event.value },
        MapRangePart::Exit => Part::Exit { frame_bytes: event.value },
        MapRangePart::PacketBounds => Part::PacketBounds,
        MapRangePart::PacketCollect => Part::PacketCollect { binding: event.binding, offset: event.value },
        MapRangePart::PacketAdvance => Part::PacketAdvance,
        MapRangePart::PacketBackEdge => Part::PacketBackEdge,
        MapRangePart::PacketTail => Part::PacketTail,
        MapRangePart::ScalarBounds => Part::ScalarBounds,
        MapRangePart::ScalarSelection => Part::ScalarSelection { binding: event.binding, offset: event.value },
        MapRangePart::ScalarAdvance => Part::ScalarAdvance,
        MapRangePart::ScalarBackEdge => Part::ScalarBackEdge,
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
    let (mut graph, phases) = allocation::Graph::new(stage);
    let allocation = Allocation::new(stage, &mut graph);
    let packet = packet::Plan::new(program, stage).map(|plan| {
        let allocation = PacketAllocation::new(stage, &mut graph, &plan.address_only);
        (plan, allocation)
    });
    drop(graph);
    let mut frame_layout = Frame::new(
        &allocation,
        packet.as_ref().map(|(_, allocation)| allocation),
    );
    let tile_scope = stage.has_tile_scope();
    let data_end = if tile_scope {
        frame_layout.layout_tile(stage, &phases, layout)
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
        packet: packet.as_ref().map_or(std::ptr::null(), |packet| std::ptr::from_ref(packet).cast()),
        binding, word, selected, frame, slots: e.frame.end as u32,
        buffer_bytes: BUFFER_BYTES, stride_field: BUFFER_STRIDE,
        tile_scope: u32::from(tile_scope), has_phases: u32::from(!phases.is_empty()),
    };
    let mut range = MapRange { emitter: &mut e, phases: &phases, inputs, result: Ok(()) };
    let stack_bytes = unsafe {
        emit_map_range(input, MapRangeHost {
            context: std::ptr::from_mut(&mut range).cast(),
            enter: map_range_enter, leave: map_range_leave,
            reserve: map_range_reserve, commit: map_range_commit,
            body: map_range_body, observe: map_range_observe,
        })
    };
    range.result?;
    Ok(e.finish(stack_bytes, true))
}
