//! 四个独立元素的 SSE2 降低；计划只包含已证明的访问与可表达指令，尾部共用标量 IR。
use super::*;
use allocation::{PacketAllocation, PacketPlace};
use memory::{PacketAddress, PacketViewAddress};
use super::operands::ScalarValueHost;
use crate::builtin_types::ScalarEncoding;
use crate::ir::dataflow::HOST;
use std::collections::BTreeSet;
#[path="packet/control.rs"]
mod control;
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
    IntegerDivide,
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
    pub fn new(program: &Program, stage: &'a Stage, tile_scope: bool) -> Option<Self> {
        control::run(program, stage, tile_scope)
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
    IntegerDivide,
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
    instruction: *const Inst,
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
            instruction: std::ptr::null(),
        }
    }
}

type PacketBodySlot = *mut std::ffi::c_void;

#[repr(C)]
struct PacketBodySpan {
    start: usize,
    first: usize,
}

#[repr(C)]
struct PacketBodyHost {
    step: unsafe extern "C" fn(PacketBodySlot, usize) -> PacketBodyOp,
    span: unsafe extern "C" fn(PacketBodySlot) -> PacketBodySpan,
    range: unsafe extern "C" fn(PacketBodySlot, usize, PacketBodySpan),
    reserve: unsafe extern "C" fn(PacketBodySlot, usize) -> *mut u8,
    finish: unsafe extern "C" fn(PacketBodySlot, usize),
    values: ScalarValueHost,
}

struct PacketBody<'a, 'b, 'c> {
    emitter: &'a mut Emitter<'b>,
    allocation: &'a PacketAllocation,
    code: &'a [Step<'c>],
    sparse: u32,
    result: Result<(), &'static str>,
}

unsafe fn packet_body<'a, 'b, 'c, 'd>(slot: PacketBodySlot) -> &'a mut PacketBody<'b, 'c, 'd> {
    unsafe { &mut *slot.cast() }
}

unsafe extern "C" fn packet_body_span(slot: PacketBodySlot) -> PacketBodySpan {
    let emitter = &unsafe { packet_body(slot) }.emitter;
    PacketBodySpan {
        start: emitter.code.len(),
        first: emitter.operation_ranges.len(),
    }
}

unsafe extern "C" fn packet_body_range(slot: PacketBodySlot, index: usize, span: PacketBodySpan) {
    let context = unsafe { packet_body(slot) };
    context.emitter.operation_range(
        context.code[index].instruction, span.start, span.first,
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

unsafe extern "C" fn division_location(slot: *const std::ffi::c_void, reg: Reg) -> operands::MoveLocation {
    let body = unsafe { &*slot.cast::<PacketBody<'_, '_, '_>>() };
    body.allocation.location(PrivateSlot::Register(reg)).into()
}

unsafe extern "C" fn division_scalar(slot: *const std::ffi::c_void, reg: Reg) -> ScalarEncoding {
    let body = unsafe { &*slot.cast::<PacketBody<'_, '_, '_>>() };
    body.emitter.registers[reg].into()
}

unsafe extern "C" fn packet_body_step(slot: PacketBodySlot, index: usize) -> PacketBodyOp {
    use PacketBodyKind as Kind;
    use PrivateSlot::Register;
    let context = unsafe { packet_body(slot) };
    let allocation = context.allocation;
    let emitter = &mut context.emitter;
    let place = |value| allocation.location(value).into();
    let register = |value| place(Register(value));
    let sparse = context.sparse;
    let emit = |e: &mut Emitter<'_>, block: &[Step<'_>]| e.packet_block(block, allocation, sparse);
    let load = |e: &mut Emitter<'_>, reg| e.packet_lane(allocation, reg, 0, 0);
    let step = &context.code[index];
    let flow = match &step.kind {
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
        StepKind::IntegerDivide => {
            let mut op = PacketBodyOp::new(Kind::IntegerDivide);
            op.instruction = step.instruction;
            return op;
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
            op.flag = u32::from(emitter.registers[*dst] == Scalar::Bool);
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
            op.from = emitter.registers[*a].into();
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
    pub(super) fn emit_packet_plan(
        &mut self, plan: &Plan<'_>, allocation: &PacketAllocation, sparse: u32,
    ) -> Result<(), &'static str> {
        self.packet_block(&plan.code, allocation, sparse)
    }

    fn packet_block(
        &mut self, code: &[Step<'_>], allocation: &PacketAllocation, sparse: u32,
    ) -> Result<(), &'static str> {
        let slots = self.frame.end as u32;
        let mut context = PacketBody { emitter: self, allocation, code, sparse, result: Ok(()) };
        let context_slot: PacketBodySlot = std::ptr::from_mut(&mut context).cast();
        unsafe {
            emit_packet_body(
                context_slot, code.len(), sparse, slots,
                PacketBodyHost {
                    step: packet_body_step,
                    span: packet_body_span,
                    range: packet_body_range,
                    reserve: reserve_packet_bytes,
                    finish: finish_packet_bytes,
                    values: ScalarValueHost {
                        ir: HOST, context: context_slot,
                        location: division_location, scalar: division_scalar,
                    },
                },
            );
        }
        context.result
    }
}
