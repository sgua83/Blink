//! 按 SPIR-V 1.4 标准直接编码；不调用 HLSL/GLSL 或第三方编译器。
use crate::ir::*;
mod append;
mod atomic;
mod compact;
mod custom_reduce;
mod prefix;
mod program;
pub use program::{ProgramImage, ShaderCode, StageCode, compile_program};
mod reduce;
mod returns;
mod scan;
mod segmented;
mod selection;
mod sort;
mod subgroup;
mod texture;
mod tile;
mod tile_collective;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    Main,
    ReductionTail,
    ScanTail,
    ScanAdd,
    ScanRootAdd,
    CompactScatter,
    SelectionArguments,
    SelectionScan,
    SelectionScatter,
    SortHistogram(u32),
    SortScatter(u32),
    SortKey(usize),
    SortScan,
    SortGather,
    DispatchArguments(PrefixCount),
}
use std::collections::BTreeMap;

/// GPU 存储页的最大字节数；每页只容纳完整记录，页表使用 64 位设备地址。
pub const PAGE_BYTES: u32 = 256 * 1024 * 1024;

fn op(out: &mut Vec<u32>, code: u16, args: &[u32]) {
    out.push(((args.len() as u32 + 1) << 16) | code as u32);
    out.extend_from_slice(args);
}
fn string(s: &str) -> Vec<u32> {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    while b.len() % 4 != 0 {
        b.push(0);
    }
    b.chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}
struct Emitter<'a> {
    next: u32,
    types: BTreeMap<Scalar, u32>,
    pointers: BTreeMap<Scalar, u32>,
    constants: BTreeMap<(Scalar, u32), u32>,
    defs: Vec<u32>,
    decorations: Vec<u32>,
    body: Vec<u32>,
    buffers: Vec<u32>,
    accelerations: Vec<u32>,
    textures: Vec<u32>,
    image_type: u32,
    sampled_type: u32,
    sampled_pointer: u32,
    vec4_type: u32,
    uint2_type: u32,
    acceleration_type: u32,
    ray_variables: BTreeMap<Reg, u32>,
    position_queries: std::collections::BTreeSet<Reg>,
    transform_queries: [std::collections::BTreeSet<Reg>; 2],
    vec3_type: u32,
    vec2_type: u32,
    push: u32,
    ptr_push_u32: u32,
    ptr_buffer_u32: u32,
    ptr_table_u64: u32,
    ptr_physical_u64: u32,
    u64_type: u32,
    index: u32,
    regs: Vec<u32>,
    locals: Vec<u32>,
    private_arrays: Vec<(std::ops::Range<usize>, u32)>,
    private_words: std::collections::BTreeSet<usize>,
    stage: &'a Stage,
    program: &'a Program,
    ext: u32,
    subgroup_inputs: BTreeMap<SubgroupOp, u32>,
    ballot_type: u32,
    tile_shared: u32,
    tile_pointer: u32,
    tile_length: u32,
    tile_lane: u32,
    current_label: u32,
    return_scopes: Vec<returns::ReturnScope>,
}
impl Emitter<'_> {
    fn private_address(&mut self, base: usize, index: Reg, word: u32) -> u32 {
        let (span, variable) = self
            .private_arrays
            .iter()
            .find(|(span, _)| span.contains(&base))
            .unwrap();
        let offset = (base - span.start) as u32 + word;
        let variable = *variable;
        let mut at = self.value(index);
        if offset != 0 {
            let offset = self.constant(Scalar::U32, offset);
            let sum = self.id();
            self.emit(128, &[self.types[&Scalar::U32], sum, at, offset]);
            at = sum;
        }
        let pointer = self.id();
        self.emit(65, &[self.pointers[&Scalar::U32], pointer, variable, at]);
        pointer
    }
    fn id(&mut self) -> u32 {
        let v = self.next;
        self.next += 1;
        v
    }
    fn constant(&mut self, s: Scalar, bits: u32) -> u32 {
        if let Some(id) = self.constants.get(&(s, bits)) {
            return *id;
        }
        let id = self.id();
        if s == Scalar::Bool {
            op(
                &mut self.defs,
                if bits == 0 { 42 } else { 41 },
                &[self.types[&s], id],
            );
        } else {
            op(&mut self.defs, 43, &[self.types[&s], id, bits]);
        }
        self.constants.insert((s, bits), id);
        id
    }
    fn emit(&mut self, code: u16, args: &[u32]) {
        if code == 248 {
            self.current_label = args[0];
        }
        op(&mut self.body, code, args);
    }
    fn value(&self, r: Reg) -> u32 {
        self.regs[r]
    }
    fn ty(&self, r: Reg) -> u32 {
        self.types[&self.stage.registers[r]]
    }
    fn signed_magnitude(&mut self, value: u32) -> (u32, u32) {
        let i = self.types[&Scalar::I32];
        let shift = self.constant(Scalar::I32, 31);
        let sign = self.id();
        self.emit(195, &[i, sign, value, shift]);
        let flipped = self.id();
        self.emit(198, &[i, flipped, value, sign]);
        let magnitude = self.id();
        self.emit(130, &[i, magnitude, flipped, sign]);
        let bits = self.id();
        self.emit(124, &[self.types[&Scalar::U32], bits, magnitude]);
        (bits, sign)
    }
    fn signed_divide(&mut self, result: IntegerDivisionResult, src: Reg, divisor: u32) {
        // 幅值留在 u32，可表示 abs(MIN32)；所有符号恢复按 32 位模运算完成。
        let (numerator, nsign) = self.signed_magnitude(self.value(src));
        let (denominator, dsign) = self.signed_magnitude(divisor);
        let i = self.types[&Scalar::I32];
        for (operator, dst) in result.outputs() {
            let raw = self.id();
            self.emit(
                if operator == IntegerDivisionOp::Quotient {
                    134
                } else {
                    137
                },
                &[self.types[&Scalar::U32], raw, numerator, denominator],
            );
            let signed = self.id();
            self.emit(124, &[i, signed, raw]);
            let sign = if operator == IntegerDivisionOp::Quotient {
                let sign = self.id();
                self.emit(198, &[i, sign, nsign, dsign]);
                sign
            } else {
                nsign
            };
            let flipped = self.id();
            self.emit(198, &[i, flipped, signed, sign]);
            self.emit(130, &[i, self.value(dst), flipped, sign]);
        }
    }
    fn address(&mut self, binding: usize, index: Reg, word: u32) -> u32 {
        self.address_scalar(
            binding,
            self.value(index),
            self.program.buffers[binding].stride / 4,
            word,
        )
    }
    fn address_scalar(&mut self, binding: usize, index: u32, stride_words: u32, word: u32) -> u32 {
        let (page, offset) = self.record_location(index, stride_words, word);
        let zero = self.constant(Scalar::U32, 0);
        let table = self.id();
        self.emit(
            65,
            &[self.ptr_table_u64, table, self.buffers[binding], zero, page],
        );
        let base = self.id();
        self.emit(61, &[self.u64_type, base, table]);
        self.word_pointer(base, offset)
    }
    fn record_copy_type(&mut self, lanes: u32) -> (u32, u32) {
        let mut vector = if lanes == 4 {
            self.ballot_type
        } else {
            self.uint2_type
        };
        if vector == 0 {
            vector = self.id();
            op(
                &mut self.defs,
                23,
                &[vector, self.types[&Scalar::U32], lanes],
            );
            if lanes == 4 {
                self.ballot_type = vector;
            } else {
                self.uint2_type = vector;
            }
        }
        let pointer = self.id();
        op(&mut self.defs, 32, &[pointer, 5349, vector]);
        (vector, pointer)
    }
    // 记录页不拆分记录；按原始字复制，保留填充及 bool／浮点编码。
    fn copy_record(
        &mut self,
        source_binding: usize,
        source_index: u32,
        target_binding: usize,
        target_index: u32,
    ) {
        let stride = self.program.buffers[source_binding].stride / 4;
        let source = self.address_scalar(source_binding, source_index, stride, 0);
        let target = self.address_scalar(target_binding, target_index, stride, 0);
        let blocks = stride / 4 + (stride % 4) / 2 + stride % 2;
        let bases = (blocks > 1).then(|| {
            let from = self.id();
            let to = self.id();
            self.emit(117, &[self.u64_type, from, source]);
            self.emit(117, &[self.u64_type, to, target]);
            (from, to)
        });
        let mut word = 0;
        for lanes in [4, 2, 1] {
            if stride - word < lanes {
                continue;
            }
            let (ty, pointer) = if lanes == 1 {
                (self.types[&Scalar::U32], self.ptr_buffer_u32)
            } else {
                self.record_copy_type(lanes)
            };
            while stride - word >= lanes {
                let (mut from, mut to) = if word == 0 {
                    (source, target)
                } else {
                    let (from, to) = bases.unwrap();
                    let bytes = self.constant(Scalar::U32, word * 4);
                    (self.word_pointer(from, bytes), self.word_pointer(to, bytes))
                };
                if lanes != 1 {
                    let vector_from = self.id();
                    let vector_to = self.id();
                    self.emit(124, &[pointer, vector_from, from]);
                    self.emit(124, &[pointer, vector_to, to]);
                    (from, to) = (vector_from, vector_to);
                }
                let value = self.id();
                self.emit(61, &[ty, value, from, 2, 4]);
                self.emit(62, &[to, value, 2, 4]);
                word += lanes;
            }
        }
    }
    fn record_location(&mut self, index: u32, stride_words: u32, word: u32) -> (u32, u32) {
        let capacity = self.constant(Scalar::U32, PAGE_BYTES / (stride_words * 4));
        let page = self.id();
        self.emit(134, &[self.types[&Scalar::U32], page, index, capacity]);
        let local = self.id();
        self.emit(137, &[self.types[&Scalar::U32], local, index, capacity]);
        let stride = self.constant(Scalar::U32, stride_words);
        let scaled = self.id();
        self.emit(132, &[self.types[&Scalar::U32], scaled, local, stride]);
        let offset = self.constant(Scalar::U32, word);
        let at = self.id();
        self.emit(128, &[self.types[&Scalar::U32], at, scaled, offset]);
        let four = self.constant(Scalar::U32, 4);
        let bytes = self.id();
        self.emit(132, &[self.types[&Scalar::U32], bytes, at, four]);
        (page, bytes)
    }
    fn word_pointer(&mut self, base: u32, bytes: u32) -> u32 {
        let offset64 = self.id();
        self.emit(113, &[self.u64_type, offset64, bytes]);
        let address = self.id();
        self.emit(128, &[self.u64_type, address, base, offset64]);
        let ptr = self.id();
        self.emit(120, &[self.ptr_buffer_u32, ptr, address]);
        ptr
    }
    fn view_address(&mut self, table: &[Reg; 2], index: Reg, stride: u32, word: u32) -> u32 {
        let low = self.id();
        let high = self.id();
        self.emit(113, &[self.u64_type, low, self.value(table[0])]);
        self.emit(113, &[self.u64_type, high, self.value(table[1])]);
        let width = self.constant(Scalar::U32, 32);
        let shifted = self.id();
        self.emit(196, &[self.u64_type, shifted, high, width]);
        let address = self.id();
        self.emit(197, &[self.u64_type, address, low, shifted]);
        let (page, offset) = self.record_location(self.value(index), stride / 4, word);
        let page64 = self.id();
        self.emit(113, &[self.u64_type, page64, page]);
        let three = self.constant(Scalar::U32, 3);
        let page_bytes = self.id();
        self.emit(196, &[self.u64_type, page_bytes, page64, three]);
        let entry = self.id();
        self.emit(128, &[self.u64_type, entry, address, page_bytes]);
        let pointer = self.id();
        self.emit(120, &[self.ptr_physical_u64, pointer, entry]);
        let base = self.id();
        self.emit(61, &[self.u64_type, base, pointer, 2, 8]);
        self.word_pointer(base, offset)
    }
    fn indexed_address(&mut self, source: &IndexedSource, offset: Reg, word: u32) -> u32 {
        let u = self.types[&Scalar::U32];
        if let IndexedSource::Tile { tile, index } = source {
            let width = self.constant(Scalar::U32, self.stage.workgroup_width());
            let scaled = self.id();
            self.emit(132, &[u, scaled, self.value(offset), width]);
            let at = self.id();
            self.emit(128, &[u, at, self.value(*index), scaled]);
            return self.tile_word_address(self.stage.shared_word(*tile, word), at);
        }
        let pointer = match source {
            IndexedSource::Data { binding, index } => self.address(*binding, *index, word),
            IndexedSource::View {
                address,
                index,
                stride,
            } => self.view_address(address, *index, *stride, word),
            _ => unreachable!(),
        };
        let address = self.id();
        self.emit(117, &[self.u64_type, address, pointer]);
        let four = self.constant(Scalar::U32, 4);
        let bytes = self.id();
        self.emit(132, &[u, bytes, self.value(offset), four]);
        self.word_pointer(address, bytes)
    }
    fn block(
        &mut self,
        code: &[Inst],
        loop_targets: Option<(u32, u32)>,
    ) -> Result<bool, &'static str> {
        for inst in code {
            match inst {
                Inst::Print { .. } => return Err("GPU 后端不能执行 CPU 标准输出"),
                Inst::ExternalCall(_) => return Err("GPU 后端不能调用 CPU C ABI 外部函数"),
                Inst::TextureSample {
                    result,
                    texture,
                    coordinate,
                    lod,
                } => self.texture_sample(*result, *texture, *coordinate, *lod),
                Inst::TextureLevels { dst, texture } => {
                    let image = self.texture_image(*texture);
                    self.emit(106, &[self.types[&Scalar::U32], self.value(*dst), image]);
                }
                Inst::TextureAddress { .. }
                | Inst::TextureFormat { .. }
                | Inst::TextureRead { .. } => return Err("CPU纹理降低指令不能进入SPIR-V"),
                Inst::TextureSize {
                    result,
                    texture,
                    level,
                } => {
                    let image = self.texture_image(*texture);
                    let size = self.id();
                    self.emit(103, &[self.uint2_type, size, image, self.value(*level)]);
                    for (i, r) in result.iter().enumerate() {
                        self.emit(81, &[self.ty(*r), self.value(*r), size, i as u32]);
                    }
                }
                Inst::TextureFetch {
                    result,
                    texture,
                    coordinate,
                    level,
                } => {
                    let image = self.texture_image(*texture);
                    let coord = self.id();
                    self.emit(
                        80,
                        &[
                            self.uint2_type,
                            coord,
                            self.value(coordinate[0]),
                            self.value(coordinate[1]),
                        ],
                    );
                    let value = self.id();
                    self.emit(
                        95,
                        &[self.vec4_type, value, image, coord, 2, self.value(*level)],
                    );
                    for (i, r) in result.iter().enumerate() {
                        self.emit(81, &[self.ty(*r), self.value(*r), value, i as u32]);
                    }
                }
                Inst::Scope(body) => self.function_scope(body)?,
                Inst::Exit => {
                    self.function_exit();
                    return Ok(false);
                }
                Inst::Append(append) => self.append(append),
                Inst::Atomic(atomic) => self.atomic(atomic),
                Inst::TileBarrier => self.tile_barrier(),
                Inst::TilePublish(publication) => self.publish_values(publication),
                Inst::TileCollective(collective) => self.tile_collective(collective),
                Inst::TileLength(dst) => {
                    self.emit(83, &[self.ty(*dst), self.value(*dst), self.tile_length])
                }
                Inst::TileLane(dst) => {
                    self.emit(83, &[self.ty(*dst), self.value(*dst), self.tile_lane])
                }
                Inst::TileLoad {
                    dst,
                    tile,
                    index,
                    word,
                } => self.tile_load(*dst, *tile, *index, *word),
                Inst::Subgroup {
                    dst,
                    op,
                    value,
                    argument,
                } => self.subgroup(*dst, *op, *value, *argument),
                Inst::ViewLoad {
                    dst,
                    address,
                    index,
                    stride,
                    word,
                } => {
                    let ptr = self.view_address(address, *index, *stride, *word);
                    let raw = self.id();
                    self.emit(61, &[self.types[&Scalar::U32], raw, ptr, 2, 4]);
                    self.from_word(*dst, raw);
                }
                Inst::RayQuery {
                    scene,
                    origin,
                    direction,
                    minimum,
                    maximum,
                    result,
                } => {
                    self.ray(*scene, origin, direction, *minimum, *maximum, result);
                }
                Inst::LoadIndexed {
                    dst,
                    source,
                    offset,
                    word,
                } => {
                    let ptr = self.indexed_address(source, *offset, *word);
                    let raw = self.id();
                    self.emit(61, &[self.types[&Scalar::U32], raw, ptr, 2, 4]);
                    self.from_word(*dst, raw);
                }
                Inst::StoreIndexed {
                    source,
                    offset,
                    word,
                    value,
                } => {
                    let ptr = self.indexed_address(source, *offset, *word);
                    let raw = self.to_word(*value);
                    self.emit(62, &[ptr, raw, 2, 4]);
                }
                Inst::Constant(dst, s, b) => {
                    let c = self.constant(*s, *b);
                    self.emit(83, &[self.ty(*dst), self.value(*dst), c]);
                }
                Inst::Index(dst) => self.emit(83, &[self.ty(*dst), self.value(*dst), self.index]),
                Inst::Uniform(dst, offset) => {
                    let zero = self.constant(Scalar::U32, 0);
                    let word = self.constant(Scalar::U32, offset / 4);
                    let ptr = self.id();
                    self.emit(65, &[self.ptr_push_u32, ptr, self.push, zero, word]);
                    let raw = self.id();
                    self.emit(61, &[self.types[&Scalar::U32], raw, ptr, 2, 4]);
                    self.from_word(*dst, raw);
                }
                Inst::LoadLocal(dst, slot) => {
                    if self.private_words.contains(slot) {
                        let raw = self.id();
                        self.emit(61, &[self.types[&Scalar::U32], raw, self.locals[*slot]]);
                        self.from_word(*dst, raw);
                    } else {
                        self.emit(61, &[self.ty(*dst), self.value(*dst), self.locals[*slot]])
                    }
                }
                Inst::StoreLocal(slot, src) => {
                    let value = if self.private_words.contains(slot) {
                        self.to_word(*src)
                    } else {
                        self.value(*src)
                    };
                    self.emit(62, &[self.locals[*slot], value])
                }
                Inst::LoadLocalIndexed {
                    dst,
                    base,
                    index,
                    word,
                    ..
                } => {
                    let pointer = self.private_address(*base, *index, *word);
                    let raw = self.id();
                    self.emit(61, &[self.types[&Scalar::U32], raw, pointer]);
                    self.from_word(*dst, raw);
                }
                Inst::StoreLocalIndexed {
                    base,
                    index,
                    word,
                    value,
                    ..
                } => {
                    let pointer = self.private_address(*base, *index, *word);
                    let value = self.to_word(*value);
                    self.emit(62, &[pointer, value]);
                }
                Inst::Load {
                    dst,
                    binding,
                    index,
                    word,
                } => {
                    let ptr = self.address(*binding, *index, *word);
                    let raw = self.id();
                    self.emit(61, &[self.types[&Scalar::U32], raw, ptr, 2, 4]);
                    self.from_word(*dst, raw);
                }
                Inst::Store {
                    binding,
                    index,
                    word,
                    value,
                } => {
                    let ptr = self.address(*binding, *index, *word);
                    let raw = self.to_word(*value);
                    self.emit(62, &[ptr, raw, 2, 4]);
                }
                Inst::Unary {
                    dst,
                    op: operation,
                    a,
                } => {
                    let s = self.stage.registers[*a];
                    if *operation == UnaryOp::UnsignedNegate {
                        let zero = self.constant(s, 0);
                        self.emit(
                            130,
                            &[self.ty(*dst), self.value(*dst), zero, self.value(*a)],
                        );
                    } else {
                        let opcode = match operation {
                            UnaryOp::FloatNegate => 127,
                            UnaryOp::SignedNegate | UnaryOp::UnsignedNegate => 126,
                            UnaryOp::LogicalNot => 168,
                            UnaryOp::BitNot => 200,
                            UnaryOp::Bitcast => 124,
                        };
                        self.emit(opcode, &[self.ty(*dst), self.value(*dst), self.value(*a)]);
                    }
                }
                Inst::IntegerDivide {
                    result,
                    src,
                    divisor,
                } => {
                    let scalar = self.stage.registers[*src];
                    let divisor = match divisor {
                        IntegerDivisor::Constant(divisor) => self.constant(scalar, divisor.get()),
                        IntegerDivisor::Value(divisor) => self.value(*divisor),
                    };
                    if scalar == Scalar::I32 {
                        // Vulkan 未启用 maintenance8 时，负操作数的 OpSRem 没有定义。
                        // 常量与动态除数共用幅值降低，保持语言的带符号除余规则。
                        self.signed_divide(*result, *src, divisor);
                        continue;
                    }
                    for (op, dst) in result.outputs() {
                        let opcode = match op {
                            IntegerDivisionOp::Quotient => 134,
                            IntegerDivisionOp::Remainder => 137,
                        };
                        self.emit(
                            opcode,
                            &[self.ty(dst), self.value(dst), self.value(*src), divisor],
                        );
                    }
                }
                Inst::Binary {
                    dst,
                    op: operation,
                    a,
                    b,
                } => {
                    let s = self.stage.registers[*a];
                    let f = s == Scalar::F32;
                    let signed = s == Scalar::I32;
                    let opcode = match operation {
                        BinaryOp::FAdd => 129,
                        BinaryOp::FSub => 131,
                        BinaryOp::FMul => 133,
                        BinaryOp::FDiv => 136,
                        BinaryOp::IAdd => 128,
                        BinaryOp::ISub => 130,
                        BinaryOp::IMul => 132,
                        BinaryOp::And => 199,
                        BinaryOp::Or => 197,
                        BinaryOp::Xor => 198,
                        BinaryOp::ShiftLeft => 196,
                        BinaryOp::ShiftRightLogical => 194,
                        BinaryOp::ShiftRightArithmetic => 195,
                        BinaryOp::Compare(c) => match c {
                            Comparison::Equal => {
                                if f {
                                    180
                                } else if s == Scalar::Bool {
                                    164
                                } else {
                                    170
                                }
                            }
                            Comparison::NotEqual => {
                                if f {
                                    183
                                } else if s == Scalar::Bool {
                                    165
                                } else {
                                    171
                                }
                            }
                            Comparison::Less => {
                                if f {
                                    184
                                } else if signed {
                                    177
                                } else {
                                    176
                                }
                            }
                            Comparison::Greater => {
                                if f {
                                    186
                                } else if signed {
                                    173
                                } else {
                                    172
                                }
                            }
                            Comparison::LessEqual => {
                                if f {
                                    188
                                } else if signed {
                                    179
                                } else {
                                    178
                                }
                            }
                            Comparison::GreaterEqual => {
                                if f {
                                    190
                                } else if signed {
                                    175
                                } else {
                                    174
                                }
                            }
                        },
                    };
                    let b = if matches!(
                        operation,
                        BinaryOp::ShiftLeft
                            | BinaryOp::ShiftRightLogical
                            | BinaryOp::ShiftRightArithmetic
                    ) {
                        let mask = self.constant(s, 31);
                        let temp = self.id();
                        self.emit(199, &[self.types[&s], temp, self.value(*b), mask]);
                        temp
                    } else {
                        self.value(*b)
                    };
                    self.emit(
                        opcode,
                        &[self.ty(*dst), self.value(*dst), self.value(*a), b],
                    );
                    if matches!(
                        operation,
                        BinaryOp::FAdd | BinaryOp::FSub | BinaryOp::FMul | BinaryOp::FDiv
                    ) {
                        let id = self.value(*dst);
                        op(&mut self.decorations, 71, &[id, 42]);
                    }
                }
                Inst::Cast { dst, src } => {
                    let a = self.stage.registers[*src];
                    let b = self.stage.registers[*dst];
                    if a == Scalar::Bool {
                        let one = self.constant(
                            b,
                            if b == Scalar::F32 {
                                1.0f32.to_bits()
                            } else {
                                1
                            },
                        );
                        let zero = self.constant(b, 0);
                        self.emit(
                            169,
                            &[self.ty(*dst), self.value(*dst), self.value(*src), one, zero],
                        );
                    } else if b == Scalar::Bool {
                        let zero = self.constant(a, 0);
                        self.emit(
                            if a == Scalar::F32 { 183 } else { 171 },
                            &[self.ty(*dst), self.value(*dst), self.value(*src), zero],
                        );
                    } else {
                        let opcode = match (a, b) {
                            (Scalar::F32, Scalar::U32) => 109,
                            (Scalar::F32, Scalar::I32) => 110,
                            (Scalar::U32, Scalar::F32) => 112,
                            (Scalar::I32, Scalar::F32) => 111,
                            _ => 124,
                        };
                        self.emit(opcode, &[self.ty(*dst), self.value(*dst), self.value(*src)]);
                    }
                }
                Inst::Math {
                    dst,
                    op: operation,
                    args,
                } => {
                    let opcode = match operation {
                        MathOp::Abs => 4,
                        MathOp::Floor => 8,
                        MathOp::Sin => 13,
                        MathOp::Cos => 14,
                        MathOp::Pow => 26,
                        MathOp::Exp => 27,
                        MathOp::Log => 28,
                        MathOp::Exp2 => 29,
                        MathOp::Log2 => 30,
                        MathOp::Sqrt => 31,
                        MathOp::Min => 37,
                        MathOp::Max => 40,
                    };
                    let mut operands = vec![self.ty(*dst), self.value(*dst), self.ext, opcode];
                    operands.extend(args.iter().map(|r| self.value(*r)));
                    self.emit(12, &operands);
                }
                Inst::Select {
                    dst,
                    condition,
                    a,
                    b,
                } => self.emit(
                    169,
                    &[
                        self.ty(*dst),
                        self.value(*dst),
                        self.value(*condition),
                        self.value(*a),
                        self.value(*b),
                    ],
                ),
                Inst::If { condition, yes, no } => {
                    let y = self.id();
                    let n = self.id();
                    let merge = self.id();
                    self.emit(247, &[merge, 0]);
                    self.emit(250, &[self.value(*condition), y, n]);
                    self.emit(248, &[y]);
                    let yr = self.block(yes, loop_targets)?;
                    if yr {
                        self.emit(249, &[merge]);
                    }
                    self.emit(248, &[n]);
                    let nr = self.block(no, loop_targets)?;
                    if nr {
                        self.emit(249, &[merge]);
                    }
                    self.emit(248, &[merge]);
                    if !yr && !nr {
                        self.emit(255, &[]);
                        return Ok(false);
                    }
                }
                Inst::Loop {
                    condition,
                    test,
                    body,
                } => {
                    let header = self.id();
                    let check = self.id();
                    let run = self.id();
                    let next = self.id();
                    let end = self.id();
                    let returning = self.begin_return_loop(body, end);
                    self.emit(249, &[header]);
                    self.emit(248, &[header]);
                    self.emit(246, &[end, next, 0]);
                    self.emit(249, &[check]);
                    self.emit(248, &[check]);
                    self.block(condition, Some((end, next)))?;
                    self.loop_exit_edge(end, self.current_label);
                    self.emit(250, &[self.value(*test), run, end]);
                    self.emit(248, &[run]);
                    if self.block(body, Some((end, next)))? {
                        self.emit(249, &[next]);
                    }
                    self.emit(248, &[next]);
                    self.emit(249, &[header]);
                    self.emit(248, &[end]);
                    if returning {
                        self.finish_return_loop();
                    }
                }
                Inst::Break => {
                    self.loop_exit_edge(loop_targets.unwrap().0, self.current_label);
                    self.emit(249, &[loop_targets.unwrap().0]);
                    return Ok(false);
                }
                Inst::Continue => {
                    self.emit(249, &[loop_targets.unwrap().1]);
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    fn from_word(&mut self, dst: Reg, raw: u32) {
        match self.stage.registers[dst] {
            Scalar::U32 => self.emit(83, &[self.ty(dst), self.value(dst), raw]),
            Scalar::Bool => {
                let zero = self.constant(Scalar::U32, 0);
                self.emit(171, &[self.ty(dst), self.value(dst), raw, zero]);
            }
            _ => self.emit(124, &[self.ty(dst), self.value(dst), raw]),
        }
    }
    fn to_word(&mut self, value: Reg) -> u32 {
        let s = self.stage.registers[value];
        if s == Scalar::U32 {
            return self.value(value);
        }
        let raw = self.id();
        if s == Scalar::Bool {
            let one = self.constant(Scalar::U32, 1);
            let zero = self.constant(Scalar::U32, 0);
            self.emit(
                169,
                &[self.types[&Scalar::U32], raw, self.value(value), one, zero],
            );
        } else {
            self.emit(124, &[self.types[&Scalar::U32], raw, self.value(value)]);
        }
        raw
    }
    fn ray(
        &mut self,
        scene: usize,
        origin: &[Reg; 3],
        direction: &[Reg; 3],
        minimum: Reg,
        maximum: Reg,
        result: &RayResult,
    ) {
        let query = self.ray_variables[&result.registers()[0]];
        let acceleration = self.id();
        self.emit(
            61,
            &[
                self.acceleration_type,
                acceleration,
                self.accelerations[scene],
            ],
        );
        let o = self.id();
        let d = self.id();
        self.emit(
            80,
            &[
                self.vec3_type,
                o,
                self.value(origin[0]),
                self.value(origin[1]),
                self.value(origin[2]),
            ],
        );
        self.emit(
            80,
            &[
                self.vec3_type,
                d,
                self.value(direction[0]),
                self.value(direction[1]),
                self.value(direction[2]),
            ],
        );
        let flags = self.constant(
            Scalar::U32,
            if matches!(result, RayResult::Any(_)) {
                5
            } else {
                1
            },
        );
        let mask = self.constant(Scalar::U32, 255);
        self.emit(
            4473,
            &[
                query,
                acceleration,
                flags,
                mask,
                o,
                self.value(minimum),
                d,
                self.value(maximum),
            ],
        );
        let header = self.id();
        let body = self.id();
        let next = self.id();
        let end = self.id();
        self.emit(249, &[header]);
        self.emit(248, &[header]);
        self.emit(246, &[end, next, 0]);
        self.emit(249, &[body]);
        self.emit(248, &[body]);
        let more = self.id();
        self.emit(4477, &[self.types[&Scalar::Bool], more, query]);
        self.emit(250, &[more, next, end]);
        self.emit(248, &[next]);
        self.emit(249, &[header]);
        self.emit(248, &[end]);
        let committed = self.constant(Scalar::U32, 1);
        let kind = self.id();
        self.emit(4479, &[self.types[&Scalar::U32], kind, query, committed]);
        let zero = self.constant(Scalar::U32, 0);
        self.emit(
            171,
            &[
                self.types[&Scalar::Bool],
                self.value(result.registers()[0]),
                kind,
                zero,
            ],
        );
        if let RayResult::Closest(regs) = result {
            let yes = self.id();
            let no = self.id();
            let merged = self.id();
            self.emit(247, &[merged, 0]);
            self.emit(250, &[self.value(regs[0]), yes, no]);
            self.emit(248, &[yes]);
            let distance = self.id();
            let barycentric = self.id();
            let u = self.id();
            let v = self.id();
            let instance = self.id();
            let primitive = self.id();
            self.emit(
                6018,
                &[self.types[&Scalar::F32], distance, query, committed],
            );
            self.emit(6024, &[self.vec2_type, barycentric, query, committed]);
            self.emit(81, &[self.types[&Scalar::F32], u, barycentric, 0]);
            self.emit(81, &[self.types[&Scalar::F32], v, barycentric, 1]);
            self.emit(
                6020,
                &[self.types[&Scalar::U32], instance, query, committed],
            );
            self.emit(
                6023,
                &[self.types[&Scalar::U32], primitive, query, committed],
            );
            let mut fields = Vec::new();
            if self.position_queries.contains(&regs[0]) {
                let length = self.constant(Scalar::U32, 3);
                let array_type = self.id();
                op(&mut self.defs, 28, &[array_type, self.vec3_type, length]);
                let points = self.id();
                self.emit(5340, &[array_type, points, query, committed]);
                for v in 0..3 {
                    for c in 0..3 {
                        let component = self.id();
                        self.emit(81, &[self.types[&Scalar::F32], component, points, v, c]);
                        fields.push((regs[6 + v as usize * 3 + c as usize], component));
                    }
                }
            }
            for (slot, opcode) in [(0, 6031), (1, 6032)] {
                if self.transform_queries[slot].contains(&regs[0]) {
                    let matrix_type = self.id();
                    op(&mut self.defs, 24, &[matrix_type, self.vec3_type, 4]);
                    let matrix = self.id();
                    self.emit(opcode, &[matrix_type, matrix, query, committed]);
                    // SPIR-V以列为复合索引，B的float3x4以行存储。
                    for row in 0..3 {
                        for column in 0..4 {
                            let component = self.id();
                            self.emit(
                                81,
                                &[self.types[&Scalar::F32], component, matrix, column, row],
                            );
                            fields.push((
                                regs[15 + slot * 12 + row as usize * 4 + column as usize],
                                component,
                            ));
                        }
                    }
                }
            }
            self.emit(249, &[merged]);
            self.emit(248, &[no]);
            self.emit(249, &[merged]);
            self.emit(248, &[merged]);
            let z = self.constant(Scalar::F32, 0);
            for (r, value, miss) in [
                (regs[1], distance, self.value(maximum)),
                (regs[2], u, z),
                (regs[3], v, z),
                (regs[4], instance, zero),
                (regs[5], primitive, zero),
            ] {
                self.emit(245, &[self.ty(r), self.value(r), value, yes, miss, no]);
            }
            for (r, value) in fields {
                self.emit(245, &[self.ty(r), self.value(r), value, yes, z, no]);
            }
        }
    }
}
/// u32 元素域按 64 路分组递归至单组所需的最大层数。
pub const PREFIX_LEVELS: usize = u32::BITS.div_ceil(6) as usize;
pub fn emit(program: &Program, stage_index: usize) -> Result<Vec<u32>, &'static str> {
    emit_pass(program, stage_index, Pass::Main)
}
fn ray_results(code: &[Inst], out: &mut Vec<Reg>) {
    for i in code {
        match i {
            Inst::RayQuery { result, .. } => out.push(result.registers()[0]),
            Inst::Scope(body) => ray_results(body, out),
            Inst::If { yes, no, .. } => {
                ray_results(yes, out);
                ray_results(no, out);
            }
            Inst::Loop {
                condition, body, ..
            } => {
                ray_results(condition, out);
                ray_results(body, out);
            }
            _ => {}
        }
    }
}
pub fn emit_pass(
    program: &Program,
    stage_index: usize,
    pass: Pass,
) -> Result<Vec<u32>, &'static str> {
    if program.buffers.iter().any(|b| {
        b.stride > PAGE_BYTES
            || b.ty
                .view_fields()
                .iter()
                .any(|(_, ty)| ty.size > PAGE_BYTES)
    }) {
        return Err("GPU 页寻址要求单条记录能容纳于一个物理页");
    }
    let stage = &program.stages[stage_index];
    let custom_kernel =
        if let Operation::CustomReduce(r)
        | Operation::CustomScan(CustomScan { values: r, .. })
        | Operation::CustomSegmentedReduce(CustomSegmentedReduction { values: r, .. }) =
            &stage.operation
        {
            Some(r.kernel(
                stage,
                pass == Pass::Main
                    || matches!(stage.operation, Operation::CustomSegmentedReduce(_))
                        && pass == Pass::ScanRootAdd,
                pass != Pass::ScanAdd
                    && (pass != Pass::ScanRootAdd
                        || matches!(stage.operation, Operation::CustomSegmentedReduce(_))),
            ))
        } else {
            None
        };
    let stage = custom_kernel.as_ref().map(|k| &k.stage).unwrap_or(stage);
    let mut e = Emitter {
        next: 1,
        types: BTreeMap::new(),
        pointers: BTreeMap::new(),
        constants: BTreeMap::new(),
        defs: Vec::new(),
        decorations: Vec::new(),
        body: Vec::new(),
        buffers: Vec::new(),
        accelerations: Vec::new(),
        textures: Vec::new(),
        image_type: 0,
        sampled_type: 0,
        sampled_pointer: 0,
        vec4_type: 0,
        uint2_type: 0,
        acceleration_type: 0,
        ray_variables: BTreeMap::new(),
        position_queries: stage.ray_result_queries(6..15),
        transform_queries: [
            stage.ray_result_queries(15..27),
            stage.ray_result_queries(27..39),
        ],
        vec3_type: 0,
        vec2_type: 0,
        push: 0,
        ptr_push_u32: 0,
        ptr_buffer_u32: 0,
        ptr_table_u64: 0,
        ptr_physical_u64: 0,
        u64_type: 0,
        index: 0,
        regs: Vec::new(),
        locals: Vec::new(),
        private_arrays: Vec::new(),
        private_words: std::collections::BTreeSet::new(),
        stage,
        program,
        ext: 0,
        subgroup_inputs: BTreeMap::new(),
        ballot_type: 0,
        tile_shared: 0,
        tile_pointer: 0,
        tile_length: 0,
        tile_lane: 0,
        current_label: 0,
        return_scopes: Vec::new(),
    };
    let mut prelude = Vec::new();
    op(&mut prelude, 17, &[1]);
    op(&mut prelude, 17, &[11]);
    op(&mut prelude, 17, &[5347]);
    let float32_preserve = matches!(
        pass,
        Pass::Main | Pass::ReductionTail | Pass::ScanTail | Pass::ScanAdd | Pass::ScanRootAdd
    ) && stage.requires_float32_preserve();
    if float32_preserve {
        op(&mut prelude, 17, &[4464]);
        op(&mut prelude, 17, &[4466]);
    }
    let ranks = matches!(pass, Pass::SortHistogram(_) | Pass::SortScatter(_));
    let subgroup_operations = if pass == Pass::Main {
        stage.subgroup_operations()
    } else {
        Default::default()
    };
    let subgroup_caps = SubgroupCapabilities::from_operations(subgroup_operations.iter().copied());
    let maximal_reconvergence = pass == Pass::Main && stage.requires_maximal_reconvergence();
    for (needed, capability) in [
        (subgroup_caps.basic || ranks, 61),
        (subgroup_caps.vote, 62),
        (subgroup_caps.arithmetic, 63),
        (subgroup_caps.ballot || ranks, 64),
        (subgroup_caps.shuffle, 65),
        (subgroup_caps.shuffle_relative, 66),
    ] {
        if needed {
            op(&mut prelude, 17, &[capability]);
        }
    }
    if !program.accelerations.is_empty() {
        op(&mut prelude, 17, &[4472]);
    }
    if !e.position_queries.is_empty() {
        op(&mut prelude, 17, &[5391]);
    }
    if !program.textures.is_empty() {
        op(&mut prelude, 17, &[50]);
    }
    if program.requires_texture_indexing() {
        op(&mut prelude, 17, &[5301]);
        op(&mut prelude, 17, &[5307]);
    }
    // SPIR-V 的逻辑布局要求全部 Capability 先于全部 Extension。
    if program.requires_texture_indexing() {
        op(&mut prelude, 10, &string("SPV_EXT_descriptor_indexing"));
    }
    if float32_preserve {
        op(&mut prelude, 10, &string("SPV_KHR_float_controls"));
    }
    if maximal_reconvergence {
        op(&mut prelude, 10, &string("SPV_KHR_maximal_reconvergence"));
    }
    if !program.accelerations.is_empty() {
        op(&mut prelude, 10, &string("SPV_KHR_ray_query"));
    }
    if !e.position_queries.is_empty() {
        op(
            &mut prelude,
            10,
            &string("SPV_KHR_ray_tracing_position_fetch"),
        );
    }
    op(&mut prelude, 10, &string("SPV_KHR_physical_storage_buffer"));
    e.ext = e.id();
    let mut words = vec![e.ext];
    words.extend(string("GLSL.std.450"));
    op(&mut prelude, 11, &words);
    op(&mut prelude, 14, &[5348, 1]);
    let void = e.id();
    op(&mut e.defs, 19, &[void]);
    for s in [Scalar::F32, Scalar::U32, Scalar::I32, Scalar::Bool] {
        let id = e.id();
        match s {
            Scalar::F32 => op(&mut e.defs, 22, &[id, 32]),
            Scalar::Bool => op(&mut e.defs, 20, &[id]),
            _ => op(&mut e.defs, 21, &[id, 32, (s == Scalar::I32) as u32]),
        };
        e.types.insert(s, id);
        let ptr = e.id();
        op(&mut e.defs, 32, &[ptr, 7, id]);
        e.pointers.insert(s, ptr);
    }
    let u = e.types[&Scalar::U32];
    e.vec3_type = e.id();
    e.vec2_type = e.id();
    op(&mut e.defs, 23, &[e.vec3_type, e.types[&Scalar::F32], 3]);
    op(&mut e.defs, 23, &[e.vec2_type, e.types[&Scalar::F32], 2]);
    if !program.textures.is_empty() {
        e.vec4_type = e.id();
        op(&mut e.defs, 23, &[e.vec4_type, e.types[&Scalar::F32], 4]);
        e.uint2_type = e.id();
        op(&mut e.defs, 23, &[e.uint2_type, u, 2]);
        e.image_type = e.id();
        op(
            &mut e.defs,
            25,
            &[e.image_type, e.types[&Scalar::F32], 1, 0, 0, 0, 1, 0],
        );
        e.sampled_type = e.id();
        op(&mut e.defs, 27, &[e.sampled_type, e.image_type]);
        let pointer = e.id();
        op(&mut e.defs, 32, &[pointer, 0, e.sampled_type]);
        e.sampled_pointer = pointer;
        for i in 0..program.textures.len() {
            let pointer = if let Some(count) = program.textures[i].count {
                let length = e.constant(Scalar::U32, count);
                let array = e.id();
                op(&mut e.defs, 28, &[array, e.sampled_type, length]);
                let pointer = e.id();
                op(&mut e.defs, 32, &[pointer, 0, array]);
                pointer
            } else {
                pointer
            };
            let id = e.id();
            op(&mut e.defs, 59, &[pointer, id, 0]);
            op(&mut e.decorations, 71, &[id, 34, 0]);
            op(
                &mut e.decorations,
                71,
                &[
                    id,
                    33,
                    (program.buffers.len() + program.accelerations.len() + i) as u32,
                ],
            );
            e.textures.push(id);
        }
    }
    let mut ray_pointer = 0;
    if !program.accelerations.is_empty() {
        e.acceleration_type = e.id();
        op(&mut e.defs, 5341, &[e.acceleration_type]);
        let pointer = e.id();
        op(&mut e.defs, 32, &[pointer, 0, e.acceleration_type]);
        for i in 0..program.accelerations.len() {
            let id = e.id();
            op(&mut e.defs, 59, &[pointer, id, 0]);
            op(&mut e.decorations, 71, &[id, 34, 0]);
            op(
                &mut e.decorations,
                71,
                &[id, 33, (program.buffers.len() + i) as u32],
            );
            e.accelerations.push(id);
        }
        let query_type = e.id();
        op(&mut e.defs, 4472, &[query_type]);
        ray_pointer = e.id();
        op(&mut e.defs, 32, &[ray_pointer, 7, query_type]);
    }
    e.u64_type = e.id();
    op(&mut e.defs, 21, &[e.u64_type, 64, 0]);
    let v3 = e.id();
    op(&mut e.defs, 23, &[v3, u, 3]);
    let ptr_input = e.id();
    op(&mut e.defs, 32, &[ptr_input, 1, v3]);
    let global = e.id();
    op(&mut e.defs, 59, &[ptr_input, global, 1]);
    op(&mut e.decorations, 71, &[global, 11, 28]);
    let mut num_groups = 0;
    if (stage.prefix.is_some() || stage.selection.is_some())
        && !matches!(pass, Pass::DispatchArguments(_) | Pass::SelectionArguments)
    {
        num_groups = e.id();
        op(&mut e.defs, 59, &[ptr_input, num_groups, 1]);
        op(&mut e.decorations, 71, &[num_groups, 11, 24]);
    }
    let array = e.id();
    op(&mut e.defs, 29, &[array, e.u64_type]);
    op(&mut e.decorations, 71, &[array, 6, 8]);
    let block = e.id();
    op(&mut e.defs, 30, &[block, array]);
    op(&mut e.decorations, 71, &[block, 2]);
    op(&mut e.decorations, 72, &[block, 0, 35, 0]);
    let ptr = e.id();
    op(&mut e.defs, 32, &[ptr, 12, block]);
    e.ptr_buffer_u32 = e.id();
    op(&mut e.defs, 32, &[e.ptr_buffer_u32, 5349, u]);
    e.ptr_table_u64 = e.id();
    op(&mut e.defs, 32, &[e.ptr_table_u64, 12, e.u64_type]);
    e.ptr_physical_u64 = e.id();
    op(&mut e.defs, 32, &[e.ptr_physical_u64, 5349, e.u64_type]);
    for i in 0..program.buffers.len() + program.auxiliary_buffers() {
        let id = e.id();
        op(&mut e.defs, 59, &[ptr, id, 12]);
        op(&mut e.decorations, 71, &[id, 34, 0]);
        op(
            &mut e.decorations,
            71,
            &[
                id,
                33,
                if i >= program.buffers.len() {
                    (i + program.accelerations.len() + program.textures.len()) as u32
                } else {
                    i as u32
                },
            ],
        );
        op(&mut e.decorations, 71, &[id, 24]); // 页表始终只读，访问权限仍由 IR 定义。
        e.buffers.push(id);
    }
    let length = e.constant(Scalar::U32, program.uniform_bytes / 4);
    let push_array = e.id();
    op(&mut e.defs, 28, &[push_array, u, length]);
    op(&mut e.decorations, 71, &[push_array, 6, 4]);
    let push_block = e.id();
    op(&mut e.defs, 30, &[push_block, push_array]);
    op(&mut e.decorations, 71, &[push_block, 2]);
    op(&mut e.decorations, 72, &[push_block, 0, 35, 0]);
    let push_ptr = e.id();
    op(&mut e.defs, 32, &[push_ptr, 9, push_block]);
    e.push = e.id();
    op(&mut e.defs, 59, &[push_ptr, e.push, 9]);
    e.ptr_push_u32 = e.id();
    op(&mut e.defs, 32, &[e.ptr_push_u32, 9, u]);
    let scan_local = (matches!(
        stage.operation,
        Operation::ScanExclusive(_) | Operation::CustomScan(_) | Operation::Compact(_)
    ) && matches!(pass, Pass::Main | Pass::ScanTail))
        || (stage.selection.is_some() && matches!(pass, Pass::SelectionScan | Pass::ScanTail))
        || (matches!(stage.operation, Operation::StableSort(_))
            && matches!(pass, Pass::SortScan | Pass::ScanTail))
        || (matches!(
            stage.operation,
            Operation::SegmentedReduce(_) | Operation::CustomSegmentedReduce(_)
        ) && matches!(pass, Pass::Main | Pass::ScanTail | Pass::ScanRootAdd));
    let reduction = (matches!(
        stage.operation,
        Operation::Reduce(_) | Operation::CustomReduce(_)
    ) && matches!(pass, Pass::Main | Pass::ReductionTail))
        || scan_local;
    let shared_scalar = Scalar::U32;
    let mut local_input = 0;
    let mut shared = 0;
    let mut shared_pointer = 0;
    if reduction {
        let input_ptr = e.id();
        op(&mut e.defs, 32, &[input_ptr, 1, u]);
        local_input = e.id();
        op(&mut e.defs, 59, &[input_ptr, local_input, 1]);
        op(&mut e.decorations, 71, &[local_input, 11, 29]);
    }
    let mut subgroup_inputs = Vec::new();
    let mut ballot_type = 0;
    if ranks {
        let input_ptr = e.id();
        op(&mut e.defs, 32, &[input_ptr, 1, u]);
        for builtin in [40, 41] {
            let v = e.id();
            op(&mut e.defs, 59, &[input_ptr, v, 1]);
            op(&mut e.decorations, 71, &[v, 11, builtin]);
            subgroup_inputs.push(v);
        }
    }
    if subgroup_caps.ballot || ranks {
        ballot_type = e.id();
        op(&mut e.defs, 23, &[ballot_type, u, 4]);
        e.ballot_type = ballot_type;
    }
    for (operation, builtin) in [
        (SubgroupOp::Lane, 41),
        (SubgroupOp::Size, 36),
        (SubgroupOp::Id, 40),
    ] {
        if subgroup_operations.contains(&operation) {
            let input_ptr = e.id();
            op(&mut e.defs, 32, &[input_ptr, 1, u]);
            let variable = e.id();
            op(&mut e.defs, 59, &[input_ptr, variable, 1]);
            op(&mut e.decorations, 71, &[variable, 11, builtin]);
            e.subgroup_inputs.insert(operation, variable);
        }
    }
    if reduction || ranks {
        let count = e.constant(
            Scalar::U32,
            if ranks {
                32
            } else if let Operation::Reduce(r) = &stage.operation {
                64 * r.ty.components.len() as u32
            } else if let Operation::CustomReduce(r) = &stage.operation {
                64 * r.ty.components.len() as u32
            } else if let Operation::CustomScan(r) = &stage.operation {
                64 * r.summary_words() as u32
            } else if let Operation::CustomSegmentedReduce(r) = &stage.operation {
                64 * r.summary_words() as u32
            } else if let Operation::ScanExclusive(r) = &stage.operation {
                64 * r.summary_words() as u32
            } else if let Operation::SegmentedReduce(r) = &stage.operation {
                64 * r.summary_words() as u32
            } else {
                64
            },
        );
        let array = e.id();
        op(&mut e.defs, 28, &[array, e.types[&shared_scalar], count]);
        let array_ptr = e.id();
        op(&mut e.defs, 32, &[array_ptr, 4, array]);
        shared = e.id();
        op(&mut e.defs, 59, &[array_ptr, shared, 4]);
        shared_pointer = e.id();
        op(
            &mut e.defs,
            32,
            &[shared_pointer, 4, e.types[&shared_scalar]],
        );
    }
    if pass == Pass::Main && stage.tile_bytes() != 0 {
        let words = u32::try_from(stage.tile_bytes() / 4)
            .map_err(|_| "tile 共享数组超过 SPIR-V 长度范围")?;
        let length = e.constant(Scalar::U32, words);
        let array = e.id();
        op(&mut e.defs, 28, &[array, u, length]);
        let array_ptr = e.id();
        op(&mut e.defs, 32, &[array_ptr, 4, array]);
        e.tile_shared = e.id();
        op(&mut e.defs, 59, &[array_ptr, e.tile_shared, 4]);
        e.tile_pointer = e.id();
        op(&mut e.defs, 32, &[e.tile_pointer, 4, u]);
    }
    let fn_ty = e.id();
    op(&mut e.defs, 33, &[fn_ty, void]);
    let main = e.id();
    let entry = e.id();
    let execute = e.id();
    let end = e.id();
    let mut entry_words = vec![5, main];
    entry_words.extend(string("main"));
    entry_words.push(global);
    if num_groups != 0 {
        entry_words.push(num_groups);
    }
    entry_words.extend_from_slice(&e.buffers);
    entry_words.extend_from_slice(&e.accelerations);
    entry_words.extend_from_slice(&e.textures);
    entry_words.push(e.push);
    if shared != 0 {
        entry_words.push(shared);
    }
    if reduction {
        entry_words.push(local_input);
    }
    entry_words.extend_from_slice(&subgroup_inputs);
    entry_words.extend(e.subgroup_inputs.values().copied());
    if e.tile_shared != 0 {
        entry_words.push(e.tile_shared);
    }
    op(&mut prelude, 15, &entry_words);
    op(&mut prelude, 16, &[main, 17, stage.workgroup_width(), 1, 1]);
    if maximal_reconvergence {
        op(&mut prelude, 16, &[main, 6023]);
    }
    if float32_preserve {
        op(&mut prelude, 16, &[main, 4459, 32]);
        op(&mut prelude, 16, &[main, 4461, 32]);
    }
    e.regs = (0..stage.registers.len()).map(|_| e.id()).collect();
    e.emit(54, &[void, main, 0, fn_ty]);
    e.emit(248, &[entry]);
    let mut rays = Vec::new();
    ray_results(&stage.code, &mut rays);
    for result in rays {
        let id = e.id();
        e.emit(59, &[ray_pointer, id, 7]);
        e.ray_variables.insert(result, id);
    }
    e.locals.resize(stage.locals.len(), 0);
    for span in private_spans(&stage.code) {
        let length = e.constant(Scalar::U32, span.len() as u32);
        let array = e.id();
        op(&mut e.defs, 28, &[array, u, length]);
        let pointer = e.id();
        op(&mut e.defs, 32, &[pointer, 7, array]);
        let variable = e.id();
        e.emit(59, &[pointer, variable, 7]);
        e.private_words.extend(span.clone());
        e.private_arrays.push((span, variable));
    }
    for (slot, s) in stage.locals.iter().enumerate() {
        if e.private_words.contains(&slot) {
            continue;
        }
        let id = e.id();
        e.emit(59, &[e.pointers[s], id, 7]);
        e.locals[slot] = id;
    }
    let mut temporary = 0;
    if ranks {
        temporary = e.id();
        e.emit(59, &[e.pointers[&shared_scalar], temporary, 7]);
    }
    for (span, variable) in e.private_arrays.clone() {
        for slot in span.clone() {
            let at = e.constant(Scalar::U32, (slot - span.start) as u32);
            let pointer = e.id();
            e.emit(65, &[e.pointers[&Scalar::U32], pointer, variable, at]);
            e.locals[slot] = pointer;
        }
    }
    let gid = e.id();
    e.emit(61, &[v3, gid, global]);
    let local_index = e.id();
    e.emit(81, &[u, local_index, gid, 0]);
    let (count, active) = if num_groups != 0 {
        e.prefix_index(
            gid,
            num_groups,
            v3,
            !matches!(stage.operation, Operation::Map)
                || (stage.selection.is_some() && pass != Pass::Main),
        )
    } else {
        let zero = e.constant(Scalar::U32, 0);
        let one = e.constant(Scalar::U32, 1);
        let base_ptr = e.id();
        e.emit(65, &[e.ptr_push_u32, base_ptr, e.push, zero, one]);
        let base = e.id();
        e.emit(61, &[u, base, base_ptr]);
        let wide = e.u64_type;
        let local_wide = e.id();
        e.emit(113, &[wide, local_wide, local_index]);
        let base_wide = e.id();
        e.emit(113, &[wide, base_wide, base]);
        let index_wide = e.id();
        e.emit(128, &[wide, index_wide, local_wide, base_wide]);
        let ptr = e.id();
        e.emit(65, &[e.ptr_push_u32, ptr, e.push, zero, zero]);
        let count = e.id();
        e.emit(61, &[u, count, ptr]);
        let count_wide = e.id();
        e.emit(113, &[wide, count_wide, count]);
        let active = e.id();
        e.emit(
            176,
            &[e.types[&Scalar::Bool], active, index_wide, count_wide],
        );
        e.index = e.id();
        e.emit(113, &[u, e.index, index_wide]);
        (count, active)
    };
    if let Some(kernel) = &custom_kernel
        && !matches!(pass, Pass::DispatchArguments(_))
    {
        e.block(&kernel.stage.code[..kernel.invariant_end], None)?;
    }
    if let Pass::DispatchArguments(prefix) = pass {
        e.prefix_arguments(&prefix, local_index);
    } else if pass == Pass::SelectionArguments {
        if let Some(prefix) = stage.prefix {
            e.prefix_arguments(&prefix, local_index);
        } else {
            e.selection_source_arguments(local_index);
        }
    } else if let Some(selection) = stage.selection
        && pass != Pass::Main
    {
        if pass == Pass::SelectionScatter {
            e.selection_scatter(selection, count, active);
        } else if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
            e.scan(
                &Scan::counts(selection.scan(program.selection_buffer())),
                pass == Pass::ScanTail,
                local,
                shared,
                shared_pointer,
                active,
                count,
            );
        } else {
            e.emit(247, &[end, 0]);
            e.emit(250, &[active, execute, end]);
            e.emit(248, &[execute]);
            e.scan_add(
                &Scan::counts(selection.scan(program.selection_buffer())),
                false,
            );
            e.emit(249, &[end]);
            e.emit(248, &[end]);
        }
    } else if let Operation::StableSort(s) = &stage.operation {
        match pass {
            Pass::Main => e.sort_initialize(s, s.keys.len() - 1, true, active),
            Pass::SortKey(key) => e.sort_initialize(s, key, false, active),
            Pass::SortGather => e.sort_gather(s, active),
            Pass::SortHistogram(shift) | Pass::SortScatter(shift) => e.sort_rank(
                s,
                shift,
                pass == Pass::SortHistogram(shift),
                count,
                &subgroup_inputs,
                ballot_type,
                shared,
                shared_pointer,
                temporary,
            ),
            Pass::SortScan | Pass::ScanTail => {
                let local = e.id();
                e.emit(61, &[u, local, local_input]);
                let r = Reduction {
                    source: s.source,
                    source_word: 0,
                    target: s.target,
                    target_word: 0,
                };
                e.scan(
                    &Scan::counts(r),
                    true,
                    local,
                    shared,
                    shared_pointer,
                    active,
                    count,
                );
            }
            Pass::ScanAdd => {
                e.emit(247, &[end, 0]);
                e.emit(250, &[active, execute, end]);
                e.emit(248, &[execute]);
                e.scan_add(
                    &Scan::counts(Reduction {
                        source: s.source,
                        source_word: 0,
                        target: s.target,
                        target_word: 0,
                    }),
                    false,
                );
                e.emit(249, &[end]);
                e.emit(248, &[end]);
            }
            _ => return Err("排序操作不支持该代码生成阶段"),
        }
    } else if let Operation::CustomSegmentedReduce(r) = &stage.operation {
        let local = if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
            local
        } else {
            0
        };
        e.custom_segmented_reduce(
            r,
            custom_kernel.as_ref().unwrap(),
            pass,
            local,
            shared,
            shared_pointer,
            active,
            count,
        )?;
    } else if let Operation::CustomScan(r) = &stage.operation {
        let kernel = custom_kernel.as_ref().unwrap();
        if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
            e.custom_scan(
                r,
                kernel,
                pass == Pass::ScanTail,
                local,
                shared,
                shared_pointer,
                active,
                count,
            )?;
        } else {
            e.emit(247, &[end, 0]);
            e.emit(250, &[active, execute, end]);
            e.emit(248, &[execute]);
            e.custom_scan_add(r, kernel, pass == Pass::ScanRootAdd)?;
            e.emit(249, &[end]);
            e.emit(248, &[end]);
        }
    } else if let Operation::CustomReduce(r) = &stage.operation {
        let local = e.id();
        e.emit(61, &[u, local, local_input]);
        e.custom_reduce(
            r,
            custom_kernel.as_ref().unwrap(),
            pass == Pass::ReductionTail,
            local,
            shared,
            shared_pointer,
            active,
            count,
        )?;
    } else if let Operation::Reduce(r) = &stage.operation {
        let local = e.id();
        e.emit(61, &[u, local, local_input]);
        e.reduce(
            r,
            pass == Pass::ReductionTail,
            local,
            shared,
            shared_pointer,
            active,
            count,
        );
    } else if let Operation::SegmentedReduce(r) = &stage.operation {
        let local = if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
            local
        } else {
            0
        };
        e.segmented_reduce(r, pass, local, shared, shared_pointer, active, count);
    } else if let Operation::Compact(c) = &stage.operation {
        if pass == Pass::CompactScatter {
            e.compact_scatter(c, count, active);
        } else if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
            e.scan(
                &Scan::counts(c.scan()),
                pass == Pass::ScanTail,
                local,
                shared,
                shared_pointer,
                active,
                count,
            );
        } else {
            e.emit(247, &[end, 0]);
            e.emit(250, &[active, execute, end]);
            e.emit(248, &[execute]);
            e.scan_add(&Scan::counts(c.scan()), false);
            e.emit(249, &[end]);
            e.emit(248, &[end]);
        }
    } else if let Operation::ScanExclusive(r) = &stage.operation {
        if let Some(head) = r.head_word {
            if scan_local {
                let local = e.id();
                e.emit(61, &[u, local, local_input]);
                e.segmented_scan(
                    r,
                    head,
                    pass == Pass::ScanTail,
                    local,
                    shared,
                    shared_pointer,
                    active,
                    count,
                );
            } else {
                e.emit(247, &[end, 0]);
                e.emit(250, &[active, execute, end]);
                e.emit(248, &[execute]);
                match pass {
                    Pass::ScanAdd => e.segmented_add(r),
                    Pass::ScanRootAdd => e.segmented_root_add(r),
                    _ => return Err("分段扫描不支持该代码生成阶段"),
                }
                e.emit(249, &[end]);
                e.emit(248, &[end]);
            }
        } else {
            if scan_local {
                let local = e.id();
                e.emit(61, &[u, local, local_input]);
                e.scan(
                    r,
                    pass == Pass::ScanTail,
                    local,
                    shared,
                    shared_pointer,
                    active,
                    count,
                );
            } else {
                e.emit(247, &[end, 0]);
                e.emit(250, &[active, execute, end]);
                e.emit(248, &[execute]);
                e.scan_add(r, pass == Pass::ScanRootAdd);
                e.emit(249, &[end]);
                e.emit(248, &[end]);
            }
        }
    } else {
        if stage.has_tile_scope() {
            e.tile_input(count, active);
        }
        if stage.has_tile_steps() {
            e.tile_phases(active)?;
        } else {
            e.emit(247, &[end, 0]);
            e.emit(250, &[active, execute, end]);
            e.emit(248, &[execute]);
            if stage.selection.is_some() {
                let at = e.address_scalar(program.buffers.len(), e.index, 1, 0);
                let original = e.id();
                e.emit(61, &[u, original, at, 2, 4]);
                e.index = original;
            }
            if e.block(&stage.code, None)? {
                e.emit(249, &[end]);
            }
            e.emit(248, &[end]);
        }
    }
    e.emit(253, &[]);
    e.emit(56, &[]);
    let mut out = vec![0x07230203, 0x00010400, 0, e.next, 0];
    out.extend(prelude);
    out.extend(e.decorations);
    out.extend(e.defs);
    out.extend(e.body);
    Ok(out)
}
