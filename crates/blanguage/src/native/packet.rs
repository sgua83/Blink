//! 四个独立元素的 SSE2 降低；计划只包含已证明的访问与可表达指令，尾部共用标量 IR。
use super::*;
use allocation::{PacketAllocation, PacketPlace};
use operands::Operand;
use std::collections::BTreeSet;
#[path="packet/control.rs"]
mod control;
#[path="packet/integer.rs"]
mod integer;
#[path="packet/memory.rs"]
mod memory;

#[derive(Clone, Copy)]
#[repr(u32)]
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
        stride: ElementStride,
        uniform: bool,
    },
}
// The temporary lowering step borrows its original instruction; it owns no source copy.
struct Step<'a> {
    instruction: &'a Inst,
    kind: StepKind<'a>,
}
enum StepKind<'a> {
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
        op: BinaryOp,
        a: Reg,
        b: Reg,
        uniform_count: bool,
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
        args: &'a [Reg],
    },
    Select {
        dst: Reg,
        condition: Reg,
        a: Reg,
        b: Reg,
    },
    If {
        condition: Reg,
        yes: Vec<Step<'a>>,
        no: Vec<Step<'a>>,
    },
    Loop {
        condition: Vec<Step<'a>>,
        test: Reg,
        body: Vec<Step<'a>>,
    },
    Scope(Vec<Step<'a>>),
    Exit,
    Break,
    Continue,
}
pub(super) struct Plan<'a> {
    code: Vec<Step<'a>>,
    pub address_only: BTreeSet<Reg>,
}
impl<'a> Plan<'a> {
    pub fn new(program: &Program, stage: &'a Stage) -> Option<Self> {
        if stage.has_tile_scope() || stage.writes.is_empty()
            || stage.registers.iter().chain(stage.locals.values().flat_map(|ty| ty.components.iter().map(|(scalar, _)| scalar))).any(|scalar| scalar.size() != 4)
        {
            return None;
        }
        let uniform = control::uniform_registers(stage);
        let mut indices = BTreeSet::new();
        control::visit(&stage.code, &mut |inst| {
            if let InstKind::Index(dst) = &(inst).kind {
                indices.insert(*dst);
            }
        });
        let mut code = Self::block(program, stage, &stage.code, &uniform, &indices)?;
        let mut values = BTreeSet::new();
        control::visit(&stage.code, &mut |inst| match &(inst).kind {
            InstKind::Load { .. } => {}
            InstKind::Store { value, .. } => {
                values.insert(*value);
            }
            _ => { inst.all_operands(&mut |reg| { values.insert(reg); true }); }
        });
        let address_only = indices.difference(&values).copied().collect();
        control::remove_address_indices(&mut code, &address_only);
        Some(Self { code, address_only })
    }

    fn block(
        program: &Program,
        stage: &Stage,
        instructions: &'a [Inst],
        uniform: &[bool],
        indices: &BTreeSet<Reg>,
    ) -> Option<Vec<Step<'a>>> {
        let mut code = Vec::new();
        for inst in instructions {
            let step = match &(inst).kind {
                InstKind::Constant(dst, _, bits) => StepKind::Constant(*dst, *bits as u32),
                InstKind::Index(dst) => StepKind::Index(*dst),
                InstKind::Uniform(dst, offset) => StepKind::Uniform(*dst, *offset),
                InstKind::LoadLocal(dst, slot) => {
                    StepKind::Copy(PrivateSlot::Register(*dst), PrivateSlot::Local(*slot))
                }
                InstKind::StoreLocal(slot, src) => {
                    StepKind::Copy(PrivateSlot::Local(*slot), PrivateSlot::Register(*src))
                }
                InstKind::Load {
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
                    if broadcast || element {
                        StepKind::Load {
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
                        StepKind::Gather {
                            dst: *dst,
                            source: ReadSource::Dataset(*binding),
                            index: *index,
                            word: *word,
                            broadcast: false,
                        }
                    }
                }
                InstKind::ViewLoad {
                    dst,
                    address,
                    index,
                    stride,
                    word,
                } => {
                    if *word as u64 * 4 > i32::MAX as u64 {
                        return None;
                    }
                    let same_address = uniform[address[0]] && uniform[address[1]];
                    let broadcast = same_address && uniform[*index] && stride.all_operands(&mut |reg| uniform[reg]);
                    StepKind::Gather {
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
                InstKind::Store {
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
                    StepKind::Store {
                        binding: *binding,
                        index: Address::Element,
                        word: *word,
                        value: *value,
                    }
                }
                InstKind::Unary { dst, op, a } => StepKind::Unary {
                    dst: *dst,
                    op: *op,
                    a: *a,
                },
                InstKind::Binary { dst, op, a, b } => StepKind::Binary {
                    dst: *dst, op: *op, a: *a, b: *b, uniform_count: uniform[*b],
                },
                InstKind::IntegerDivide {
                    result,
                    src,
                    divisor,
                } => StepKind::IntegerDivide {
                    result: *result,
                    src: *src,
                    divisor: *divisor,
                },
                InstKind::Cast { dst, src } => {
                    let from = stage.registers[*src];
                    let into = stage.registers[*dst];
                    if operands::bit_copy(from, into) {
                        StepKind::Copy(PrivateSlot::Register(*dst), PrivateSlot::Register(*src))
                    } else {
                        StepKind::Cast {
                            dst: *dst,
                            src: *src,
                            from,
                            into,
                        }
                    }
                }
                InstKind::Math { dst, op, args } => {
                    let op = match op {
                        MathOp::Sqrt => Math::Sqrt,
                        MathOp::Abs => Math::Abs,
                        MathOp::Min => Math::Min,
                        MathOp::Max => Math::Max,
                        _ => return None,
                    };
                    StepKind::Math {
                        dst: *dst,
                        op,
                        args,
                    }
                }
                InstKind::Select {
                    dst,
                    condition,
                    a,
                    b,
                } => StepKind::Select {
                    dst: *dst,
                    condition: *condition,
                    a: *a,
                    b: *b,
                },
                InstKind::If { condition, yes, no } => {
                    if !uniform[*condition] {
                        return None;
                    }
                    StepKind::If {
                        condition: *condition,
                        yes: Self::block(program, stage, yes, uniform, indices)?,
                        no: Self::block(program, stage, no, uniform, indices)?,
                    }
                }
                InstKind::Loop {
                    condition,
                    test,
                    body,
                } => {
                    if !uniform[*test] {
                        return None;
                    }
                    StepKind::Loop {
                        condition: Self::block(program, stage, condition, uniform, indices)?,
                        test: *test,
                        body: Self::block(program, stage, body, uniform, indices)?,
                    }
                }
                InstKind::Scope(body) => {
                    StepKind::Scope(Self::block(program, stage, body, uniform, indices)?)
                }
                InstKind::Exit => StepKind::Exit,
                InstKind::Break => StepKind::Break,
                InstKind::Continue => StepKind::Continue,
                _ => return None,
            };
            code.push(Step { instruction: inst, kind: step });
        }
        Some(code)
    }
}
impl From<PacketPlace> for Operand {
    fn from(place: PacketPlace) -> Self {
        match place {
            PacketPlace::Register(r) => Operand::Register(r),
            PacketPlace::Stack(offset) => Operand::memory(4, offset),
        }
    }
}
impl From<PacketPlace> for operands::MoveLocation {
    fn from(place: PacketPlace) -> Self {
        match place {
            PacketPlace::Register(reg) => allocation::Location::Xmm(reg).into(),
            PacketPlace::Stack(offset) => operands::Memory::new(4, offset).into(),
        }
    }
}
unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::packet_constant"]
    fn encode_packet_constant(dst: u8, bits: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_load"]
    fn encode_packet_load(src: operands::MoveLocation, dst: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_store"]
    fn encode_packet_store(dst: operands::MoveLocation, src: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_copy"]
    fn encode_packet_copy(dst: operands::MoveLocation, src: operands::MoveLocation, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_from_word"]
    fn encode_packet_from_word(boolean: u32, dst: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_multiply"]
    fn encode_packet_multiply(broadcast: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_binary"]
    fn encode_packet_binary(dst: operands::MoveLocation, left: operands::MoveLocation, right: operands::MoveLocation,
        scalar: crate::builtin_types::ScalarEncoding, operation: crate::builtin_types::ScalarBinaryOp,
        uniform_count: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_unary"]
    fn encode_packet_unary(dst: operands::MoveLocation, src: operands::MoveLocation, operation: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_cast"]
    fn encode_packet_cast(dst: operands::MoveLocation, src: operands::MoveLocation,
        from: crate::builtin_types::ScalarEncoding, into: crate::builtin_types::ScalarEncoding, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_math"]
    fn encode_packet_math(dst: operands::MoveLocation, left: operands::MoveLocation, right: operands::MoveLocation,
        operation: Math, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_select"]
    fn encode_packet_select(dst: operands::MoveLocation, condition: operands::MoveLocation,
        left: operands::MoveLocation, right: operands::MoveLocation, output: *mut u8) -> u64;
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
        self.emit_bytes(24, |output| unsafe { encode_packet_constant(xmm, bits, output) });
    }
    fn packet_load(&mut self, allocation: &PacketAllocation, slot: PrivateSlot, xmm: u8) {
        let source = allocation.location(slot).into();
        self.emit_bytes(16, |output| unsafe { encode_packet_load(source, xmm, output) });
    }
    fn packet_store(&mut self, allocation: &PacketAllocation, slot: PrivateSlot, xmm: u8) {
        let destination = allocation.location(slot).into();
        self.emit_bytes(16, |output| unsafe { encode_packet_store(destination, xmm, output) });
    }
    fn packet_from_word(&mut self, dst: Reg, xmm: u8) {
        let boolean = u32::from(self.stage.registers[dst] == Scalar::Bool);
        self.emit_bytes(64, |output| unsafe { encode_packet_from_word(boolean, xmm, output) });
    }
    pub(super) fn packet_loop(&mut self, plan: &Plan<'_>, allocation: &PacketAllocation) {
        let begin = self.code.len();
        let sparse = self.stage.selection.is_some();
        let tail = if let Some(selection) = self.stage.selection {
            // 四个真实选中位置只在本范围栈帧内存在，不物化全域稀疏索引。
            self.bytes(&[0x31, 0xc9]); // ecx = 已收集 lane 数
            let collect = self.code.len();
            self.bytes(&[0x44, 0x39, 0xf3]);
            let tail = self.jump(Some(0x83));
            self.bytes(&[0x4d, 0x8b, 0xbc, 0x24]);
            self.u32(selection.binding as u32 * BUFFER_BYTES);
            self.bytes(&[0x89, 0xd8]);
            let memory = self.buffer_memory(selection.binding, 0, selection.word);
            self.instruction(None, &[0x83], false, 7, memory.into());
            self.bytes(&[0]);
            let skip = self.jump(Some(0x84));
            self.bytes(&[0x89, 0x9c, 0x8c]); // indices[ecx] = ebx
            self.u32(self.frame.end as u32);
            self.bytes(&[0xff, 0xc1]);
            self.patch(skip, self.code.len());
            self.bytes(&[0xff, 0xc3, 0x83, 0xf9, 0x04]);
            let again = self.jump(Some(0x85));
            self.patch(again, collect);
            self.parallel_abi(begin, crate::native::ParallelRangePart::PacketCollect { binding: selection.binding, offset: selection.word });
            tail
        } else {
            // 用 64 位检查 index+4；u32 域末端附近也不让加法提前回绕。
            self.bytes(&[0x89, 0xd8, 0x48, 0x83, 0xc0, 0x04, 0x4c, 0x39, 0xf0]);
            let tail = self.jump(Some(0x87));
            self.parallel_abi(begin, crate::native::ParallelRangePart::PacketBounds);
            tail
        };
        self.packet_block(&plan.code, allocation);
        if !sparse {
            let start = self.code.len();
            self.bytes(&[0x83, 0xc3, 0x04]);
            self.parallel_abi(start, crate::native::ParallelRangePart::PacketAdvance);
        }
        let start = self.code.len();
        let again = self.jump(None);
        self.parallel_abi(start, crate::native::ParallelRangePart::PacketBackEdge);
        self.patch(again, begin);
        self.patch(tail, self.code.len());
        if sparse {
            let start = self.code.len();
            self.bytes(&[0x85, 0xc9]);
            let empty = self.jump(Some(0x84));
            self.instruction(
                None,
                &[0x8b],
                false,
                3,
                Operand::memory(4, self.frame.end as u32),
            );
            self.patch(empty, self.code.len());
            self.parallel_abi(start, crate::native::ParallelRangePart::PacketTail);
        }
    }

    fn packet_block(&mut self, code: &[Step<'_>], allocation: &PacketAllocation) {
        use PrivateSlot::Register;
        let sparse = self.stage.selection.is_some();
        for step in code {
            let start = self.code.len();
            let first = self.operation_ranges.as_ref().map(Vec::len);
            match &step.kind {
                StepKind::If { condition, yes, no } => {
                    self.packet_lane(allocation, *condition, 0, 0);
                    self.bytes(&[0x85, 0xc0]);
                    let other = self.jump(Some(0x84));
                    self.packet_block(yes, allocation);
                    let end = self.jump(None);
                    self.patch(other, self.code.len());
                    self.packet_block(no, allocation);
                    self.patch(end, self.code.len());
                }
                StepKind::Loop {
                    condition,
                    test,
                    body,
                } => {
                    let begin = self.code.len();
                    self.packet_block(condition, allocation);
                    self.packet_lane(allocation, *test, 0, 0);
                    self.bytes(&[0x85, 0xc0]);
                    let end = self.jump(Some(0x84));
                    self.breaks.push(Vec::new());
                    self.continues.push(begin);
                    self.packet_block(body, allocation);
                    self.continues.pop();
                    let back = self.jump(None);
                    self.patch(back, begin);
                    let target = self.code.len();
                    self.patch(end, target);
                    for at in self.breaks.pop().unwrap() {
                        self.patch(at, target);
                    }
                }
                StepKind::Scope(body) => {
                    self.exits.push(Vec::new());
                    self.packet_block(body, allocation);
                    let target = self.code.len();
                    for at in self.exits.pop().unwrap() {
                        self.patch(at, target);
                    }
                }
                StepKind::Exit => {
                    let at = self.jump(None);
                    self.exits.last_mut().unwrap().push(at);
                }
                StepKind::Break => {
                    let at = self.jump(None);
                    self.breaks.last_mut().unwrap().push(at);
                }
                StepKind::Continue => {
                    let at = self.jump(None);
                    self.patch(at, *self.continues.last().unwrap());
                }
                StepKind::Constant(dst, bits) => {
                    let target = match allocation.location(Register(*dst)) {
                        PacketPlace::Register(r) => r,
                        _ => 0,
                    };
                    self.packet_constant(target, *bits);
                    self.packet_store(allocation, Register(*dst), target);
                }
                StepKind::Index(dst) => {
                    if sparse {
                        self.instruction(
                            None,
                            &[0x0f, 0x28],
                            false,
                            0,
                            Operand::memory(4, self.frame.end as u32),
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
                StepKind::Uniform(dst, offset) => {
                    let target = match allocation.location(Register(*dst)) {
                        PacketPlace::Register(r) => r,
                        _ => 0,
                    };
                    self.instruction(
                        Some(0x66),
                        &[0x0f, 0x6e],
                        false,
                        target,
                        Operand::memory(13, *offset),
                    );
                    self.shuffle(target, target, 0);
                    self.packet_from_word(*dst, target);
                    self.packet_store(allocation, Register(*dst), target);
                }
                StepKind::Copy(dst, src) => {
                    let destination = allocation.location(*dst).into();
                    let source = allocation.location(*src).into();
                    self.emit_bytes(32, |output| unsafe { encode_packet_copy(destination, source, output) });
                }
                StepKind::Load { dst, binding, index, word } =>
                    self.packet_read(allocation, *dst, *binding, index, *word),
                StepKind::Store { binding, index, word, value } =>
                    self.packet_write(allocation, *value, *binding, index, *word),
                StepKind::Gather {
                    dst,
                    source,
                    index,
                    word,
                    broadcast,
                } => self.packet_gather(allocation, *dst, source, *index, *word, *broadcast),
                StepKind::Unary { dst, op, a } => {
                    let destination = allocation.location(Register(*dst)).into();
                    let source = allocation.location(Register(*a)).into();
                    self.emit_bytes(256, |output| unsafe { encode_packet_unary(destination, source, *op as u32, output) });
                }
                StepKind::Binary { dst, op, a, b, uniform_count } => {
                    let destination = allocation.location(Register(*dst)).into();
                    let left = allocation.location(Register(*a)).into();
                    let right = allocation.location(Register(*b)).into();
                    let scalar = self.stage.registers[*a].into();
                    self.emit_bytes(256, |output| unsafe {
                        encode_packet_binary(destination, left, right, scalar, (*op).into(), u32::from(*uniform_count), output)
                    });
                }
                StepKind::IntegerDivide {
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
                StepKind::Cast { dst, src, from, into } => {
                    let destination = allocation.location(Register(*dst)).into();
                    let source = allocation.location(Register(*src)).into();
                    self.emit_bytes(256, |output| unsafe {
                        encode_packet_cast(destination, source, (*from).into(), (*into).into(), output)
                    });
                }
                StepKind::Math { dst, op, args } => {
                    let destination = allocation.location(Register(*dst)).into();
                    let left = allocation.location(Register(args[0])).into();
                    let right = allocation.location(Register(*args.last().unwrap())).into();
                    self.emit_bytes(256, |output| unsafe { encode_packet_math(destination, left, right, *op, output) });
                }
                StepKind::Select { dst, condition, a, b } => {
                    let destination = allocation.location(Register(*dst)).into();
                    let condition = allocation.location(Register(*condition)).into();
                    let left = allocation.location(Register(*a)).into();
                    let right = allocation.location(Register(*b)).into();
                    self.emit_bytes(256, |output| unsafe { encode_packet_select(destination, condition, left, right, output) });
                }
            }
            self.operation_range(step.instruction, start, first);
        }
    }
}
