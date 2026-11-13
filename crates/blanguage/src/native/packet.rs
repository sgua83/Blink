//! 四个独立元素的 SSE2 降低；计划只包含已证明的访问与可表达指令，尾部共用标量 IR。
use super::*;
use allocation::{PacketAllocation, PacketPlace};
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
impl From<PacketPlace> for operands::MoveLocation {
    fn from(place: PacketPlace) -> Self {
        match place {
            PacketPlace::Register(reg) => allocation::Location::Xmm(reg).into(),
            PacketPlace::Stack(offset) => operands::Memory::new(4, offset).into(),
        }
    }
}
#[repr(C)]
struct SparseRangeEncoding {
    length: u64,
    tail_field: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::sparse_collect"]
    fn encode_sparse_collect(data_offset: u32, stride_offset: u32, word: u32, slots: u32,
        output: *mut u8) -> SparseRangeEncoding;
    #[link_name = "bootstrap.x64_instruction::sparse_tail"]
    fn encode_sparse_tail(slots: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_constant"]
    fn encode_packet_constant(dst: operands::MoveLocation, bits: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_index"]
    fn encode_packet_index(dst: operands::MoveLocation, sparse: u32, slots: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_uniform"]
    fn encode_packet_uniform(dst: operands::MoveLocation, offset: u32, boolean: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_copy"]
    fn encode_packet_copy(dst: operands::MoveLocation, src: operands::MoveLocation, output: *mut u8) -> u64;
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
    pub(super) fn packet_loop(&mut self, plan: &Plan<'_>, allocation: &PacketAllocation) -> Result<(), &'static str> {
        let begin = self.code.len();
        let sparse = self.stage.selection.is_some();
        let tail = if let Some(selection) = self.stage.selection {
            let data = selection.binding as u32 * BUFFER_BYTES;
            self.code.reserve(96);
            let encoding = unsafe {
                let encoding = encode_sparse_collect(data, data + BUFFER_STRIDE, selection.word,
                    self.frame.end as u32, self.code.as_mut_ptr().add(begin));
                self.code.set_len(begin + encoding.length as usize);
                encoding
            };
            self.parallel_abi(begin, crate::native::ParallelRangePart::PacketCollect { binding: selection.binding, offset: selection.word });
            begin + encoding.tail_field as usize
        } else {
            let tail = self.range_bounds(true);
            self.parallel_abi(begin, crate::native::ParallelRangePart::PacketBounds);
            tail
        };
        self.packet_block(&plan.code, allocation)?;
        if !sparse {
            let start = self.code.len();
            self.range_advance(4);
            self.parallel_abi(start, crate::native::ParallelRangePart::PacketAdvance);
        }
        let start = self.code.len();
        self.jump_to(begin);
        self.parallel_abi(start, crate::native::ParallelRangePart::PacketBackEdge);
        self.patch(tail, self.code.len());
        if sparse {
            let start = self.code.len();
            let slots = self.frame.end as u32;
            self.emit_bytes(16, |output| unsafe { encode_sparse_tail(slots, output) });
            self.parallel_abi(start, crate::native::ParallelRangePart::PacketTail);
        }
        Ok(())
    }

    fn packet_block(&mut self, code: &[Step<'_>], allocation: &PacketAllocation) -> Result<(), &'static str> {
        use PrivateSlot::Register;
        let sparse = self.stage.selection.is_some();
        for step in code {
            let start = self.code.len();
            let first = self.operation_ranges.as_ref().map(Vec::len);
            match &step.kind {
                StepKind::If { condition, yes, no } => {
                    self.packet_lane(allocation, *condition, 0, 0);
                    self.emit_if(yes, no, |e, body| e.packet_block(body, allocation))?;
                }
                StepKind::Loop { condition, test, body } => {
                    self.emit_loop(condition, body, |e, code| e.packet_block(code, allocation),
                        |e| e.packet_lane(allocation, *test, 0, 0))?;
                }
                StepKind::Scope(body) => self.emit_scope(body, |e, code| e.packet_block(code, allocation))?,
                StepKind::Exit => {
                    let at = self.jump(Branch::Always);
                    self.exits.last_mut().unwrap().push(at);
                }
                StepKind::Break => {
                    let at = self.jump(Branch::Always);
                    self.breaks.last_mut().unwrap().push(at);
                }
                StepKind::Continue => {
                    self.jump_to(*self.continues.last().unwrap());
                }
                StepKind::Constant(dst, bits) => {
                    let location = allocation.location(Register(*dst)).into();
                    self.emit_bytes(24, |output| unsafe { encode_packet_constant(location, *bits, output) });
                }
                StepKind::Index(dst) => {
                    let location = allocation.location(Register(*dst)).into();
                    let slots = self.frame.end as u32;
                    self.emit_bytes(48, |output| unsafe {
                        encode_packet_index(location, u32::from(sparse), slots, output)
                    });
                }
                StepKind::Uniform(dst, offset) => {
                    let location = allocation.location(Register(*dst)).into();
                    let boolean = u32::from(self.stage.registers[*dst] == Scalar::Bool);
                    self.emit_bytes(64, |output| unsafe {
                        encode_packet_uniform(location, *offset, boolean, output)
                    });
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
        Ok(())
    }
}
