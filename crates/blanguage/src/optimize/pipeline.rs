//! Physical ownership at the B optimizer's synchronous native boundary.
//! B selects fusion, passes, iteration, retention and access. Rust retains the
//! original typed containers, source anchors, scoped Worker and actual errors.
use super::*;
use std::{ffi::c_void, mem::size_of, ptr};

#[derive(Clone, Copy)]
#[repr(C)]
struct Span { data: *const c_void, count: usize, stride: usize }
impl Span {
    const EMPTY: Self = Self { data: ptr::null(), count: 0, stride: 0 };
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
    begin: unsafe extern "C" fn(*mut c_void),
    next: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    previous: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    stage: unsafe extern "C" fn(*const c_void) -> StageView,
    append: unsafe extern "C" fn(*mut c_void),
    merge: unsafe extern "C" fn(*mut c_void),
}
#[repr(C)]
struct ProgramHost {
    fusion: FusionHost,
    batch: unsafe extern "C" fn(*mut c_void) -> u32,
    remove: unsafe extern "C" fn(*mut c_void) -> usize,
    origin: unsafe extern "C" fn(*mut c_void),
    finish: unsafe extern "C" fn(*mut c_void),
}
#[repr(C)]
struct EmptyResult { keep: u32, boundary: u32, carry: u32 }
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
#[repr(C)]
struct Root { kind: u32, tile: usize, words: Span, registers: Span, count: usize }
#[repr(C)]
struct RootResult { keep: u32, count: usize, changed: u32 }
#[repr(C)]
struct MapInput {
    code: Span, tiles: Span, prefix: Field, selection: Field,
    flags: *mut u8, binding_count: usize,
    reads: *mut usize, writes: *mut usize, cross: *mut usize, cross_count: usize,
}
#[repr(C)]
struct AccessCounts { reads: usize, writes: usize, cross: usize }
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::optimize_program"] fn b_program(host: ProgramHost);
    #[link_name = "bootstrap.ir_dataflow::optimize_empty"] fn b_empty(stage: StageView, carry: u32) -> EmptyResult;
    #[link_name = "bootstrap.ir_dataflow::optimize_stage"] fn b_stage(host: StageHost);
    #[link_name = "bootstrap.ir_dataflow::optimize_value"] fn b_value(value: *mut c_void, host: ValueHost);
    #[link_name = "bootstrap.ir_dataflow::optimize_root"] fn b_root(input: Root, preserve: u32, host: ReadHost) -> RootResult;
    #[link_name = "bootstrap.ir_dataflow::optimize_bindings"]
    fn b_bindings(input: MapInput, ir: crate::ir::dataflow::Host, host: ReadHost) -> AccessCounts;
}

enum StageInput {
    Empty,
    Initial(std::vec::IntoIter<Stage>),
    Fused(std::vec::IntoIter<(Stage, bool)>),
}
impl StageInput {
    fn next(&mut self) -> Option<(Stage, bool)> {
        match self {
            Self::Empty => None,
            Self::Initial(input) => input.next().map(|stage| (stage, true)),
            Self::Fused(input) => input.next(),
        }
    }
}
struct ProgramState<'a> {
    program: &'a mut Program, worker: &'a crate::compiler::Worker, bindings: usize,
    stages: Vec<(Stage, bool)>, input: StageInput,
    current: Option<(Stage, bool)>, removed: Vec<OperationOrigin>, error: Option<std::io::Error>,
}
pub(super) fn run(program: &mut Program, worker: &crate::compiler::Worker) -> std::io::Result<()> {
    let bindings = program.buffers.len();
    let input = StageInput::Initial(std::mem::take(&mut program.stages).into_iter());
    let mut state = ProgramState {
        program, worker, bindings, stages: Vec::new(), input,
        current: None, removed: Vec::new(), error: None,
    };
    let host = ProgramHost {
        fusion: FusionHost {
            ir: crate::ir::dataflow::HOST, context: ptr::from_mut(&mut state).cast(),
            begin, next, previous, stage: stage_view, append, merge,
        }, batch, remove, origin, finish,
    };
    unsafe { b_program(host); }
    state.error.map_or(Ok(()), Err)
}
unsafe extern "C" fn begin(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    state.input = StageInput::Fused(std::mem::take(&mut state.stages).into_iter());
}
unsafe extern "C" fn next(context: *mut c_void) -> *mut c_void {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    state.current = match state.input.next() {
        Some(current) => Some(current),
        None => { state.input = StageInput::Empty; None }
    };
    state.current.as_mut().map_or(ptr::null_mut(), |(stage, _)| ptr::from_mut(stage).cast())
}
unsafe extern "C" fn previous(context: *mut c_void) -> *mut c_void {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    state.stages.last_mut().map_or(ptr::null_mut(), |(stage, _)| ptr::from_mut(stage).cast())
}
unsafe extern "C" fn append(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    state.stages.push(state.current.take().unwrap());
}
unsafe extern "C" fn merge(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    let (mut stage, _) = state.current.take().unwrap();
    let (previous, changed) = state.stages.last_mut().unwrap();
    let following = OperationOrigin {
        source_site: stage.operation_source.clone(), generated_origin: stage.generated_origin.take(),
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
    let base = previous.registers.len();
    let local = previous.locals.last_key_value().map_or(0, |(end, _)| *end);
    map(&mut stage.code, crate::ir::rewrite::Remap::Offset { registers: base, locals: local });
    previous.registers.extend(stage.registers);
    previous.locals.extend(stage.locals.into_iter().map(|(end, ty)| (end + local, ty)));
    previous.code.extend(stage.code);
    union_bindings(&mut previous.reads, stage.reads);
    union_bindings(&mut previous.writes, stage.writes);
    retain_stage_guarantees(previous, &stage.write_regions);
    previous.prefix_guarantees.extend(stage.prefix_guarantees);
    previous.element_guarantees.extend(stage.element_guarantees);
    for (binding, written) in stage.write_regions {
        previous.write_regions.entry(binding).or_default().extend(written);
    }
    union_bindings(&mut previous.cross_access, stage.cross_access);
    *changed = true;
}
unsafe extern "C" fn batch(context: *mut c_void) -> u32 {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    // Only B-marked actual work borrows Stage. The original scoped runtime
    // joins every consumer before these references or this Vec can be moved.
    let work = state.stages.iter_mut().filter_map(|(stage, changed)| changed.then_some(stage)).collect::<Vec<_>>();
    let bindings = state.bindings;
    let result = crate::parallel::try_map(state.worker, work, |_| (), |_, stage| {
        run_stage(stage, bindings); Ok::<_, std::convert::Infallible>(())
    });
    match result {
        Ok(_) => { for (_, changed) in &mut state.stages { *changed = false; } 1 }
        Err(crate::parallel::Error::Resource(error)) => { state.error = Some(error); 0 }
        Err(crate::parallel::Error::Work(error)) => match error {},
    }
}
unsafe extern "C" fn remove(context: *mut c_void) -> usize {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    let mut carry = 0;
    state.stages.retain_mut(|(stage, _)| {
        let result = unsafe { b_empty(stage_view(ptr::from_ref(stage).cast()), carry) };
        carry = result.carry;
        stage.boundary_before = result.boundary != 0;
        if result.keep == 0 {
            state.removed.push(OperationOrigin {
                source_site: stage.operation_source.clone(), generated_origin: stage.generated_origin.clone(),
            });
        }
        result.keep != 0
    });
    state.removed.len()
}
unsafe extern "C" fn origin(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    let retained = state.program.generated_origin.take();
    state.program.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
        reason: GenerationReason::EmptyMapRemoval,
        trigger: Some(GenerationAnchor::EmptyMapRemoval { retained, stages: std::mem::take(&mut state.removed) }),
    }));
}
unsafe extern "C" fn finish(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<ProgramState<'_>>() };
    state.program.stages = std::mem::take(&mut state.stages).into_iter().map(|(stage, _)| stage).collect();
}

struct StageState<'a> { stage: &'a mut Stage, bindings: usize, words: BTreeSet<SharedWord> }
fn run_stage(stage: &mut Stage, bindings: usize) {
    let mut state = StageState { stage, bindings, words: BTreeSet::new() };
    unsafe { b_stage(StageHost {
        ir: crate::ir::dataflow::HOST, context: ptr::from_mut(&mut state).cast(), values: VALUE_HOST,
        stage: stage_state_view, custom, flow: stage_flow, eliminate: stage_eliminate, words: stage_words,
        prune, select, bindings: bindings_map, release_words, compact: stage_compact,
    }); }
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
    let mut changed = 0;
    state.stage.code.retain_mut(|inst| {
        let input = match &mut inst.kind {
            InstKind::TilePublish(value) => {
                let count = value.values.len();
                let base = value.values.as_mut_ptr();
                let (words, registers) = if count == 0 { (ptr::null_mut(), ptr::null_mut()) } else {
                    unsafe { (ptr::addr_of_mut!((*base).0).cast::<c_void>(), ptr::addr_of_mut!((*base).1).cast::<c_void>()) }
                };
                Root { kind: 1, tile: value.tile,
                    words: Span { data: words, count, stride: size_of::<(u32, Reg)>() },
                    registers: Span { data: registers, count, stride: size_of::<(u32, Reg)>() }, count }
            }
            InstKind::TileCollective(value) => Root {
                kind: 2, tile: value.tile, words: Span::EMPTY, registers: Span::EMPTY, count: value.results.len(),
            },
            _ => Root { kind: 0, tile: 0, words: Span::EMPTY, registers: Span::EMPTY, count: 0 },
        };
        let result = unsafe { b_root(input, preserve, host) };
        changed |= result.changed;
        if let InstKind::TilePublish(value) = &mut inst.kind { value.values.truncate(result.count); }
        result.keep != 0
    });
    changed
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
