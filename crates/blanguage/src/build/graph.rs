//! 调用方持有的一次编译图；只调度本次真实依赖，不保留后台消费者。
use super::{Compilation, ObjectCompilation, ObjectKind, Result, inputs::{Capture, ProviderInterface}};
use crate::{Compiler, ast::Target, object::Object, source_path};
use std::{collections::BTreeMap, ffi::{OsStr, c_void}, os::windows::ffi::OsStrExt, path::{Path, PathBuf}, ptr, sync::{Arc, Condvar, Mutex, MutexGuard}, thread};
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
    /// 对象为文件路径；入口映像为原build_entry输出目录。
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
    output: PathBuf,
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
struct GraphUnit {
    dependencies: GraphRange,
    object: usize,
}

#[repr(C)]
struct GraphEdge {
    index: usize,
    external: usize,
}

#[repr(C)]
struct GraphInput {
    units: *const GraphUnit,
    edges: *mut GraphEdge,
    count: usize,
    external_count: usize,
}

#[repr(C)]
struct GraphOutput {
    providers: *mut GraphRange,
    following: *mut GraphRange,
    waiting: *mut usize,
    followers: *mut usize,
}

const OUT_OF_RANGE: u64 = 1;
const DEPENDENCY_CYCLE: u64 = 2;
const ENTRY_PROVIDER: u64 = 3;

#[repr(C)]
struct GraphPlan {
    ids: *const usize,
    count: usize,
    status: u64,
    unit: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_graph::plan"]
    fn build_dependency_plan(input: GraphInput, output: GraphOutput) -> GraphPlan;
    #[link_name = "bootstrap.build_graph::release_plan"]
    fn release_dependency_plan(ids: *const usize);
    #[link_name = "bootstrap.build_schedule::initialize_ready"]
    fn initialize_ready(ready: *mut usize, waiting: *const usize, length: usize) -> usize;
    #[link_name = "bootstrap.build_schedule::run_worker"]
    fn run_worker(context: *mut c_void, host: WorkHost);
}

// The B planner transfers ownership of its process-heap allocation. Any
// thread can free it. Workers only share the immutable slice borrowed from it.
unsafe impl Send for GraphPlan {}

impl GraphPlan {
    fn ids(&self) -> &[usize] {
        if self.count == 0 {
            &[]
        } else {
            // The B planner returned count initialized indices in this allocation.
            unsafe { std::slice::from_raw_parts(self.ids, self.count) }
        }
    }
}

impl Drop for GraphPlan {
    fn drop(&mut self) {
        unsafe { release_dependency_plan(self.ids); }
    }
}

struct DependencyPlan<'a> {
    plan: GraphPlan,
    provider_rows: Vec<GraphRange>,
    externals: Vec<&'a Object>,
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
    let mut edges = Vec::new();
    let mut externals = Vec::new();
    let mut external_indices = BTreeMap::new();
    let input_units: Vec<_> = units.iter().map(|unit| {
        let start = edges.len();
        for dependency in unit.dependencies {
            let edge = match dependency {
                Dependency::Unit(index) => GraphEdge { index: *index, external: 0 },
                Dependency::Object(object) => {
                    let index = *external_indices.entry(*object as *const Object).or_insert_with(|| {
                        let index = externals.len();
                        externals.push(*object);
                        index
                    });
                    GraphEdge { index, external: 1 }
                }
            };
            edges.push(edge);
        }
        GraphUnit {
            dependencies: GraphRange { start, count: edges.len() - start },
            object: usize::from(matches!(unit.kind, UnitKind::Object(_))),
        }
    }).collect();

    let mut provider_rows = vec![GraphRange::default(); units.len()];
    let mut following_rows = vec![GraphRange::default(); units.len()];
    let mut waiting = vec![0; units.len()];
    let mut followers = vec![0; edges.len()];
    let mut ready = vec![0; units.len()];
    let plan = unsafe {
        build_dependency_plan(
            GraphInput {
                units: input_units.as_ptr(),
                edges: edges.as_mut_ptr(),
                count: units.len(),
                external_count: externals.len(),
            },
            GraphOutput {
                providers: provider_rows.as_mut_ptr(),
                following: following_rows.as_mut_ptr(),
                waiting: waiting.as_mut_ptr(),
                followers: followers.as_mut_ptr(),
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
    let ready_count = unsafe { initialize_ready(ready.as_mut_ptr(), waiting.as_ptr(), waiting.len()) };
    let schedule = Schedule {
        following: following_rows, followers, waiting, ready,
        counts: ScheduleCounts { ready: ready_count, unfinished: units.len() },
    };
    Ok(DependencyPlan { plan, provider_rows, externals, schedule })
}

fn dependencies<'row, 'object>(
    ids: &'row [usize], unit_count: usize, externals: &'row [&'object Object],
) -> impl Iterator<Item = Dependency<'object>> + 'row {
    ids.iter().copied().map(move |id| {
        if id < unit_count {
            Dependency::Unit(id)
        } else {
            Dependency::Object(externals[id - unit_count])
        }
    })
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
    provider_rows: &'graph [GraphRange],
    ids: &'graph [usize],
    externals: &'graph [&'source Object],
    entry_checks: &'graph [Option<Arc<crate::frontend::EntryChecks>>],
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
        self.compiler.execute(|worker| match unit.kind {
            UnitKind::Object(kind) => super::object::compile(worker, &unit.path, &unit.root,
                providers.iter().map(Provider::object), kind, &unit.output, Some(&self.inputs[index]))
                .map(|value| Output::Object(Arc::new(value))),
            UnitKind::Entry { entry, target } => super::compile_in(worker, &unit.path, &unit.root,
                entry, target, &unit.output, Vec::new(), &providers.iter().map(Provider::object).collect::<Vec<_>>(),
                Some(&self.inputs[index]), self.entry_checks[index].as_deref()).map(Output::Entry),
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
    let row = graph.provider_rows[index].slice(graph.ids);
    let providers = dependencies(row, graph.prepared.len(), graph.externals).map(|dependency| match dependency {
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
pub fn compile_graph(units: &[Unit<'_>]) -> Result<Vec<Output>> {
    Compiler::available()?.build_graph(units)
}
impl Compiler {
    pub fn build_graph(&self, units: &[Unit<'_>]) -> Result<Vec<Output>> {
        if units.is_empty() { return Ok(Vec::new()); }
        let (prepared, inputs, dependency_plan) = self.execute(|_| {
        let dependency_plan = dependency_plan(units)?;
        let prepared = units.iter().map(|unit| {
            let header = source_path::canonicalize(&source_path::with_extension(unit.path, u16::from(b'h')))?;
            let extension: Vec<_> = unit.path.extension().unwrap_or_else(|| OsStr::new("b")).encode_wide().collect();
            let path = source_path::with_extension_units(&header, &extension);
            Ok(Prepared { path, root: source_path::canonicalize(unit.root)?, output: source_path::absolute(unit.output)?,
                kind: unit.kind })
        }).collect::<Result<Vec<_>>>()?;
        let mut capture = Capture::default();
        let headers = prepared.iter().map(|unit| capture.header(&unit.path))
            .collect::<crate::ast::Result<Vec<_>>>()?;
        let mut captured = Vec::with_capacity(prepared.len());
        let ids = dependency_plan.plan.ids();
        for (unit, row) in prepared.iter().zip(&dependency_plan.provider_rows) {
            let mut providers = BTreeMap::new();
            for dependency in dependencies(row.slice(ids), units.len(), &dependency_plan.externals) {
                let (name, interface) = match dependency {
                    Dependency::Unit(id) => {
                        let header = &headers[id];
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
            captured.push(capture.capture(&unit.path, &unit.root, &providers, requests)?);
        }
        let inputs = capture.finish(captured);
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((prepared, inputs, dependency_plan))
        }).map_err(|error| match error {
            crate::parallel::Error::Work(error) => error,
            crate::parallel::Error::Resource(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
        })?;
        let DependencyPlan { plan, provider_rows, externals, schedule } = dependency_plan;
        let ids = plan.ids();
        // These entries borrow the same captured source and the same completed
        // provider objects. Output paths and selected entries remain independent.
        let mut entry_groups = BTreeMap::<_, Vec<usize>>::new();
        for (index, unit) in prepared.iter().enumerate() {
            if matches!(unit.kind, UnitKind::Entry { .. }) {
                let identity = (&unit.path, &unit.root, provider_rows[index].slice(ids));
                entry_groups.entry(identity).or_default().push(index);
            }
        }
        let mut entry_checks = vec![None; units.len()];
        for group in entry_groups.into_values().filter(|group| group.len() > 1) {
            let shared = Arc::new(crate::frontend::EntryChecks::default());
            for index in group {
                entry_checks[index] = Some(Arc::clone(&shared));
            }
        }

        let graph = GraphExecution {
            compiler: self, prepared: &prepared, inputs: &inputs, provider_rows: &provider_rows,
            ids, externals: &externals, entry_checks: &entry_checks,
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
