mod evaluation;
pub(crate) use evaluation::Call;
mod type_reference;
mod type_value;
mod loading;
pub(crate) mod declarations;
mod entry_checks;
pub(crate) use entry_checks::EntryChecks;
use loading::load;
use declarations::{Directory, OrderedDirectory};
use crate::names::Bytes;
use crate::{file_bytes::FileBytes, *};
use std::{
    cell::RefCell,
    fmt::Write,
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

unsafe extern "C" {
    #[link_name = "bootstrap.type_layout::accepts_stride"]
    fn accepts_record_stride(size: u32, alignment: u32, stride: u32) -> bool;
}

#[derive(Clone)]
pub(crate) enum DefinitionSyntax { Source, CompiledInterface }

pub(crate) struct EvaluatedConstant {
    pub(crate) value: (ir::Ty, Vec<ir::Bits>, Option<Arc<ir::GeneratedOrigin>>),
    pub(crate) inputs: BTreeSet<build::Declaration>,
}

#[derive(Clone)]
pub(crate) struct Definition {
    pub(crate) function: crate::syntax::Function,
    pub(crate) interface_declaration: Option<crate::syntax::Function>,
    pub(crate) syntax_origin: DefinitionSyntax,
    pub(crate) execution: execution::Requirement,
    pub(crate) source: Source,
    pub(crate) module: String,
    pub(crate) imports: Arc<[String]>,
    pub(crate) public: bool,
    pub(crate) compiled: Option<Arc<BTreeMap<String, Vec<u8>>>>,
    pub(crate) constant: OnceLock<Arc<EvaluatedConstant>>,
}
impl Definition {
    pub(crate) fn source_origin(&self) -> Option<ir::SourceOrigin> {
        match (&self.syntax_origin, &self.function.body()) {
            (DefinitionSyntax::Source, Some(_)) => Some(ir::SourceOrigin {
                source: self.source.clone(), span: self.function.span(),
                module: self.module.clone(), definition: self.function.name().to_owned(),
            }),
            _ => None,
        }
    }
    pub(crate) fn declaration_origin(&self) -> Option<ir::SourceDeclaration> {
        if self.function.body().is_some() { return None; }
        let representation = match &self.syntax_origin {
            DefinitionSyntax::Source => ir::DeclarationRepresentation::Source,
            DefinitionSyntax::CompiledInterface => ir::DeclarationRepresentation::CompiledInterface,
        };
        Some(ir::SourceDeclaration { source: self.source.clone(), span: self.function.span(),
            module: self.module.clone(), declaration: self.function.name().to_owned(), representation })
    }
    pub(crate) fn syntax_context(&self, instance: Option<&str>) -> (Option<Arc<ir::SourceOrigin>>, Arc<ir::SourceContext>) {
        (
            self.source_origin().map(Arc::new),
            Arc::new(ir::SourceContext{instance:instance.map(Arc::from),inlined_at:Box::default(),domain_path:Arc::from([])}),
        )
    }
}
/// 仅由已完成模块／阶段证明的无前置条件程序构造；执行器不接受条件模板。
pub struct Compilation {
    pub programs: Vec<ExecutableProgram>,
    pub templates: Vec<TemplateProgram>,
    pub open_generics: Vec<String>,
    pub applications: Vec<application::Application>,
}
type RecordDeclaration = (crate::syntax::Record, Source, String, Arc<[String]>);

pub(crate) struct ModuleDeclaration {
    pub(crate) interface: Arc<crate::syntax::Module>,
    implementation: Option<Source>,
}
impl ModuleDeclaration {
    pub(crate) fn source(&self) -> &Source {
        self.implementation.as_ref().unwrap_or(&self.interface.source)
    }
}
#[derive(Default)]
struct DeclarationInput {
    modules: Directory<ModuleDeclaration>,
    records: Directory<RecordDeclaration>,
    functions: Directory<Arc<Definition>>,
    type_sets: Directory<(crate::syntax::TypeSet, Source, String, Arc<[String]>)>,
}
impl DeclarationInput {
    fn publish(self) -> Declarations {
        Declarations {
            modules: self.modules.publish(),
            records: self.records.publish(),
            functions: self.functions.publish(),
            type_sets: self.type_sets.publish(),
        }
    }
}
pub(crate) struct Declarations {
    pub(crate) modules: OrderedDirectory<ModuleDeclaration>,
    pub(crate) records: OrderedDirectory<RecordDeclaration>,
    pub(crate) functions: OrderedDirectory<Arc<Definition>>,
    pub(crate) type_sets: OrderedDirectory<(crate::syntax::TypeSet, Source, String, Arc<[String]>)>,
}
pub(crate) struct World {
    calls: evaluation::Calls,
    pub(crate) declarations: Arc<Declarations>,
    pub(crate) worker: compiler::Worker,
    pub(crate) evaluating_constants: crate::names::ActiveDependencies,
    pub(crate) build_declarations: Option<RefCell<BTreeSet<build::Declaration>>>,
}
impl std::ops::Deref for World {
    type Target = Declarations;
    fn deref(&self) -> &Declarations { &self.declarations }
}
impl World {
    pub(crate) fn new(declarations: Arc<Declarations>, worker: compiler::Worker) -> Self {
        Self { declarations, worker, calls: evaluation::Calls::default(), evaluating_constants: crate::names::ActiveDependencies::default(), build_declarations: None }
    }
    /// The child is awaited by this World's current original invocation.
    /// The caller preserves that invocation protocol through completion.
    pub(crate) unsafe fn call<'f, F: std::future::Future + 'f>(&'f self, future: F) -> evaluation::Call<'f, F::Output> {
        unsafe { self.calls.call(future) }
    }
    /// Only original World functions using the same awaited call protocol run
    /// here. Internal calls do not recursively enter another run.
    pub(crate) unsafe fn run<F: std::future::Future>(&self, future: F) -> F::Output {
        unsafe { self.calls.run(future) }
    }
    pub(crate) fn resolve(&self, name: &str, module: &str, imports: &[String], functions: bool) -> NameResolution {
        if let Some(inputs) = &self.build_declarations {
            let query = if functions {
                build::Declaration::ValueName { name: name.into(), module: module.into(), imports: imports.to_vec() }
            } else {
                build::Declaration::RecordName { name: name.into(), module: module.into(), imports: imports.to_vec() }
            };
            inputs.borrow_mut().insert(query);
        }
        self.declarations.resolve(name, module, imports, functions)
    }
    pub(crate) fn record(&self, key: &str) -> &RecordDeclaration {
        if let Some(inputs) = &self.build_declarations {
            inputs.borrow_mut().insert(build::Declaration::Record(key.into()));
        }
        &self.records[key]
    }
}
fn source_error(path: &Path, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic {
        source: Source { path: path.into(), text: "".into() },
        span: Span::default(),
        code: "B020",
        message: format!("无法读取模块：{error}"),
    }
}

fn read_opened(path: &Path, file: &File) -> Result<Source> {
    let bytes = FileBytes::read(file).map_err(|error| source_error(path, error))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| source_error(path, "stream did not contain valid UTF-8"))?;
    Ok(Source { path: path.into(), text: text.into() })
}

pub(crate) fn read(path: &Path) -> Result<Source> {
    let file = File::open(path).map_err(|error| source_error(path, error))?;
    read_opened(path, &file)
}

pub(crate) fn read_implementation(path: &Path) -> Result<Option<Arc<syntax::Module>>> {
    match File::open(path) {
        Ok(file) => parser::parse(read_opened(path, &file)?).map(|module| Some(Arc::new(module))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(source_error(path, error)),
    }
}

pub fn compile(path: &Path, module_root: &Path) -> Result<Compilation> {
    compile_module(path, module_root, true, None, &[])
}
/// 选择公开 program 的具体静态入口，例如 shade<13> 或 run<float3,17>。
pub fn compile_entry(path: &Path, module_root: &Path, entry: &str) -> Result<Compilation> {
    compile_module(
        path,
        module_root,
        true,
        Some((EntryKind::Domain, entry)),
        &[],
    )
}
/// 使用实际已编译模块的公开接口和合同编译；代码绑定仍由对应链接／装入消费者完成。
pub fn compile_with_modules(
    path: &Path, module_root: &Path, dependencies: &[object::Object],
) -> Result<Compilation> {
    compile_module(path, module_root, true, None, dependencies)
}
/// 指定域入口消费已编译模块接口，不读取提供者的实现源码。
pub fn compile_entry_with_modules(
    path: &Path, module_root: &Path, entry: &str, dependencies: &[object::Object],
) -> Result<Compilation> {
    compile_module(path, module_root, true, Some((EntryKind::Domain, entry)), dependencies)
}
/// 编译独立 CPU 应用的普通值入口；启动与平台链接由 application 模块拥有。
pub fn compile_application(
    path: &Path,
    module_root: &Path,
    entry: &str,
) -> Result<application::Application> {
    Ok(compile_module(
        path,
        module_root,
        false,
        Some((EntryKind::Application, entry)),
        &[],
    )?
    .applications
    .remove(0))
}
/// 使用真实已编译模块接口编译应用，不读取这些模块的实现源码。
pub fn compile_application_with_modules(
    path: &Path,
    module_root: &Path,
    entry: &str,
    dependencies: &[object::Object],
) -> Result<application::Application> {
    let compiler = Compiler::available().map_err(|error| compiler::diagnostic(path, error))?;
    compiler.compile_application_with_modules(path, module_root, entry, dependencies)
}
/// 库接口与完整导入闭包同样检查；无需为函数库伪造可执行 program。
pub fn check(path: &Path, module_root: &Path) -> Result<Compilation> {
    compile_module(path, module_root, false, None, &[])
}
#[derive(Clone, Copy)]
pub(crate) enum EntryKind {
    Domain,
    Application,
}
pub(crate) fn compile_module(
    path: &Path,
    module_root: &Path,
    require_program: bool,
    requested: Option<(EntryKind, &str)>,
    dependencies: &[object::Object],
) -> Result<Compilation> {
    let compiler = Compiler::available().map_err(|error| compiler::diagnostic(path, error))?;
    compile_module_on(&compiler, path, module_root, require_program, requested, dependencies)
}
pub(crate) fn compile_module_on(
    compiler: &Compiler, path: &Path, module_root: &Path, require_program: bool,
    requested: Option<(EntryKind, &str)>, dependencies: &[object::Object],
) -> Result<Compilation> {
    compile_module_importing(compiler, path, module_root, require_program, requested,
        || module::Imports::new(dependencies))
}

fn compile_module_importing(
    compiler: &Compiler, path: &Path, module_root: &Path, require_program: bool,
    requested: Option<(EntryKind, &str)>,
    load: impl FnOnce() -> crate::codec::Result<module::Imports> + Send,
) -> Result<Compilation> {
    compiler.ast(path, |worker| {
        let imports = load().map_err(|error| Diagnostic {
            source: Source { path: path.into(), text: "".into() },
            span: Span::default(), code: "B270", message: error.to_string(),
        })?;
        let imported = (!imports.modules.is_empty()).then_some(&imports);
        let (world, entry, _) = read_world(path, module_root, imported, &[], worker, None, None)?;
        compile_world(&world, entry, require_program, requested, None)
    })
}
impl Compiler {
    /// Resolves a concrete value type using the original module/type checker, without codegen.
    pub fn type_layout(&self, path: &Path, root: &Path, request: &str) -> Result<ir::Ty> {
        self.ast(path, |worker| {
            let source = Source { path: "<layout>".into(), text: request.into() };
            let span = Span { start: 0, end: request.len() };
            let name = parser::type_query(source.clone())?;
            let (world, module, _) = read_world(path, root, None, &[], worker, None, None)?;
            let imports = &world.modules[&module].interface.imports().map(str::to_owned).collect::<Vec<_>>();
            let bindings=Arc::new(generics::Bindings::default());
            let name=unsafe { world.run(bindings.type_name(&world,&name,(&source,&module,imports),span)) }?;
            let value = unsafe { world.run(world.ty(&name, &source, span, &module, imports)) }?;
            if name.dataset {
                let count = unsafe { world.run(world.shape(&name.count, &source, span, &module, imports)) }?;
                Ok(ir::Ty::owned_dataset((value).syntax(), count))
            } else { Ok(value) }
        })
    }
    /// Returns the root name, selected byte offset and layout JSON while the source World is alive.
    pub fn type_layout_json(&self, path: &Path, root: &Path, request: &str, field: Option<&str>) -> Result<(String, u32, String)> {
        self.ast(path, |worker| {
            let source = Source { path: "<layout>".into(), text: request.into() };
            let span = Span { start: 0, end: request.len() };
            let name = parser::type_query(source.clone())?;
            let (world, module, _) = read_world(path, root, None, &[], worker, None, None)?;
            let imports = &world.modules[&module].interface.imports().map(str::to_owned).collect::<Vec<_>>();
            let bindings=Arc::new(generics::Bindings::default());
            let name=unsafe { world.run(bindings.type_name(&world,&name,(&source,&module,imports),span)) }?;
            let value = unsafe { world.run(world.ty(&name, &source, span, &module, imports)) }?;
            let value = if name.dataset {
                ir::Ty::owned_dataset(value.syntax(), unsafe { world.run(world.shape(&name.count, &source, span, &module, imports)) }?)
            } else { value };
            let (offset, selected) = if let Some(field) = field {
                value.field(field).ok_or_else(|| Diagnostic { source: source.clone(), span, code: "B141",
                    message: format!("类型 {} 没有可定位的静态字段路径 {field}", value.name) })?
            } else { (0, &value) };
            let json = unsafe {
                world.run(world.type_layout_text(selected, &source, span, &module, imports))
            }?;
            Ok((value.name, offset, json))
        })
    }
    pub fn compile(&self, path: &Path, root: &Path) -> Result<Compilation> {
        compile_module_on(self, path, root, true, None, &[])
    }
    pub fn check(&self, path: &Path, root: &Path) -> Result<Compilation> {
        compile_module_on(self, path, root, false, None, &[])
    }
    pub fn compile_entry(&self, path: &Path, root: &Path, entry: &str) -> Result<Compilation> {
        compile_module_on(self, path, root, true, Some((EntryKind::Domain, entry)), &[])
    }
    pub fn compile_with_modules(
        &self, path: &Path, root: &Path, dependencies: &[object::Object],
    ) -> Result<Compilation> {
        compile_module_on(self, path, root, true, None, dependencies)
    }
    pub fn compile_entry_with_modules(
        &self, path: &Path, root: &Path, entry: &str, dependencies: &[object::Object],
    ) -> Result<Compilation> {
        compile_module_on(self, path, root, true, Some((EntryKind::Domain, entry)), dependencies)
    }
    #[doc(hidden)]
    pub fn compile_entry_with_interfaces(
        &self, path: &Path, root: &Path, entry: &str, interfaces: Vec<module::CompiledInterface>,
    ) -> Result<Compilation> {
        compile_module_importing(self, path, root, true, Some((EntryKind::Domain, entry)),
            || module::Imports::interfaces(interfaces))
    }
    pub fn compile_application(&self, path: &Path, root: &Path, entry: &str) -> Result<application::Application> {
        Ok(compile_module_on(self, path, root, false, Some((EntryKind::Application, entry)), &[])?.applications.remove(0))
    }
    pub fn compile_application_with_modules(
        &self, path: &Path, root: &Path, entry: &str, dependencies: &[object::Object],
    ) -> Result<application::Application> {
        Ok(compile_module_on(self, path, root, false,
            Some((EntryKind::Application, entry)), dependencies)?.applications.remove(0))
    }
    /// Compiles an application against the declared interfaces of compiled providers.
    pub fn compile_application_with_interfaces(
        &self, path: &Path, root: &Path, entry: &str, interfaces: Vec<module::CompiledInterface>,
    ) -> Result<application::Application> {
        Ok(compile_module_importing(self, path, root, false, Some((EntryKind::Application, entry)),
            || module::Imports::interfaces(interfaces))?.applications.remove(0))
    }
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum SourceMode {
    Automatic,
    SelectedProgram,
    Application,
}

#[allow(dead_code)]
#[repr(u32)]
#[derive(Clone, Copy)]
enum SourceRoute {
    Function,
    CheckDomain,
    ValidateProgram,
    Program,
    Application,
}

type SourceArguments = Option<crate::syntax::List<crate::syntax::wire::ArgumentNode>>;
type SelectedSource = (String, SourceArguments, Source);

enum Job<'a> {
    Instance(generics::Instance, SourceRoute, bool),
    Selected(&'a str, &'a SourceArguments, SourceRoute, &'a Source),
    Failed(Diagnostic),
}

enum Output {
    Checked,
    Program(ir::Program),
    Application(application::Application),
}

#[repr(C)]
struct SourceCandidate {
    key: Bytes,
    definition: *const Arc<Definition>,
    source: crate::syntax::wire::SignatureNode,
    compiled: u32,
    local: u32,
    exported: u32,
    domain: u32,
    selected: u32,
}

#[allow(dead_code)]
#[repr(u32)]
enum SourceExpansion { Closed, Open, Failed }

struct SourcePlanner<'a> {
    world: &'a World,
    entry: &'a str,
    selected: Option<&'a SelectedSource>,
    definitions: declarations::Iter<'a, Arc<Definition>>,
    jobs: Vec<Job<'a>>,
    open_generics: Vec<String>,
}

#[repr(C)]
struct SourcePlanningHost<'a> {
    context: *mut SourcePlanner<'a>,
    next: unsafe extern "C" fn(*mut SourcePlanner<'a>) -> SourceCandidate,
    is_open: unsafe extern "C" fn(*mut SourcePlanner<'a>, *const Arc<Definition>) -> u32,
    schedule: unsafe extern "C" fn(
        *mut SourcePlanner<'a>, Bytes, *const Arc<Definition>, SourceRoute, u32,
    ) -> SourceExpansion,
    record_open: unsafe extern "C" fn(*mut SourcePlanner<'a>, Bytes),
    selected: unsafe extern "C" fn(*mut SourcePlanner<'a>, SourceRoute),
}

unsafe extern "C" fn next_source_candidate(context: *mut SourcePlanner<'_>) -> SourceCandidate {
    let planner = unsafe { &mut *context };
    let Some((key, definition)) = planner.definitions.next() else {
        return SourceCandidate {
            key: Bytes::absent(), definition: std::ptr::null(),
            source: crate::syntax::wire::SignatureNode {
                storage: crate::syntax::wire::StorageView {
                    nodes: std::ptr::null_mut(), node_bytes: 0, text: std::ptr::null_mut(), text_bytes: 0,
                    source: std::ptr::null_mut(), source_bytes: 0,
                },
                offset: 0,
            },
            compiled: 0, local: 0, exported: 0, domain: 0, selected: 0,
        };
    };
    SourceCandidate {
        key: Bytes::new(key), definition, source: definition.function.signature_node(),
        compiled: u32::from(definition.compiled.is_some()),
        local: u32::from(definition.module == planner.entry),
        exported: u32::from(definition.public),
        domain: u32::from(definition.execution == execution::Requirement::Domain),
        selected: u32::from(planner.selected.is_some_and(|(selected, ..)| selected == key)),
    }
}

unsafe extern "C" fn source_is_open(_: *mut SourcePlanner<'_>, definition: *const Arc<Definition>) -> u32 {
    let function = &unsafe { &*definition }.function;
    u32::from(generics::is_open(&function.types(), &function.values()))
}

unsafe extern "C" fn schedule_source_instances(
    context: *mut SourcePlanner<'_>, key: Bytes, definition: *const Arc<Definition>,
    route: SourceRoute, cpu_signature: u32,
) -> SourceExpansion {
    let planner = unsafe { &mut *context };
    let start = planner.jobs.len();
    // B enumerates at the synchronous SDK entry. Each expansion owns its World invocation.
    let expanded = unsafe {
        planner.world.run(generics::for_each_instance(planner.world, key.text(), &*definition, |instance| {
            planner.jobs.push(Job::Instance(instance, route, cpu_signature != 0));
        }))
    };
    match expanded {
        Ok(false) => SourceExpansion::Closed,
        Ok(true) => SourceExpansion::Open,
        Err(error) => {
            planner.jobs.truncate(start);
            planner.jobs.push(Job::Failed(error));
            SourceExpansion::Failed
        }
    }
}

unsafe extern "C" fn record_open_source(context: *mut SourcePlanner<'_>, key: Bytes) {
    unsafe { &mut *context }.open_generics.push(unsafe { key.text() }.to_owned());
}

unsafe extern "C" fn schedule_selected_source<'a>(context: *mut SourcePlanner<'a>, route: SourceRoute) {
    let planner = unsafe { &mut *context };
    planner.jobs.extend(planner.selected.map(|(key, arguments, source)| {
        Job::Selected(key, arguments, route, source)
    }));
}

#[repr(u32)]
enum SourceSignature { Absent, Present, Failed }

struct SourceInstance<'a> {
    world: &'a World,
    instance: &'a generics::Instance,
    signature: Option<check::KernelSignature>,
    output: std::mem::MaybeUninit<Result<Output>>,
}

#[repr(C)]
struct SourceInstanceHost<'a> {
    context: *mut SourceInstance<'a>,
    signature: unsafe extern "C" fn(*mut SourceInstance<'a>) -> SourceSignature,
    function: unsafe extern "C" fn(*mut SourceInstance<'a>),
    domain: unsafe extern "C" fn(*mut SourceInstance<'a>),
    program: unsafe extern "C" fn(*mut SourceInstance<'a>, u32),
    application: unsafe extern "C" fn(*mut SourceInstance<'a>),
}

unsafe extern "C" {
    #[link_name = "bootstrap.module_load::source_module_entry"]
    fn source_module_entry(source: crate::syntax::wire::SignatureNode, local: u32, public: u32) -> u32;
    #[link_name = "bootstrap.module_load::plan_source_module"]
    fn plan_source_module(mode: SourceMode, host: SourcePlanningHost<'_>) -> u32;
    #[link_name = "bootstrap.module_load::compile_source_instance"]
    fn compile_source_instance(route: SourceRoute, cpu_signature: u32, host: SourceInstanceHost<'_>);
}

unsafe extern "C" fn source_signature(context: *mut SourceInstance<'_>) -> SourceSignature {
    let work = unsafe { &mut *context };
    match unsafe { work.world.run(check::kernel_signature(work.world, work.instance)) } {
        Ok(Some(signature)) => {
            work.signature = Some(signature);
            SourceSignature::Present
        }
        Ok(None) => SourceSignature::Absent,
        Err(error) => {
            work.output.write(Err(error));
            SourceSignature::Failed
        }
    }
}

unsafe extern "C" fn check_source_function(context: *mut SourceInstance<'_>) {
    let work = unsafe { &mut *context };
    let result = unsafe { work.world.run(check::check_function(work.world, work.instance)) };
    work.output.write(result.map(|_| Output::Checked).map_err(|error| instance_error(error, &work.instance.name)));
}

unsafe extern "C" fn check_source_domain(context: *mut SourceInstance<'_>) {
    let work = unsafe { &mut *context };
    let result = unsafe { work.world.run(check::check_domain(work.world, work.instance, work.signature.as_ref())) };
    work.output.write(result.map(|_| Output::Checked).map_err(|error| instance_error(error, &work.instance.name)));
}

unsafe extern "C" fn build_source_program(context: *mut SourceInstance<'_>, publish: u32) {
    let work = unsafe { &mut *context };
    let signature = work.signature.take();
    let result = unsafe { work.world.run(check::compile_program(work.world, work.instance, signature)) }
        .map_err(|error| instance_error(error, &work.instance.name))
        .and_then(|mut program| {
            if publish == 0 { return Ok(Output::Checked); }
            optimize::optimize(&mut program, &work.world.worker)
                .map_err(|error| crate::compiler::diagnostic(&work.instance.definition.source.path, error))?;
            Ok(Output::Program(program))
        });
    work.output.write(result);
}

unsafe extern "C" fn build_source_application(context: *mut SourceInstance<'_>) {
    let work = unsafe { &mut *context };
    let result = unsafe { work.world.run(check::compile_application(work.world, work.instance)) };
    work.output.write(result.map(Output::Application));
}

pub(crate) fn compile_world(
    world: &World,
    entry: String,
    require_program: bool,
    requested: Option<(EntryKind, &str)>,
    entry_checks: Option<&EntryChecks>,
) -> Result<Compilation> {
    let mode = match requested {
        None => SourceMode::Automatic,
        Some((EntryKind::Domain, _)) => SourceMode::SelectedProgram,
        Some((EntryKind::Application, _)) => SourceMode::Application,
    };
    let selected = if let Some((_, request)) = requested {
        let source = Source {
            path: PathBuf::from("<entry>"),
            text: request.into(),
        };
        let (name, arguments) = parser::entry(source.clone())?;
        let key = world
            .resolve(&name, &entry, &[], true).unique()
            .ok_or_else(|| Diagnostic {
                source: source.clone(),
                span: Span::default(),
                code: "B240",
                message: format!("没有公开入口 {name}"),
            })?;
        let definition = &world.functions[&key];
        let eligible = unsafe { source_module_entry(
            definition.function.signature_node(), u32::from(definition.module == entry), u32::from(definition.public),
        ) };
        if eligible == 0 {
            return source.fail(
                Span::default(),
                "B240",
                "入口选择需要本模块对应种类的公开函数",
            );
        }
        Some((key, arguments, source))
    } else {
        None
    };
    let mut planner = SourcePlanner {
        world, entry: &entry, selected: selected.as_ref(), definitions: world.functions.iter(), jobs: Vec::new(),
        open_generics: world.records.iter()
            .filter(|(_, (record, ..))| generics::is_open(&record.types(), &record.values()))
            .map(|(name, _)| name.to_owned()).collect(),
    };
    let open_public_domain = unsafe {
        plan_source_module(mode, SourcePlanningHost {
            context: &mut planner,
            next: next_source_candidate,
            is_open: source_is_open,
            schedule: schedule_source_instances,
            record_open: record_open_source,
            selected: schedule_selected_source,
        }) != 0
    };
    let SourcePlanner { jobs, open_generics, .. } = planner;
    let declarations = &world.declarations;
    let collect_declarations = world.build_declarations.is_some();
    let compile = |world: &mut World, job: Job<'_>| -> Result<_> {
        let (instance, route, cpu_signature) = match job {
            Job::Failed(error) => return Err(error),
            Job::Instance(instance, route, cpu_signature) => (instance, route, cpu_signature),
            Job::Selected(key, arguments, route, source) => {
                let instance = unsafe { world.run(async {
                    let definition = &world.functions[key];
                    let span = Span { start: 0, end: source.text.len() };
                    let bindings = Arc::new(generics::Bindings::default());
                    let arguments = match arguments.as_ref() {
                        Some(arguments) => Some(unsafe { world.call(bindings.arguments(world, arguments,
                            (source, &definition.module, &definition.imports), span)).await }?),
                        None => None,
                    };
                    unsafe { world.call(check::generics::specialize(world, key, &[], arguments.as_deref(),
                        (source, &definition.module, &definition.imports), span)).await }
                }) }?;
                (instance, route, false)
            }
        };
        let mut work = SourceInstance {
            world, instance: &instance, signature: None, output: std::mem::MaybeUninit::uninit(),
        };
        let output = unsafe {
            compile_source_instance(route, u32::from(cpu_signature), SourceInstanceHost {
                context: &mut work,
                signature: source_signature,
                function: check_source_function,
                domain: check_source_domain,
                program: build_source_program,
                application: build_source_application,
            });
            // B either receives a signature failure or calls exactly one terminal action.
            work.output.assume_init()
        }?;
        let used = world.build_declarations.as_ref()
            .map(|inputs| std::mem::take(&mut *inputs.borrow_mut())).unwrap_or_default();
        Ok((output, used))
    };

    enum JobInputs {
        Owned(BTreeSet<build::Declaration>),
        Shared(entry_checks::SharedInputs),
    }
    let output = parallel::try_map(&world.worker, jobs, |worker| {
        let mut world = World::new(Arc::clone(declarations), worker.clone());
        if collect_declarations { world.build_declarations = Some(RefCell::new(BTreeSet::new())); }
        world
    }, |world, job| {
        if let Some(checks) = entry_checks
            && let Job::Instance(instance, route @ (SourceRoute::Function | SourceRoute::ValidateProgram), cpu_signature) = &job
        {
            let worker = world.worker.clone();
            let name = instance.name.clone();
            let source = instance.definition.source.path.clone();
            let inputs = checks.run(name, *route, *cpu_signature, &worker, &source,
                || compile(world, job).map(|(_, used)| used))?;
            return Ok((Output::Checked, JobInputs::Shared(inputs)));
        }
        compile(world, job).map(|(output, used)| (output, JobInputs::Owned(used)))
    }).map_err(|error| match error {
        parallel::Error::Work(error) => error,
        parallel::Error::Resource(error) => Diagnostic {
            source: world.modules[&entry].source().clone(), span: Span::default(), code: "B020",
            message: format!("无法创建本机编译工作：{error}"),
        },
    })?;
    let mut programs = Vec::new();
    let mut templates = Vec::new();
    let mut applications = Vec::new();
    for (output, used) in output {
        if let Some(inputs) = &world.build_declarations {
            let mut inputs = inputs.borrow_mut();
            match used {
                JobInputs::Owned(used) => inputs.extend(used),
                JobInputs::Shared(used) => inputs.extend(used.iter().cloned()),
            }
        }
        match output {
            Output::Application(application) => applications.push(application),
            Output::Checked => {}
            Output::Program(program) => {
                if !crate::ir::ProgramInterface::requires_bindings(&program) { programs.push(ExecutableProgram(program)); }
                else { templates.push(TemplateProgram(program)); }
            }
        }
    }
    if require_program && programs.is_empty() && templates.is_empty() {
        if open_public_domain {
            return world.modules[&entry].source().fail(
                Span::default(),
                "B240",
                "开放泛型 program 需要选择具体入口，例如 compile_entry(..., \"run<13>\")",
            );
        }
        return world.modules[&entry].source().fail(
            Span::default(),
            "B021",
            "入口模块没有公开的 program；函数库使用 check，并在 program 中实例化原生代码",
        );
    }
    Ok(Compilation {
        programs,
        templates,
        open_generics,
        applications,
    })
}

#[derive(Clone)]
pub(crate) struct ExportRequest {
    pub(crate) source: Source,
    pub(crate) name: String,
    pub(crate) arguments: Option<crate::syntax::List<crate::syntax::wire::ArgumentNode>>,
}

impl ExportRequest {
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let source = Source { path: "<module-export>".into(), text: text.into() };
        let (name, arguments) = parser::entry(source.clone())?;
        Ok(Self { source, name, arguments })
    }

    pub(crate) fn visit_type_owners(&self, visit: impl FnMut(&str) -> bool) -> bool {
        self.arguments.as_ref().is_none_or(|arguments| loading::visit_export_types(arguments, visit))
    }
}

pub(crate) struct CapturedExport {
    pub(crate) request: ExportRequest,
    pub(crate) owners: Vec<String>,
}

pub(crate) enum ExportRequests<'a> {
    Text(std::slice::Iter<'a, String>),
    Parsed(std::vec::IntoIter<ExportRequest>),
    Captured(std::slice::Iter<'a, Result<CapturedExport>>),
}

impl<'a> Iterator for ExportRequests<'a> {
    type Item = Result<std::borrow::Cow<'a, ExportRequest>>;

    fn next(&mut self) -> Option<Self::Item> {
        use std::borrow::Cow;
        match self {
            Self::Text(requests) => requests.next().map(|text| ExportRequest::parse(text).map(Cow::Owned)),
            Self::Parsed(requests) => requests.next().map(|request| Ok(Cow::Owned(request))),
            Self::Captured(requests) => requests.next().map(|request| {
                request.as_ref().map(|capture| Cow::Borrowed(&capture.request)).map_err(Clone::clone)
            }),
        }
    }
}

pub(crate) fn read_world<'a>(
    path: &Path,
    module_root: &Path,
    imported: Option<&module::Imports>,
    requests: &'a [String],
    worker: &compiler::Worker,
    inputs: Option<&'a build::Inputs>,
    build_declarations: Option<RefCell<BTreeSet<build::Declaration>>>,
) -> Result<(World, String, ExportRequests<'a>)> {
    let mut world = DeclarationInput::default();
    let entry = load(
        path,
        module_root,
        &mut world,
        imported,
        inputs,
    )?;
    let requests = if let Some(imported) = imported {
        if let Some(captured) = inputs.and_then(build::Inputs::exports) {
            loading::load_captured_exports(captured, module_root, &mut world, imported, inputs)?;
            ExportRequests::Captured(captured.iter())
        } else {
            ExportRequests::Parsed(loading::prepare_exports(requests, module_root, &mut world, imported, inputs)?.into_iter())
        }
    } else {
        ExportRequests::Text(requests.iter())
    };
    let mut world = world.publish();
    execution::infer(&mut world, build_declarations.as_ref());
    let mut world = World::new(Arc::new(world), worker.clone());
    loading::prepare_world(&mut world, build_declarations.is_some())?;
    world.build_declarations = build_declarations;
    Ok((world, entry, requests))
}
pub(crate) async fn validate_types(world: &World) -> Result<()> {
    for name in world.type_sets.keys() {
        unsafe { world.call(world.validate_type_set(name, &mut BTreeSet::new())).await }?;
    }
    for (name, (record, source, module, imports)) in &world.records {
        generics::validate_record_parameters(record, source)?;
        unsafe { world.call(generics::for_each_combination(
            world,
            &record.types(),
            &record.values(),
            (source, record.span(), module, imports),
            async |arguments, row| {
                let mut ty = TypeName::named(name);
                ty.arguments = arguments.into_iter().map(|argument| GenericArgument::Type(argument.syntax))
                    .chain(row.iter().map(|&value| GenericArgument::Value(Shape::Literal(value))))
                    .collect();
                unsafe { world.call(world.ty(&ty, source, record.span(), module, imports)).await }?;
                Ok(())
            },
        )).await }?;
    }

    Ok(())
}
pub(crate) fn instance_error(mut error: Diagnostic, name: &str) -> Diagnostic {
    if name.contains('<') {
        error.message += &format!("（检查泛型实例 {name}）");
    }
    error
}

impl Declarations {
    pub(crate) fn compiled_function<'a, 'name>(
        &'a self, symbol: &'name str,
    ) -> Option<(&'name str, &'a Definition, &'a [u8])> {
        // Instances and domain entry points keep their full symbol within the declaring module.
        let end = symbol.find(['<', '#']).unwrap_or(symbol.len());
        let name = &symbol[..end];
        let declaration = self.functions.get(name)?;
        let contract = declaration.compiled.as_ref()?.get(symbol)?;
        Some((name, declaration, contract))
    }


    pub(crate) fn resolve(
        &self,
        name: &str,
        module: &str,
        imports: &[String],
        functions: bool,
    ) -> NameResolution {
        if functions { self.functions.resolve(name, module, imports) }
        else { self.records.resolve(name, module, imports) }
    }
}
impl World {
    pub(crate) async fn type_reference(&self, name: &TypeName, source: &Source, span: Span, module: &str, imports: &[String]) -> crate::Result<TypeName> {
        let builtin=crate::builtin_types::Builtin::recognize(&name.name);
        unsafe{self.call(type_reference::bind(self,name,source,span,module,imports,builtin)).await}.map(|bound|bound.name)
    }
    async fn type_layout_text(&self, value: &ir::Ty, source: &Source, span: Span,
        module: &str, imports: &[String]) -> Result<String> {
        let mut layout = ir::ExpandedLayout::new(value);
        while let Some(reference) = layout.next() {
            let element = unsafe {
                self.call(self.ty_inner(reference, source, span, module, imports, &mut BTreeSet::new())).await
            }?;
            layout.expand(element);
        }
        Ok(layout.finish())
    }

    pub(crate) async fn stored_borrows(
        &self, ty: &ir::Ty, source: &Source, span: Span, module: &str, imports: &[String],
        cleanup: bool,
    ) -> crate::Result<bool> {
        let mode = if cleanup { ir::StorageMode::CleanupBorrow } else { ir::StorageMode::Borrow };
        self.storage_query(ty, (source, span, module, imports),
            &mut BTreeSet::new(), mode).await
    }

    pub(crate) async fn cleanup_requirements(
        &self, ty: &ir::Ty, source: &Source, span: Span, module: &str, imports: &[String],
    ) -> crate::Result<bool> {
        self.storage_query(ty, (source, span, module, imports),
            &mut BTreeSet::new(), ir::StorageMode::CleanupRequirement).await
    }

    async fn storage_query(
        &self, ty: &ir::Ty, origin: (&Source, Span, &str, &[String]),
        consumed: &mut BTreeSet<(String, ir::StorageMode)>, mode: ir::StorageMode,
    ) -> crate::Result<bool> {
        use ir::StorageNeed;

        let mut walk = ir::StorageWalk::new(ty, mode);
        let mut found = false;
        loop {
            let step = walk.step(found);
            let child = match step.need {
                StorageNeed::Absent => return Ok(false),
                StorageNeed::Present => return Ok(true),
                StorageNeed::Enter => {
                    if !consumed.insert((ty.name.clone(), walk.mode())) {
                        return Ok(false);
                    }
                    continue;
                }
                StorageNeed::Contract => {
                    let cleanup = &self.functions[ty.cleanup.as_ref().unwrap()];
                    return Ok(cleanup.function.contracts().iter()
                        .any(|contract| contract.kind() == ContractKind::Requires));
                }
                StorageNeed::Field => ty.fields[step.index].2.as_ref(),
                StorageNeed::Variant => ty.variants[step.index].ty.as_ref(),
                StorageNeed::Array => ty.array.as_deref().unwrap(),
                StorageNeed::Dataset => {
                    let (source, span, module, imports) = origin;
                    let reference = ty.dataset.as_deref().unwrap();
                    let element = unsafe { self.call(self.ty_inner(reference, source, span, module, imports,
                        &mut BTreeSet::new())).await }?;
                    found = unsafe { self.call(self.storage_query(&element, origin, consumed, walk.mode())).await }?;
                    continue;
                }
            };
            found = unsafe { self.call(self.storage_query(child, origin, consumed, walk.mode())).await }?;
        }
    }

    pub(crate) async fn validate_type_references(
        &self, ty: &ir::Ty, source: &Source, span: Span, module: &str, imports: &[String],
        consumed: &mut BTreeSet<String>,
    ) -> crate::Result<()> {
        use ir::ReferenceNeed;

        let mut walk = ir::ReferenceWalk::new(ty);
        let mut parameters = Vec::new();
        let mut result = None;
        loop {
            let step = walk.step();
            let nested = match step.need {
                ReferenceNeed::Done => return Ok(()),
                ReferenceNeed::Enter => {
                    if !consumed.insert(ty.name.clone()) {
                        return Ok(());
                    }
                    continue;
                }
                ReferenceNeed::Signature => {
                    let function = ty.function.as_deref().unwrap();
                    (parameters, result, _) = unsafe {
                        self.call(self.c_function_signature(function, source, span, module, imports)).await
                    }?;
                    unsafe {
                        self.call(crate::check::outputs::validate_function_contracts(
                            self, &parameters, result.as_ref(), &function.contracts, source, span,
                        )).await
                    }?;
                    walk.signature(parameters.len(), result.is_some());
                    continue;
                }
                ReferenceNeed::Parameter => match &parameters[step.index].kind {
                    ir::CallParameterKind::Value(ty) | ir::CallParameterKind::Reference { ty, .. } => ty,
                },
                ReferenceNeed::Result => result.as_ref().unwrap(),
                ReferenceNeed::Argument => &ty.arguments[step.index],
                ReferenceNeed::Field => &ty.fields[step.index].2,
                ReferenceNeed::Variant => &ty.variants[step.index].ty,
                ReferenceNeed::Array => ty.array.as_deref().unwrap(),
                ReferenceNeed::View => ty.view.as_deref().unwrap(),
                ReferenceNeed::Dataset => {
                    let reference = ty.dataset.as_deref().unwrap();
                    let element = unsafe {
                        self.call(self.ty_inner(reference, source, span, module, imports, &mut BTreeSet::new())).await
                    }?;
                    return unsafe {
                        self.call(self.validate_type_references(&element, source, span, module, imports, consumed)).await
                    };
                }
            };
            unsafe {
                self.call(self.validate_type_references(nested, source, span, module, imports, consumed)).await
            }?;
        }
    }
    pub(crate) async fn ty(
        &self,
        name: &TypeName,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
    ) -> Result<ir::Ty> {
        let ty = unsafe { self.call(self.ty_inner(name, source, span, module, imports, &mut BTreeSet::new())) .await }?;
        unsafe { self.call(self.validate_type_references(&ty, source, span, module, imports, &mut BTreeSet::new())) .await }?;
        Ok(ty)
    }
    pub(crate) async fn ty_inner(
        &self,
        name: &TypeName,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
        stack: &mut BTreeSet<String>,
    ) -> Result<ir::Ty> {
        unsafe { self.call(type_value::resolve(self, name, source, span, module, imports, stack)) .await }
    }
}

/// 可见声明查询的事实；局部绑定及实际调用诊断仍由原消费者负责。
pub(crate) enum NameResolution {
    Missing,
    Unique(String),
    Ambiguous,
}
impl NameResolution {
    pub(crate) fn unique(self) -> Option<String> {
        match self {
            Self::Unique(name) => Some(name),
            Self::Missing | Self::Ambiguous => None,
        }
    }
    pub(crate) fn is_missing(&self) -> bool { matches!(self, Self::Missing) }
}

