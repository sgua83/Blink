//! B owns complete custom-kernel preparation. Rust retains only the original
//! typed bodies and source construction.
use super::*;
use std::{ffi::c_void, ptr};

#[derive(Clone, Copy)]
#[repr(C)]
struct Node { data: *const c_void, kind: u32 }
#[repr(C)]
struct Input { source: Node, project: u32, identity: u32 }
#[repr(C)]
struct Header { registers: usize, locals: usize }
#[repr(C)]
struct Host {
    ir: crate::ir::dataflow::Host, context: *mut c_void,
    header: unsafe extern "C" fn(*mut c_void) -> Header,
    clone: unsafe extern "C" fn(*mut c_void, usize, Node, usize) -> crate::ir::rewrite::Span,
    rewrite: crate::ir::rewrite::Host,
    push_root: unsafe extern "C" fn(*mut c_void, usize, Reg),
    part_len: unsafe extern "C" fn(*const c_void, usize) -> usize,
    alloc_uniform: extern "C" fn(usize) -> *mut u8,
    release_uniform: unsafe extern "C" fn(*mut u8, usize),
    open_part: unsafe extern "C" fn(*mut c_void, usize) -> BlockSpan,
    hoist: unsafe extern "C" fn(*mut c_void, usize, *mut Inst, u32, usize),
    move_instruction: unsafe extern "C" fn(*mut Inst, *mut Inst),
    close_part: unsafe extern "C" fn(*mut c_void, usize, usize),
    value_flow: unsafe extern "C" fn(*mut c_void),
    boundaries: unsafe extern "C" fn(*mut c_void, usize, usize, usize),
    append: unsafe extern "C" fn(*mut c_void, usize),
}
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::reduction_kernel"] fn b_kernel(input: Input, host: Host);
}
struct State {
    kernel: ReductionKernel,
    parts: [Vec<Inst>; 3],
}
impl State {
    fn host(&mut self) -> Host {
        Host { ir: crate::ir::dataflow::HOST, context: ptr::from_mut(self).cast(),
            header, clone, rewrite: crate::ir::rewrite::HOST, push_root, part_len, alloc_uniform, release_uniform,
            open_part, hoist, move_instruction, close_part, value_flow,
            boundaries, append }
    }
}
pub(crate) fn kernel(reduction: &CustomReduction, stage: &Stage, project: bool, identity: bool) -> ReductionKernel {
    // Copy only metadata that the real kernel retains. Its code/registers/locals
    // are built directly from the selected value bodies, without old Stage clones.
    let stage = Stage {
        domain: stage.domain, prefix: stage.prefix, selection: stage.selection,
        operation: stage.operation.clone(), operation_source: stage.operation_source.clone(),
        generated_origin: stage.generated_origin.clone(), boundary_before: stage.boundary_before,
        registers: Vec::new(), locals: BTreeMap::new(), code: Vec::new(),
        reads: stage.reads.clone(), writes: stage.writes.clone(), write_regions: stage.write_regions.clone(),
        prefix_guarantees: stage.prefix_guarantees.clone(), element_guarantees: stage.element_guarantees.clone(),
        cross_access: stage.cross_access.clone(), tiles: Vec::new(),
    };
    let kernel = ReductionKernel { stage, invariant_end: 0,
        initialize_end: 0, project_end: 0, source: Vec::new(), projected: Vec::new(),
        initial: Vec::new(), left: Vec::new(), right: Vec::new(), result: Vec::new() };
    let mut state = State { kernel, parts: Default::default() };
    let input = Input { source: Node { data: ptr::from_ref(reduction).cast(), kind: 23 },
        project: u32::from(project), identity: u32::from(identity) };
    let host = state.host();
    unsafe { b_kernel(input, host); }
    state.kernel
}
unsafe extern "C" fn header(context: *mut c_void) -> Header {
    let stage = unsafe { &(*context.cast::<State>()).kernel.stage };
    Header { registers: stage.registers.len(), locals: stage.locals.last_key_value().map_or(0, |(end, _)| *end) }
}
unsafe extern "C" fn clone(context: *mut c_void, part: usize, source: Node, local: usize) -> crate::ir::rewrite::Span {
    let state = unsafe { &mut *context.cast::<State>() };
    let value = unsafe { &*source.data.cast::<ValueCode>() };
    state.parts[part - 1] = value.code.clone();
    state.kernel.stage.registers.extend(&value.registers);
    state.kernel.stage.locals.extend(value.locals.iter().map(|(end, ty)| (end + local, ty.clone())));
    crate::ir::rewrite::Span::slice(&mut state.parts[part - 1])
}
fn root_values(kernel: &mut ReductionKernel, root: usize) -> &mut Vec<Reg> {
    match root {
        0 => &mut kernel.source, 1 => &mut kernel.projected, 2 => &mut kernel.initial,
        3 => &mut kernel.left, 4 => &mut kernel.right, _ => &mut kernel.result,
    }
}
unsafe extern "C" fn push_root(context: *mut c_void, root: usize, reg: Reg) {
    root_values(unsafe { &mut (*context.cast::<State>()).kernel }, root).push(reg);
}
unsafe extern "C" fn part_len(context: *const c_void, part: usize) -> usize {
    let state = unsafe { &*context.cast::<State>() };
    if part == 0 { state.kernel.stage.code.len() } else { state.parts[part - 1].len() }
}
extern "C" fn alloc_uniform(count: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; count].into_boxed_slice()).cast()
}

unsafe extern "C" fn release_uniform(buffer: *mut u8, count: usize) {
    unsafe { drop(Box::from_raw(ptr::slice_from_raw_parts_mut(buffer, count))); }
}

unsafe extern "C" fn open_part(context: *mut c_void, part: usize) -> BlockSpan {
    let code = unsafe { &mut (*context.cast::<State>()).parts[part - 1] };
    let span = unsafe { instruction_span(code) };
    // B takes the live slots and returns only the initialized retained prefix.
    unsafe { code.set_len(0); }
    span
}

unsafe extern "C" fn hoist(context: *mut c_void, part: usize, instruction: *mut Inst,
    operation: u32, uniform_operand_count: usize,
) {
    use ReductionUniformOperation as Op;
    const OPERATIONS: [Op; 9] = [Op::Constant, Op::Uniform, Op::BufferStride, Op::Unary,
        Op::Binary, Op::Cast, Op::Math, Op::Select, Op::ConstantIntegerDivide];
    const PARTITIONS: [ReductionPartition; 3] = [ReductionPartition::Identity, ReductionPartition::Project, ReductionPartition::Combine];
    let mut inst = unsafe { instruction.read() };
    let retained = OperationOrigin::from_instruction(&inst);
    inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
        reason: GenerationReason::ReductionUniformExtract,
        trigger: Some(GenerationAnchor::ReductionUniformExtract {
            retained, partition: PARTITIONS[part - 1], operation: OPERATIONS[operation as usize], uniform_operand_count,
        }),
    }));
    // The final entry owns the hoisted instruction. The source part is disjoint.
    unsafe { (*context.cast::<State>()).kernel.stage.code.push(inst); }
}

unsafe extern "C" fn move_instruction(target: *mut Inst, source: *mut Inst) {
    unsafe { target.write(source.read()); }
}

unsafe extern "C" fn close_part(context: *mut c_void, part: usize, kept: usize) {
    unsafe { (*context.cast::<State>()).parts[part - 1].set_len(kept); }
}
unsafe extern "C" fn value_flow(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<State>() };
    value_flow::run_reduction(&mut state.parts, &mut state.kernel);
}
unsafe extern "C" fn boundaries(context: *mut c_void, invariant: usize, initialize: usize, project: usize) {
    let kernel = unsafe { &mut (*context.cast::<State>()).kernel };
    kernel.invariant_end = invariant; kernel.initialize_end = initialize; kernel.project_end = project;
}
unsafe extern "C" fn append(context: *mut c_void, part: usize) {
    let state = unsafe { &mut *context.cast::<State>() };
    state.kernel.stage.code.append(&mut state.parts[part - 1]);
}
