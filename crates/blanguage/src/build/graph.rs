//! 调用方持有的一次编译图；只调度本次真实依赖，不保留后台消费者。
use super::{Compilation, Inputs, ObjectCompilation, ObjectKind, Result};
use crate::{Compiler, ast::Target, object::Object};
use std::{collections::BTreeSet, path::{Path, PathBuf}, sync::{Arc, Condvar, Mutex}, thread};
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
fn expand<'a>(
    index: usize, units: &[Unit<'a>], active: &mut BTreeSet<usize>,
    expanded: &mut [Option<Vec<Dependency<'a>>>],
) -> Result<Vec<Dependency<'a>>> {
    let unit = units.get(index).ok_or_else(|| std::io::Error::other(format!("构建单元引用越界：{index}")))?;
    if let Some(value) = &expanded[index] { return Ok(value.clone()); }
    if !active.insert(index) { return Err(format!("构建依赖形成循环：单元{index} {}", unit.path.display()).into()); }
    if !unit.dependencies.is_empty() && !matches!(unit.kind, UnitKind::Object(ObjectKind::Module(_) | ObjectKind::Application(_))) {
        return Err(format!("单元{index}的入口映像／算法库尚不消费独立COFF模块导入").into());
    }
    let mut providers = Vec::new();
    let mut seen = BTreeSet::new();
    for dependency in unit.dependencies {
        let mut values = match dependency {
            Dependency::Unit(id) => {
                let source = units.get(*id).ok_or_else(|| std::io::Error::other(format!("构建单元引用越界：{id}")))?;
                if !matches!(source.kind, UnitKind::Object(_)) {
                    return Err(format!("单元{id}的入口映像不是COFF模块提供者").into());
                }
                expand(*id, units, active, expanded)?
            }
            Dependency::Object(_) => Vec::new(),
        };
        values.push(*dependency);
        for value in values {
            let key = match value { Dependency::Unit(id) => (false, id),
                Dependency::Object(object) => (true, object as *const Object as usize) };
            if seen.insert(key) { providers.push(value); }
        }
    }
    active.remove(&index);
    expanded[index] = Some(providers.clone());
    Ok(providers)
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
        let mut expanded = vec![None; units.len()];
        for index in 0..units.len() { expand(index, units, &mut BTreeSet::new(), &mut expanded)?; }
        let prepared = units.iter().enumerate().map(|(index, unit)| {
            let header = unit.path.with_extension("h").canonicalize()?;
            let path = header.with_extension(unit.path.extension().unwrap_or_else(|| std::ffi::OsStr::new("b")));
            Ok(Prepared { path, root: unit.root.canonicalize()?, output: std::path::absolute(unit.output)?,
                kind: unit.kind, providers: expanded[index].take().unwrap() })
        }).collect::<Result<Vec<_>>>()?;
        let mut inputs = Inputs::default();
        for unit in &prepared { inputs.header(&unit.path)?; }
        for unit in &prepared {
            let mut names = BTreeSet::new();
            let mut interfaces = Vec::new();
            for provider in &unit.providers {
                match provider {
                    Dependency::Unit(id) => {
                        let parsed = inputs.header(&prepared[*id].path)?;
                        names.insert(parsed.name.clone());
                        interfaces.push(inputs.capture_interface(&parsed.name, &parsed.source.text, Some(&parsed))?);
                    }
                    Dependency::Object(object) => {
                        names.insert(object.name().into());
                        if let crate::object::Kind::Module { interface, .. } = &object.kind {
                            interfaces.push(inputs.capture_interface(object.name(), interface, None)?);
                        }
                    }
                }
            }
            let mut visited = BTreeSet::new();
            inputs.capture(&unit.path, &unit.root, &names, false, &mut visited)?;
            for interface in interfaces {
                for import in &interface.imports {
                    if !names.contains(import) {
                        inputs.capture(&unit.root.join(import.replace('.', "/")).with_extension("b"),
                            &unit.root, &names, true, &mut visited)?;
                    }
                }
            }
        }
        let mut following = vec![Vec::new(); units.len()];
        let waiting = units.iter().enumerate().map(|(index, unit)| {
            let dependencies = unit.dependencies.iter().filter_map(|dependency| match dependency {
                Dependency::Unit(id) => Some(*id), Dependency::Object(_) => None,
            }).collect::<BTreeSet<_>>();
            for id in &dependencies { following[*id].push(index); }
            dependencies.len()
        }).collect::<Vec<_>>();
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
                    providers.iter().map(Provider::object), kind, &unit.output, Some(&inputs))
                    .map(|value| Output::Object(Arc::new(value))),
                UnitKind::Entry { entry, target } => super::compile_in(worker, &unit.path, &unit.root,
                    entry, target, &unit.output, Some(&inputs)).map(Output::Entry),
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
