//! B owns complete custom-kernel preparation. Rust retains only the original
//! typed bodies, native value-flow containers and source construction.
use super::*;
use std::{ffi::c_void, mem::{self, size_of}, ptr};

#[derive(Clone, Copy)]
#[repr(C)]
struct Node { data: *const c_void, kind: u32 }
#[derive(Clone, Copy)]
#[repr(C)]
struct Span { data: *const c_void, count: usize, stride: usize }
impl Span {
    fn of<T>(values: &[T]) -> Self {
        Self { data: values.as_ptr().cast(), count: values.len(), stride: size_of::<T>() }
    }
}
#[repr(C)]
struct Input { source: Node, project: u32, identity: u32 }
#[repr(C)]
struct Header { registers: usize, locals: usize }
#[repr(C)]
struct Extract { eligible: u32, operation: u32, inputs: usize }
#[repr(C)]
struct Host {
    ir: crate::ir::dataflow::Host, context: *mut c_void,
    header: unsafe extern "C" fn(*mut c_void) -> Header,
    clone: unsafe extern "C" fn(*mut c_void, usize, Node, usize),
    remap: unsafe extern "C" fn(*mut c_void, usize, usize, usize),
    push_root: unsafe extern "C" fn(*mut c_void, usize, Reg),
    roots: unsafe extern "C" fn(*mut c_void, usize) -> Span,
    part: unsafe extern "C" fn(*mut c_void, usize) -> Span,
    uniform: unsafe extern "C" fn(*mut c_void, usize) -> *mut u8,
    extract: unsafe extern "C" fn(*mut c_void, usize, *mut u8),
    release_uniform: unsafe extern "C" fn(*mut c_void),
    entry: unsafe extern "C" fn(*mut c_void),
    seed: unsafe extern "C" fn(*mut c_void),
    body: unsafe extern "C" fn(*mut c_void, usize, u32),
    retain: unsafe extern "C" fn(*mut c_void, usize),
    forward: unsafe extern "C" fn(*mut c_void, usize, usize, Reg),
    origin: unsafe extern "C" fn(*mut c_void),
    alias: unsafe extern "C" fn(*mut c_void, Reg) -> Reg,
    invariant: unsafe extern "C" fn(*mut c_void, Reg),
    boundaries: unsafe extern "C" fn(*mut c_void, usize, usize, usize),
    append: unsafe extern "C" fn(*mut c_void, usize),
    finish: unsafe extern "C" fn(*mut c_void),
}
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::reduction_kernel"] fn b_kernel(input: Input, host: Host);
    #[link_name = "bootstrap.ir_dataflow::reduction_extract"] fn b_extract(source: Node, uniform: *mut u8, ir: crate::ir::dataflow::Host) -> Extract;
}
struct State {
    kernel: ReductionKernel,
    parts: [Vec<Inst>; 4],
    uniform: Vec<u8>,
    flow: value_flow::ReductionFlow,
}
impl State {
    fn host(&mut self) -> Host {
        Host { ir: crate::ir::dataflow::HOST, context: ptr::from_mut(self).cast(),
            header, clone, remap, push_root, roots, part, uniform, extract, release_uniform,
            entry, seed, body, retain, forward, origin, alias, invariant, boundaries, append, finish }
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
    let kernel = ReductionKernel { stage, invariant_end: 0, invariants: BTreeSet::new(),
        initialize_end: 0, project_end: 0, source: Vec::new(), projected: Vec::new(),
        initial: Vec::new(), left: Vec::new(), right: Vec::new(), result: Vec::new() };
    let mut state = State { kernel, parts: Default::default(), uniform: Vec::new(), flow: Default::default() };
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
unsafe extern "C" fn clone(context: *mut c_void, part: usize, source: Node, local: usize) {
    let state = unsafe { &mut *context.cast::<State>() };
    let value = unsafe { &*source.data.cast::<ValueCode>() };
    state.parts[part] = value.code.clone();
    state.kernel.stage.registers.extend(&value.registers);
    state.kernel.stage.locals.extend(value.locals.iter().map(|(end, ty)| (end + local, ty.clone())));
}
unsafe extern "C" fn remap(context: *mut c_void, part: usize, registers: usize, locals: usize) {
    let code = unsafe { &mut (*context.cast::<State>()).parts[part] };
    map(code, crate::ir::rewrite::Remap::Offset { registers, locals });
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
unsafe extern "C" fn roots(context: *mut c_void, root: usize) -> Span {
    let values = root_values(unsafe { &mut (*context.cast::<State>()).kernel }, root);
    Span { data: values.as_mut_ptr().cast(), count: values.len(), stride: size_of::<Reg>() }
}
unsafe extern "C" fn part(context: *mut c_void, part: usize) -> Span {
    Span::of(unsafe { &(*context.cast::<State>()).parts[part] })
}
unsafe extern "C" fn uniform(context: *mut c_void, count: usize) -> *mut u8 {
    let bytes = unsafe { &mut (*context.cast::<State>()).uniform };
    *bytes = vec![0; count]; bytes.as_mut_ptr()
}
unsafe extern "C" fn extract(context: *mut c_void, part: usize, uniform: *mut u8) {
    use ReductionUniformOperation as Op;
    const OPERATIONS: [Op; 9] = [Op::Constant, Op::Uniform, Op::BufferStride, Op::Unary,
        Op::Binary, Op::Cast, Op::Math, Op::Select, Op::ConstantIntegerDivide];
    const PARTITIONS: [ReductionPartition; 3] = [ReductionPartition::Identity, ReductionPartition::Project, ReductionPartition::Combine];
    let parts = unsafe { &mut (*context.cast::<State>()).parts };
    let (prefix, suffix) = parts.split_at_mut(part);
    // extract_if is the original Vec's physical compaction and move operation.
    // Its predicate now asks B about the actual instruction, without Rust classification.
    prefix[0].extend(suffix[0].extract_if(.., |inst| {
        let result = unsafe { b_extract(Node { data: ptr::from_ref(inst).cast(), kind: 0 }, uniform, crate::ir::dataflow::HOST) };
        if result.eligible == 0 { return false; }
        let retained = OperationOrigin::from_instruction(inst);
        inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
            reason: GenerationReason::ReductionUniformExtract,
            trigger: Some(GenerationAnchor::ReductionUniformExtract {
                retained, partition: PARTITIONS[part - 1], operation: OPERATIONS[result.operation as usize],
                uniform_operand_count: result.inputs,
            }),
        }));
        true
    }));
}
unsafe extern "C" fn release_uniform(context: *mut c_void) {
    unsafe { (*context.cast::<State>()).uniform = Vec::new(); }
}
unsafe extern "C" fn entry(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<State>() };
    state.flow.entry(&mut state.parts[0], &state.kernel.stage.registers);
}
unsafe extern "C" fn seed(context: *mut c_void) { unsafe { (*context.cast::<State>()).flow.seed(); } }
unsafe extern "C" fn body(context: *mut c_void, part: usize, take: u32) {
    let state = unsafe { &mut *context.cast::<State>() };
    state.flow.body(&mut state.parts[part], &state.kernel.stage.registers, take != 0);
}
unsafe extern "C" fn retain(context: *mut c_void, part: usize) {
    let state = unsafe { &mut *context.cast::<State>() };
    state.flow.retain(&mut state.parts[part]);
}
unsafe extern "C" fn forward(context: *mut c_void, root: usize, component: usize, reg: Reg) {
    let role = match root { 3 => ReductionInputRole::Left, 4 => ReductionInputRole::Right, _ => ReductionInputRole::Source };
    unsafe { (*context.cast::<State>()).flow.forward(role, component, reg); }
}
unsafe extern "C" fn origin(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<State>() };
    state.flow.origin(&mut state.kernel.stage.generated_origin);
}
unsafe extern "C" fn alias(context: *mut c_void, reg: Reg) -> Reg {
    // Borrow only the alias owner; the disjoint root Vec has an exclusive raw view.
    unsafe { (&(*context.cast::<State>()).flow).root(reg) }
}
unsafe extern "C" fn invariant(context: *mut c_void, reg: Reg) { unsafe { (*context.cast::<State>()).kernel.invariants.insert(reg); } }
unsafe extern "C" fn boundaries(context: *mut c_void, invariant: usize, initialize: usize, project: usize) {
    let kernel = unsafe { &mut (*context.cast::<State>()).kernel };
    kernel.invariant_end = invariant; kernel.initialize_end = initialize; kernel.project_end = project;
}
unsafe extern "C" fn append(context: *mut c_void, part: usize) {
    let parts = unsafe { &mut (*context.cast::<State>()).parts };
    let body = mem::take(&mut parts[part]); parts[0].extend(body);
}
unsafe extern "C" fn finish(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<State>() };
    state.kernel.stage.code = mem::take(&mut state.parts[0]);
}
