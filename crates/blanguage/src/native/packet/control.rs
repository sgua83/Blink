//! B owns SSE2 analysis and plan decisions; this module owns the original buffers and Steps.
use super::*;
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Block { instructions: *const Inst, count: usize }
impl Block {
    fn from(code: &[Inst]) -> Self { Self { instructions: code.as_ptr(), count: code.len() } }
    const EMPTY: Self = Self { instructions: std::ptr::NonNull::<Inst>::dangling().as_ptr(), count: 0 };
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum Kind { Unsupported, Constant, Index, Uniform, LocalLoad, LocalStore, Load, ViewLoad, Store, Unary, Binary, Divide, Cast, Math, Select, If, Loop, Scope, Exit, Break, Continue }
#[repr(u32)]
enum Operation { Constant, Index, Uniform, LocalLoad, LocalStore, Load, ViewLoad, Store, Unary, Binary, Divide, Cast, Copy, Math, Select, If, Loop, Scope, Exit, Break, Continue }
#[repr(u32)]
enum Visit { Operands, Results, Stride, Widths }
#[repr(C)]
struct Node {
    first: Block, second: Block,
    instruction: *const Inst, payload: *const c_void, other: *const c_void, arguments: *const Reg, argument_count: usize,
    dst: Reg, a: Reg, b: Reg, c: Reg, binding: usize, bits: u64,
    word: u32, stride: u32, writes: u32, kind: Kind, math: u32,
    from: crate::builtin_types::ScalarEncoding, into: crate::builtin_types::ScalarEncoding,
}
#[repr(C)]
struct Header { registers: usize, slots: usize, writes: usize, tile: u32, selected: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct State { owner: *mut Vec<u8>, bytes: *mut u8, count: usize }
#[repr(C)]
struct Decision { operation: Operation, uniform_count: u32, dataset: PacketAddress, view: PacketViewAddress, math: Math }
type Callback = unsafe extern "C" fn(*mut c_void, usize) -> u32;
#[repr(C)]
struct Host {
    read: unsafe extern "C" fn(*const Context<'_>, Block, usize) -> Node,
    visit: unsafe extern "C" fn(*const Context<'_>, *const c_void, Visit, *mut c_void, Callback) -> u32,
    state_new: unsafe extern "C" fn(usize, *const u8) -> State,
    state_drop: unsafe extern "C" fn(State),
    plan_new: unsafe extern "C" fn() -> *mut c_void,
    plan_drop: unsafe extern "C" fn(*mut c_void),
    append: unsafe extern "C" fn(*mut c_void, Node, Decision, *mut c_void, *mut c_void),
    address_only: unsafe extern "C" fn(*mut Context<'_>, Reg),
}
struct Context<'a> { program: &'a Program, stage: &'a Stage, address_only: &'a mut BTreeSet<Reg> }
// The erased vectors keep the exact Stage borrow supplied by run; they never leave Plan<'a>.
type Steps = Vec<Step<'static>>;
unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::packet_plan"]
    fn packet_plan(context: *mut Context<'_>, code: Block, header: Header, host: Host) -> *mut c_void;
}
pub(super) fn run<'a>(program: &Program, stage: &'a Stage) -> Option<Plan<'a>> {
    let mut address_only = BTreeSet::new();
    let mut context = Context { program, stage, address_only: &mut address_only };
    let header = Header { registers: stage.registers.len(),
        slots: stage.registers.len() + stage.locals.last_key_value().map_or(0, |(end, _)| *end),
        writes: stage.writes.len(), tile: u32::from(stage.has_tile_scope()), selected: u32::from(stage.selection.is_some()) };
    let code = unsafe { packet_plan(&mut context, Block::from(&stage.code), header, HOST) };
    if code.is_null() { return None; }
    let code = unsafe { *Box::from_raw(code.cast::<Vec<Step<'a>>>()) };
    Some(Plan { code, address_only })
}
unsafe extern "C" fn read(context: *const Context<'_>, code: Block, index: usize) -> Node {
    let context = unsafe { &*context }; let instruction = unsafe { code.instructions.add(index) };
    let inst = unsafe { &*instruction };
    let mut node = Node { first: Block::EMPTY, second: Block::EMPTY, instruction,
        payload: std::ptr::null(), other: std::ptr::null(), arguments: std::ptr::NonNull::<Reg>::dangling().as_ptr(), argument_count: 0,
        dst: 0, a: 0, b: 0, c: 0, binding: 0, bits: 0, word: 0, stride: 0, writes: 0,
        kind: Kind::Unsupported, math: 0, from: Scalar::U32.into(), into: Scalar::U32.into() };
    match &inst.kind {
        InstKind::Constant(dst, _, bits) => { node.kind = Kind::Constant; node.dst = *dst; node.bits = *bits; }
        InstKind::Index(dst) => { node.kind = Kind::Index; node.dst = *dst; }
        InstKind::Uniform(dst, offset) => { node.kind = Kind::Uniform; node.dst = *dst; node.word = *offset; }
        InstKind::LoadLocal(dst, slot) => { node.kind = Kind::LocalLoad; node.dst = *dst; node.a = *slot; }
        InstKind::StoreLocal(slot, src) => { node.kind = Kind::LocalStore; node.dst = *slot; node.a = *src; }
        InstKind::Load { dst, binding, index, word } => { node.kind = Kind::Load; node.dst = *dst; node.binding = *binding; node.a = *index; node.word = *word; node.stride = context.program.buffers[*binding].stride; node.writes = u32::from(context.stage.writes.contains(binding)); }
        InstKind::ViewLoad { dst, address, index, stride, word } => { node.kind = Kind::ViewLoad; node.dst = *dst; node.a = address[0]; node.b = address[1]; node.c = *index; node.word = *word; node.payload = (stride as *const ElementStride).cast(); }
        InstKind::Store { binding, index, word, value } => { node.kind = Kind::Store; node.binding = *binding; node.a = *index; node.b = *value; node.word = *word; node.stride = context.program.buffers[*binding].stride; }
        InstKind::Unary { dst, op, a } => { node.kind = Kind::Unary; node.dst = *dst; node.a = *a; node.payload = (op as *const UnaryOp).cast(); }
        InstKind::Binary { dst, op, a, b } => { node.kind = Kind::Binary; node.dst = *dst; node.a = *a; node.b = *b; node.payload = (op as *const BinaryOp).cast(); }
        InstKind::IntegerDivide { result, src, divisor } => { node.kind = Kind::Divide; node.a = *src; node.payload = (result as *const IntegerDivisionResult).cast(); node.other = (divisor as *const IntegerDivisor).cast(); }
        InstKind::Cast { dst, src } => { node.kind = Kind::Cast; node.dst = *dst; node.a = *src; let from = &context.stage.registers[*src]; let into = &context.stage.registers[*dst]; node.from = (*from).into(); node.into = (*into).into(); node.payload = (from as *const Scalar).cast(); node.other = (into as *const Scalar).cast(); }
        InstKind::Math { dst, op, args } => { node.kind = Kind::Math; node.dst = *dst; node.math = *op as u32; node.arguments = args.as_ptr(); node.argument_count = args.len(); }
        InstKind::Select { dst, condition, a, b } => { node.kind = Kind::Select; node.dst = *dst; node.a = *condition; node.b = *a; node.c = *b; }
        InstKind::If { condition, yes, no } => { node.kind = Kind::If; node.a = *condition; node.first = Block::from(yes); node.second = Block::from(no); }
        InstKind::Loop { condition, test, body } => { node.kind = Kind::Loop; node.a = *test; node.first = Block::from(condition); node.second = Block::from(body); }
        InstKind::Scope(body) => { node.kind = Kind::Scope; node.first = Block::from(body); }
        InstKind::Exit => node.kind = Kind::Exit,
        InstKind::Break => node.kind = Kind::Break,
        InstKind::Continue => node.kind = Kind::Continue,
        _ => {}
    }
    node
}
unsafe extern "C" fn visit(context: *const Context<'_>, source: *const c_void, kind: Visit, target: *mut c_void, callback: Callback) -> u32 {
    let mut apply = |value| unsafe { callback(target, value) != 0 };
    u32::from(match kind {
        Visit::Operands => unsafe { &*source.cast::<Inst>() }.all_operands(&mut apply),
        Visit::Results => unsafe { &*source.cast::<Inst>() }.results().all(&mut apply),
        Visit::Stride => unsafe { &*source.cast::<ElementStride>() }.all_operands(&mut apply),
        Visit::Widths => { let stage = unsafe { &*context }.stage; stage.registers.iter()
            .chain(stage.locals.values().flat_map(|ty| ty.components.iter().map(|(scalar, _)| scalar)))
            .all(|scalar| apply(scalar.size() as usize)) }
    })
}
unsafe extern "C" fn state_new(count: usize, source: *const u8) -> State {
    let data = if source.is_null() { vec![0; count] } else { unsafe { std::slice::from_raw_parts(source, count) }.to_vec() };
    let mut data = Box::new(data); let result = State { owner: data.as_mut(), bytes: data.as_mut_ptr(), count };
    let _ = Box::into_raw(data); result
}
unsafe extern "C" fn state_drop(state: State) { drop(unsafe { Box::from_raw(state.owner) }); }
unsafe extern "C" fn plan_new() -> *mut c_void { Box::into_raw(Box::new(Steps::new())).cast() }
unsafe extern "C" fn plan_drop(plan: *mut c_void) { drop(unsafe { Box::from_raw(plan.cast::<Steps>()) }); }
unsafe fn take_plan(plan: *mut c_void) -> Steps { unsafe { *Box::from_raw(plan.cast::<Steps>()) } }
unsafe extern "C" fn append(plan: *mut c_void, node: Node, decision: Decision, first: *mut c_void, second: *mut c_void) {
    use PrivateSlot::{Register, Local};
    let kind = match decision.operation {
        Operation::Constant => StepKind::Constant(node.dst, node.bits as u32),
        Operation::Index => StepKind::Index(node.dst),
        Operation::Uniform => StepKind::Uniform(node.dst, node.word),
        Operation::LocalLoad => StepKind::Copy(Register(node.dst), Local(node.a)),
        Operation::LocalStore => StepKind::Copy(Local(node.dst), Register(node.a)),
        Operation::Load => StepKind::Load { dst: node.dst, binding: node.binding, index: node.a, word: node.word, mode: decision.dataset },
        Operation::ViewLoad => StepKind::ViewLoad { dst: node.dst, address: [node.a, node.b], stride: unsafe { *node.payload.cast::<ElementStride>() }, index: node.c, word: node.word, mode: decision.view },
        Operation::Store => StepKind::Store { binding: node.binding, word: node.word, value: node.b, mode: decision.dataset },
        Operation::Unary => StepKind::Unary { dst: node.dst, op: unsafe { *node.payload.cast::<UnaryOp>() }, a: node.a },
        Operation::Binary => StepKind::Binary { dst: node.dst, op: unsafe { *node.payload.cast::<BinaryOp>() }, a: node.a, b: node.b, uniform_count: decision.uniform_count != 0 },
        Operation::Divide => StepKind::IntegerDivide { result: unsafe { *node.payload.cast::<IntegerDivisionResult>() }, src: node.a, divisor: unsafe { *node.other.cast::<IntegerDivisor>() } },
        Operation::Cast => StepKind::Cast { dst: node.dst, src: node.a, from: unsafe { *node.payload.cast::<Scalar>() }, into: unsafe { *node.other.cast::<Scalar>() } },
        Operation::Copy => StepKind::Copy(Register(node.dst), Register(node.a)),
        Operation::Math => StepKind::Math { dst: node.dst, op: decision.math, args: unsafe { std::slice::from_raw_parts(node.arguments, node.argument_count) } },
        Operation::Select => StepKind::Select { dst: node.dst, condition: node.a, a: node.b, b: node.c },
        Operation::If => StepKind::If { condition: node.a, yes: unsafe { take_plan(first) }, no: unsafe { take_plan(second) } },
        Operation::Loop => StepKind::Loop { condition: unsafe { take_plan(first) }, test: node.a, body: unsafe { take_plan(second) } },
        Operation::Scope => StepKind::Scope(unsafe { take_plan(first) }),
        Operation::Exit => StepKind::Exit,
        Operation::Break => StepKind::Break,
        Operation::Continue => StepKind::Continue,
    };
    unsafe { &mut *plan.cast::<Steps>() }.push(Step { instruction: unsafe { &*node.instruction }, kind });
}
unsafe extern "C" fn address_only(context: *mut Context<'_>, reg: Reg) { unsafe { &mut *context }.address_only.insert(reg); }
const HOST: Host = Host { read, visit, state_new, state_drop, plan_new, plan_drop, append, address_only };
