//! 调用方持有的一次编译图；只调度本次真实依赖，不保留后台消费者。
use super::{ByteField, Compilation, ObjectCompilation, ObjectKind, Result, inputs::{Capture, CapturedUnit, ProviderInterface}};
use crate::{Compiler, ast::Target, object::Object, source_path};
use std::{collections::BTreeMap, ffi::{OsStr, c_void}, marker::PhantomData, mem::MaybeUninit, os::windows::ffi::OsStrExt, path::{Path, PathBuf}, ptr, sync::{Arc, Condvar, Mutex, MutexGuard}, thread};
#[derive(Clone, Copy)]
pub enum Dependency<'a> {
    Unit(usize),
    Object(&'a Object),
}
#[derive(Clone, Copy)]
pub enum UnitKind<'a> {
    Object(ObjectKind<'a>),
    Entry { entry: &'a str, target: Target },
}
pub struct Unit<'a> {
    pub path: &'a Path,
    pub root: &'a Path,
    pub kind: UnitKind<'a>,
    pub dependencies: &'a [Dependency<'a>],
    /// 对象为文件路径；入口映像为原build_entry输出目录。图内每个正式文件只属于一个单元。
    pub output: &'a Path,
}
pub enum Output {
    Object(Arc<ObjectCompilation>),
    Entry(Compilation),
}
impl Output {
    /// 交接入口映像；COFF对象不能按CPU／GPU程序装入。
    pub fn into_entry(self) -> Result<Compilation> {
        match self {
            Self::Entry(compilation) => Ok(compilation),
            Self::Object(object) => Err(format!("COFF对象{}不是入口映像", object.object.name()).into()),
        }
    }
}
struct Prepared<'a> {
    path: PathBuf,
    root: PathBuf,
    artifact: PathBuf,
    replace_existing: bool,
    kind: UnitKind<'a>,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GraphRange {
    start: usize,
    count: usize,
}

impl GraphRange {
    fn slice<T>(self, values: &[T]) -> &[T] {
        &values[self.start..self.start + self.count]
    }
}

#[repr(C)]
struct EntryKey { path: ByteField, root: ByteField, providers: GraphRange, unit: usize }
#[repr(C)]
struct CaptureSource {
    context: *mut c_void,
    unit_count: usize,
    entry_count: usize,
    ids: *const usize,
    key: unsafe extern "C" fn(*mut c_void, usize, *mut EntryKey) -> bool,
    capture: unsafe extern "C" fn(*mut c_void, usize, bool),
}

#[repr(C)]
struct GraphRow {
    dependencies: *const c_void,
    count: usize,
    object: usize,
}

#[repr(C)]
struct GraphEdge {
    index: usize,
    external: usize,
}

#[repr(C)]
struct GraphSource<'object> {
    units: *const c_void,
    count: usize,
    edge_count: usize,
    row: unsafe extern "C" fn(*const c_void, usize) -> GraphRow,
    edge: unsafe extern "C" fn(*const c_void, usize) -> GraphEdge,
    objects: PhantomData<&'object Object>,
}

unsafe extern "C" fn graph_row(units: *const c_void, index: usize) -> GraphRow {
    let unit = unsafe { &*units.cast::<Unit<'_>>().add(index) };
    GraphRow {
        dependencies: unit.dependencies.as_ptr().cast(),
        count: unit.dependencies.len(),
        object: usize::from(matches!(unit.kind, UnitKind::Object(_))),
    }
}

unsafe extern "C" fn graph_edge(dependencies: *const c_void, index: usize) -> GraphEdge {
    match unsafe { *dependencies.cast::<Dependency<'_>>().add(index) } {
        Dependency::Unit(index) => GraphEdge { index, external: 0 },
        Dependency::Object(object) => GraphEdge { index: ptr::from_ref(object).expose_provenance(), external: 1 },
    }
}

#[repr(C)]
struct GraphOutput {
    providers: *mut GraphRange,
    following: *mut GraphRange,
    waiting: *mut usize,
    followers: *mut usize,
    ready: *mut usize,
    counts: *mut ScheduleCounts,
}

const OUT_OF_RANGE: u64 = 1;
const DEPENDENCY_CYCLE: u64 = 2;
const ENTRY_PROVIDER: u64 = 3;

#[repr(C)]
struct GraphPlan<'object> {
    ids: *const usize,
    count: usize,
    status: u64,
    unit: usize,
    objects: PhantomData<&'object Object>,
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_graph::plan"]
    fn build_dependency_plan<'object>(source: GraphSource<'object>, output: GraphOutput) -> GraphPlan<'object>;
    #[link_name = "bootstrap.build_graph::release_plan"]
    fn release_dependency_plan(ids: *const usize);
    #[link_name = "bootstrap.build_graph::capture_sources"]
    fn capture_sources(source: CaptureSource, indices: *mut usize) -> i32;
    #[link_name = "bootstrap.build_schedule::run_worker"]
    fn run_worker(context: *mut c_void, host: WorkHost);
}

// The planner transfers an immutable allocation and retains no input borrow
// except the original Object references. The process heap is shared by workers.
unsafe impl Send for GraphPlan<'_> {}
unsafe impl Sync for GraphPlan<'_> {}

impl<'object> GraphPlan<'object> {
    fn ids(&self) -> &[usize] {
        if self.count == 0 {
            &[]
        } else {
            // The B planner returned count initialized indices in this allocation.
            unsafe { std::slice::from_raw_parts(self.ids, self.count) }
        }
    }

    fn dependencies(&self, row: GraphRange, unit_count: usize) -> impl Iterator<Item = Dependency<'object>> + '_ {
        let ids = self.ids();
        row.slice(ids).iter().copied().map(move |id| {
            if id < unit_count {
                Dependency::Unit(id)
            } else {
                let object = ptr::with_exposed_provenance::<Object>(ids[id - unit_count]);
                // Each identity in the prefix came from a borrowed Dependency::Object.
                Dependency::Object(unsafe { &*object })
            }
        })
    }
}

impl Drop for GraphPlan<'_> {
    fn drop(&mut self) {
        unsafe { release_dependency_plan(self.ids); }
    }
}

struct DependencyPlan<'a> {
    plan: GraphPlan<'a>,
    provider_rows: Vec<GraphRange>,
    schedule: Schedule,
}

struct Schedule {
    following: Vec<GraphRange>,
    followers: Vec<usize>,
    waiting: Vec<usize>,
    ready: Vec<usize>,
    counts: ScheduleCounts,
}

#[repr(C)]
struct ScheduleCounts {
    ready: usize,
    unfinished: usize,
}

fn dependency_plan<'a>(units: &[Unit<'a>]) -> Result<DependencyPlan<'a>> {
    let edge_count = units.iter().map(|unit| unit.dependencies.len()).sum();
    let mut provider_rows = vec![GraphRange::default(); units.len()];
    let mut following_rows = vec![GraphRange::default(); units.len()];
    let mut waiting = vec![0; units.len()];
    let mut followers = vec![0; edge_count];
    let mut ready = vec![0; units.len()];
    let mut counts = MaybeUninit::uninit();
    let plan = unsafe {
        build_dependency_plan(
            GraphSource {
                units: units.as_ptr().cast(),
                count: units.len(),
                edge_count,
                row: graph_row,
                edge: graph_edge,
                objects: PhantomData::<&'a Object>,
            },
            GraphOutput {
                providers: provider_rows.as_mut_ptr(),
                following: following_rows.as_mut_ptr(),
                waiting: waiting.as_mut_ptr(),
                followers: followers.as_mut_ptr(),
                ready: ready.as_mut_ptr(),
                counts: counts.as_mut_ptr(),
            },
        )
    };
    let index = plan.unit;
    match plan.status {
        0 => {}
        OUT_OF_RANGE => {
            return Err(std::io::Error::other(format!("构建单元引用越界：{index}")).into());
        }
        DEPENDENCY_CYCLE => {
            return Err(format!("构建依赖形成循环：单元{index} {}", units[index].path.display()).into());
        }
        ENTRY_PROVIDER => {
            return Err(format!("单元{index}的入口映像不是COFF模块提供者").into());
        }
        status => return Err(std::io::Error::from_raw_os_error(status as i32).into()),
    }
    let schedule = Schedule {
        following: following_rows, followers, waiting, ready,
        // The successful plan initializes both counts before returning.
        counts: unsafe { counts.assume_init() },
    };
    Ok(DependencyPlan { plan, provider_rows, schedule })
}

enum Provider<'a> {
    Built(Arc<ObjectCompilation>),
    Object(&'a Object),
}
impl Provider<'_> {
    fn object(&self) -> &Object {
        match self { Self::Built(value) => &value.object, Self::Object(value) => value }
    }
}

struct State {
    schedule: Schedule,
    outputs: Vec<Option<Output>>,
    // Zero gives a worker-start error precedence over any unit error.
    // Unit positions use index + 1, so the original input order breaks ties
    // between failures from work already in flight.
    failure: Option<(usize, Box<dyn std::error::Error + Send + Sync>)>,
}
impl State {
    fn failure_rank(&self) -> usize {
        self.failure.as_ref().map_or(usize::MAX, |(rank, _)| *rank)
    }
}

#[repr(C)]
struct WorkQueue {
    ready: *mut usize,
    waiting: *mut usize,
    following: *const GraphRange,
    followers: *const usize,
    counts: *mut ScheduleCounts,
    failure: usize,
}
#[repr(C)]
struct WorkResult {
    failure: usize,
    success: u32,
}
#[repr(C)]
struct WorkHost {
    acquire: unsafe extern "C" fn(*mut c_void) -> WorkQueue,
    wait: unsafe extern "C" fn(*mut c_void) -> WorkQueue,
    execute: unsafe extern "C" fn(*mut c_void, usize) -> WorkResult,
    retire: unsafe extern "C" fn(*mut c_void, u32) -> WorkQueue,
    release: unsafe extern "C" fn(*mut c_void),
}

struct GraphExecution<'graph, 'source> {
    compiler: &'graph Compiler,
    prepared: &'graph [Prepared<'source>],
    inputs: &'graph [super::Inputs],
    capture_indices: &'graph [usize],
    provider_rows: &'graph [GraphRange],
    plan: &'graph GraphPlan<'source>,
    state: Mutex<State>,
    changed: Condvar,
}
impl GraphExecution<'_, '_> {
    fn run_worker(&self) {
        let mut worker = GraphWorker { execution: self, guard: None, result: None };
        unsafe { run_worker(ptr::from_mut(&mut worker).cast(), WorkHost {
            acquire: acquire_graph, wait: wait_graph, execute: execute_unit,
            retire: retire_unit, release: release_graph,
        }); }
    }

    fn compile(&self, index: usize, providers: &[Provider<'_>]) -> Result<Output> {
        let unit = &self.prepared[index];
        let inputs = &self.inputs[self.capture_indices[index]];
        let artifact = super::Artifact { path: &unit.artifact, replace_existing: unit.replace_existing };
        self.compiler.execute(|worker| match unit.kind {
            UnitKind::Object(kind) => super::object::compile(worker, &unit.path, &unit.root,
                providers.iter().map(Provider::object), kind, artifact, Some(inputs))
                .map(|value| Output::Object(Arc::new(value))),
            UnitKind::Entry { entry, target } => super::compile_in(worker, &unit.path, &unit.root,
                entry, target, super::EntryOutput::Artifact(artifact), Vec::new(), providers.iter().map(Provider::object),
                Some(inputs), inputs.entry_checks()).map(Output::Entry),
        }).map_err(|error| match error {
            crate::parallel::Error::Work(error) => error,
            crate::parallel::Error::Resource(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
        })
    }
}

// One synchronous B worker owns the same guard across observation and wait.
// Queue pointers expire before the next host call; they never cross a lock
// release or borrow of the protected state inside that call.
struct GraphWorker<'graph, 'source> {
    execution: &'graph GraphExecution<'graph, 'source>,
    guard: Option<MutexGuard<'graph, State>>,
    result: Option<(usize, Result<Output>)>,
}

impl GraphWorker<'_, '_> {
    fn queue(&mut self) -> WorkQueue {
        let state = self.guard.as_deref_mut().unwrap();
        let failure = state.failure_rank();
        let schedule = &mut state.schedule;
        WorkQueue {
            ready: schedule.ready.as_mut_ptr(),
            waiting: schedule.waiting.as_mut_ptr(),
            following: schedule.following.as_ptr(),
            followers: schedule.followers.as_ptr(),
            counts: &mut schedule.counts,
            failure,
        }
    }
}

unsafe extern "C" fn acquire_graph(context: *mut c_void) -> WorkQueue {
    let worker = unsafe { &mut *context.cast::<GraphWorker<'_, '_>>() };
    worker.guard = Some(worker.execution.state.lock().unwrap());
    worker.queue()
}

unsafe extern "C" fn wait_graph(context: *mut c_void) -> WorkQueue {
    let worker = unsafe { &mut *context.cast::<GraphWorker<'_, '_>>() };
    let guard = worker.guard.take().unwrap();
    worker.guard = Some(worker.execution.changed.wait(guard).unwrap());
    worker.queue()
}

unsafe extern "C" fn execute_unit(context: *mut c_void, index: usize) -> WorkResult {
    let worker = unsafe { &mut *context.cast::<GraphWorker<'_, '_>>() };
    let graph = worker.execution;
    let state = worker.guard.take().unwrap();
    let providers = graph.plan.dependencies(graph.provider_rows[index], graph.prepared.len()).map(|dependency| match dependency {
        Dependency::Unit(id) => match state.outputs[id].as_ref().unwrap() {
            Output::Object(value) => Provider::Built(value.clone()),
            Output::Entry(_) => unreachable!(),
        },
        Dependency::Object(value) => Provider::Object(value),
    }).collect::<Vec<_>>();
    drop(state);
    let result = graph.compile(index, &providers);
    let success = u32::from(result.is_ok());
    worker.result = Some((index, result));
    let guard = graph.state.lock().unwrap();
    let failure = guard.failure_rank();
    worker.guard = Some(guard);
    WorkResult { failure, success }
}

unsafe extern "C" fn retire_unit(context: *mut c_void, keep: u32) -> WorkQueue {
    let worker = unsafe { &mut *context.cast::<GraphWorker<'_, '_>>() };
    let (index, result) = worker.result.take().unwrap();
    if keep != 0 {
        let state = worker.guard.as_deref_mut().unwrap();
        match result {
            Ok(output) => state.outputs[index] = Some(output),
            Err(error) => state.failure = Some((index + 1, error)),
        }
    }
    worker.queue()
}

unsafe extern "C" fn release_graph(context: *mut c_void) {
    let worker = unsafe { &mut *context.cast::<GraphWorker<'_, '_>>() };
    worker.execution.changed.notify_all();
    drop(worker.guard.take());
}
struct GraphCapture<'graph, 'source> {
    prepared: &'graph [Prepared<'source>],
    dependencies: &'graph DependencyPlan<'source>,
    headers: &'graph [Arc<crate::syntax::Module>],
    capture: Capture,
    captured: Vec<CapturedUnit>,
}

unsafe extern "C" fn capture_key(context: *mut c_void, index: usize, key: *mut EntryKey) -> bool {
    let state = unsafe { &*context.cast::<GraphCapture<'_, '_>>() };
    let unit = &state.prepared[index];
    if !matches!(unit.kind, UnitKind::Entry { .. }) { return false; }
    unsafe { key.write(EntryKey {
        path: unit.path.as_os_str().as_encoded_bytes().into(),
        root: unit.root.as_os_str().as_encoded_bytes().into(),
        providers: state.dependencies.provider_rows[index],
        unit: index,
    }); }
    true
}

unsafe extern "C" fn capture_unit(context: *mut c_void, index: usize, shared: bool) {
    let state = unsafe { &mut *context.cast::<GraphCapture<'_, '_>>() };
    let unit = &state.prepared[index];
    let row = state.dependencies.provider_rows[index];
    let mut providers = BTreeMap::new();
    for dependency in state.dependencies.plan.dependencies(row, state.prepared.len()) {
        let (name, interface) = match dependency {
            Dependency::Unit(id) => {
                let header = &state.headers[id];
                (header.name(), ProviderInterface::Unit(header))
            }
            Dependency::Object(object) => (object.name(), ProviderInterface::Object(object)),
        };
        providers.entry(name).or_insert(interface);
    }
    let requests = match unit.kind {
        UnitKind::Object(ObjectKind::Module(requests)) => requests,
        _ => &[],
    };
    state.captured.push(state.capture.capture(&unit.path, &unit.root, &providers, requests, shared));
}

pub fn compile_graph(units: &[Unit<'_>]) -> Result<Vec<Output>> {
    Compiler::available()?.build_graph(units)
}
impl Compiler {
    pub fn build_graph(&self, units: &[Unit<'_>]) -> Result<Vec<Output>> {
        if units.is_empty() { return Ok(Vec::new()); }
        let (prepared, inputs, capture_indices, dependency_plan) = self.execute(|_| {
        let dependency_plan = dependency_plan(units)?;
        let mut destinations = BTreeMap::new();
        let mut entry_count = 0;
        let prepared = units.iter().enumerate().map(|(index, unit)| {
            let header = source_path::canonicalize(&source_path::with_extension(unit.path, u16::from(b'h')))?;
            let extension: Vec<_> = unit.path.extension().unwrap_or_else(|| OsStr::new("b")).encode_wide().collect();
            let path = source_path::with_extension_units(&header, &extension);
            let root = source_path::canonicalize(unit.root)?;
            let output = source_path::absolute(unit.output)?;
            let artifact = match unit.kind {
                UnitKind::Object(_) => output,
                UnitKind::Entry { entry, target } => {
                    entry_count += 1;
                    super::entry_artifact(&path, &root, entry, super::entry_backend(target)?, &output)
                }
            };
            let replace_existing = if units.len() > 1 {
                let (destination, exists) = source_path::output_destination(&artifact)?;
                if let Some(first) = destinations.insert(destination, index) {
                    return Err(format!("构建图单元 {first} 与 {index} 使用同一产物路径：{}", artifact.display()).into());
                }
                exists
            } else {
                true
            };
            Ok(Prepared { path, root, artifact, replace_existing, kind: unit.kind })
        }).collect::<Result<Vec<_>>>()?;
        drop(destinations);
        let mut capture = Capture::default();
        let headers = prepared.iter().map(|unit| capture.header(&unit.path))
            .collect::<crate::ast::Result<Vec<_>>>()?;
        let mut state = GraphCapture {
            prepared: &prepared, dependencies: &dependency_plan, headers: &headers,
            capture, captured: Vec::new(),
        };
        let mut capture_indices = vec![prepared.len(); prepared.len()];
        // The planner borrows the original paths and provider rows during
        // the synchronous capture. It writes the final worker indices here.
        let status = unsafe { capture_sources(CaptureSource {
            context: ptr::from_mut(&mut state).cast(),
            unit_count: prepared.len(), entry_count, ids: dependency_plan.plan.ids().as_ptr(),
            key: capture_key, capture: capture_unit,
        }, capture_indices.as_mut_ptr()) };
        if status != 0 { return Err(std::io::Error::from_raw_os_error(status).into()); }
        let inputs = state.capture.finish(state.captured);
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((prepared, inputs, capture_indices, dependency_plan))
        }).map_err(|error| match error {
            crate::parallel::Error::Work(error) => error,
            crate::parallel::Error::Resource(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
        })?;
        let DependencyPlan { plan, provider_rows, schedule } = dependency_plan;
        let graph = GraphExecution {
            compiler: self, prepared: &prepared, inputs: &inputs, capture_indices: &capture_indices, provider_rows: &provider_rows,
            plan: &plan,
            state: Mutex::new(State {
                schedule, outputs: (0..units.len()).map(|_| None).collect(), failure: None,
            }),
            changed: Condvar::new(),
        };
        let work = || graph.run_worker();
        thread::scope(|scope| {
            for _ in 1..units.len().min(self.workers()) {
                if let Err(error) = thread::Builder::new().spawn_scoped(scope, &work) {
                    graph.state.lock().unwrap().failure = Some((0, Box::new(error)));
                    graph.changed.notify_all();
                    break;
                }
            }
            work();
        });
        let state = graph.state.into_inner().unwrap();
        if let Some((_, error)) = state.failure { return Err(error); }
        Ok(state.outputs.into_iter().map(Option::unwrap).collect())
    }
}
