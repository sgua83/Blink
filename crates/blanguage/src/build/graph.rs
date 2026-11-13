//! 调用方持有的一次编译图；只调度本次真实依赖，不保留后台消费者。
use super::{Compilation, ObjectCompilation, ObjectKind, Result, inputs::{Capture, ProviderInterface}};
use crate::{Compiler, ast::Target, object::Object};
use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::{Arc, Condvar, Mutex}, thread};
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
    #[link_name = "bootstrap.build_schedule::take_ready"]
    fn take_ready(ready: *mut usize, count: &mut usize) -> usize;
    #[link_name = "bootstrap.build_schedule::complete_unit"]
    fn complete_unit(
        ready: *mut usize, count: &mut usize, waiting: *mut usize,
        followers: *const usize, length: usize,
    );
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
    ready_count: usize,
}

impl Schedule {
    fn take(&mut self) -> Option<usize> {
        let index = unsafe { take_ready(self.ready.as_mut_ptr(), &mut self.ready_count) };
        (index != usize::MAX).then_some(index)
    }

    fn complete(&mut self, index: usize) {
        let followers = self.following[index].slice(&self.followers);
        unsafe {
            complete_unit(
                self.ready.as_mut_ptr(), &mut self.ready_count, self.waiting.as_mut_ptr(),
                followers.as_ptr(), followers.len(),
            );
        }
    }
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
        following: following_rows, followers, waiting, ready, ready_count,
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
    failures: Vec<(Option<usize>, Box<dyn std::error::Error + Send + Sync>)>,
    unfinished: usize,
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
            let mut path = unit.path.to_path_buf();
            path.set_extension("h");
            let mut path = path.canonicalize()?;
            path.set_extension(unit.path.extension().unwrap_or_else(|| std::ffi::OsStr::new("b")));
            Ok(Prepared { path, root: unit.root.canonicalize()?, output: std::path::absolute(unit.output)?,
                kind: unit.kind })
        }).collect::<Result<Vec<_>>>()?;
        let mut capture = Capture::default();
        for unit in &prepared { capture.header(&unit.path)?; }
        let mut interfaces = Vec::with_capacity(prepared.len());
        let ids = dependency_plan.plan.ids();
        for (unit, row) in prepared.iter().zip(&dependency_plan.provider_rows) {
            let mut providers = BTreeMap::new();
            for dependency in dependencies(row.slice(ids), units.len(), &dependency_plan.externals) {
                let (name, interface) = match dependency {
                    Dependency::Unit(id) => {
                        let header = capture.header(&prepared[id].path)?;
                        (header.name().to_owned(), ProviderInterface::Unit(header))
                    }
                    Dependency::Object(object) => (object.name().to_owned(), ProviderInterface::Object(object)),
                };
                providers.entry(name).or_insert(interface);
            }
            interfaces.push(capture.capture(&unit.path, &unit.root, &providers)?);
        }
        let inputs = capture.finish(interfaces);
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((prepared, inputs, dependency_plan))
        }).map_err(|error| match error {
            crate::parallel::Error::Work(error) => error,
            crate::parallel::Error::Resource(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
        })?;
        let DependencyPlan { plan, provider_rows, externals, schedule } = dependency_plan;
        let ids = plan.ids();
        let state = Mutex::new(State {
            schedule, outputs: (0..units.len()).map(|_| None).collect(),
            failures: Vec::new(), unfinished: units.len(),
        });
        let changed = Condvar::new();
        let work = || loop {
            let (index, providers) = {
                let mut state = state.lock().unwrap();
                loop {
                    if !state.failures.is_empty() || state.unfinished == 0 { return; }
                    if let Some(index) = state.schedule.take() {
                        let row = provider_rows[index].slice(ids);
                        let providers = dependencies(row, units.len(), &externals).map(|dependency| match dependency {
                            Dependency::Unit(id) => match state.outputs[id].as_ref().unwrap() {
                                Output::Object(value) => Provider::Built(value.clone()),
                                Output::Entry(_) => unreachable!(),
                            },
                            Dependency::Object(value) => Provider::Object(value),
                        }).collect::<Vec<_>>();
                        break (index, providers);
                    }
                    state = changed.wait(state).unwrap();
                }
            };
            let unit = &prepared[index];
            let result = self.execute(|worker| match unit.kind {
                UnitKind::Object(kind) => super::object::compile(worker, &unit.path, &unit.root,
                    providers.iter().map(Provider::object), kind, &unit.output, Some(&inputs[index]))
                    .map(|value| Output::Object(Arc::new(value))),
                UnitKind::Entry { entry, target } => super::compile_in(worker, &unit.path, &unit.root,
                    entry, target, &unit.output, Vec::new(), &providers.iter().map(Provider::object).collect::<Vec<_>>(), Some(&inputs[index])).map(Output::Entry),
            }).map_err(|error| match error {
                crate::parallel::Error::Work(error) => error,
                crate::parallel::Error::Resource(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
            });
            let mut state = state.lock().unwrap();
            state.unfinished -= 1;
            match result {
                Ok(output) => {
                    state.outputs[index] = Some(output);
                    state.schedule.complete(index);
                }
                Err(error) => state.failures.push((Some(index), error)),
            }
            changed.notify_all();
        };
        thread::scope(|scope| {
            for _ in 1..units.len().min(self.workers()) {
                if let Err(error) = thread::Builder::new().spawn_scoped(scope, &work) {
                    state.lock().unwrap().failures.push((None, Box::new(error)));
                    changed.notify_all();
                    break;
                }
            }
            work();
        });
        let mut state = state.into_inner().unwrap();
        state.failures.sort_by_key(|(index, _)| *index);
        if !state.failures.is_empty() { return Err(state.failures.remove(0).1); }
        Ok(state.outputs.into_iter().map(Option::unwrap).collect())
    }
}
