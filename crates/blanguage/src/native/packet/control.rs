//! Rust holds the original instruction and subplan storage for the B analysis and emitter.
use super::*;
use std::ffi::c_void;

#[repr(C)]
struct Header { registers: usize, slots: usize, writes: usize, tile: u32, selected: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Decision {
    operation: u32,
    uniform_count: u32,
    dataset: u32,
    view: u32,
    math: u32,
}
#[repr(C)]
struct Host {
    ir: crate::ir::dataflow::Host,
    stride: unsafe extern "C" fn(*const Context<'_>, usize) -> u32,
    written: unsafe extern "C" fn(*const Context<'_>, usize) -> u32,
    scalar: unsafe extern "C" fn(*const Context<'_>, Reg) -> ScalarEncoding,
    widths: unsafe extern "C" fn(*const Context<'_>, unsafe extern "C" fn(usize) -> u32) -> u32,
    uses_storage: unsafe extern "C" fn(*mut Vec<u8>, usize) -> *mut u8,
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
    fn packet_plan(context: *const Context<'_>, code: Span, header: Header, uses: *mut Vec<u8>, host: Host) -> *mut c_void;
}
pub(super) fn run<'a>(
    program: &Program, stage: &Stage, code: &'a [Inst], registers: &'a [Scalar], tile_scope: bool,
) -> Option<(Plan<'a>, Vec<u8>)> {
    let context = Context { program, stage, registers };
    let header = Header { registers: registers.len(),
        slots: registers.len() + stage.locals.last_key_value().map_or(0, |(end, _)| *end),
        writes: stage.writes.len(), tile: u32::from(tile_scope), selected: u32::from(stage.selection.is_some()) };
    let mut uses = Vec::new();
    let plan = unsafe { packet_plan(&context, Span::slice(code), header, &mut uses, PLAN_HOST) };
    if plan.is_null() { return None; }
    let code = unsafe { *Box::from_raw(plan.cast::<Vec<Step<'a>>>()) };
    Some((Plan { code }, uses))
}
unsafe extern "C" fn stride(context: *const Context<'_>, binding: usize) -> u32 {
    unsafe { &*context }.program.buffers[binding].stride
}

unsafe extern "C" fn written(context: *const Context<'_>, binding: usize) -> u32 {
    u32::from(unsafe { &*context }.stage.writes.contains(&binding))
}

unsafe extern "C" fn scalar(context: *const Context<'_>, register: Reg) -> ScalarEncoding {
    unsafe { &*context }.registers[register].into()
}

unsafe extern "C" fn widths(context: *const Context<'_>, visit: unsafe extern "C" fn(usize) -> u32) -> u32 {
    let context = unsafe { &*context };
    u32::from(context.registers.iter()
        .chain(context.stage.locals.values().flat_map(|ty| ty.components.iter().map(|(scalar, _)| scalar)))
        .all(|scalar| unsafe { visit(scalar.size() as usize) != 0 }))
}

unsafe extern "C" fn uses_storage(output: *mut Vec<u8>, count: usize) -> *mut u8 {
    let uses = unsafe { &mut *output };
    uses.resize(count, 0);
    uses.as_mut_ptr()
}
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
const PLAN_HOST: Host = Host { ir: crate::ir::dataflow::HOST, stride, written, scalar, widths,
    uses_storage, plan_new, plan_drop, append };
