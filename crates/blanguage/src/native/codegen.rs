//! Windows x64 原生机器码后端。每个阶段编译为独立原生范围函数。
use crate::ir::*;
use super::{BufferAddress,NativeCode,NativeSymbol,Relocation,ContinuationCode,ProgramImage,RangeResult,SortCode,StageCode};
#[path="allocation.rs"]
mod allocation;
#[path="append.rs"]
mod append;
#[path="atomic.rs"]
mod atomic;
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
#[path="integer.rs"]
mod integer;
#[path="operands.rs"]
mod operands;
use operands::Memory;
#[path="packet.rs"]
mod packet;
#[path="program.rs"]
pub(crate) mod program;
#[path="reads.rs"]
mod reads;
pub use program::compile_program;
#[path="reduce.rs"]
mod reduce;
#[path="scan.rs"]
mod scan;
#[path="segmented_reduce.rs"]
mod segmented_reduce;
#[path="sort.rs"]
mod sort;
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
struct FrameEncoding {
    length: u64,
    probe_field: u64,
}

#[repr(C)]
struct TailRange {
    start: u64,
    end: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::allocate_frame"]
    fn encode_stack_frame(frame: u32, output: *mut u8) -> FrameEncoding;
    #[link_name = "bootstrap.x64_instruction::release_frame"]
    fn encode_frame_release(frame: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::stack_register"]
    fn encode_stack_register(register: u8, restore: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::stack_probe_size"]
    fn stack_probe_size() -> u64;
    #[link_name = "bootstrap.x64_instruction::append_stack_probe"]
    fn append_stack_probe(image: *mut u8, end: u64, call_field: u64) -> u64;
    #[link_name = "bootstrap.x64_instruction::append_literal"]
    fn append_literal(image: *mut u8, end: u64, bits: u64, width: u32, references: *const usize, count: u64) -> TailRange;
    #[link_name = "bootstrap.x64_instruction::patch_relative"]
    fn patch_relative(field: *mut u8, offset: u64, target: u64);
    #[link_name = "bootstrap.x64_instruction::encode_branch"]
    fn encode_branch(condition: u8, test_bits: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::buffer_load"]
    fn encode_buffer_load(offset: u32, target: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::buffer_index"]
    fn encode_buffer_index(stride_offset: u32, index: u8, base: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::buffer_advance"]
    fn encode_buffer_advance(stride_offset: u32, target: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::buffer_selection"]
    fn encode_buffer_selection(data_offset: u32, stride_offset: u32, word: u32, output: *mut u8) -> u64;
}

enum Branch {
    Always,
    Flags(u8),
    Zero32,
    Nonzero64,
}

struct Emitter<'a> {
    code: Vec<u8>,
    unwind: Vec<UnwindOperation>,
    prolog: u8,
    probe: Option<usize>,
    relocations: Vec<Relocation>,
    calls: Vec<Option<usize>>,
    operation_ranges: Option<Vec<super::OperationRange>>,
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
    fn finish(mut self, stack_bytes: u32) -> NativeCode {
        let function_end = self.code.len();
        let mut tail_regions = Vec::new();
        let mut tail_references = Vec::new();
        let unwind = unwind::encode(self.prolog, &self.unwind);
        if let Some(at) = self.probe {
            let start = self.code.len();
            self.code.reserve(unsafe { stack_probe_size() } as usize);
            unsafe {
                let end = append_stack_probe(self.code.as_mut_ptr(), start as u64, at as u64);
                self.code.set_len(end as usize);
            }
            tail_references.push(super::TailReference { target: start, rel32_fields: vec![at] });
            tail_regions.push(super::Region { end: self.code.len(), kind: super::RegionKind::StackProbe });
        }
        for ((bits, wide), references) in self.literals {
            let start = self.code.len();
            let width = if wide { 8 } else { 4 };
            self.code.reserve(width as usize + 3);
            let range = unsafe {
                let range = append_literal(self.code.as_mut_ptr(), start as u64, bits, width,
                    references.as_ptr(), references.len() as u64);
                self.code.set_len(range.end as usize);
                range
            };
            let target = range.start as usize;
            if target != start {
                tail_regions.push(super::Region { end: target, kind: super::RegionKind::Alignment });
            }
            tail_regions.push(super::Region { end: range.end as usize, kind: super::RegionKind::Literal });
            tail_references.push(super::TailReference { target, rel32_fields: references });
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
            sources: super::CodeSources::Emitted { operation_ranges: self.operation_ranges, function_abi: self.function_abi },
        }
    }
    const RANGE_SAVED_REGISTERS: &'static [u8] = &[12, 13, 14, 15, 3];

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
    fn release_frame(&mut self, frame: u32) {
        self.emit_bytes(7, |output| unsafe { encode_frame_release(frame, output) });
    }
    fn pop_register(&mut self, register: u8) {
        self.emit_bytes(2, |output| unsafe { encode_stack_register(register, 1, output) });
    }
    fn enter(&mut self, frame: u32) {
        self.push_registers(Self::RANGE_SAVED_REGISTERS);
        self.allocate_frame(frame);
        self.preserve_registers(false);
        self.bytes(&[
            0x49, 0x89, 0xcc, 0x49, 0x89, 0xd5, 0x45, 0x89, 0xc6, 0x44, 0x89, 0xcb,
        ]);
    }
    fn leave(&mut self, frame: u32) {
        self.bytes(&[0x31, 0xc0]);
        let exit=self.code.len();
        for at in std::mem::take(&mut self.exits[0]){self.patch(at,exit);}
        self.preserve_registers(true);
        if frame != 0 {
            self.release_frame(frame);
        }
        for &register in Self::RANGE_SAVED_REGISTERS.iter().rev() {
            self.pop_register(register);
        }
        self.bytes(&[0xc3]);
    }
    fn bytes(&mut self, b: &[u8]) {
        self.code.extend_from_slice(b);
    }
    fn u32(&mut self, n: u32) {
        self.bytes(&n.to_le_bytes());
    }
    fn jump(&mut self, branch: Branch) -> usize {
        let (condition, test_bits) = match branch {
            Branch::Always => (0, 0),
            Branch::Flags(condition) => (condition, 0),
            Branch::Zero32 => (0x84, 32),
            Branch::Nonzero64 => (0x85, 64),
        };
        self.emit_bytes(9, |output| unsafe { encode_branch(condition, test_bits, output) });
        self.code.len() - 4
    }
    fn patch(&mut self, at: usize, target: usize) {
        let field = &mut self.code[at..at + 4];
        unsafe { patch_relative(field.as_mut_ptr(), at as u64, target as u64); }
    }
    fn buffer_load(&mut self, offset: u32, target: u8) {
        self.emit_bytes(8, |output| unsafe { encode_buffer_load(offset, target, output) });
    }
    fn address(&mut self, binding: usize, index: Reg) {
        self.buffer_load(binding as u32 * BUFFER_BYTES, 15);
        self.load(index);
        self.offset_buffer_address(binding);
    }
    fn data_memory(&mut self, binding: usize, index: Reg, word: u32) -> Memory {
        self.buffer_load(binding as u32 * BUFFER_BYTES, 15);
        self.load(index);
        self.buffer_index(binding, 0, None);
        Memory::indexed(15, 0, 0, word)
    }
    fn buffer_index(&mut self, binding: usize, index: u8, base: Option<u8>) {
        let stride = binding as u32 * BUFFER_BYTES + BUFFER_STRIDE;
        self.emit_bytes(12, |output| unsafe {
            encode_buffer_index(stride, index, base.unwrap_or(u8::MAX), output)
        });
    }
    fn offset_buffer_address(&mut self, binding: usize) {
        self.buffer_index(binding, 0, Some(15));
    }
    fn buffer_advance(&mut self, binding: usize, target: u8) {
        let stride = binding as u32 * BUFFER_BYTES + BUFFER_STRIDE;
        self.emit_bytes(8, |output| unsafe { encode_buffer_advance(stride, target, output) });
    }
    fn buffer_selection(&mut self, binding: usize, word: u32) {
        let data = binding as u32 * BUFFER_BYTES;
        self.emit_bytes(32, |output| unsafe {
            encode_buffer_selection(data, data + BUFFER_STRIDE, word, output)
        });
    }
    fn private_memory(
        &mut self,
        base: usize,
        index: Reg,
        word: u32,
    ) -> Result<Memory, &'static str> {
        let allocation::Location::Stack(offset) =
            self.allocation.location(PrivateSlot::Local(base))
        else {
            return Err("索引私有存储没有原生栈地址");
        };
        Ok(self.value_memory(4, self.location(index), 1, offset + word))
    }
    fn indexed_address(&mut self, source: &IndexedSource, offset: Reg, word: u32) {
        let stride = self.indexed_base(source);
        let index = self.indexed_offset(offset, word);
        self.load_location(index);
        self.offset_address(stride);
    }
    fn indexed_memory(&mut self, source: &IndexedSource, offset: Reg, word: u32) -> Memory {
        let stride = self.indexed_base(source);
        let index = self.indexed_offset(offset, word);
        self.value_memory(15, index, stride, 0)
    }
    fn indexed_base(&mut self, source: &IndexedSource) -> u32 {
        match source {
            IndexedSource::Data { binding, index } => self.address(*binding, *index),
            IndexedSource::View { address, index, stride }=>{
                self.address_pair(*address);
                let memory=self.view_memory(15,self.location(*index),*stride,0);
                self.instruction(None,&[0x8d],true,15,memory.into());
            },
            IndexedSource::Pointer { address, index, stride, .. } => {
                self.address_pair(*address);
                self.load(*index);
                self.offset_address(*stride);
            }
            IndexedSource::Tile { tile, index } => {
                self.load(*index);
                self.bytes(&[0x4c, 0x8d, 0xbc, 0x84]);
                self.u32(
                    self.shared_stream(self.stage.shared_word(*tile, 0)),
                );
                return self.stage.workgroup_width();
            }
        }
        1
    }
    fn indexed_offset(&mut self, offset: Reg, word: u32) -> allocation::Location {
        if word != 0 {
            self.load(offset);
            self.bytes(&[0x05]);
            self.u32(word);
            allocation::Location::Gpr(0)
        } else {
            self.location(offset)
        }
    }
    fn abi_range(&mut self, start: usize, kind: super::AbiRangeKind) {
        if let Some(abi) = &mut self.function_abi {
            abi.ranges.push(super::AbiRange { start, end: self.code.len(), kind });
        }
    }
    fn parallel_abi(&mut self, start: usize, part: super::ParallelRangePart) {
        self.abi_range(start, super::AbiRangeKind::ParallelRange { part, origin: OperationOrigin {
            source_site: self.stage.operation_source.clone(), generated_origin: self.stage.generated_origin.clone(),
        } });
    }
    fn operation_range(&mut self, inst: &Inst, start: usize, first: Option<usize>) {
        if let (Some(first), Some(ranges)) = (first, &mut self.operation_ranges) {
            ranges.push(super::OperationRange { start, end: self.code.len(),
                includes_nested_operations: ranges.len() != first, origin: OperationOrigin::from_instruction(inst) });
        }
    }
    fn block(&mut self, code: &[Inst]) -> Result<(), &'static str> {
        for inst in code {
            let start = self.code.len();
            let first = self.operation_ranges.as_ref().map(Vec::len);
            match &(inst).kind {
                InstKind::Call(call) => self.native_call(call)?,
                InstKind::Raise(error) => {
                    self.load(*error);
                    let at=self.jump(Branch::Always);
                    self.exits[0].push(at);
                }
                InstKind::Print { dst, value } => {
                    let scalar = self.stage.registers[*value];
                    self.copy_scalar(allocation::Location::Gpr(1), self.location(*value), scalar);
                    if scalar.is_float() {
                        self.copy_scalar(allocation::Location::Gpr(2), allocation::Location::Immediate(u64::from(scalar.bit_width())), Scalar::U32);
                    }
                    self.emit_symbol_target(NativeSymbol::Print(scalar), true);
                    self.store(*dst);
                }
                InstKind::TextureBindingAddress { dst, texture } => {
                    self.texture_binding_address(*texture);
                    self.bytes(&[0x4c, 0x89, 0xf8]);
                    self.store(*dst);
                }
                InstKind::TextureSample { .. } | InstKind::Cleanup { .. } => return Err("原生操作尚未降低到CPU执行IR"),
                InstKind::TextureSize {
                    result,
                    texture,
                    level,
                } => {
                    self.texture_level(*texture, *level);
                    for (axis, dst) in result.iter().enumerate() {
                        self.read_scalar(*dst, 15, axis as u32 * 4);
                    }
                }
                InstKind::TextureLevels { dst, texture } => {
                    self.texture_pointer(*texture);
                    self.read_scalar(*dst, 15, 0);
                }
                InstKind::TextureAddress { dst, texture } => {
                    self.texture_pointer(*texture);
                    self.read_scalar(*dst, 15, 4);
                }
                InstKind::TextureFormat { dst, texture } => {
                    self.texture_pointer(*texture);
                    self.read_scalar(*dst, 15, 16);
                }
                InstKind::TextureRead {
                    dst,
                    texture,
                    level,
                    coordinate,
                    byte,
                } => {
                    self.texture_texel(*texture, *level, coordinate, if *byte { 0 } else { 2 });
                    if *byte {
                        self.bytes(&[0x41, 0x0f, 0xb6, 0x07]);
                        self.store(*dst);
                    } else {
                        self.read_scalar(*dst, 15, 0);
                    }
                }
                InstKind::TextureFetch {
                    result,
                    texture,
                    level,
                    coordinate,
                } => {
                    self.texture_texel(*texture, *level, coordinate, 4);
                    for (i, dst) in result.iter().enumerate() {
                        self.read_scalar(*dst, 15, i as u32 * 4);
                    }
                }
                InstKind::Scope(body) => {
                    self.exits.push(Vec::new());
                    self.block(body)?;
                    let target = self.code.len();
                    for at in self.exits.pop().unwrap() {
                        self.patch(at, target);
                    }
                }
                InstKind::Exit => {
                    let at = self.jump(Branch::Always);
                    self.exits.last_mut().unwrap().push(at);
                }
                InstKind::Append(append) => self.append(append),
                InstKind::Atomic(atomic) => self.atomic(atomic),
                // tile_phases 先完成全部有效 lane，再进入下一块内阶段。
                InstKind::TileBarrier => {}
                InstKind::TilePublish(publication) => self.publish_values(publication),
                InstKind::TileCollective(collective) => self.tile_collective(collective),
                InstKind::TileLength(dst) => self.tile_length(*dst),
                InstKind::TileLane(dst) => self.tile_lane(*dst),
                InstKind::TileLoad {
                    dst,
                    tile,
                    index,
                    word,
                } => self.tile_load(*dst, *tile, *index, *word, None),
                InstKind::Subgroup { .. } => return Err("CPU 后端不能执行 GPU subgroup 协作"),
                InstKind::RayQuery { .. } => return Err("CPU 后端不能执行硬件光线查询"),
                InstKind::ViewLoad {
                    dst,
                    address,
                    index,
                    stride,
                    word,
                } => {
                    self.address_pair(*address);
                    let memory = self.view_memory(15, self.location(*index), *stride, *word);
                    self.read_scalar_address(*dst, memory);
                }
                InstKind::LoadIndexed {
                    dst,
                    source,
                    offset,
                    word,
                } => {
                    if let IndexedSource::Tile { tile, index } = source {
                        self.tile_load(*dst, *tile, *index, *word, Some(*offset));
                    } else {
                        let memory = self.indexed_memory(source, *offset, *word);
                        self.read_scalar_address(*dst, memory);
                    }
                }
                InstKind::StoreIndexed {
                    source,
                    offset,
                    word,
                    value,
                } => {
                    let memory = self.indexed_memory(source, *offset, *word);
                    self.write_scalar_address(*value, memory);
                }
                InstKind::PositionAddress { dst, address } => { self.reference_address(address)?; self.store(*dst); }
                InstKind::BufferStride(dst,binding) => {
                    self.buffer_load(*binding as u32 * BUFFER_BYTES + BUFFER_STRIDE, 0);
                    self.store(*dst);
                }
                InstKind::FunctionAddress(dst, target) => {
                    match target {
                        CallTarget::Defined(name) => self.emit_symbol_target(NativeSymbol::Function(name.clone()), false),
                        CallTarget::External(name) => self.emit_symbol_target(NativeSymbol::External(name.clone()), false),
                        CallTarget::Indirect(reg) => self.load(*reg),
                    }
                    self.store(*dst);
                }
                InstKind::Constant(dst, _, bits) => self.constant(*dst, *bits),
                InstKind::Index(dst) => {
                    self.copy(self.location(*dst), allocation::Location::Gpr(3));
                }
                InstKind::Uniform(dst, off) => {
                    self.read_scalar(*dst, 13, *off);
                }
                InstKind::LoadLocal(dst, slot) => {
                    let source = self.allocation.location(PrivateSlot::Local(*slot));
                    if self.stage.registers[*dst] == Scalar::Bool
                        && let allocation::Location::Stack(offset) = source
                    {
                        self.read_scalar(*dst, 4, offset);
                    } else {
                        self.copy_scalar(self.location(*dst), source, self.stage.registers[*dst]);
                    }
                }
                InstKind::StoreLocal(slot, src) => {
                    self.copy_scalar(
                        self.allocation.location(PrivateSlot::Local(*slot)),
                        self.location(*src),
                        self.stage.registers[*src],
                    );
                }
                InstKind::LoadLocalIndexed {
                    dst,
                    base,
                    index,
                    word,
                    ..
                } => {
                    let memory = self.private_memory(*base, *index, *word)?;
                    self.read_scalar_address(*dst, memory);
                }
                InstKind::StoreLocalIndexed {
                    base,
                    index,
                    word,
                    value,
                    ..
                } => {
                    let memory = self.private_memory(*base, *index, *word)?;
                    self.write_scalar_address(*value, memory);
                }
                InstKind::Load {
                    dst,
                    binding,
                    index,
                    word,
                } => {
                    let memory = self.data_memory(*binding, *index, *word);
                    self.read_scalar_address(*dst, memory);
                }
                InstKind::Store {
                    binding,
                    index,
                    word,
                    value,
                } => {
                    let memory = self.data_memory(*binding, *index, *word);
                    self.write_scalar_address(*value, memory);
                }
                InstKind::Unary { dst, op, a } => self.unary(*dst, *a, *op),
                InstKind::Binary { dst, op, a, b } => match op {
                    BinaryOp::Compare(comparison) => self.compare(*dst, *a, *b, *comparison),
                    operation => self.binary(*dst, *a, *b, *operation),
                },
                InstKind::IntegerDivide {
                    result,
                    src,
                    divisor,
                } => match divisor {
                    IntegerDivisor::Constant(divisor) => {
                        self.integer_divide(*result, *src, *divisor)
                    }
                    IntegerDivisor::Value(divisor) => {
                        self.integer_divide_value(*result, *src, *divisor)
                    }
                },
                InstKind::Cast { dst, src } => self.cast(*dst, *src),
                InstKind::Math { dst, op, args } => {
                    let name = match op {
                        MathOp::Sqrt => { self.float_math(*dst, args[0], true); None }
                        MathOp::Min => { self.float_binary(*dst, args[0], args[1], 0x5d); None }
                        MathOp::Max => { self.float_binary(*dst, args[0], args[1], 0x5f); None }
                        MathOp::Abs => { self.float_math(*dst, args[0], false); None }
                        MathOp::Sin => Some("sin"), MathOp::Cos => Some("cos"), MathOp::Floor => Some("floor"),
                        MathOp::Exp => Some("exp"), MathOp::Exp2 => Some("exp2"), MathOp::Log => Some("log"),
                        MathOp::Log2 => Some("log2"), MathOp::Pow => Some("pow"),
                    };
                    if let Some(name) = name {
                        if args.len() == 2 { self.float(args[1], 1); }
                        self.float(args[0], 0);
                        self.emit_symbol_target(NativeSymbol::External(crate::ast::ExternalName {
                            library: "ucrtbase.dll".into(),
                            symbol: if self.stage.registers[*dst] == Scalar::F32 { format!("{name}f") } else { name.into() },
                        }), true);
                        self.store_float(*dst);
                    }
                },
                InstKind::Select {
                    dst,
                    condition,
                    a,
                    b,
                } => {
                    self.select(*dst, *condition, *a, *b);
                }
                InstKind::If { condition, yes, no } => {
                    self.load(*condition);
                    let other = self.jump(Branch::Zero32);
                    self.block(yes)?;
                    if no.is_empty() {
                        self.patch(other, self.code.len());
                    } else {
                        let end = self.jump(Branch::Always);
                        self.patch(other, self.code.len());
                        self.block(no)?;
                        self.patch(end, self.code.len());
                    }
                }
                InstKind::Loop {
                    condition,
                    test,
                    body,
                } => {
                    let begin = self.code.len();
                    self.block(condition)?;
                    self.load(*test);
                    let end = self.jump(Branch::Zero32);
                    self.breaks.push(Vec::new());
                    self.continues.push(begin);
                    self.block(body)?;
                    self.continues.pop();
                    let back = self.jump(Branch::Always);
                    self.patch(back, begin);
                    let target = self.code.len();
                    self.patch(end, target);
                    for b in self.breaks.pop().unwrap() {
                        self.patch(b, target);
                    }
                }
                InstKind::Break => {
                    let at = self.jump(Branch::Always);
                    self.breaks.last_mut().unwrap().push(at);
                }
                InstKind::Continue => {
                    let at = self.jump(Branch::Always);
                    self.patch(at, *self.continues.last().unwrap());
                }
            }
            self.operation_range(inst, start, first);
        }
        Ok(())
    }
}
/// 所有范围共用RangeEntry：buffers、uniforms、end、begin、output，返回执行状态。
/// 算法输出（包括计数）只写第五参数，map及无摘要续阶段不读取该指针。
pub fn emit(program: &Program, index: usize) -> Result<NativeCode, &'static str> {
    if let Operation::CustomSegmentedReduce(r) = &program.stages[index].operation {
        return custom_reduce::emit(
            program,
            index,
            &r.values,
            custom_reduce::Pass::SegmentReduce,
            Some(r.head_word),
        );
    }
    if let Operation::CustomReduce(r) = &program.stages[index].operation {
        return custom_reduce::emit(program, index, r, custom_reduce::Pass::Reduce, None);
    }
    if let Operation::CustomScan(r) = &program.stages[index].operation {
        return custom_reduce::emit(
            program,
            index,
            &r.values,
            custom_reduce::Pass::Scan,
            r.head_word,
        );
    }
    if let Operation::SegmentedReduce(r) = &program.stages[index].operation {
        return Ok(segmented_reduce::emit(program, r, false));
    }
    if let Operation::StableSort(s) = &program.stages[index].operation {
        return Ok(sort::initialize(program, s, s.keys.len() - 1, true));
    }
    if let Operation::Compact(c) = &program.stages[index].operation {
        return Ok(compact::emit_count(program, c));
    }
    if let Operation::ScanExclusive(r) = &program.stages[index].operation {
        return Ok(scan::emit_scan(program, r, false));
    }
    if let Operation::Reduce(r) = &program.stages[index].operation {
        return Ok(reduce::emit(program, r));
    }
    let mut lowered = program.stages[index].clone();
    texture::lower(&mut lowered);
    reads::place(&mut lowered.code, &lowered.writes);
    let stage = &lowered;
    let mut graph = allocation::Graph::new(stage);
    let allocation = Allocation::new(stage, &graph);
    let packet = packet::Plan::new(program, stage).map(|plan| {
        let allocation = PacketAllocation::new(stage, &mut graph, &plan.address_only);
        (plan, allocation)
    });
    drop(graph);
    let mut frame_layout = Frame::new(
        &allocation,
        packet.as_ref().map(|(_, allocation)| allocation),
    );
    let mut tile_bytes = 0;
    if stage.has_tile_scope() {
        let shared = stage.tile_layout(false);
        let stride = u64::from(stage.workgroup_width()) * 4;
        tile_bytes = 4 + shared.values().map(|(_, slot, _)| (*slot as u64 + 1) * stride).max().unwrap_or(0);
        let base = frame_layout.end + 4 + if stage.has_tile_steps() { 4 } else { 0 };
        // 本次栈帧持有共享流的实际字节位置；发射地址只消费这一份布局。
        frame_layout.tile_shared = shared.into_iter()
            .map(|(word, (_, slot, _))| (word, (base + slot as u64 * stride) as u32)).collect();
        if stage.has_tile_steps() {
            tile_bytes += 4;
            // 本次 CPU 栈帧拥有私有流的唯一字节布局；保存／恢复不再重建阶段活跃图。
            for slot in stage.tile_private_slots() {
                frame_layout.tile_private.insert(slot, (frame_layout.end + tile_bytes) as u32);
                tile_bytes += u64::from(stage.private_scalar(slot).size().div_ceil(4) * 4)
                    * u64::from(stage.workgroup_width());
            }
        }
    }
    let bytes = frame_layout.end
        + if packet.is_some() && stage.selection.is_some() { 16 } else { 0 }
        + tile_bytes;
    let frame = i32::try_from(bytes.div_ceil(16) * 16)
        .map_err(|_| "原生 tile 栈帧超过 x64 范围函数 ABI")? as u32;
    let mut e = Emitter {
        unwind: Vec::new(),
        prolog: 0,
        probe: None,
        code: Vec::new(),
        relocations: Vec::new(),
        calls: Vec::new(),
        operation_ranges: Some(Vec::new()),
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
    let start = e.code.len();
    e.enter(frame);
    e.parallel_abi(start, super::ParallelRangePart::Enter { frame_bytes: frame });
    if let Some((plan, allocation)) = packet {
        e.packet_loop(&plan, &allocation);
    }
    let begin = e.code.len();
    e.bytes(&[0x44, 0x39, 0xf3]);
    let end = e.jump(Branch::Flags(0x83));
    e.parallel_abi(begin, super::ParallelRangePart::ScalarBounds);
    let skip_selection = stage.selection.map(|selection| {
        let start = e.code.len();
        e.buffer_selection(selection.binding, selection.word);
        let skip = e.jump(Branch::Flags(0x84));
        e.parallel_abi(start, super::ParallelRangePart::ScalarSelection { binding: selection.binding, offset: selection.word });
        skip
    });
    if stage.has_tile_steps() {
        e.tile_phases()?;
    } else {
        if stage.has_tile_scope() {
            e.tile_input();
        }
        e.block(&stage.code)?;
        if let Some(skip) = skip_selection {
            e.patch(skip, e.code.len());
        }
        let start = e.code.len();
        e.bytes(&[0xff, 0xc3]);
        e.parallel_abi(start, super::ParallelRangePart::ScalarAdvance);
    }
    let start = e.code.len();
    let back = e.jump(Branch::Always);
    e.parallel_abi(start, super::ParallelRangePart::ScalarBackEdge);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    let start = e.code.len();
    e.leave(frame);
    e.parallel_abi(start, super::ParallelRangePart::Exit { frame_bytes: frame });
    Ok(e.finish(frame + 40))
}
