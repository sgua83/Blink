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
pub(crate) use unwind::encode as unwind_info;
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

struct Emitter<'a> {
    code: Vec<u8>,
    unwind: Vec<(u8, Vec<u8>)>,
    prolog: u8,
    probe: Option<usize>,
    relocations: Vec<Relocation>,
    calls: Vec<Option<usize>>,
    operation_ranges: Option<Vec<super::OperationRange>>,
    literals: std::collections::BTreeMap<(Bits, bool), Vec<usize>>,
    stage: &'a Stage,
    program: &'a Program,
    breaks: Vec<Vec<usize>>,
    exits: Vec<Vec<usize>>,
    continues: Vec<usize>,
    allocation: Allocation,
    frame: Frame,
}
impl Emitter<'_> {
    // Literal bytes belong to this code image, after all returning instructions
    // and any local helpers. Their references move with the same image.
    fn finish(mut self, stack_bytes: u32, function_abi: Option<super::FunctionAbi>) -> NativeCode {
        let function_end = self.code.len();
        let mut tail_regions = Vec::new();
        let mut tail_references = Vec::new();
        let unwind = unwind::encode(self.prolog, std::mem::take(&mut self.unwind));
        if let Some(at) = self.probe {
            let start = self.code.len();
            self.patch(at, start);
            tail_references.push(super::TailReference { target: start, rel32_fields: vec![at] });
            self.bytes(&[0x4c, 0x8b, 0xd4, 0x49, 0x83, 0xc2, 0x08, 0x49, 0x89, 0xc3]);
            let begin = self.code.len();
            self.bytes(&[0x49, 0x81, 0xfb]);
            self.u32(4096);
            let tail = self.jump(Some(0x82));
            self.bytes(&[0x49, 0x81, 0xea]);
            self.u32(4096);
            self.bytes(&[0x41, 0x85, 0x02, 0x49, 0x81, 0xeb]);
            self.u32(4096);
            let back = self.jump(None);
            self.patch(back, begin);
            self.patch(tail, self.code.len());
            self.bytes(&[0x4d, 0x29, 0xda, 0x41, 0x85, 0x02, 0xc3]);
            tail_regions.push(super::Region { end: self.code.len(), kind: super::RegionKind::StackProbe });
        }
        for ((bits, wide), references) in std::mem::take(&mut self.literals) {
            let aligned = self.code.len().div_ceil(4) * 4;
            if aligned != self.code.len() {
                self.code.resize(aligned, 0);
                tail_regions.push(super::Region { end: aligned, kind: super::RegionKind::Alignment });
            }
            let target = self.code.len();
            if wide { self.bytes(&bits.to_le_bytes()); } else { self.u32(bits as u32); }
            tail_regions.push(super::Region { end: self.code.len(), kind: super::RegionKind::Literal });
            for &at in &references {
                self.patch(at, target);
            }
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
            sources: super::CodeSources::Emitted { operation_ranges: self.operation_ranges, function_abi },
        }
    }
    fn push_registers(&mut self, registers: &[u8]) {
        for &register in registers {
            if register >= 8 { self.bytes(&[0x41]); }
            self.bytes(&[0x50 + (register & 7)]);
            let at = self.code.len() as u8;
            self.unwind.push((at, vec![at, register << 4]));
        }
        self.prolog = self.code.len() as u8;
    }
    fn allocate_frame(&mut self, frame: u32) {
        if frame >= 4096 {
            self.bytes(&[0xb8]);
            self.u32(frame);
            self.bytes(&[0xe8]);
            self.probe = Some(self.code.len());
            self.u32(0);
        }
        if frame != 0 {
            self.bytes(&[0x48, 0x81, 0xec]);
            self.u32(frame);
            let at = self.code.len() as u8;
            self.unwind.push((at, unwind::allocation(at, frame)));
            self.prolog = at;
        }
    }
    fn enter(&mut self, frame: u32) {
        self.push_registers(&[12, 13, 14, 15, 3]);
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
        if frame > 0 {
            self.bytes(&[0x48, 0x81, 0xc4]);
            self.u32(frame);
        }
        self.bytes(&[0x5b, 0x41, 0x5f, 0x41, 0x5e, 0x41, 0x5d, 0x41, 0x5c, 0xc3]);
    }
    fn bytes(&mut self, b: &[u8]) {
        self.code.extend_from_slice(b);
    }
    fn u32(&mut self, n: u32) {
        self.bytes(&n.to_le_bytes());
    }
    fn jump(&mut self, condition: Option<u8>) -> usize {
        if let Some(c) = condition {
            self.bytes(&[0x0f, c]);
        } else {
            self.bytes(&[0xe9]);
        }
        let at = self.code.len();
        self.u32(0);
        at
    }
    fn patch(&mut self, at: usize, target: usize) {
        let relative = (target as i64 - (at + 4) as i64) as i32;
        self.code[at..at + 4].copy_from_slice(&relative.to_le_bytes());
    }
    fn address(&mut self, binding: usize, index: Reg) {
        self.bytes(&[0x4d, 0x8b, 0xbc, 0x24]);
        self.u32(binding as u32 * BUFFER_BYTES);
        self.load(index);
        self.offset_buffer_address(binding);
    }
    fn data_memory(&mut self, binding: usize, index: Reg, word: u32) -> Memory {
        self.bytes(&[0x4d, 0x8b, 0xbc, 0x24]);
        self.u32(binding as u32 * BUFFER_BYTES);
        self.load(index);
        self.buffer_memory(binding, 0, word)
    }
    fn scale_buffer_index(&mut self, table: u8, binding: usize, index: u8) {
        self.instruction(None, &[0x0f, 0xaf], true, index,
            operands::Operand::memory(table, binding as u32 * BUFFER_BYTES + BUFFER_STRIDE));
    }
    fn offset_buffer_address(&mut self, binding: usize) {
        self.scale_buffer_index(12, binding, 0);
        self.bytes(&[0x49, 0x01, 0xc7]);
    }
    fn buffer_memory(&mut self, binding: usize, index: u8, word: u32) -> Memory {
        self.scale_buffer_index(12, binding, index);
        Memory::indexed(15, index, 0, word)
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
                self.load(address[1]);
                self.bytes(&[0x41,0x89,0xc7,0x49,0xc1,0xe7,32]);
                self.load(address[0]);self.bytes(&[0x49,0x09,0xc7]);
                let memory=self.view_memory(15,self.location(*index),*stride,0);
                self.instruction(None,&[0x8d],true,15,memory.into());
            },
            IndexedSource::Pointer { address, index, stride, .. } => {
                self.load(address[1]);
                self.bytes(&[0x41, 0x89, 0xc7, 0x49, 0xc1, 0xe7, 32]);
                self.load(address[0]);
                self.bytes(&[0x49, 0x09, 0xc7]);
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
    fn comparison_result(&mut self, dst: Reg, operation: Comparison, s: Scalar) {
        let target = match self.location(dst) {
            allocation::Location::Gpr(r) => r,
            _ => 0,
        };
        if s.is_float() {
            let c = match operation {
                Comparison::Equal => 0x94,
                Comparison::NotEqual => 0x95,
                Comparison::Less => 0x92,
                Comparison::Greater => 0x97,
                Comparison::LessEqual => 0x96,
                Comparison::GreaterEqual => 0x93,
            };
            self.bytes(&[0x0f, c, 0xc0]);
            if operation == Comparison::NotEqual {
                self.bytes(&[0x0f, 0x9a, 0xc2, 0x08, 0xd0]);
            } else {
                self.bytes(&[0x0f, 0x9b, 0xc2, 0x20, 0xd0]);
            }
        } else {
            let signed = s.is_signed();
            let c = match operation {
                Comparison::Equal => 0x94,
                Comparison::NotEqual => 0x95,
                Comparison::Less => {
                    if signed {
                        0x9c
                    } else {
                        0x92
                    }
                }
                Comparison::Greater => {
                    if signed {
                        0x9f
                    } else {
                        0x97
                    }
                }
                Comparison::LessEqual => {
                    if signed {
                        0x9e
                    } else {
                        0x96
                    }
                }
                Comparison::GreaterEqual => {
                    if signed {
                        0x9d
                    } else {
                        0x93
                    }
                }
            };
            self.bytes(&[0x0f, c, 0xc0]);
        }
        self.instruction(
            None,
            &[0x0f, 0xb6],
            false,
            target,
            operands::Operand::Register(0),
        );
        self.copy(self.location(dst), allocation::Location::Gpr(target));
    }
    fn parallel_abi(&self, abi: &mut super::FunctionAbi, start: usize, part: super::ParallelRangePart) {
        abi.ranges.push(super::AbiRange { start, end: self.code.len(),
            kind: super::AbiRangeKind::ParallelRange { part, origin: OperationOrigin {
                source_site: self.stage.operation_source.clone(), generated_origin: self.stage.generated_origin.clone(),
            } } });
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
                    let at=self.jump(None);
                    self.exits[0].push(at);
                }
                InstKind::Print { dst, value } => {
                    let scalar = self.stage.registers[*value];
                    self.copy_scalar(allocation::Location::Gpr(1), self.location(*value), scalar);
                    if scalar.is_float() { self.bytes(&[0xba]); self.u32(scalar.bit_width()); }
                    let reference = self.native_code_address(NativeSymbol::Print(scalar));
                    self.invoke(Some(reference));
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
                    let at = self.jump(None);
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
                    self.load(address[1]);
                    self.bytes(&[0x41, 0x89, 0xc7, 0x49, 0xc1, 0xe7, 32]);
                    self.load(address[0]);
                    self.bytes(&[0x49, 0x09, 0xc7]);
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
                    self.instruction(None,&[0x8b],true,0,operands::Operand::memory(12,*binding as u32 * BUFFER_BYTES + BUFFER_STRIDE));
                    self.store(*dst);
                }
                InstKind::FunctionAddress(dst, target) => {
                    match target {
                        CallTarget::Defined(name) => { self.native_code_address(NativeSymbol::Function(name.clone())); }
                        CallTarget::External(name) => { self.native_code_address(NativeSymbol::External(name.clone())); }
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
                InstKind::Unary { dst, op, a } => {
                    match op {
                        UnaryOp::FloatNegate => {
                            self.float_sign(*dst, *a, true);
                        }
                        UnaryOp::SignedNegate | UnaryOp::UnsignedNegate => {
                            self.integer_unary(*dst, *a, 0xf7, 3, None)
                        }
                        UnaryOp::LogicalNot => {
                            // IR 布尔值已经规范为 0／1；其逻辑非就是翻转这一位。
                            self.integer_unary(*dst, *a, 0x83, 6, Some(1))
                        }
                        UnaryOp::BitNot => self.integer_unary(*dst, *a, 0xf7, 2, None),
                        UnaryOp::Bitcast => self.copy_scalar(self.location(*dst), self.location(*a), self.stage.registers[*dst]),
                    };
                }
                InstKind::Binary { dst, op, a, b } => match op {
                    BinaryOp::FAdd => self.float_binary(*dst, *a, *b, 0x58),
                    BinaryOp::FSub => self.float_binary(*dst, *a, *b, 0x5c),
                    BinaryOp::FMul => self.float_binary(*dst, *a, *b, 0x59),
                    BinaryOp::FDiv => self.float_binary(*dst, *a, *b, 0x5e),
                    BinaryOp::Compare(comparison) => self.compare(*dst, *a, *b, *comparison),
                    BinaryOp::IAdd => self.integer_binary(*dst, *a, *b, &[0x03], Some(0)),
                    BinaryOp::ISub => self.integer_binary(*dst, *a, *b, &[0x2b], Some(5)),
                    BinaryOp::IMul => self.integer_binary(*dst, *a, *b, &[0x0f, 0xaf], None),
                    BinaryOp::And => self.integer_binary(*dst, *a, *b, &[0x23], Some(4)),
                    BinaryOp::Or => self.integer_binary(*dst, *a, *b, &[0x0b], Some(1)),
                    BinaryOp::Xor => self.integer_binary(*dst, *a, *b, &[0x33], Some(6)),
                    BinaryOp::ShiftLeft => self.shift(*dst, *a, *b, 4),
                    BinaryOp::ShiftRightLogical => self.shift(*dst, *a, *b, 5),
                    BinaryOp::ShiftRightArithmetic => self.shift(*dst, *a, *b, 7),
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
                        MathOp::Sqrt => { self.float_sqrt(*dst, args[0]); None }
                        MathOp::Min => { self.float_binary(*dst, args[0], args[1], 0x5d); None }
                        MathOp::Max => { self.float_binary(*dst, args[0], args[1], 0x5f); None }
                        MathOp::Abs => { self.float_sign(*dst, args[0], false); None }
                        MathOp::Sin => Some("sin"), MathOp::Cos => Some("cos"), MathOp::Floor => Some("floor"),
                        MathOp::Exp => Some("exp"), MathOp::Exp2 => Some("exp2"), MathOp::Log => Some("log"),
                        MathOp::Log2 => Some("log2"), MathOp::Pow => Some("pow"),
                    };
                    if let Some(name) = name {
                        if args.len() == 2 { self.float(args[1], 1); }
                        self.float(args[0], 0);
                        let reference = self.native_code_address(NativeSymbol::External(crate::ast::ExternalName {
                            library: "ucrtbase.dll".into(),
                            symbol: if self.stage.registers[*dst] == Scalar::F32 { format!("{name}f") } else { name.into() },
                        }));
                        self.invoke(Some(reference));
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
                    self.bytes(&[0x85, 0xc0]);
                    let other = self.jump(Some(0x84));
                    self.block(yes)?;
                    let end = self.jump(None);
                    self.patch(other, self.code.len());
                    self.block(no)?;
                    self.patch(end, self.code.len());
                }
                InstKind::Loop {
                    condition,
                    test,
                    body,
                } => {
                    let begin = self.code.len();
                    self.block(condition)?;
                    self.load(*test);
                    self.bytes(&[0x85, 0xc0]);
                    let end = self.jump(Some(0x84));
                    self.breaks.push(Vec::new());
                    self.continues.push(begin);
                    self.block(body)?;
                    self.continues.pop();
                    let back = self.jump(None);
                    self.patch(back, begin);
                    let target = self.code.len();
                    self.patch(end, target);
                    for b in self.breaks.pop().unwrap() {
                        self.patch(b, target);
                    }
                }
                InstKind::Break => {
                    let at = self.jump(None);
                    self.breaks.last_mut().unwrap().push(at);
                }
                InstKind::Continue => {
                    let at = self.jump(None);
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
        literals: std::collections::BTreeMap::new(),
        stage,
        program,
        breaks: Vec::new(),
        exits: vec![Vec::new()],
        continues: Vec::new(),
        allocation,
        frame: frame_layout,
    };
    let mut abi = super::FunctionAbi { definition: None, instance: None, ranges: Vec::new() };
    let start = e.code.len();
    e.enter(frame);
    e.parallel_abi(&mut abi, start, super::ParallelRangePart::Enter { frame_bytes: frame });
    if let Some((plan, allocation)) = packet {
        e.packet_loop(&plan, &allocation, &mut abi);
    }
    let begin = e.code.len();
    e.bytes(&[0x44, 0x39, 0xf3]);
    let end = e.jump(Some(0x83));
    e.parallel_abi(&mut abi, begin, super::ParallelRangePart::ScalarBounds);
    let skip_selection = stage.selection.map(|selection| {
        let start = e.code.len();
        // R12 is the data table, EBX the original record index.
        e.bytes(&[0x49, 0x8b, 0x84, 0x24]);
        e.u32(selection.binding as u32 * BUFFER_BYTES);
        e.bytes(&[0x89, 0xd9]);
        e.scale_buffer_index(12, selection.binding, 1);
        let memory = Memory::indexed(0, 1, 0, selection.word);
        e.instruction(None, &[0x83], false, 7, memory.into());
        e.bytes(&[0]);
        let skip = e.jump(Some(0x84));
        e.parallel_abi(&mut abi, start, super::ParallelRangePart::ScalarSelection { binding: selection.binding, offset: selection.word });
        skip
    });
    if stage.has_tile_steps() {
        e.tile_phases(&mut abi)?;
    } else {
        if stage.has_tile_scope() {
            e.tile_input(&mut abi);
        }
        e.block(&stage.code)?;
        if let Some(skip) = skip_selection {
            e.patch(skip, e.code.len());
        }
        let start = e.code.len();
        e.bytes(&[0xff, 0xc3]);
        e.parallel_abi(&mut abi, start, super::ParallelRangePart::ScalarAdvance);
    }
    let start = e.code.len();
    let back = e.jump(None);
    e.parallel_abi(&mut abi, start, super::ParallelRangePart::ScalarBackEdge);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    let start = e.code.len();
    e.leave(frame);
    e.parallel_abi(&mut abi, start, super::ParallelRangePart::Exit { frame_bytes: frame });
    Ok(e.finish(frame + 40, Some(abi)))
}
