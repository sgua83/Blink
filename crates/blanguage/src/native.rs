//! Windows x64 原生机器码后端。每个阶段编译为独立原生范围函数。
use crate::ir::*;
mod allocation;
mod append;
mod atomic;
use allocation::{Allocation, Frame, PacketAllocation};
mod compact;
mod custom_reduce;
mod integer;
mod operands;
mod packet;
mod reduce;
mod scan;
mod segmented_reduce;
mod sort;
mod texture;
mod tile;
pub use compact::emit_compact_scatter;
pub use custom_reduce::{
    emit_custom_reduce_tail, emit_custom_scan_add, emit_custom_scan_prefix,
    emit_custom_segmented_finish, emit_custom_segmented_prefix,
};
pub use scan::emit_scan_add;
pub use segmented_reduce::emit_segmented_reduce_finish;
pub use sort::{emit_sort_gather, emit_sort_histogram, emit_sort_scatter};

#[derive(Clone, Debug)]
pub struct Relocation {
    pub offset: usize,
    pub intrinsic: NativeIntrinsic,
}
#[derive(Clone, Copy, Debug)]
pub enum NativeIntrinsic {
    Sin,
    Cos,
    Floor,
    Exp,
    Exp2,
    Log,
    Log2,
    Pow,
}
#[derive(Clone, Debug)]
pub struct NativeCode {
    pub bytes: Vec<u8>,
    pub relocations: Vec<Relocation>,
    pub stack_bytes: u32,
}
struct Emitter<'a> {
    code: Vec<u8>,
    relocations: Vec<Relocation>,
    stage: &'a Stage,
    program: &'a Program,
    breaks: Vec<Vec<usize>>,
    exits: Vec<Vec<usize>>,
    continues: Vec<usize>,
    allocation: Allocation,
    frame: Frame,
}
impl Emitter<'_> {
    fn enter(&mut self, frame: u32) {
        self.bytes(&[0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x53]);
        let mut remaining = frame;
        while remaining >= 4096 {
            self.bytes(&[0x48, 0x81, 0xec]);
            self.u32(4096);
            self.bytes(&[0x85, 0x04, 0x24]);
            remaining -= 4096;
        }
        if remaining > 0 {
            self.bytes(&[0x48, 0x81, 0xec]);
            self.u32(remaining);
        }
        self.preserve_registers(false);
        self.bytes(&[
            0x49, 0x89, 0xcc, 0x49, 0x89, 0xd5, 0x45, 0x89, 0xc6, 0x44, 0x89, 0xcb,
        ]);
    }
    fn leave(&mut self, frame: u32) {
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
        self.u32(binding as u32 * 8);
        self.load(index);
        self.bytes(&[0x48, 0x69, 0xc0]);
        self.u32(self.program.buffers[binding].stride);
        self.bytes(&[0x49, 0x01, 0xc7]);
    }
    fn private_address(&mut self, base: usize, index: Reg, word: u32) -> Result<(), &'static str> {
        let allocation::Location::Stack(offset) =
            self.allocation.location(PrivateSlot::Local(base))
        else {
            return Err("索引私有存储没有原生栈地址");
        };
        self.load(index);
        self.bytes(&[0x4c, 0x8d, 0xbc, 0x84]);
        self.u32(offset + word * 4);
        Ok(())
    }
    fn indexed_address(&mut self, source: &IndexedSource, offset: Reg, word: u32) {
        match source {
            IndexedSource::Data { binding, index } => self.address(*binding, *index),
            IndexedSource::View {
                address,
                index,
                stride,
            } => {
                self.load(address[1]);
                self.bytes(&[0x41, 0x89, 0xc7, 0x49, 0xc1, 0xe7, 32]);
                self.load(address[0]);
                self.bytes(&[0x49, 0x09, 0xc7]);
                self.load(*index);
                self.bytes(&[0x48, 0x69, 0xc0]);
                self.u32(*stride);
                self.bytes(&[0x49, 0x01, 0xc7]);
            }
            IndexedSource::Tile { tile, index } => {
                self.load(*index);
                self.bytes(&[0x4c, 0x8d, 0xbc, 0x84]);
                self.u32(
                    self.tile_storage_base()
                        + self.stage.word_offset(self.stage.shared_word(*tile, 0)) as u32,
                );
                self.load(offset);
                if word != 0 {
                    self.bytes(&[0x05]);
                    self.u32(word);
                }
                self.bytes(&[0x48, 0x69, 0xc0]);
                self.u32(self.stage.workgroup_width() * 4);
                self.bytes(&[0x49, 0x01, 0xc7]);
                return;
            }
        }
        self.load(offset);
        if word != 0 {
            self.bytes(&[0x05]);
            self.u32(word);
        }
        self.bytes(&[0x4d, 0x8d, 0x3c, 0x87]);
    }
    fn comparison_result(&mut self, operation: Comparison, s: Scalar) {
        if s == Scalar::F32 {
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
            self.bytes(&[0x0f, 0xb6, 0xc0]);
        } else {
            let signed = s == Scalar::I32;
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
            self.bytes(&[0x0f, c, 0xc0, 0x0f, 0xb6, 0xc0]);
        }
    }
    fn math_call(&mut self, intrinsic: NativeIntrinsic, args: &[Reg]) {
        if args.len() == 2 {
            self.float(args[1], 1);
        }
        self.bytes(&[0x48, 0xb8]);
        let offset = self.code.len();
        self.bytes(&[0; 8]);
        self.relocations.push(Relocation { offset, intrinsic });
        self.bytes(&[0xff, 0xd0]);
    }
    fn math_result(&mut self, dst: Reg, intrinsic: NativeIntrinsic, args: &[Reg]) {
        self.float(args[0], 0);
        self.math_call(intrinsic, args);
        self.store_float(dst);
    }
    fn block(&mut self, code: &[Inst]) -> Result<(), &'static str> {
        for inst in code {
            match inst {
                Inst::TextureSample { .. } => return Err("原生采样尚未降低到纹理读取 IR"),
                Inst::TextureSize {
                    result,
                    texture,
                    level,
                } => {
                    self.texture_level(*texture, *level);
                    self.read_scalar(result[0], 15, 0);
                    self.read_scalar(result[1], 15, 4);
                }
                Inst::TextureLevels { dst, texture } => {
                    self.texture_pointer(*texture);
                    self.read_scalar(*dst, 15, 0);
                }
                Inst::TextureFetch {
                    result,
                    texture,
                    level,
                    coordinate,
                } => {
                    self.texture_level(*texture, *level);
                    self.load(coordinate[1]);
                    self.bytes(&[0x41, 0x8b, 0x0f, 0x48, 0x0f, 0xaf, 0xc1, 0x48, 0x89, 0xc2]);
                    self.load(coordinate[0]);
                    self.bytes(&[0x48, 0x01, 0xd0, 0x48, 0xc1, 0xe0, 4]);
                    self.bytes(&[0x4d, 0x8b, 0x7f, 8, 0x49, 0x01, 0xc7]);
                    for (i, dst) in result.iter().enumerate() {
                        self.read_scalar(*dst, 15, i as u32 * 4);
                    }
                }
                Inst::Scope(body) => {
                    self.exits.push(Vec::new());
                    self.block(body)?;
                    let target = self.code.len();
                    for at in self.exits.pop().unwrap() {
                        self.patch(at, target);
                    }
                }
                Inst::Exit => {
                    let at = self.jump(None);
                    self.exits.last_mut().unwrap().push(at);
                }
                Inst::Append(append) => self.append(append),
                Inst::Atomic(atomic) => self.atomic(atomic),
                // tile_phases 先完成全部有效 lane，再进入下一块内阶段。
                Inst::TileBarrier => {}
                Inst::TilePublish(publication) => self.publish_values(publication),
                Inst::TileCollective(collective) => self.tile_collective(collective),
                Inst::TileLength(dst) => self.tile_length(*dst),
                Inst::TileLane(dst) => self.tile_lane(*dst),
                Inst::TileLoad {
                    dst,
                    tile,
                    index,
                    word,
                } => self.tile_load(*dst, *tile, *index, *word),
                Inst::Subgroup { .. } => return Err("CPU 后端不能执行 GPU subgroup 协作"),
                Inst::RayQuery { .. } => return Err("CPU 后端不能执行硬件光线查询"),
                Inst::ViewLoad {
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
                    self.load(*index);
                    self.bytes(&[0x48, 0x69, 0xc0]);
                    self.u32(*stride);
                    self.bytes(&[0x49, 0x01, 0xc7]);
                    self.read_scalar(*dst, 15, *word * 4);
                }
                Inst::LoadIndexed {
                    dst,
                    source,
                    offset,
                    word,
                } => {
                    self.indexed_address(source, *offset, *word);
                    self.read_scalar(*dst, 15, 0);
                }
                Inst::StoreIndexed {
                    source,
                    offset,
                    word,
                    value,
                } => {
                    self.indexed_address(source, *offset, *word);
                    self.write_memory(self.location(*value), 15, 0);
                }
                Inst::Constant(dst, _, bits) => self.constant(*dst, *bits),
                Inst::Index(dst) => {
                    self.copy(self.location(*dst), allocation::Location::Gpr(3));
                }
                Inst::Uniform(dst, off) => {
                    self.read_scalar(*dst, 13, *off);
                }
                Inst::LoadLocal(dst, slot) => {
                    self.copy(
                        self.location(*dst),
                        self.allocation.location(PrivateSlot::Local(*slot)),
                    );
                }
                Inst::StoreLocal(slot, src) => {
                    self.copy(
                        self.allocation.location(PrivateSlot::Local(*slot)),
                        self.location(*src),
                    );
                }
                Inst::LoadLocalIndexed {
                    dst,
                    base,
                    index,
                    word,
                    ..
                } => {
                    self.private_address(*base, *index, *word)?;
                    self.read_scalar(*dst, 15, 0);
                }
                Inst::StoreLocalIndexed {
                    base,
                    index,
                    word,
                    value,
                    ..
                } => {
                    self.private_address(*base, *index, *word)?;
                    self.write_memory(self.location(*value), 15, 0);
                }
                Inst::Load {
                    dst,
                    binding,
                    index,
                    word,
                } => {
                    self.address(*binding, *index);
                    self.read_scalar(*dst, 15, word * 4);
                }
                Inst::Store {
                    binding,
                    index,
                    word,
                    value,
                } => {
                    self.address(*binding, *index);
                    self.write_memory(self.location(*value), 15, word * 4);
                }
                Inst::Unary { dst, op, a } => {
                    self.load(*a);
                    match op {
                        UnaryOp::FloatNegate => {
                            self.bytes(&[0x35]);
                            self.u32(0x80000000);
                        }
                        UnaryOp::SignedNegate | UnaryOp::UnsignedNegate => {
                            self.bytes(&[0xf7, 0xd8])
                        }
                        UnaryOp::LogicalNot => {
                            self.bytes(&[0x85, 0xc0, 0x0f, 0x94, 0xc0, 0x0f, 0xb6, 0xc0])
                        }
                        UnaryOp::BitNot => self.bytes(&[0xf7, 0xd0]),
                    };
                    self.store(*dst);
                }
                Inst::Binary { dst, op, a, b } => match op {
                    BinaryOp::FAdd => self.float_binary(*dst, *a, *b, 0x58),
                    BinaryOp::FSub => self.float_binary(*dst, *a, *b, 0x5c),
                    BinaryOp::FMul => self.float_binary(*dst, *a, *b, 0x59),
                    BinaryOp::FDiv => self.float_binary(*dst, *a, *b, 0x5e),
                    BinaryOp::Compare(comparison) => self.compare(*dst, *a, *b, *comparison),
                    BinaryOp::IAdd => self.integer_binary(*dst, *a, *b, &[0x03]),
                    BinaryOp::ISub => self.integer_binary(*dst, *a, *b, &[0x2b]),
                    BinaryOp::IMul => self.integer_binary(*dst, *a, *b, &[0x0f, 0xaf]),
                    BinaryOp::And => self.integer_binary(*dst, *a, *b, &[0x23]),
                    BinaryOp::Or => self.integer_binary(*dst, *a, *b, &[0x0b]),
                    BinaryOp::Xor => self.integer_binary(*dst, *a, *b, &[0x33]),
                    BinaryOp::ShiftLeft => self.shift(*dst, *a, *b, 4),
                    BinaryOp::ShiftRightLogical => self.shift(*dst, *a, *b, 5),
                    BinaryOp::ShiftRightArithmetic => self.shift(*dst, *a, *b, 7),
                },
                Inst::IntegerDivide {
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
                Inst::Cast { dst, src } => {
                    let a = self.stage.registers[*src];
                    let b = self.stage.registers[*dst];
                    if a == b {
                        self.copy(self.location(*dst), self.location(*src));
                        continue;
                    }
                    if a == Scalar::F32 {
                        self.float(*src, 0);
                        if b == Scalar::Bool {
                            self.bytes(&[0x0f, 0x57, 0xc9]);
                            self.bytes(&[0x0f, 0x2e, 0xc1]);
                            self.comparison_result(Comparison::NotEqual, Scalar::F32);
                            self.store(*dst);
                        } else {
                            self.bytes(if b == Scalar::U32 {
                                &[0xf3, 0x48, 0x0f, 0x2c, 0xc0]
                            } else {
                                &[0xf3, 0x0f, 0x2c, 0xc0]
                            });
                            self.store(*dst);
                        }
                    } else {
                        self.load(*src);
                        if b == Scalar::F32 {
                            self.bytes(if a == Scalar::I32 {
                                &[0xf3, 0x0f, 0x2a, 0xc0]
                            } else {
                                &[0xf3, 0x48, 0x0f, 0x2a, 0xc0]
                            });
                            self.store_float(*dst);
                        } else {
                            if b == Scalar::Bool {
                                self.bytes(&[0x85, 0xc0, 0x0f, 0x95, 0xc0, 0x0f, 0xb6, 0xc0]);
                            }
                            self.store(*dst);
                        }
                    }
                }
                Inst::Math { dst, op, args } => match op {
                    MathOp::Sqrt => self.float_sqrt(*dst, args[0]),
                    MathOp::Min => self.float_binary(*dst, args[0], args[1], 0x5d),
                    MathOp::Max => self.float_binary(*dst, args[0], args[1], 0x5f),
                    MathOp::Abs => {
                        self.load(args[0]);
                        self.bytes(&[0x25]);
                        self.u32(0x7fffffff);
                        self.store(*dst);
                    }
                    MathOp::Sin => self.math_result(*dst, NativeIntrinsic::Sin, args),
                    MathOp::Cos => self.math_result(*dst, NativeIntrinsic::Cos, args),
                    MathOp::Floor => self.math_result(*dst, NativeIntrinsic::Floor, args),
                    MathOp::Exp => self.math_result(*dst, NativeIntrinsic::Exp, args),
                    MathOp::Exp2 => self.math_result(*dst, NativeIntrinsic::Exp2, args),
                    MathOp::Log => self.math_result(*dst, NativeIntrinsic::Log, args),
                    MathOp::Log2 => self.math_result(*dst, NativeIntrinsic::Log2, args),
                    MathOp::Pow => self.math_result(*dst, NativeIntrinsic::Pow, args),
                },
                Inst::Select {
                    dst,
                    condition,
                    a,
                    b,
                } => {
                    self.select(*dst, *condition, *a, *b);
                }
                Inst::If { condition, yes, no } => {
                    self.load(*condition);
                    self.bytes(&[0x85, 0xc0]);
                    let other = self.jump(Some(0x84));
                    self.block(yes)?;
                    let end = self.jump(None);
                    self.patch(other, self.code.len());
                    self.block(no)?;
                    self.patch(end, self.code.len());
                }
                Inst::Loop {
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
                Inst::Break => {
                    let at = self.jump(None);
                    self.breaks.last_mut().unwrap().push(at);
                }
                Inst::Continue => {
                    let at = self.jump(None);
                    self.patch(at, *self.continues.last().unwrap());
                }
            }
        }
        Ok(())
    }
}
/// ABI: (buffer pointers, push-constant words, end index, begin index)。
/// 全域归约另以第五个参数接收本范围的结果地址。
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
        return Ok(sort::emit_initialize(program, s));
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
    let lowered = texture::lower(&program.stages[index]);
    let stage = &lowered;
    let mut graph = allocation::Graph::new(stage);
    let allocation = Allocation::new(stage, &graph);
    let packet = packet::Plan::new(program, stage).map(|plan| {
        let allocation = PacketAllocation::new(stage, &mut graph, &plan.address_only);
        (plan, allocation)
    });
    drop(graph);
    let frame_layout = Frame::new(
        &allocation,
        packet.as_ref().map(|(_, allocation)| allocation),
    );
    let bytes = frame_layout.end
        + if packet.is_some() && stage.selection.is_some() {
            16
        } else {
            0
        }
        + if stage.has_tile_scope() {
            4 + stage.tile_data_bytes()
                + if stage.has_tile_steps() {
                    4 + stage.tile_private_slots().len() as u64 * stage.workgroup_width() as u64 * 4
                } else {
                    0
                }
        } else {
            0
        };
    let frame = i32::try_from(bytes.div_ceil(16) * 16)
        .map_err(|_| "原生 tile 栈帧超过 x64 范围函数 ABI")? as u32;
    let mut e = Emitter {
        code: Vec::new(),
        relocations: Vec::new(),
        stage,
        program,
        breaks: Vec::new(),
        exits: Vec::new(),
        continues: Vec::new(),
        allocation,
        frame: frame_layout,
    };
    e.enter(frame);
    if let Some((plan, allocation)) = packet {
        e.packet_loop(&plan, &allocation);
    }
    let begin = e.code.len();
    e.bytes(&[0x44, 0x39, 0xf3]);
    let end = e.jump(Some(0x83));
    let skip_selection = stage.selection.map(|selection| {
        // R12 is the data table, EBX the original record index.
        e.bytes(&[0x49, 0x8b, 0x84, 0x24]);
        e.u32(selection.binding as u32 * 8);
        e.bytes(&[0x89, 0xd9, 0x48, 0x69, 0xc9]);
        e.u32(program.buffers[selection.binding].stride);
        e.bytes(&[0x48, 0x01, 0xc8, 0x83, 0xb8]);
        e.u32(selection.word * 4);
        e.bytes(&[0]);
        e.jump(Some(0x84))
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
        e.bytes(&[0xff, 0xc3]);
    }
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.leave(frame);
    Ok(NativeCode {
        bytes: e.code,
        relocations: e.relocations,
        stack_bytes: frame,
    })
}
