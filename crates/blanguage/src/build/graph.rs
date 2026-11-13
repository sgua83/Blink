//! 调用方持有的一次编译图；只调度本次真实依赖，不保留后台消费者。
use super::{Compilation, ObjectCompilation, ObjectKind, Result, inputs::{Capture, ProviderInterface}};
use crate::{Compiler, ast::Target, object::Object};
use std::{collections::{BTreeMap, BTreeSet}, path::{Path, PathBuf}, sync::{Arc, Condvar, Mutex}, thread};
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
    providers: Vec<Dependency<'a>>,
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
    edges: *const GraphEdge,
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
}

impl Drop for GraphPlan {
    fn drop(&mut self) {
        unsafe { release_dependency_plan(self.ids); }
    }
}

struct DependencyPlan<'a> {
    providers: Vec<Vec<Dependency<'a>>>,
    following: Vec<Vec<usize>>,
    waiting: Vec<usize>,
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
    let plan = unsafe {
        build_dependency_plan(
            GraphInput {
                units: input_units.as_ptr(),
                edges: edges.as_ptr(),
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
    // B owns the flat provider buffer until these rows have been transferred.
    let ids = if plan.count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(plan.ids, plan.count) }
    };
    let providers = provider_rows.into_iter().map(|row| {
        row.slice(ids).iter().map(|&id| {
            if id < units.len() { Dependency::Unit(id) }
            else { Dependency::Object(externals[id - units.len()]) }
        }).collect()
    }).collect();
    let following = following_rows.into_iter().map(|row| row.slice(&followers).to_vec()).collect();
    Ok(DependencyPlan { providers, following, waiting })
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
    ready: BTreeSet<usize>,
    waiting: Vec<usize>,
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
        let (prepared, inputs, following, waiting) = self.execute(|_| {
        let DependencyPlan { providers, following, waiting } = dependency_plan(units)?;
        let prepared = units.iter().zip(providers).map(|(unit, providers)| {
            let mut path = unit.path.to_path_buf();
            path.set_extension("h");
            let mut path = path.canonicalize()?;
            path.set_extension(unit.path.extension().unwrap_or_else(|| std::ffi::OsStr::new("b")));
            Ok(Prepared { path, root: unit.root.canonicalize()?, output: std::path::absolute(unit.output)?,
                kind: unit.kind, providers })
        }).collect::<Result<Vec<_>>>()?;
        let mut capture = Capture::default();
        for unit in &prepared { capture.header(&unit.path)?; }
        let mut interfaces = Vec::with_capacity(prepared.len());
        for unit in &prepared {
            let mut providers = BTreeMap::new();
            for provider in &unit.providers {
                let (name, interface) = match *provider {
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
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((prepared, inputs, following, waiting))
        }).map_err(|error| match error {
            crate::parallel::Error::Work(error) => error,
            crate::parallel::Error::Resource(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
        })?;
        let state = Mutex::new(State {
            ready: waiting.iter().enumerate().filter_map(|(index, count)| (*count == 0).then_some(index)).collect(),
            waiting, outputs: (0..units.len()).map(|_| None).collect(),
            failures: Vec::new(), unfinished: units.len(),
        });
        let changed = Condvar::new();
        let work = || loop {
            let (index, providers) = {
                let mut state = state.lock().unwrap();
                loop {
                    if !state.failures.is_empty() || state.unfinished == 0 { return; }
                    if let Some(index) = state.ready.pop_first() {
                        let providers = prepared[index].providers.iter().map(|dependency| match dependency {
                            Dependency::Unit(id) => match state.outputs[*id].as_ref().unwrap() {
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
                    for id in &following[index] {
                        state.waiting[*id] -= 1;
                        if state.waiting[*id] == 0 { state.ready.insert(*id); }
                    }
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
