//! Physical ownership at the B optimizer's synchronous native boundary.
//! B selects fusion, passes, iteration, retention and access. Rust retains the
//! original typed containers, source anchors, scoped Worker and actual errors.
use super::*;
use std::{ffi::c_void, mem::size_of, ptr};
use crate::ir::rewrite;

#[derive(Clone, Copy)]
#[repr(C)]
struct Span { data: *const c_void, count: usize, stride: usize }
impl Span {
    fn of<T>(values: &[T]) -> Self {
        Self { data: values.as_ptr().cast(), count: values.len(), stride: size_of::<T>() }
    }
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Field { binding: usize, word: u32, present: u32 }
impl From<Option<(usize, u32)>> for Field {
    fn from(value: Option<(usize, u32)>) -> Self {
        value.map_or_else(Self::default, |(binding, word)| Self { binding, word, present: 1 })
    }
}
#[repr(C)]
struct StageView {
    code: Span, reads: Span, writes: Span, cross: Span,
    domain: usize, prefix: Field, selection: Field, operation: u32, boundary: u32,
}
unsafe extern "C" fn stage_view(owner: *const c_void) -> StageView {
    let stage = unsafe { &*owner.cast::<Stage>() };
    let operation = match stage.operation {
        Operation::Map => 0, Operation::Reduce(_) => 1, Operation::CustomReduce(_) => 2,
        Operation::CustomScan(_) => 3, Operation::CustomSegmentedReduce(_) => 4,
        Operation::SegmentedReduce(_) => 5, Operation::ScanExclusive(_) => 6,
        Operation::Compact(_) => 7, Operation::StableSort(_) => 8,
    };
    StageView {
        code: Span::of(&stage.code), reads: Span::of(&stage.reads), writes: Span::of(&stage.writes),
        cross: Span::of(&stage.cross_access), domain: stage.domain,
        prefix: stage.prefix.map(|v| (v.binding, v.word)).into(),
        selection: stage.selection.map(|v| (v.binding, v.word)).into(),
        operation, boundary: u32::from(stage.boundary_before),
    }
}
#[repr(C)]
struct FusionHost {
    ir: crate::ir::dataflow::Host, context: *mut c_void,
    at: unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void,
    stage: unsafe extern "C" fn(*const c_void) -> StageView,
    swap: unsafe extern "C" fn(*mut c_void, usize, usize),
    truncate: unsafe extern "C" fn(*mut c_void, usize),
    rewrite: rewrite::Host,
    layout: unsafe extern "C" fn(*const c_void, *mut c_void) -> FusionLayout,
    origin: unsafe extern "C" fn(*mut c_void, *mut c_void),
    retain: unsafe extern "C" fn(*mut c_void, *const c_void),
    commit: unsafe extern "C" fn(*mut c_void, usize, usize, usize),
    joined: unsafe extern "C" fn(*mut c_void, usize),
}
#[repr(C)]
struct FusionOffsets { registers: usize, locals: usize }
#[repr(C)]
struct FusionLayout { code: rewrite::Span, offsets: FusionOffsets }
#[repr(C)]
struct ProgramHost {
    fusion: FusionHost,
    batch: unsafe extern "C" fn(*mut c_void, u32) -> u32,
    removed: unsafe extern "C" fn(*mut c_void, usize),
    boundary: unsafe extern "C" fn(*mut c_void, usize),
    origin: unsafe extern "C" fn(*mut c_void),
}
#[repr(C)]
struct Values { project: *mut c_void, identity: *mut c_void, combine: *mut c_void }
#[derive(Clone, Copy)]
#[repr(C)]
struct ValueHost {
    flow: unsafe extern "C" fn(*mut c_void),
    eliminate: unsafe extern "C" fn(*mut c_void),
    compact: unsafe extern "C" fn(*mut c_void),
}
const VALUE_HOST: ValueHost = ValueHost { flow: value_flow, eliminate: value_eliminate, compact: value_compact };
#[repr(C)]
struct StageHost {
    ir: crate::ir::dataflow::Host, context: *mut c_void, values: ValueHost,
    stage: unsafe extern "C" fn(*const c_void) -> StageView,
    custom: unsafe extern "C" fn(*mut c_void) -> Values,
    flow: unsafe extern "C" fn(*mut c_void),
    eliminate: unsafe extern "C" fn(*mut c_void),
    words: unsafe extern "C" fn(*mut c_void),
    prune: unsafe extern "C" fn(*mut c_void, u32) -> u32,
    select: unsafe extern "C" fn(*mut c_void) -> u32,
    bindings: unsafe extern "C" fn(*mut c_void),
    release_words: unsafe extern "C" fn(*mut c_void),
    compact: unsafe extern "C" fn(*mut c_void),
}
#[derive(Clone, Copy)]
#[repr(C)]
struct ReadHost {
    context: *const c_void,
    contains: unsafe extern "C" fn(*const c_void, u32, usize, u32) -> u32,
    scalar_bytes: unsafe extern "C" fn(*const c_void, usize) -> u32,
}
#[derive(PartialEq, Eq)]
#[repr(u32)]
enum RootAction { Keep, Drop, Trim }
#[repr(C)]
struct RootEdit { action: RootAction, count: usize }
#[repr(C)]
struct MapInput {
    code: Span, tiles: Span, prefix: Field, selection: Field,
    flags: *mut u8, binding_count: usize,
    reads: *mut usize, writes: *mut usize, cross: *mut usize, cross_count: usize,
}
#[repr(C)]
struct AccessCounts { reads: usize, writes: usize, cross: usize }
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::optimize_program"] fn b_program(count: usize, host: ProgramHost);
    #[link_name = "bootstrap.ir_dataflow::optimize_stage"] fn b_stage(host: StageHost, has_tiles: u32);
    #[link_name = "bootstrap.ir_dataflow::optimize_value"] fn b_value(value: *mut c_void, host: ValueHost);
    #[link_name = "bootstrap.ir_dataflow::optimize_root"]
    fn b_root(instruction: *mut c_void, preserve: u32, host: ReadHost, rewrite: crate::ir::rewrite::Host) -> RootEdit;
    #[link_name = "bootstrap.ir_dataflow::optimize_bindings"]
    fn b_bindings(input: MapInput, ir: crate::ir::dataflow::Host, host: ReadHost) -> AccessCounts;
}

struct ProgramState<'a> {
    program: &'a mut Program,
    worker: &'a crate::compiler::Worker,
    joined: Vec<usize>,
    removed: Vec<OperationOrigin>,
    error: Option<std::io::Error>,
}
pub(super) fn run(program: &mut Program, worker: &crate::compiler::Worker) -> std::io::Result<()> {
    let count = program.stages.len();
    let mut state = ProgramState {
        program, worker, joined: Vec::new(), removed: Vec::new(), error: None,
    };
    let host = ProgramHost {
        fusion: FusionHost {
            ir: crate::ir::dataflow::HOST, context: ptr::from_mut(&mut state).cast(),
            at: stage_at, stage: stage_view, swap: swap_stages, truncate: truncate_stages,
            rewrite: rewrite::HOST, layout: fusion_layout, origin: fusion_origin,
            retain: fusion_retain, commit: fusion_commit, joined,
        },
        batch, removed: removed_stage, boundary: carry_boundary, origin,
    };
    unsafe { b_program(count, host); }
    state.error.map_or(Ok(()), Err)
}
unsafe extern "C" fn stage_at(context: *mut c_void, index: usize) -> *mut c_void {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    // A synchronous B step uses the slot until movement or truncation of the table.
    unsafe { state.program.stages.as_mut_ptr().add(index).cast() }
}
unsafe extern "C" fn swap_stages(context: *mut c_void, first: usize, second: usize) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    state.program.stages.swap(first, second);
}
unsafe extern "C" fn truncate_stages(context: *mut c_void, count: usize) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    state.program.stages.truncate(count);
}
unsafe extern "C" fn fusion_layout(previous: *const c_void, current: *mut c_void) -> FusionLayout {
    let previous = unsafe { &*previous.cast::<Stage>() };
    let current = unsafe { &mut *current.cast::<Stage>() };
    FusionLayout {
        code: rewrite::Span::slice(&mut current.code),
        offsets: FusionOffsets {
            registers: previous.registers.len(),
            locals: previous.locals.last_key_value().map_or(0, |(&end, _)| end),
        },
    }
}
unsafe extern "C" fn fusion_origin(previous: *mut c_void, current: *mut c_void) {
    let previous = unsafe { &mut *previous.cast::<Stage>() };
    let stage = unsafe { &mut *current.cast::<Stage>() };
    let following = OperationOrigin {
        source_site: stage.operation_source.take(), generated_origin: stage.generated_origin.take(),
    };
    if let Some(GenerationAnchor::StageFusion { stages }) = previous.generated_origin.as_mut()
        .and_then(std::sync::Arc::get_mut).and_then(|origin| origin.trigger.as_mut())
    {
        stages.push(following);
    } else {
        let retained = OperationOrigin {
            source_site: previous.operation_source.clone(), generated_origin: previous.generated_origin.take(),
        };
        previous.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
            reason: GenerationReason::StageFusion,
            trigger: Some(GenerationAnchor::StageFusion { stages: vec![retained, following] }),
        }));
    }
}
unsafe extern "C" fn fusion_retain(previous: *mut c_void, current: *const c_void) {
    let previous = unsafe { &mut *previous.cast::<Stage>() };
    let stage = unsafe { &*current.cast::<Stage>() };
    retain_stage_guarantees(previous, &stage.write_regions);
}
unsafe extern "C" fn fusion_commit(context: *mut c_void, target: usize, source: usize, local: usize) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    let (retained, following) = state.program.stages.split_at_mut(source);
    let previous = &mut retained[target];
    let stage = &mut following[0];
    previous.registers.extend(std::mem::take(&mut stage.registers));
    previous.locals.extend(std::mem::take(&mut stage.locals).into_iter().map(|(end, ty)| (end + local, ty)));
    previous.code.extend(std::mem::take(&mut stage.code));
    union_bindings(&mut previous.reads, std::mem::take(&mut stage.reads));
    union_bindings(&mut previous.writes, std::mem::take(&mut stage.writes));
    previous.prefix_guarantees.extend(std::mem::take(&mut stage.prefix_guarantees));
    previous.element_guarantees.extend(std::mem::take(&mut stage.element_guarantees));
    for (binding, written) in std::mem::take(&mut stage.write_regions) {
        previous.write_regions.entry(binding).or_default().extend(written);
    }
    union_bindings(&mut previous.cross_access, std::mem::take(&mut stage.cross_access));
}
unsafe extern "C" fn joined(context: *mut c_void, index: usize) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    state.joined.push(index);
}
unsafe extern "C" fn batch(context: *mut c_void, joined_only: u32) -> u32 {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    let bindings = state.program.buffers.len();
    let mut stages = state.program.stages.iter_mut();
    let work = if joined_only == 0 {
        stages.collect()
    } else {
        let mut next = 0;
        std::mem::take(&mut state.joined).into_iter().map(|index| {
            let stage = stages.nth(index - next).unwrap();
            next = index + 1;
            stage
        }).collect()
    };
    let result = crate::parallel::try_map(state.worker, work, |_| (), |_, stage| {
        run_stage(stage, bindings); Ok::<_, std::convert::Infallible>(())
    });
    match result {
        Ok(_) => 1,
        Err(crate::parallel::Error::Resource(error)) => { state.error = Some(error); 0 }
        Err(crate::parallel::Error::Work(error)) => match error {},
    }
}
unsafe extern "C" fn removed_stage(context: *mut c_void, index: usize) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    let stage = &mut state.program.stages[index];
    state.removed.push(OperationOrigin {
        source_site: stage.operation_source.take(), generated_origin: stage.generated_origin.take(),
    });
}
unsafe extern "C" fn carry_boundary(context: *mut c_void, index: usize) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    state.program.stages[index].boundary_before = true;
}
unsafe extern "C" fn origin(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    let retained = state.program.generated_origin.take();
    state.program.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
        reason: GenerationReason::EmptyMapRemoval,
        trigger: Some(GenerationAnchor::EmptyMapRemoval { retained, stages: std::mem::take(&mut state.removed) }),
    }));
}
struct StageState<'a> { stage: &'a mut Stage, bindings: usize, words: BTreeSet<SharedWord> }
fn run_stage(stage: &mut Stage, bindings: usize) {
    let has_tiles = u32::from(!stage.tiles.is_empty());
    let mut state = StageState { stage, bindings, words: BTreeSet::new() };
    unsafe { b_stage(StageHost {
        ir: crate::ir::dataflow::HOST, context: ptr::from_mut(&mut state).cast(), values: VALUE_HOST,
        stage: stage_state_view, custom, flow: stage_flow, eliminate: stage_eliminate, words: stage_words,
        prune, select, bindings: bindings_map, release_words, compact: stage_compact,
    }, has_tiles); }
}
unsafe extern "C" fn stage_state_view(context: *const c_void) -> StageView {
    let state = unsafe { &*context.cast::<StageState<'_>>() };
    unsafe { stage_view(ptr::from_ref(&*state.stage).cast()) }
}
unsafe extern "C" fn custom(context: *mut c_void) -> Values {
    let state = unsafe { &mut *context.cast::<StageState<'_>>() };
    match &mut state.stage.operation {
        Operation::CustomReduce(value)
        | Operation::CustomScan(CustomScan { values: value, .. })
        | Operation::CustomSegmentedReduce(CustomSegmentedReduction { values: value, .. }) => Values {
            project: value.project.as_mut().map_or(ptr::null_mut(), |v| ptr::from_mut(v).cast()),
            identity: ptr::from_mut(&mut value.identity).cast(), combine: ptr::from_mut(&mut value.combine).cast(),
        },
        _ => Values { project: ptr::null_mut(), identity: ptr::null_mut(), combine: ptr::null_mut() },
    }
}
unsafe extern "C" fn stage_flow(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<StageState<'_>>() };
    value_flow::run(&mut state.stage.code, &state.stage.registers);
}
unsafe extern "C" fn stage_eliminate(context: *mut c_void) {
    let stage = &mut unsafe { &mut *context.cast::<StageState<'_>>() }.stage;
    let removed = eliminate::run(&mut stage.code, &mut BTreeSet::new(), &BTreeSet::new(),
        &BTreeSet::new(), &BTreeSet::new(), &stage.registers);
    GeneratedOrigin::eliminated_operations(&mut stage.generated_origin, removed);
}
unsafe extern "C" fn stage_words(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<StageState<'_>>() };
    state.words = state.stage.storage_words();
}
unsafe extern "C" fn release_words(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<StageState<'_>>() };
    state.words = BTreeSet::new();
}
struct ReadState<'a> { words: &'a BTreeSet<SharedWord>, registers: &'a [Scalar] }
impl ReadState<'_> {
    fn host(&self) -> ReadHost { ReadHost { context: ptr::from_ref(self).cast(), contains, scalar_bytes } }
}
unsafe extern "C" fn contains(context: *const c_void, kind: u32, index: usize, word: u32) -> u32 {
    let state = unsafe { &*context.cast::<ReadState<'_>>() };
    let value = match kind {
        0 => SharedWord::Published { tile: index, word },
        _ => SharedWord::Input { binding: index, word },
    };
    u32::from(state.words.contains(&value))
}
unsafe extern "C" fn scalar_bytes(context: *const c_void, reg: usize) -> u32 {
    unsafe { &*context.cast::<ReadState<'_>>() }.registers[reg].size()
}
unsafe extern "C" fn prune(context: *mut c_void, preserve: u32) -> u32 {
    let state = unsafe { &mut *context.cast::<StageState<'_>>() };
    let read = ReadState { words: &state.words, registers: &state.stage.registers };
    let host = read.host();
    let mut changed = false;
    state.stage.code.retain_mut(|inst| {
        let edit = unsafe { b_root(ptr::from_mut(inst).cast(), preserve, host, crate::ir::rewrite::HOST) };
        changed |= edit.action != RootAction::Keep;
        if edit.action == RootAction::Trim
            && let InstKind::TilePublish(publication) = &mut inst.kind
        {
            publication.values.truncate(edit.count);
        }
        edit.action != RootAction::Drop
    });
    u32::from(changed)
}
unsafe extern "C" fn select(context: *mut c_void) -> u32 {
    let stage = &mut unsafe { &mut *context.cast::<StageState<'_>>() }.stage;
    let local_count = stage.locals.last_key_value().map_or(0, |(&end, _)| end);
    u32::from(selection::run(&mut stage.code, &stage.registers, stage.domain, local_count))
}
unsafe extern "C" fn bindings_map(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<StageState<'_>>() };
    let stage = &mut state.stage;
    let mut flags = vec![0; state.bindings];
    stage.reads.clear(); stage.reads.reserve(state.bindings);
    stage.writes.clear(); stage.writes.reserve(state.bindings);
    let input = MapInput {
        code: Span::of(&stage.code), tiles: Span::of(&stage.tiles),
        prefix: stage.prefix.map(|v| (v.binding, v.word)).into(),
        selection: stage.selection.map(|v| (v.binding, v.word)).into(),
        flags: flags.as_mut_ptr(), binding_count: state.bindings,
        reads: stage.reads.as_mut_ptr(), writes: stage.writes.as_mut_ptr(),
        cross: stage.cross_access.as_mut_ptr(), cross_count: stage.cross_access.len(),
    };
    let read = ReadState { words: &state.words, registers: &stage.registers };
    unsafe {
        let counts = b_bindings(input, crate::ir::dataflow::HOST, read.host());
        stage.reads.set_len(counts.reads); stage.writes.set_len(counts.writes);
        stage.cross_access.truncate(counts.cross);
    }
    stage.write_regions.retain(|binding, _| flags[*binding] & 2 != 0);
}
unsafe extern "C" fn stage_compact(context: *mut c_void) {
    let stage = &mut unsafe { &mut *context.cast::<StageState<'_>>() }.stage;
    compact_slots(&mut stage.code, &mut stage.registers, &mut stage.locals, &mut [], &mut []);
}
pub(super) fn run_value(value: &mut ValueCode) { unsafe { b_value(ptr::from_mut(value).cast(), VALUE_HOST); } }
unsafe extern "C" fn value_flow(context: *mut c_void) {
    value_flow::run_value(unsafe { &mut *context.cast::<ValueCode>() });
}
unsafe extern "C" fn value_eliminate(context: *mut c_void) {
    let value = unsafe { &mut *context.cast::<ValueCode>() };
    let mut needed = value.outputs.iter().copied().map(PrivateSlot::Register).collect();
    let removed = eliminate::run(&mut value.code, &mut needed, &BTreeSet::new(),
        &BTreeSet::new(), &BTreeSet::new(), &value.registers);
    GeneratedOrigin::eliminated_operations(&mut value.generated_origin, removed);
}
unsafe extern "C" fn value_compact(context: *mut c_void) {
    let value = unsafe { &mut *context.cast::<ValueCode>() };
    compact_slots(&mut value.code, &mut value.registers, &mut value.locals, &mut value.inputs, &mut value.outputs);
}
