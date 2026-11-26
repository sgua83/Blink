//! 四个独立元素的 SSE2 降低；计划只包含已证明的访问与可表达指令，尾部共用标量 IR。
use super::*;
use allocation::{PacketAllocation, PacketPlace};
use super::operands::ScalarValueHost;
use crate::builtin_types::ScalarEncoding;
use crate::ir::dataflow::{HOST, Span};
use super::structure::{self, Control, ControlKind, ScalarCursor};
#[path="packet/control.rs"]
mod control;
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
struct PacketAddress(u32);

#[derive(Clone, Copy, Default)]
#[repr(transparent)]
struct PacketViewAddress(u32);

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
}
impl<'a> Plan<'a> {
    pub fn new(
        program: &Program, stage: &Stage, code: &'a [Inst], registers: &'a [Scalar],
        tile_scope: bool,
    ) -> Option<(Self, Vec<u8>)> {
        control::run(program, stage, code, registers, tile_scope)
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
    fn emit_packet_body(context: Slot, emitter: Slot, code: Span, layout: PacketLayout, host: PacketBodyHost);
    #[link_name = "bootstrap.x64_instruction::packet_lane"]
    fn encode_packet_lane(src: operands::MoveLocation, lane: u8, gpr: u8, output: *mut u8) -> u64;
}

type Slot = *mut std::ffi::c_void;

#[repr(u32)]
enum PacketBodyKind {
    Constant, Index, Uniform, Copy, Unary, Binary, Cast, Math, Select, IntegerDivide,
    Load, Store, View, Control,
}

#[repr(C)]
struct PacketBodyOp {
    places: [operands::MoveLocation; 6],
    control: Control,
    instruction: *const Inst,
    binding: usize,
    index: Reg,
    stride: u64,
    from: ScalarEncoding,
    into: ScalarEncoding,
    kind: PacketBodyKind,
    literal: u32,
    flag: u32,
    unary: u32,
    binary: crate::builtin_types::ScalarBinaryOp,
    math: Math,
    dataset: PacketAddress,
    view: PacketViewAddress,
    dynamic_stride: u32,
}

impl PacketBodyOp {
    fn new(kind: PacketBodyKind, instruction: &Inst) -> Self {
        Self {
            places: std::array::from_fn(|_| allocation::Location::Immediate(0).into()),
            control: Control { first: Span::slice::<Step<'_>>(&[]), second: Span::slice::<Step<'_>>(&[]),
                register: 0, kind: ControlKind::Exit },
            instruction,
            binding: 0,
            index: 0,
            stride: 0,
            from: Scalar::U32.into(),
            into: Scalar::U32.into(),
            kind,
            literal: 0,
            flag: 0,
            unary: 0,
            binary: crate::builtin_types::ScalarBinaryOp::Add,
            math: Math::Sqrt,
            dataset: PacketAddress::default(),
            view: PacketViewAddress::default(),
            dynamic_stride: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PacketLayout {
    sparse: u32,
    slots: u32,
    buffer_bytes: u32,
    stride_field: u32,
}

#[repr(C)]
struct PacketBodyHost {
    control: structure::Host,
    step: unsafe extern "C" fn(Slot, *const Step<'_>) -> PacketBodyOp,
    mark: unsafe extern "C" fn(Slot) -> ScalarCursor,
    publish: unsafe extern "C" fn(Slot, *const Inst, ScalarCursor),
    reserve: unsafe extern "C" fn(Slot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(Slot, usize),
    values: ScalarValueHost,
}

struct PacketBody<'a> {
    allocation: &'a PacketAllocation,
    registers: &'a [Scalar],
    layout: PacketLayout,
}

unsafe extern "C" fn packet_location(context: *const std::ffi::c_void, reg: Reg) -> operands::MoveLocation {
    let body = unsafe { &*context.cast::<PacketBody<'_>>() };
    body.allocation.location(PrivateSlot::Register(reg)).into()
}

unsafe extern "C" fn packet_scalar(context: *const std::ffi::c_void, reg: Reg) -> ScalarEncoding {
    let body = unsafe { &*context.cast::<PacketBody<'_>>() };
    body.registers[reg].into()
}

fn packet_host(context: Slot) -> PacketBodyHost {
    PacketBodyHost {
        control: structure::control_host(emit_packet_child, test_packet),
        step: packet_body_step,
        mark: structure::mark_scalar,
        publish: structure::publish_scalar,
        reserve: structure::reserve_scalar,
        commit: structure::commit_scalar,
        values: ScalarValueHost { ir: HOST, context, location: packet_location, scalar: packet_scalar },
    }
}

unsafe extern "C" fn emit_packet_child(context: Slot, emitter: Slot, code: Span) -> u32 {
    let layout = unsafe { &*context.cast::<PacketBody<'_>>() }.layout;
    unsafe { emit_packet_body(context, emitter, code, layout, packet_host(context)) };
    0
}

unsafe extern "C" fn test_packet(context: Slot, emitter: Slot, reg: Reg) {
    let source = unsafe { packet_location(context, reg) };
    let emitter = unsafe { &mut *emitter.cast::<Emitter<'_>>() };
    emitter.emit_bytes(16, |output| unsafe { encode_packet_lane(source, 0, 0, output) });
}

unsafe extern "C" fn packet_body_step(context: Slot, step: *const Step<'_>) -> PacketBodyOp {
    use PacketBodyKind as Kind;
    use PrivateSlot::Register;
    let context = unsafe { &*context.cast::<PacketBody<'_>>() };
    let step = unsafe { &*step };
    let place = |value| context.allocation.location(value).into();
    let register = |value| place(Register(value));
    let op = |kind| PacketBodyOp::new(kind, step.instruction);
    let empty = &[];
    let control = match &step.kind {
        StepKind::If { condition, yes, no } => (ControlKind::If, *condition, yes.as_slice(), no.as_slice()),
        StepKind::Loop { condition, test, body } => (ControlKind::Loop, *test, condition.as_slice(), body.as_slice()),
        StepKind::Scope(body) => (ControlKind::Scope, 0, body.as_slice(), empty.as_slice()),
        StepKind::Exit => (ControlKind::Exit, 0, empty.as_slice(), empty.as_slice()),
        StepKind::Break => (ControlKind::Break, 0, empty.as_slice(), empty.as_slice()),
        StepKind::Continue => (ControlKind::Continue, 0, empty.as_slice(), empty.as_slice()),
        StepKind::Load { dst, binding, index, word, mode } => {
            let mut out = op(Kind::Load);
            out.binding = *binding;
            out.index = *index;
            out.literal = *word;
            out.dataset = *mode;
            out.places[0] = register(*dst);
            out.flag = u32::from(context.registers[*dst] == Scalar::Bool);
            return out;
        }
        StepKind::Store { binding, word, value, mode } => {
            let mut out = op(Kind::Store);
            out.binding = *binding;
            out.literal = *word;
            out.dataset = *mode;
            out.places[0] = register(*value);
            return out;
        }
        StepKind::ViewLoad { dst, address, stride, index, word, mode } => {
            let mut out = op(Kind::View);
            out.places[0] = register(*dst);
            out.places[1] = register(*index);
            out.places[2] = register(address[0]);
            out.places[3] = register(address[1]);
            match stride {
                ElementStride::Constant(bytes) => out.stride = *bytes,
                ElementStride::Value([low, high]) => {
                    out.places[4] = register(*low);
                    out.places[5] = register(*high);
                    out.dynamic_stride = 1;
                }
            }
            out.literal = *word;
            out.view = *mode;
            out.flag = u32::from(context.registers[*dst] == Scalar::Bool);
            return out;
        }
        StepKind::IntegerDivide => return op(Kind::IntegerDivide),
        StepKind::Constant(dst, bits) => {
            let mut op = op(Kind::Constant);
            op.places[0] = register(*dst);
            op.literal = *bits;
            return op;
        }
        StepKind::Index(dst) => {
            let mut op = op(Kind::Index);
            op.places[0] = register(*dst);
            return op;
        }
        StepKind::Uniform(dst, offset) => {
            let mut op = op(Kind::Uniform);
            op.places[0] = register(*dst);
            op.literal = *offset;
            op.flag = u32::from(context.registers[*dst] == Scalar::Bool);
            return op;
        }
        StepKind::Copy(dst, src) => {
            let mut op = op(Kind::Copy);
            op.places[0] = place(*dst);
            op.places[1] = place(*src);
            return op;
        }
        StepKind::Unary { dst, op: source, a } => {
            let mut op = op(Kind::Unary);
            op.places[0] = register(*dst);
            op.places[1] = register(*a);
            op.unary = *source as u32;
            return op;
        }
        StepKind::Binary { dst, op: source, a, b, uniform_count } => {
            let mut op = op(Kind::Binary);
            op.places[0] = register(*dst);
            op.places[1] = register(*a);
            op.places[2] = register(*b);
            op.from = context.registers[*a].into();
            op.binary = (*source).into();
            op.flag = u32::from(*uniform_count);
            return op;
        }
        StepKind::Cast { dst, src, from, into } => {
            let mut op = op(Kind::Cast);
            op.places[0] = register(*dst);
            op.places[1] = register(*src);
            op.from = (*from).into();
            op.into = (*into).into();
            return op;
        }
        StepKind::Math { dst, op: source, args } => {
            let mut op = op(Kind::Math);
            op.places[0] = register(*dst);
            op.places[1] = register(args[0]);
            op.places[2] = register(*args.last().unwrap());
            op.math = *source;
            return op;
        }
        StepKind::Select { dst, condition, a, b } => {
            let mut op = op(Kind::Select);
            op.places[0] = register(*dst);
            op.places[1] = register(*condition);
            op.places[2] = register(*a);
            op.places[3] = register(*b);
            return op;
        }
    };
    let mut out = op(Kind::Control);
    out.control = Control { first: Span::slice(control.2), second: Span::slice(control.3),
        register: control.1, kind: control.0 };
    out
}

impl Emitter<'_> {
    pub(super) fn emit_packet_plan(&mut self, plan: &Plan<'_>, allocation: &PacketAllocation, sparse: u32) {
        let layout = PacketLayout { sparse, slots: self.frame.end as u32, buffer_bytes: BUFFER_BYTES,
            stride_field: BUFFER_STRIDE };
        let mut body = PacketBody { allocation, registers: self.registers, layout };
        let context = std::ptr::from_mut(&mut body).cast();
        let emitter = std::ptr::from_mut(self).cast();
        unsafe { emit_packet_body(context, emitter, Span::slice(&plan.code), layout, packet_host(context)) };
    }
}
