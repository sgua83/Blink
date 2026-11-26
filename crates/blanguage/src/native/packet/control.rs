//! Rust holds the original instruction and subplan storage for the B analysis and emitter.
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
enum Visit { Operands, Results, Stride, Widths }
#[repr(C)]
struct Node {
    first: Block, second: Block,
    instruction: *const Inst, payload: *const c_void,
    dst: Reg, a: Reg, b: Reg, c: Reg,
    word: u32, stride: u32, writes: u32, kind: Kind, math: u32,
    from: crate::builtin_types::ScalarEncoding, into: crate::builtin_types::ScalarEncoding,
}
#[repr(C)]
struct Header { registers: usize, slots: usize, writes: usize, tile: u32, selected: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct State { owner: *mut Vec<u8>, bytes: *mut u8, count: usize }
#[repr(C)]
struct Product { plan: *mut c_void, uses: State }
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Decision {
    operation: u32,
    uniform_count: u32,
    dataset: u32,
    view: u32,
    math: u32,
}
type Callback = unsafe extern "C" fn(*mut c_void, usize) -> u32;
#[repr(C)]
struct Host {
    read: unsafe extern "C" fn(*const Context<'_>, Block, usize) -> Node,
    visit: unsafe extern "C" fn(*const Context<'_>, *const c_void, Visit, *mut c_void, Callback) -> u32,
    state_new: unsafe extern "C" fn(usize, *const u8) -> State,
    state_drop: unsafe extern "C" fn(State),
    plan_new: unsafe extern "C" fn() -> *mut c_void,
    plan_drop: unsafe extern "C" fn(*mut c_void),
    append: unsafe extern "C" fn(*mut c_void, *const Inst, Decision, *mut c_void, *mut c_void),
}
struct Context<'a> {
    program: &'a Program,
    stage: &'a Stage,
    registers: &'a [Scalar],
}
// The erased vectors borrow the placed instructions and registers until Plan<'a> is consumed.
type Steps = Vec<Step<'static>>;
unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::packet_plan"]
    fn packet_plan(context: *mut Context<'_>, code: Block, header: Header, host: Host) -> Product;
}
pub(super) fn run<'a>(
    program: &Program, stage: &Stage, code: &'a [Inst], registers: &'a [Scalar], tile_scope: bool,
) -> Option<(Plan<'a>, Vec<u8>)> {
    let mut context = Context { program, stage, registers };
    let header = Header { registers: registers.len(),
        slots: registers.len() + stage.locals.last_key_value().map_or(0, |(end, _)| *end),
        writes: stage.writes.len(), tile: u32::from(tile_scope), selected: u32::from(stage.selection.is_some()) };
    let product = unsafe { packet_plan(&mut context, Block::from(code), header, HOST) };
    if product.plan.is_null() { return None; }
    // Only a completed plan transfers its step vector and the original uses buffer.
    let code = unsafe { *Box::from_raw(product.plan.cast::<Vec<Step<'a>>>()) };
    let uses = unsafe { *Box::from_raw(product.uses.owner) };
    Some((Plan { code }, uses))
}
unsafe extern "C" fn read(context: *const Context<'_>, code: Block, index: usize) -> Node {
    let context = unsafe { &*context }; let instruction = unsafe { code.instructions.add(index) };
    let inst = unsafe { &*instruction };
    let mut node = Node { first: Block::EMPTY, second: Block::EMPTY, instruction,
        payload: std::ptr::null(), dst: 0, a: 0, b: 0, c: 0, word: 0, stride: 0, writes: 0,
        kind: Kind::Unsupported, math: 0, from: Scalar::U32.into(), into: Scalar::U32.into() };
    match &inst.kind {
        InstKind::Constant(dst, _, _) => { node.kind = Kind::Constant; node.dst = *dst; }
        InstKind::Index(dst) => { node.kind = Kind::Index; node.dst = *dst; }
        InstKind::Uniform(dst, offset) => { node.kind = Kind::Uniform; node.dst = *dst; node.word = *offset; }
        InstKind::LoadLocal(dst, slot) => { node.kind = Kind::LocalLoad; node.dst = *dst; node.a = *slot; }
        InstKind::StoreLocal(slot, src) => { node.kind = Kind::LocalStore; node.dst = *slot; node.a = *src; }
        InstKind::Load { dst, binding, index, word } => { node.kind = Kind::Load; node.dst = *dst; node.a = *index; node.word = *word; node.stride = context.program.buffers[*binding].stride; node.writes = u32::from(context.stage.writes.contains(binding)); }
        InstKind::ViewLoad { dst, address, index, stride, word } => { node.kind = Kind::ViewLoad; node.dst = *dst; node.a = address[0]; node.b = address[1]; node.c = *index; node.word = *word; node.payload = (stride as *const ElementStride).cast(); }
        InstKind::Store { binding, index, word, value } => { node.kind = Kind::Store; node.a = *index; node.b = *value; node.word = *word; node.stride = context.program.buffers[*binding].stride; }
        InstKind::Unary { dst, a, .. } => { node.kind = Kind::Unary; node.dst = *dst; node.a = *a; }
        InstKind::Binary { dst, a, b, .. } => { node.kind = Kind::Binary; node.dst = *dst; node.a = *a; node.b = *b; }
        InstKind::IntegerDivide { .. } => node.kind = Kind::Divide,
        InstKind::Cast { dst, src } => { node.kind = Kind::Cast; node.dst = *dst; node.a = *src; node.from = context.registers[*src].into(); node.into = context.registers[*dst].into(); }
        InstKind::Math { dst, op, .. } => { node.kind = Kind::Math; node.dst = *dst; node.math = *op as u32; }
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
        Visit::Widths => { let context = unsafe { &*context }; context.registers.iter()
            .chain(context.stage.locals.values().flat_map(|ty| ty.components.iter().map(|(scalar, _)| scalar)))
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
unsafe fn take_plan(plan: *mut c_void) -> Steps {
    if plan.is_null() { Vec::new() } else { unsafe { *Box::from_raw(plan.cast::<Steps>()) } }
}
unsafe extern "C" fn append(plan: *mut c_void, instruction: *const Inst, decision: Decision, first: *mut c_void, second: *mut c_void) {
    let step = Step {
        instruction: unsafe { &*instruction },
        decision,
        first: unsafe { take_plan(first) },
        second: unsafe { take_plan(second) },
    };
    unsafe { &mut *plan.cast::<Steps>() }.push(step);
}
const HOST: Host = Host { read, visit, state_new, state_drop, plan_new, plan_drop, append };
