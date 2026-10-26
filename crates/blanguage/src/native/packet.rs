//! 四个独立元素的 SSE2 降低；计划只包含已证明的访问与可表达指令，尾部共用标量 IR。
use super::*;
use allocation::{PacketAllocation, PacketPlace};
use operands::Operand;
use std::collections::BTreeSet;
mod integer;
mod memory;

enum Binary {
    Float(u8),
    Integer(u8),
    Multiply,
    Compare(Comparison, Scalar),
    Shift(u8),
    ShiftEach(u8),
}
enum Math {
    Sqrt,
    Abs,
    Min,
    Max,
}
enum Address {
    Element,
    Uniform(Reg),
}

enum ReadSource {
    Dataset(usize),
    View {
        address: [Reg; 2],
        stride: u32,
        uniform: bool,
    },
}
enum Step {
    Constant(Reg, u32),
    Index(Reg),
    Uniform(Reg, u32),
    Copy(PrivateSlot, PrivateSlot),
    Load {
        dst: Reg,
        binding: usize,
        index: Address,
        word: u32,
    },
    Gather {
        dst: Reg,
        source: ReadSource,
        index: Reg,
        word: u32,
        broadcast: bool,
    },
    Store {
        binding: usize,
        index: Address,
        word: u32,
        value: Reg,
    },
    Unary {
        dst: Reg,
        op: UnaryOp,
        a: Reg,
    },
    Binary {
        dst: Reg,
        op: Binary,
        a: Reg,
        b: Reg,
    },
    IntegerDivide {
        result: IntegerDivisionResult,
        src: Reg,
        divisor: IntegerDivisor,
    },
    Cast {
        dst: Reg,
        src: Reg,
        from: Scalar,
        into: Scalar,
    },
    Math {
        dst: Reg,
        op: Math,
        args: Vec<Reg>,
    },
    Select {
        dst: Reg,
        condition: Reg,
        a: Reg,
        b: Reg,
    },
}
pub(super) struct Plan {
    code: Vec<Step>,
    pub address_only: BTreeSet<Reg>,
}
impl Plan {
    pub fn new(program: &Program, stage: &Stage) -> Option<Self> {
        if stage.has_tile_scope() || stage.writes.is_empty() {
            return None;
        }
        let mut uniform = vec![false; stage.registers.len()];
        let mut locals = vec![false; stage.locals.len()];
        let mut indices = BTreeSet::new();
        let mut code = Vec::new();
        for inst in &stage.code {
            let step = match inst {
                Inst::Constant(dst, _, bits) => {
                    uniform[*dst] = true;
                    Step::Constant(*dst, *bits)
                }
                Inst::Index(dst) => {
                    indices.insert(*dst);
                    Step::Index(*dst)
                }
                Inst::Uniform(dst, offset) => {
                    uniform[*dst] = true;
                    Step::Uniform(*dst, *offset)
                }
                Inst::LoadLocal(dst, slot) => {
                    uniform[*dst] = locals[*slot];
                    Step::Copy(PrivateSlot::Register(*dst), PrivateSlot::Local(*slot))
                }
                Inst::StoreLocal(slot, src) => {
                    locals[*slot] = uniform[*src];
                    Step::Copy(PrivateSlot::Local(*slot), PrivateSlot::Register(*src))
                }
                Inst::Load {
                    dst,
                    binding,
                    index,
                    word,
                } => {
                    let broadcast = uniform[*index] && !stage.writes.contains(binding);
                    let element = indices.contains(index);
                    if !broadcast && !element && stage.writes.contains(binding) {
                        return None;
                    }
                    let stride = program.buffers[*binding].stride as u64;
                    let last =
                        *word as u64 * 4 + if element && !broadcast { 3 * stride } else { 0 };
                    if stride > i32::MAX as u64 || last > i32::MAX as u64 {
                        return None;
                    }
                    uniform[*dst] = broadcast;
                    if broadcast || element {
                        Step::Load {
                            dst: *dst,
                            binding: *binding,
                            index: if broadcast {
                                Address::Uniform(*index)
                            } else {
                                Address::Element
                            },
                            word: *word,
                        }
                    } else {
                        Step::Gather {
                            dst: *dst,
                            source: ReadSource::Dataset(*binding),
                            index: *index,
                            word: *word,
                            broadcast: false,
                        }
                    }
                }
                Inst::ViewLoad {
                    dst,
                    address,
                    index,
                    stride,
                    word,
                } => {
                    if *stride > i32::MAX as u32 || *word as u64 * 4 > i32::MAX as u64 {
                        return None;
                    }
                    let same_address = uniform[address[0]] && uniform[address[1]];
                    let broadcast = same_address && uniform[*index];
                    uniform[*dst] = broadcast;
                    Step::Gather {
                        dst: *dst,
                        source: ReadSource::View {
                            address: *address,
                            stride: *stride,
                            uniform: same_address,
                        },
                        index: *index,
                        word: *word,
                        broadcast,
                    }
                }
                Inst::Store {
                    binding,
                    index,
                    word,
                    value,
                } => {
                    if !indices.contains(index) {
                        return None;
                    }
                    if *word as u64 * 4 + 3 * program.buffers[*binding].stride as u64
                        > i32::MAX as u64
                    {
                        return None;
                    }
                    Step::Store {
                        binding: *binding,
                        index: Address::Element,
                        word: *word,
                        value: *value,
                    }
                }
                Inst::Unary { dst, op, a } => {
                    uniform[*dst] = uniform[*a];
                    Step::Unary {
                        dst: *dst,
                        op: *op,
                        a: *a,
                    }
                }
                Inst::Binary { dst, op, a, b } => {
                    let op = match op {
                        BinaryOp::FAdd => Binary::Float(0x58),
                        BinaryOp::FSub => Binary::Float(0x5c),
                        BinaryOp::FMul => Binary::Float(0x59),
                        BinaryOp::FDiv => Binary::Float(0x5e),
                        BinaryOp::IAdd => Binary::Integer(0xfe),
                        BinaryOp::ISub => Binary::Integer(0xfa),
                        BinaryOp::And => Binary::Integer(0xdb),
                        BinaryOp::Or => Binary::Integer(0xeb),
                        BinaryOp::Xor => Binary::Integer(0xef),
                        BinaryOp::IMul => Binary::Multiply,
                        BinaryOp::Compare(c) => Binary::Compare(*c, stage.registers[*a]),
                        BinaryOp::ShiftLeft if uniform[*b] => Binary::Shift(0xf2),
                        BinaryOp::ShiftRightLogical if uniform[*b] => Binary::Shift(0xd2),
                        BinaryOp::ShiftRightArithmetic if uniform[*b] => Binary::Shift(0xe2),
                        BinaryOp::ShiftLeft => Binary::ShiftEach(4),
                        BinaryOp::ShiftRightLogical => Binary::ShiftEach(5),
                        BinaryOp::ShiftRightArithmetic => Binary::ShiftEach(7),
                    };
                    uniform[*dst] = uniform[*a] && uniform[*b];
                    Step::Binary {
                        dst: *dst,
                        op,
                        a: *a,
                        b: *b,
                    }
                }
                Inst::IntegerDivide {
                    result,
                    src,
                    divisor,
                } => {
                    let same = uniform[*src]
                        && match divisor {
                            IntegerDivisor::Constant(_) => true,
                            IntegerDivisor::Value(r) => uniform[*r],
                        };
                    for (_, dst) in result.outputs() {
                        uniform[dst] = same;
                    }
                    Step::IntegerDivide {
                        result: *result,
                        src: *src,
                        divisor: *divisor,
                    }
                }
                Inst::Cast { dst, src } => {
                    uniform[*dst] = uniform[*src];
                    let from = stage.registers[*src];
                    let into = stage.registers[*dst];
                    if operands::bit_copy(from, into) {
                        Step::Copy(PrivateSlot::Register(*dst), PrivateSlot::Register(*src))
                    } else {
                        Step::Cast {
                            dst: *dst,
                            src: *src,
                            from,
                            into,
                        }
                    }
                }
                Inst::Math { dst, op, args } => {
                    let op = match op {
                        MathOp::Sqrt => Math::Sqrt,
                        MathOp::Abs => Math::Abs,
                        MathOp::Min => Math::Min,
                        MathOp::Max => Math::Max,
                        _ => return None,
                    };
                    uniform[*dst] = args.iter().all(|r| uniform[*r]);
                    Step::Math {
                        dst: *dst,
                        op,
                        args: args.clone(),
                    }
                }
                Inst::Select {
                    dst,
                    condition,
                    a,
                    b,
                } => {
                    uniform[*dst] = uniform[*condition] && uniform[*a] && uniform[*b];
                    Step::Select {
                        dst: *dst,
                        condition: *condition,
                        a: *a,
                        b: *b,
                    }
                }
                _ => return None,
            };
            code.push(step);
        }
        let mut values = BTreeSet::new();
        for inst in &stage.code {
            match inst {
                Inst::Load { .. } => {}
                Inst::Store { value, .. } => {
                    values.insert(*value);
                }
                _ => {
                    values.extend(inst.operands());
                }
            }
        }
        let address_only: BTreeSet<_> = indices.difference(&values).copied().collect();
        code.retain(|step| !matches!(step, Step::Index(r) if address_only.contains(r)));
        Some(Self { code, address_only })
    }
}
impl From<PacketPlace> for Operand {
    fn from(place: PacketPlace) -> Self {
        match place {
            PacketPlace::Register(r) => Operand::Register(r),
            PacketPlace::Stack(offset) => Operand::Memory { base: 4, offset },
        }
    }
}
impl Emitter<'_> {
    fn packed(&mut self, prefix: Option<u8>, opcode: u8, dst: u8, src: u8) {
        self.instruction(prefix, &[0x0f, opcode], false, dst, Operand::Register(src));
    }
    fn shuffle(&mut self, dst: u8, src: u8, pattern: u8) {
        self.packed(Some(0x66), 0x70, dst, src);
        self.bytes(&[pattern]);
    }
    fn packet_constant(&mut self, xmm: u8, bits: u32) {
        if bits == 0 {
            self.packed(Some(0x66), 0xef, xmm, xmm);
        } else if bits == u32::MAX {
            self.packed(Some(0x66), 0x76, xmm, xmm);
        } else {
            self.bytes(&[0xb8]);
            self.u32(bits);
            self.instruction(Some(0x66), &[0x0f, 0x6e], false, xmm, Operand::Register(0));
            self.shuffle(xmm, xmm, 0);
        }
    }
    fn packet_load(&mut self, allocation: &PacketAllocation, slot: PrivateSlot, xmm: u8) {
        let source = allocation.location(slot);
        if source != PacketPlace::Register(xmm) {
            self.instruction(None, &[0x0f, 0x28], false, xmm, source.into());
        }
    }
    fn packet_store(&mut self, allocation: &PacketAllocation, slot: PrivateSlot, xmm: u8) {
        match allocation.location(slot) {
            PacketPlace::Register(dst) => {
                if dst != xmm {
                    self.packed(None, 0x28, dst, xmm);
                }
            }
            place @ PacketPlace::Stack(_) => {
                self.instruction(None, &[0x0f, 0x29], false, xmm, place.into());
            }
        }
    }
    fn packet_address(
        &mut self,
        allocation: &PacketAllocation,
        binding: usize,
        index: &Address,
        lane: u32,
    ) {
        match index {
            Address::Element if self.stage.selection.is_some() => {
                self.instruction(
                    None,
                    &[0x8b],
                    false,
                    0,
                    Operand::Memory {
                        base: 4,
                        offset: self.frame.end as u32 + lane * 4,
                    },
                );
            }
            Address::Element => self.bytes(&[0x89, 0xd8]),
            Address::Uniform(r) => match allocation.location(PrivateSlot::Register(*r)) {
                PacketPlace::Register(xmm) => {
                    self.instruction(Some(0x66), &[0x0f, 0x7e], false, xmm, Operand::Register(0))
                }
                source @ PacketPlace::Stack(_) => {
                    self.instruction(None, &[0x8b], false, 0, source.into())
                }
            },
        }
        self.bytes(&[0x4d, 0x8b, 0xbc, 0x24]);
        self.u32(binding as u32 * 8);
        self.bytes(&[0x48, 0x69, 0xc0]);
        self.u32(self.program.buffers[binding].stride);
        self.bytes(&[0x49, 0x01, 0xc7]);
    }
    fn packet_binary(
        &mut self,
        allocation: &PacketAllocation,
        dst: Reg,
        a: Reg,
        b: Reg,
        prefix: Option<u8>,
        opcode: u8,
    ) {
        let (d, a_place, b_place) = (
            allocation.location(PrivateSlot::Register(dst)),
            allocation.location(PrivateSlot::Register(a)),
            allocation.location(PrivateSlot::Register(b)),
        );
        let target = match d {
            PacketPlace::Register(r) if d != b_place || a_place == b_place => r,
            _ => 0,
        };
        self.packet_load(allocation, PrivateSlot::Register(a), target);
        self.instruction(prefix, &[0x0f, opcode], false, target, b_place.into());
        self.packet_store(allocation, PrivateSlot::Register(dst), target);
    }
    fn packet_bool(&mut self, xmm: u8) {
        self.packet_constant(3, 1);
        self.packed(Some(0x66), 0xdb, xmm, 3);
    }
    fn packet_from_word(&mut self, dst: Reg, xmm: u8) {
        if self.stage.registers[dst] == Scalar::Bool {
            self.packet_constant(1, 0);
            self.packed(Some(0x66), 0x76, xmm, 1);
            // 零的比较掩码为 -1，其余为 0；加 1 得到规范的 0／1。
            self.packet_constant(1, 1);
            self.packed(Some(0x66), 0xfe, xmm, 1);
        }
    }
    fn packet_compare(
        &mut self,
        allocation: &PacketAllocation,
        dst: Reg,
        a: Reg,
        b: Reg,
        op: Comparison,
        scalar: Scalar,
    ) {
        let swap = matches!(op, Comparison::Greater | Comparison::GreaterEqual)
            && scalar == Scalar::F32
            || matches!(op, Comparison::Less | Comparison::GreaterEqual) && scalar != Scalar::F32;
        let (a, b) = if swap { (b, a) } else { (a, b) };
        let (d, left, right) = (
            allocation.location(PrivateSlot::Register(dst)),
            allocation.location(PrivateSlot::Register(a)),
            allocation.location(PrivateSlot::Register(b)),
        );
        let equality = matches!(op, Comparison::Equal | Comparison::NotEqual);
        let bias = !equality && matches!(scalar, Scalar::U32 | Scalar::Bool);
        let target = match d {
            PacketPlace::Register(r) if bias || d != right || left == right => r,
            _ => 0,
        };
        if bias {
            // 无符号顺序先保留右值；它可能与结果共用位置。
            self.packet_load(allocation, PrivateSlot::Register(b), 1);
        }
        self.packet_load(allocation, PrivateSlot::Register(a), target);
        if scalar == Scalar::F32 {
            self.instruction(None, &[0x0f, 0xc2], false, target, right.into());
            self.bytes(&[match op {
                Comparison::Equal => 0,
                Comparison::NotEqual => 4,
                Comparison::Less | Comparison::Greater => 1,
                Comparison::LessEqual | Comparison::GreaterEqual => 2,
            }]);
        } else {
            if bias {
                self.packet_constant(2, 0x80000000);
                self.packed(Some(0x66), 0xef, target, 2);
                self.packed(Some(0x66), 0xef, 1, 2);
            }
            self.instruction(
                Some(0x66),
                &[0x0f, if equality { 0x76 } else { 0x66 }],
                false,
                target,
                if bias {
                    Operand::Register(1)
                } else {
                    right.into()
                },
            );
            if matches!(
                op,
                Comparison::NotEqual | Comparison::LessEqual | Comparison::GreaterEqual
            ) {
                self.packet_constant(2, u32::MAX);
                self.packed(Some(0x66), 0xef, target, 2);
            }
        }
        self.packet_bool(target);
        self.packet_store(allocation, PrivateSlot::Register(dst), target);
    }
    fn packet_cast(&mut self, from: Scalar, into: Scalar) {
        if into == Scalar::Bool {
            self.packet_constant(1, 0);
            if from == Scalar::F32 {
                self.packed(None, 0xc2, 0, 1);
                self.bytes(&[4]);
            } else {
                self.packed(Some(0x66), 0x76, 0, 1);
                self.packet_constant(1, u32::MAX);
                self.packed(Some(0x66), 0xef, 0, 1);
            }
            self.packet_bool(0);
        } else if into == Scalar::F32 && from != Scalar::F32 {
            if from == Scalar::U32 || from == Scalar::Bool {
                // 高低 16 位均能精确转换，乘二的幂也精确，只有最终加法产生一次舍入。
                self.packed(None, 0x28, 1, 0);
                self.packet_constant(2, 0xffff);
                self.packed(Some(0x66), 0xdb, 0, 2);
                self.packed(Some(0x66), 0x72, 2, 1);
                self.bytes(&[16]);
                self.packed(None, 0x5b, 0, 0);
                self.packed(None, 0x5b, 1, 1);
                self.packet_constant(2, 65536.0f32.to_bits());
                self.packed(None, 0x59, 1, 2);
                self.packed(None, 0x58, 0, 1);
            } else {
                self.packed(None, 0x5b, 0, 0);
            }
        } else if from == Scalar::F32 && into != Scalar::F32 {
            if into == Scalar::U32 {
                self.packet_constant(1, 2147483648.0f32.to_bits());
                self.packed(None, 0x28, 3, 1);
                self.packed(None, 0xc2, 1, 0);
                self.bytes(&[2]);
                self.packed(Some(0x66), 0xdb, 3, 1);
                self.packed(None, 0x5c, 0, 3);
                self.packed(Some(0xf3), 0x5b, 0, 0);
                self.packet_constant(2, 0x80000000);
                self.packed(Some(0x66), 0xdb, 1, 2);
                self.packed(Some(0x66), 0xef, 0, 1);
            } else {
                self.packed(Some(0xf3), 0x5b, 0, 0);
            }
        }
    }
    pub(super) fn packet_loop(&mut self, plan: &Plan, allocation: &PacketAllocation) {
        use PrivateSlot::Register;
        let begin = self.code.len();
        let sparse = self.stage.selection.is_some();
        let tail = if let Some(selection) = self.stage.selection {
            // 四个真实选中位置只在本范围栈帧内存在，不物化全域稀疏索引。
            self.bytes(&[0x31, 0xc9]); // ecx = 已收集 lane 数
            let collect = self.code.len();
            self.bytes(&[0x44, 0x39, 0xf3]);
            let tail = self.jump(Some(0x83));
            self.bytes(&[0x4d, 0x8b, 0xbc, 0x24]);
            self.u32(selection.binding as u32 * 8);
            self.bytes(&[0x89, 0xd8, 0x48, 0x69, 0xc0]);
            self.u32(self.program.buffers[selection.binding].stride);
            self.bytes(&[0x49, 0x01, 0xc7, 0x41, 0x83, 0xbf]);
            self.u32(selection.word * 4);
            self.bytes(&[0]);
            let skip = self.jump(Some(0x84));
            self.bytes(&[0x89, 0x9c, 0x8c]); // indices[ecx] = ebx
            self.u32(self.frame.end as u32);
            self.bytes(&[0xff, 0xc1]);
            self.patch(skip, self.code.len());
            self.bytes(&[0xff, 0xc3, 0x83, 0xf9, 0x04]);
            let again = self.jump(Some(0x85));
            self.patch(again, collect);
            tail
        } else {
            // 用 64 位检查 index+4；u32 域末端附近也不让加法提前回绕。
            self.bytes(&[0x89, 0xd8, 0x48, 0x83, 0xc0, 0x04, 0x4c, 0x39, 0xf0]);
            self.jump(Some(0x87))
        };
        for step in &plan.code {
            match step {
                Step::Constant(dst, bits) => {
                    let target = match allocation.location(Register(*dst)) {
                        PacketPlace::Register(r) => r,
                        _ => 0,
                    };
                    self.packet_constant(target, *bits);
                    self.packet_store(allocation, Register(*dst), target);
                }
                Step::Index(dst) => {
                    if sparse {
                        self.instruction(
                            None,
                            &[0x0f, 0x28],
                            false,
                            0,
                            Operand::Memory {
                                base: 4,
                                offset: self.frame.end as u32,
                            },
                        );
                        self.packet_store(allocation, Register(*dst), 0);
                        continue;
                    }
                    // 从 [0,1,2,3] 加广播的真实范围起点形成源语言索引。
                    for (i, xmm) in [0, 1, 2, 3].into_iter().enumerate() {
                        self.bytes(&[0xb8]);
                        self.u32(i as u32);
                        self.instruction(
                            Some(0x66),
                            &[0x0f, 0x6e],
                            false,
                            xmm,
                            Operand::Register(0),
                        );
                    }
                    self.packed(Some(0x66), 0x62, 0, 1);
                    self.packed(Some(0x66), 0x62, 2, 3);
                    self.packed(Some(0x66), 0x6c, 0, 2);
                    self.instruction(Some(0x66), &[0x0f, 0x6e], false, 1, Operand::Register(3));
                    self.shuffle(1, 1, 0);
                    self.packed(Some(0x66), 0xfe, 0, 1);
                    self.packet_store(allocation, Register(*dst), 0);
                }
                Step::Uniform(dst, offset) => {
                    let target = match allocation.location(Register(*dst)) {
                        PacketPlace::Register(r) => r,
                        _ => 0,
                    };
                    self.instruction(
                        Some(0x66),
                        &[0x0f, 0x6e],
                        false,
                        target,
                        Operand::Memory {
                            base: 13,
                            offset: *offset,
                        },
                    );
                    self.shuffle(target, target, 0);
                    self.packet_from_word(*dst, target);
                    self.packet_store(allocation, Register(*dst), target);
                }
                Step::Copy(dst, src) => {
                    let d = allocation.location(*dst);
                    let s = allocation.location(*src);
                    if d != s {
                        match (d, s) {
                            (PacketPlace::Register(r), _) => self.packet_load(allocation, *src, r),
                            (_, PacketPlace::Register(r)) => self.packet_store(allocation, *dst, r),
                            _ => {
                                self.packet_load(allocation, *src, 0);
                                self.packet_store(allocation, *dst, 0);
                            }
                        }
                    }
                }
                Step::Load {
                    dst,
                    binding,
                    index,
                    word,
                } => {
                    let stride = self.program.buffers[*binding].stride;
                    let broadcast = matches!(index, Address::Uniform(_));
                    if !sparse || broadcast {
                        self.packet_address(allocation, *binding, index, 0);
                    }
                    let target = match allocation.location(Register(*dst)) {
                        PacketPlace::Register(r) => r,
                        _ => 0,
                    };
                    if !sparse && !broadcast && stride == 4 {
                        self.instruction(
                            None,
                            &[0x0f, 0x10],
                            false,
                            target,
                            Operand::Memory {
                                base: 15,
                                offset: word * 4,
                            },
                        );
                    } else {
                        for lane in 0..if broadcast { 1 } else { 4 } {
                            if sparse && !broadcast {
                                self.packet_address(allocation, *binding, index, lane);
                            }
                            self.instruction(
                                Some(0x66),
                                &[0x0f, 0x6e],
                                false,
                                if lane == 0 { target } else { lane as u8 },
                                Operand::Memory {
                                    base: 15,
                                    offset: word * 4 + if sparse { 0 } else { lane * stride },
                                },
                            );
                        }
                        if broadcast {
                            self.shuffle(target, target, 0);
                        } else {
                            self.packed(Some(0x66), 0x62, target, 1);
                            self.packed(Some(0x66), 0x62, 2, 3);
                            self.packed(Some(0x66), 0x6c, target, 2);
                        }
                    }
                    self.packet_from_word(*dst, target);
                    self.packet_store(allocation, Register(*dst), target);
                }
                Step::Store {
                    binding,
                    index,
                    word,
                    value,
                } => {
                    if !sparse {
                        self.packet_address(allocation, *binding, index, 0);
                    }
                    let stride = self.program.buffers[*binding].stride;
                    let source = match allocation.location(Register(*value)) {
                        PacketPlace::Register(r) => r,
                        PacketPlace::Stack(_) => {
                            self.packet_load(allocation, Register(*value), 1);
                            1
                        }
                    };
                    if !sparse && stride == 4 {
                        self.instruction(
                            None,
                            &[0x0f, 0x11],
                            false,
                            source,
                            Operand::Memory {
                                base: 15,
                                offset: word * 4,
                            },
                        );
                    } else {
                        for lane in 0..4 {
                            if sparse {
                                self.packet_address(allocation, *binding, index, lane);
                            }
                            if lane != 0 {
                                self.shuffle(0, source, (lane * 0x55) as u8);
                            }
                            self.instruction(
                                Some(0x66),
                                &[0x0f, 0x7e],
                                false,
                                if lane == 0 { source } else { 0 },
                                Operand::Memory {
                                    base: 15,
                                    offset: word * 4 + if sparse { 0 } else { lane * stride },
                                },
                            );
                        }
                    }
                }
                Step::Gather {
                    dst,
                    source,
                    index,
                    word,
                    broadcast,
                } => self.packet_gather(allocation, *dst, source, *index, *word, *broadcast),
                Step::Unary { dst, op, a } => {
                    let target = match allocation.location(Register(*dst)) {
                        PacketPlace::Register(r) => r,
                        _ => 0,
                    };
                    match op {
                        UnaryOp::Bitcast => self.packet_load(allocation, Register(*a), target),
                        UnaryOp::FloatNegate => {
                            self.packet_load(allocation, Register(*a), target);
                            self.packet_constant(1, 0x80000000);
                            self.packed(Some(0x66), 0xef, target, 1);
                        }
                        UnaryOp::SignedNegate | UnaryOp::UnsignedNegate => {
                            // 目标可能与输入同址，先保存减数再清零目标。
                            self.packet_load(allocation, Register(*a), 1);
                            self.packet_constant(target, 0);
                            self.packed(Some(0x66), 0xfa, target, 1);
                        }
                        UnaryOp::LogicalNot => {
                            self.packet_load(allocation, Register(*a), target);
                            self.packet_constant(1, 1);
                            self.packed(Some(0x66), 0xef, target, 1);
                        }
                        UnaryOp::BitNot => {
                            self.packet_load(allocation, Register(*a), target);
                            self.packet_constant(1, u32::MAX);
                            self.packed(Some(0x66), 0xef, target, 1);
                        }
                    }
                    self.packet_store(allocation, Register(*dst), target);
                }
                Step::Binary { dst, op, a, b } => match op {
                    Binary::Float(opcode) => {
                        self.packet_binary(allocation, *dst, *a, *b, None, *opcode)
                    }
                    Binary::Integer(opcode) => {
                        self.packet_binary(allocation, *dst, *a, *b, Some(0x66), *opcode)
                    }
                    Binary::Compare(op, scalar) => {
                        self.packet_compare(allocation, *dst, *a, *b, *op, *scalar);
                    }
                    Binary::Shift(opcode) => {
                        let target = match allocation.location(Register(*dst)) {
                            PacketPlace::Register(r) => r,
                            _ => 0,
                        };
                        // 先取得计数，再覆盖可能与右值共用的目标位置。
                        self.packet_load(allocation, Register(*b), 1);
                        self.instruction(Some(0x66), &[0x0f, 0x7e], false, 1, Operand::Register(0));
                        self.bytes(&[0x83, 0xe0, 0x1f]);
                        self.instruction(Some(0x66), &[0x0f, 0x6e], false, 1, Operand::Register(0));
                        self.packet_load(allocation, Register(*a), target);
                        self.packed(Some(0x66), *opcode, target, 1);
                        self.packet_store(allocation, Register(*dst), target);
                    }
                    Binary::ShiftEach(kind) => {
                        self.packet_shift_values(allocation, *dst, *a, *b, *kind)
                    }
                    Binary::Multiply => {
                        self.packet_load(allocation, Register(*a), 0);
                        self.packet_load(allocation, Register(*b), 1);
                        self.packet_multiply(false);
                        self.packet_store(allocation, Register(*dst), 0);
                    }
                },
                Step::IntegerDivide {
                    result,
                    src,
                    divisor,
                } => match divisor {
                    IntegerDivisor::Constant(divisor) => {
                        self.packet_divide(allocation, *result, *src, *divisor)
                    }
                    IntegerDivisor::Value(divisor) => {
                        self.packet_divide_value(allocation, *result, *src, *divisor)
                    }
                },
                Step::Cast {
                    dst,
                    src,
                    from,
                    into,
                } => {
                    self.packet_load(allocation, Register(*src), 0);
                    self.packet_cast(*from, *into);
                    self.packet_store(allocation, Register(*dst), 0);
                }
                Step::Math { dst, op, args } => match op {
                    Math::Min => self.packet_binary(allocation, *dst, args[0], args[1], None, 0x5d),
                    Math::Max => self.packet_binary(allocation, *dst, args[0], args[1], None, 0x5f),
                    Math::Sqrt => {
                        let target = match allocation.location(Register(*dst)) {
                            PacketPlace::Register(r) => r,
                            _ => 0,
                        };
                        self.instruction(
                            None,
                            &[0x0f, 0x51],
                            false,
                            target,
                            allocation.location(Register(args[0])).into(),
                        );
                        self.packet_store(allocation, Register(*dst), target);
                    }
                    Math::Abs => {
                        let target = match allocation.location(Register(*dst)) {
                            PacketPlace::Register(r) => r,
                            _ => 0,
                        };
                        self.packet_load(allocation, Register(args[0]), target);
                        self.packet_constant(1, 0x7fffffff);
                        self.packed(Some(0x66), 0xdb, target, 1);
                        self.packet_store(allocation, Register(*dst), target);
                    }
                },
                Step::Select {
                    dst,
                    condition,
                    a,
                    b,
                } => {
                    let target = match allocation.location(Register(*dst)) {
                        PacketPlace::Register(r) => r,
                        _ => 0,
                    };
                    // 先保留假值；掩码若覆盖真值位置，还须先保存真值。
                    self.packet_load(allocation, Register(*b), 1);
                    let a = allocation.location(Register(*a));
                    let source = if a == PacketPlace::Register(target) {
                        self.instruction(None, &[0x0f, 0x28], false, 2, a.into());
                        Operand::Register(2)
                    } else {
                        a.into()
                    };
                    self.packet_load(allocation, Register(*condition), target);
                    self.packet_constant(3, 0);
                    self.packed(Some(0x66), 0x76, target, 3);
                    self.packed(Some(0x66), 0xdb, 1, target);
                    self.instruction(Some(0x66), &[0x0f, 0xdf], false, target, source);
                    self.packed(Some(0x66), 0xeb, target, 1);
                    self.packet_store(allocation, Register(*dst), target);
                }
            }
        }
        if !sparse {
            self.bytes(&[0x83, 0xc3, 0x04]);
        }
        let again = self.jump(None);
        self.patch(again, begin);
        self.patch(tail, self.code.len());
        if sparse {
            self.bytes(&[0x85, 0xc9]);
            let empty = self.jump(Some(0x84));
            self.instruction(
                None,
                &[0x8b],
                false,
                3,
                Operand::Memory {
                    base: 4,
                    offset: self.frame.end as u32,
                },
            );
            self.patch(empty, self.code.len());
        }
    }
}
