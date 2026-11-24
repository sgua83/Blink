//! 四个独立元素的 SSE2 降低；计划只包含已证明的访问与可表达指令，尾部共用标量 IR。
use super::*;
use allocation::{PacketAllocation, PacketPlace};
use memory::{PacketAddress, PacketViewAddress};
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
        index: Reg,
        word: u32,
        mode: PacketAddress,
    },
    ViewLoad {
        dst: Reg,
        address: [Reg; 2],
        stride: ElementStride,
        index: Reg,
        word: u32,
        mode: PacketViewAddress,
    },
    Store {
        binding: usize,
        word: u32,
        value: Reg,
        mode: PacketAddress,
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
    pub fn new(program: &Program, stage: &'a Stage) -> Option<Self> { control::run(program, stage) }
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
    #[link_name = "bootstrap.x64_instruction::emit_packet_body"]
    fn emit_packet_body(context: PacketBodySlot, count: usize, sparse: u32, slots: u32, host: PacketBodyHost);
}

#[repr(u32)]
enum PacketBodyKind {
    Done,
    Failed,
    Constant,
    Index,
    Uniform,
    Copy,
    Unary,
    Binary,
    Cast,
    Math,
    Select,
}

#[repr(C)]
struct PacketBodyOp {
    places: [operands::MoveLocation; 4],
    from: crate::builtin_types::ScalarEncoding,
    into: crate::builtin_types::ScalarEncoding,
    kind: PacketBodyKind,
    literal: u32,
    flag: u32,
    unary: u32,
    binary: crate::builtin_types::ScalarBinaryOp,
    math: Math,
}

impl PacketBodyOp {
    fn new(kind: PacketBodyKind) -> Self {
        Self {
            places: std::array::from_fn(|_| allocation::Location::Xmm(0).into()),
            from: Scalar::U32.into(),
            into: Scalar::U32.into(),
            kind,
            literal: 0,
            flag: 0,
            unary: 0,
            binary: crate::builtin_types::ScalarBinaryOp::Add,
            math: Math::Sqrt,
        }
    }
}

type PacketBodySlot = *mut std::ffi::c_void;

#[repr(C)]
struct PacketBodySpan {
    start: usize,
    first: usize,
    observed: u32,
}

#[repr(C)]
struct PacketBodyHost {
    step: unsafe extern "C" fn(PacketBodySlot, usize) -> PacketBodyOp,
    span: unsafe extern "C" fn(PacketBodySlot) -> PacketBodySpan,
    range: unsafe extern "C" fn(PacketBodySlot, usize, PacketBodySpan),
    reserve: unsafe extern "C" fn(PacketBodySlot, usize) -> *mut u8,
    finish: unsafe extern "C" fn(PacketBodySlot, usize),
}

struct PacketBody<'a, 'b, 'c> {
    emitter: &'a mut Emitter<'b>,
    allocation: &'a PacketAllocation,
    code: &'a [Step<'c>],
    result: Result<(), &'static str>,
}

unsafe fn packet_body<'a, 'b, 'c, 'd>(slot: PacketBodySlot) -> &'a mut PacketBody<'b, 'c, 'd> {
    unsafe { &mut *slot.cast() }
}

unsafe extern "C" fn packet_body_span(slot: PacketBodySlot) -> PacketBodySpan {
    let emitter = &unsafe { packet_body(slot) }.emitter;
    let count = emitter.operation_ranges.as_ref().map(Vec::len);
    PacketBodySpan {
        start: emitter.code.len(),
        first: count.unwrap_or(0),
        observed: u32::from(count.is_some()),
    }
}

unsafe extern "C" fn packet_body_range(slot: PacketBodySlot, index: usize, span: PacketBodySpan) {
    let context = unsafe { packet_body(slot) };
    context.emitter.operation_range(
        context.code[index].instruction, span.start, (span.observed != 0).then_some(span.first),
    );
}

unsafe extern "C" fn reserve_packet_bytes(slot: PacketBodySlot, count: usize) -> *mut u8 {
    let code = &mut unsafe { packet_body(slot) }.emitter.code;
    code.reserve(count);
    unsafe { code.as_mut_ptr().add(code.len()) }
}

unsafe extern "C" fn finish_packet_bytes(slot: PacketBodySlot, count: usize) {
    let code = &mut unsafe { packet_body(slot) }.emitter.code;
    unsafe { code.set_len(code.len() + count) }
}

unsafe extern "C" fn packet_body_step(slot: PacketBodySlot, index: usize) -> PacketBodyOp {
    use PacketBodyKind as Kind;
    use PrivateSlot::Register;
    let context = unsafe { packet_body(slot) };
    let allocation = context.allocation;
    let emitter = &mut context.emitter;
    let place = |value| allocation.location(value).into();
    let register = |value| place(Register(value));
    let emit = |e: &mut Emitter<'_>, block: &[Step<'_>]| e.packet_block(block, allocation);
    let load = |e: &mut Emitter<'_>, reg| e.packet_lane(allocation, reg, 0, 0);
    let flow = match &context.code[index].kind {
        StepKind::If { condition, yes, no } => Flow::If(*condition, yes, no),
        StepKind::Loop { condition, test, body } => Flow::Loop(condition, *test, body),
        StepKind::Scope(body) => Flow::Scope(body),
        StepKind::Exit => Flow::Exit,
        StepKind::Break => Flow::Break,
        StepKind::Continue => Flow::Continue,
        StepKind::Load { dst, binding, index, word, mode } => {
            emitter.packet_read(allocation, *dst, *binding, *index, *word, *mode);
            return PacketBodyOp::new(Kind::Done);
        }
        StepKind::Store { binding, word, value, mode } => {
            emitter.packet_write(allocation, *value, *binding, *word, *mode);
            return PacketBodyOp::new(Kind::Done);
        }
        StepKind::ViewLoad { dst, address, stride, index, word, mode } => {
            emitter.packet_view(allocation, *dst, *address, *stride, *index, *word, *mode);
            return PacketBodyOp::new(Kind::Done);
        }
        StepKind::IntegerDivide { result, src, divisor } => {
            match divisor {
                IntegerDivisor::Constant(divisor) => emitter.packet_divide(allocation, *result, *src, *divisor),
                IntegerDivisor::Value(divisor) => emitter.packet_divide_value(allocation, *result, *src, *divisor),
            }
            return PacketBodyOp::new(Kind::Done);
        }
        StepKind::Constant(dst, bits) => {
            let mut op = PacketBodyOp::new(Kind::Constant);
            op.places[0] = register(*dst);
            op.literal = *bits;
            return op;
        }
        StepKind::Index(dst) => {
            let mut op = PacketBodyOp::new(Kind::Index);
            op.places[0] = register(*dst);
            return op;
        }
        StepKind::Uniform(dst, offset) => {
            let mut op = PacketBodyOp::new(Kind::Uniform);
            op.places[0] = register(*dst);
            op.literal = *offset;
            op.flag = u32::from(emitter.stage.registers[*dst] == Scalar::Bool);
            return op;
        }
        StepKind::Copy(dst, src) => {
            let mut op = PacketBodyOp::new(Kind::Copy);
            op.places[0] = place(*dst);
            op.places[1] = place(*src);
            return op;
        }
        StepKind::Unary { dst, op: source, a } => {
            let mut op = PacketBodyOp::new(Kind::Unary);
            op.places[0] = register(*dst);
            op.places[1] = register(*a);
            op.unary = *source as u32;
            return op;
        }
        StepKind::Binary { dst, op: source, a, b, uniform_count } => {
            let mut op = PacketBodyOp::new(Kind::Binary);
            op.places[0] = register(*dst);
            op.places[1] = register(*a);
            op.places[2] = register(*b);
            op.from = emitter.stage.registers[*a].into();
            op.binary = (*source).into();
            op.flag = u32::from(*uniform_count);
            return op;
        }
        StepKind::Cast { dst, src, from, into } => {
            let mut op = PacketBodyOp::new(Kind::Cast);
            op.places[0] = register(*dst);
            op.places[1] = register(*src);
            op.from = (*from).into();
            op.into = (*into).into();
            return op;
        }
        StepKind::Math { dst, op: source, args } => {
            let mut op = PacketBodyOp::new(Kind::Math);
            op.places[0] = register(*dst);
            op.places[1] = register(args[0]);
            op.places[2] = register(*args.last().unwrap());
            op.math = *source;
            return op;
        }
        StepKind::Select { dst, condition, a, b } => {
            let mut op = PacketBodyOp::new(Kind::Select);
            op.places = [register(*dst), register(*condition), register(*a), register(*b)];
            return op;
        }
    };
    context.result = emitter.lower_control(flow, emit, load);
    PacketBodyOp::new(if context.result.is_ok() { Kind::Done } else { Kind::Failed })
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
        let start = self.code.len();
        let back_edge = self.finish_range(begin, tail, 0, if sparse { 0 } else { 4 });
        if back_edge != start {
            self.parallel_abi_range(start..back_edge, crate::native::ParallelRangePart::PacketAdvance);
        }
        self.parallel_abi(back_edge, crate::native::ParallelRangePart::PacketBackEdge);
        if sparse {
            let start = self.code.len();
            let slots = self.frame.end as u32;
            self.emit_bytes(16, |output| unsafe { encode_sparse_tail(slots, output) });
            self.parallel_abi(start, crate::native::ParallelRangePart::PacketTail);
        }
        Ok(())
    }

    fn packet_block(&mut self, code: &[Step<'_>], allocation: &PacketAllocation) -> Result<(), &'static str> {
        let sparse = u32::from(self.stage.selection.is_some());
        let slots = self.frame.end as u32;
        let mut context = PacketBody { emitter: self, allocation, code, result: Ok(()) };
        unsafe {
            emit_packet_body(
                std::ptr::from_mut(&mut context).cast(), code.len(), sparse, slots,
                PacketBodyHost {
                    step: packet_body_step,
                    span: packet_body_span,
                    range: packet_body_range,
                    reserve: reserve_packet_bytes,
                    finish: finish_packet_bytes,
                },
            );
        }
        context.result
    }
}
