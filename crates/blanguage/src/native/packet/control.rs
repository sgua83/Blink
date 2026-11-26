//! Borrow the Rust IR during B packet analysis and emission.
use super::*;

#[repr(C)]
struct Header { registers: usize, slots: usize, writes: usize, tile: u32, selected: u32 }
#[repr(C)]
struct Host {
    ir: crate::ir::dataflow::Host,
    stride: unsafe extern "C" fn(*const Context<'_>, usize) -> u32,
    written: unsafe extern "C" fn(*const Context<'_>, usize) -> u32,
    scalar: unsafe extern "C" fn(*const Context<'_>, Reg) -> ScalarEncoding,
    widths: unsafe extern "C" fn(*const Context<'_>, unsafe extern "C" fn(usize) -> u32) -> u32,
    uses_storage: unsafe extern "C" fn(*mut Vec<u8>, usize) -> *mut u8,
}
struct Context<'a> {
    program: &'a Program,
    stage: &'a Stage,
    registers: &'a [Scalar],
}
unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::packet_plan"]
    fn packet_plan(context: *const Context<'_>, code: Span, header: Header, uses: *mut Vec<u8>, plan: *mut Span, host: Host) -> bool;
}
pub(super) fn run<'a>(
    program: &Program, stage: &Stage, code: &'a [Inst], registers: &'a [Scalar], tile_scope: bool,
) -> Option<(Plan<'a>, Vec<u8>)> {
    let context = Context { program, stage, registers };
    let header = Header { registers: registers.len(),
        slots: registers.len() + stage.locals.last_key_value().map_or(0, |(end, _)| *end),
        writes: stage.writes.len(), tile: u32::from(tile_scope), selected: u32::from(stage.selection.is_some()) };
    let mut uses = Vec::new();
    let mut plan = std::mem::MaybeUninit::<Span>::uninit();
    if !unsafe { packet_plan(&context, Span::slice(code), header, &mut uses, plan.as_mut_ptr(), PLAN_HOST) } {
        return None;
    }
    let code = unsafe { plan.assume_init() };
    Some((Plan { code, _instructions: std::marker::PhantomData }, uses))
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
const PLAN_HOST: Host = Host { ir: crate::ir::dataflow::HOST, stride, written, scalar, widths,
    uses_storage };
