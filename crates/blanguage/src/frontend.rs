mod type_reference;
mod type_value;
mod loading;
pub(crate) use loading::load;
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
pub(crate) struct Declarations {
    pub(crate) sources: BTreeMap<String, Source>,
    pub(crate) interfaces: BTreeMap<String, Arc<crate::syntax::Module>>,
    pub(crate) records: BTreeMap<String, (crate::syntax::Record, Source, String, Arc<[String]>)>,
    pub(crate) functions: BTreeMap<String, Arc<Definition>>,
    pub(crate) type_sets: BTreeMap<String, (crate::syntax::TypeSet, Source, String, Arc<[String]>)>,
}
pub(crate) struct World {
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
        Self { declarations, worker, evaluating_constants: crate::names::ActiveDependencies::default(), build_declarations: None }
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
    pub(crate) fn record(&self, key: &str) -> &(crate::syntax::Record, Source, String, Arc<[String]>) {
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
    compiler.ast(path, |worker| {
        let imports = module::Imports::new(dependencies).map_err(|error| Diagnostic {
            source: Source { path: path.into(), text: "".into() },
            span: Span::default(), code: "B270", message: error.to_string(),
        })?;
        let imported = if dependencies.is_empty() { None } else { Some(&imports) };
        let (world, entry) = read_world(path, module_root, imported, &[], worker, None, None)?;
        compile_world(&world, entry, require_program, requested)
    })
}
impl Compiler {
    /// Resolves a concrete value type using the original module/type checker, without codegen.
    pub fn type_layout(&self, path: &Path, root: &Path, request: &str) -> Result<ir::Ty> {
        self.ast(path, |worker| {
            let source = Source { path: "<layout>".into(), text: request.into() };
            let span = Span { start: 0, end: request.len() };
            let name = parser::type_query(source.clone())?;
            let (world, module) = read_world(path, root, None, &[], worker, None, None)?;
            let imports = &world.interfaces[&module].imports().map(str::to_owned).collect::<Vec<_>>();
            let bindings=Arc::new(generics::Bindings::default());
            let name=bindings.type_name(&world,&name,(&source,&module,imports),span)?;
            let value = world.ty(&name, &source, span, &module, imports)?;
            if name.dataset {
                let count = world.shape(&name.count, &source, span, &module, imports)?;
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
            let (world, module) = read_world(path, root, None, &[], worker, None, None)?;
            let imports = &world.interfaces[&module].imports().map(str::to_owned).collect::<Vec<_>>();
            let bindings=Arc::new(generics::Bindings::default());
            let name=bindings.type_name(&world,&name,(&source,&module,imports),span)?;
            let value = world.ty(&name, &source, span, &module, imports)?;
            let value = if name.dataset {
                ir::Ty::owned_dataset(value.syntax(), world.shape(&name.count, &source, span, &module, imports)?)
            } else { value };
            let (offset, selected) = if let Some(field) = field {
                value.field(field).ok_or_else(|| Diagnostic { source: source.clone(), span, code: "B141",
                    message: format!("类型 {} 没有可定位的静态字段路径 {field}", value.name) })?
            } else { (0, &value) };
            let mut json = String::new();
            world.write_type_layout_json(&mut json, selected, &source, span, &module, imports, &mut BTreeSet::new())?;
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
    pub fn compile_application(&self, path: &Path, root: &Path, entry: &str) -> Result<application::Application> {
        Ok(compile_module_on(self, path, root, false, Some((EntryKind::Application, entry)), &[])?.applications.remove(0))
    }
    pub fn compile_application_with_modules(
        &self, path: &Path, root: &Path, entry: &str, dependencies: &[object::Object],
    ) -> Result<application::Application> {
        Ok(compile_module_on(self, path, root, false,
            Some((EntryKind::Application, entry)), dependencies)?.applications.remove(0))
    }
}
pub(crate) fn compile_world(
    world: &World,
    entry: String,
    require_program: bool,
    requested: Option<(EntryKind, &str)>,
) -> Result<Compilation> {
    let selected = if let Some((kind, request)) = requested {
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
        if !definition.public
            || definition.module != entry
            || definition.function.kind() != FunctionKind::Value
        {
            return source.fail(
                Span::default(),
                "B240",
                "入口选择需要本模块对应种类的公开函数",
            );
        }
        Some((key, arguments, kind, source))
    } else {
        None
    };
    enum Job<'a> {
        Instance(generics::Instance, bool),
        Failed(Diagnostic),
        Selected(&'a str, &'a Option<crate::syntax::List<crate::syntax::wire::ArgumentNode>>, EntryKind, &'a Source),
    }
    enum Output {
        Programs(Vec<ir::Program>),
        Application(application::Application),
    }
    let for_application = matches!(selected.as_ref(), Some((_, _, EntryKind::Application, _)));
    let mut jobs = Vec::new();
    let planned = (|| -> Result<()> {
        for (base, definition) in &world.functions {
            if definition.compiled.is_some() || definition.function.kind() == FunctionKind::Constant
                || definition.function.external().is_some() { continue; }
            generics::validate_declaration(definition)?;
            if selected.as_ref().is_some_and(|(key, _, _, _)| key == base) {
                continue;
            }
            let retained = selected.is_none() && definition.module == entry && definition.public;
            jobs.extend(generics::instances(&world, base, definition)?
                .into_iter().map(|instance| Job::Instance(instance, retained)));
        }
        Ok(())
    })();
    if let Err(error) = planned {
        jobs.push(Job::Failed(error));
    } else if let Some((key, arguments, kind, source)) = &selected {
        jobs.push(Job::Selected(key, arguments, *kind, source));
    }
    let declarations = &world.declarations;
    let collect_declarations = world.build_declarations.is_some();
    let output = parallel::try_map(&world.worker, jobs, |worker| {
        let mut world = World::new(Arc::clone(declarations), worker.clone());
        if collect_declarations { world.build_declarations = Some(RefCell::new(BTreeSet::new())); }
        world
    }, |world, job| {
        let output = (|| -> Result<Output> {
        let mut programs = Vec::new();
        match job {
            Job::Failed(error) => return Err(error),
            Job::Instance(instance, retained) => {
                let name = &instance.name;
                let d = &instance.definition;
                if d.execution == execution::Requirement::Domain
                    && (d.function.target() != Target::Cpu || check::kernel_interface(world, &instance)?)
                {
                    if for_application {
                        check::check_domain(world, &instance).map_err(|error| instance_error(error, name))?;
                    } else {
                        let mut program = check::compile_program(world, &instance).map_err(|error| instance_error(error, name))?;
                        if retained {
                            optimize::optimize(&mut program, &world.worker).map_err(|error| crate::compiler::diagnostic(&d.source.path, error))?;
                            programs.push(program);
                        }
                    }
                } else {
                    check::check_function(world, &instance).map_err(|error| instance_error(error, name))?;
                }
            }
            Job::Selected(key, arguments, kind, source) => {
                let definition = &world.functions[key];
                let span=Span{start:0,end:source.text.len()};
                let bindings=Arc::new(generics::Bindings::default());
                let arguments=arguments.as_ref().map(|arguments|bindings.arguments(world,arguments,
                    (source,&definition.module,&definition.imports),span)).transpose()?;
                let instance = check::generics::specialize(world, key, &[], arguments.as_deref(),
                    (source, &definition.module, &definition.imports), span)?;
                if matches!(kind, EntryKind::Application) {
                    return check::compile_application(world, &instance).map(Output::Application);
                }
                let mut program = check::compile_program(world, &instance).map_err(|error| instance_error(error, &instance.name))?;
                optimize::optimize(&mut program, &world.worker).map_err(|error| crate::compiler::diagnostic(&definition.source.path, error))?;
                programs.push(program);
            }
        }
        Ok(Output::Programs(programs))
        })()?;
        let used = world.build_declarations.as_ref()
            .map(|inputs| std::mem::take(&mut *inputs.borrow_mut())).unwrap_or_default();
        Ok((output, used))
    }).map_err(|error| match error {
        parallel::Error::Work(error) => error,
        parallel::Error::Resource(error) => Diagnostic {
            source: world.sources[&entry].clone(), span: Span::default(), code: "B020",
            message: format!("无法创建本机编译工作：{error}"),
        },
    })?;
    let mut programs = Vec::new();
    let mut templates = Vec::new();
    let mut applications = Vec::new();
    for (output, used) in output {
        if let Some(inputs) = &world.build_declarations { inputs.borrow_mut().extend(used); }
        match output {
            Output::Application(application) => applications.push(application),
            Output::Programs(images) => for program in images {
                if !crate::ir::ProgramInterface::requires_bindings(&program) { programs.push(ExecutableProgram(program)); }
                else { templates.push(TemplateProgram(program)); }
            },
        }
    }
    let open_generics: Vec<_> = world
        .records
        .iter()
        .filter(|(_, (record, ..))| generics::is_open(&record.types(), &record.values()))
        .map(|(name, _)| name.clone())
        .chain(
            world
                .functions
                .iter()
                .filter(|(_, d)| generics::is_open(&d.function.types(), &d.function.values()))
                .map(|(name, _)| name.clone()),
        )
        .collect();
    if require_program && programs.is_empty() && templates.is_empty() {
        if world.functions.values().any(|d| {
            d.module == entry
                && d.public
                && d.execution == execution::Requirement::Domain
                && generics::is_open(&d.function.types(), &d.function.values())
        }) {
            return world.sources[&entry].fail(
                Span::default(),
                "B240",
                "开放泛型 program 需要选择具体入口，例如 compile_entry(..., \"run<13>\")",
            );
        }
        return world.sources[&entry].fail(
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
pub(crate) fn read_world(
    path: &Path,
    module_root: &Path,
    imported: Option<&module::Imports>,
    requests: &[String],
    worker: &compiler::Worker,
    inputs: Option<&build::Inputs>,
    build_declarations: Option<RefCell<BTreeSet<build::Declaration>>>,
) -> Result<(World, String)> {
    let mut world = Declarations {
        sources: BTreeMap::new(),
        interfaces: BTreeMap::new(),
        records: BTreeMap::new(),
        functions: BTreeMap::new(),
        type_sets: BTreeMap::new(),
    };
    let entry = load(
        path,
        module_root,
        &mut world,
        imported,
        None,
        inputs,
    )?;
    if let Some(imported) = imported {
        for request in requests {
            let (_, arguments) = parser::entry(Source { path: "<module-export>".into(), text: request.clone().into() })?;
            let mut arguments=arguments.into_iter().flatten().collect::<Vec<_>>();
            while let Some(argument) = arguments.pop() {
                if let crate::syntax::ArgumentKind::Type(ty) = argument.kind() {
                    if let Some((owner, _)) = ty.name().split_once("::")
                        && imported.modules.contains_key(owner)
                    {
                        load(&crate::source_path::module(module_root, owner, u16::from(b'b')), module_root,
                            &mut world, Some(imported), Some(owner), inputs)?;
                    }
                    arguments.extend(ty.arguments());
                }
            }
        }
    }
    execution::infer(&mut world, build_declarations.as_ref());
    for definition in world.functions.values().filter(|definition| definition.compiled.is_some()) {
        generics::validate_declaration(definition)?;
    }
    let mut world = World::new(Arc::new(world), worker.clone());
    if build_declarations.is_some() {
        world.build_declarations = Some(RefCell::new(BTreeSet::new()));
    }
    for (key, definition) in &world.functions {
        if definition.function.kind() == FunctionKind::Constant {
            generics::validate_declaration(definition)?;
            check::constant_value(&world, key)?;
        }
    }
    world.build_declarations = None;
    for definition in world.functions.values().filter(|definition| {
        definition.function.types().is_empty() && definition.function.values().is_empty()
    }) {
        world.check_interface_types(definition, &Arc::new(generics::Bindings::default()))?;
    }
    validate_types(&world)?;
    for (key,definition) in world.functions.iter().filter(|(_,definition)|definition.function.c_abi()) {
        check::external::validate_signature(definition)?;
        if definition.function.types().is_empty()&&definition.function.values().is_empty(){
            let instance=generics::instantiate(&world,key,definition,Vec::new(),&[])?;
            check::external::check_declaration(&world,&instance)?;
        }
    }
    world.build_declarations = build_declarations;
    Ok((world, entry))
}
pub(crate) fn validate_types(world: &World) -> Result<()> {
    for name in world.type_sets.keys() {
        world.for_each_set_member(name, &mut BTreeSet::new(), |_| {})?;
    }
    for (name, (record, source, module, imports)) in &world.records {
        generics::validate_record_parameters(record, source)?;
        generics::for_each_combination(
            world,
            &record.types(),
            &record.values(),
            (source, record.span(), module, imports),
            |arguments, row| {
                let mut ty = TypeName::named(name);
                ty.arguments = arguments.into_iter().map(|argument| GenericArgument::Type(argument.syntax))
                    .chain(row.iter().map(|&value| GenericArgument::Value(Shape::Literal(value))))
                    .collect();
                world.ty(&ty, source, record.span(), module, imports)?;
                Ok(())
            },
        )?;
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
    pub(crate) fn resolve(
        &self,
        name: &str,
        module: &str,
        imports: &[String],
        functions: bool,
    ) -> NameResolution {
        crate::names::resolve(name,module,imports,|name| {
            if functions {
                self.functions.get_key_value(name).map(|(key,declaration)|crate::names::Declaration {
                    name:key,owner:&declaration.module,public:declaration.public,
                })
            } else {
                self.records.get_key_value(name).map(|(key,(_,source,owner,_))|crate::names::Declaration {
                    name:key,owner,public:source.path.extension().is_some_and(|extension|extension=="h"),
                })
            }
        })
    }
}
impl World {
    pub(crate) fn type_reference(&self, name: &TypeName, source: &Source, span: Span, module: &str, imports: &[String]) -> crate::Result<TypeName> {
        type_reference::bind(self,name,source,span,module,imports)
    }
    fn write_type_layout_json(&self, out: &mut String, value: &ir::Ty, source: &Source, span: Span,
        module: &str, imports: &[String], active: &mut BTreeSet<String>) -> Result<()> {
        active.insert(value.name.clone());
        let result = ir::write_type_json(out, value, &mut |out, reference| {
            let element = self.ty_inner(reference, source, span, module, imports, &mut BTreeSet::new())?;
            write!(out, ",\"element_stride_bytes\":{},\"element\":", ir::align_up(element.size, element.align)).unwrap();
            if active.contains(&element.name) {
                out.push_str("{\"kind\":\"type_reference\",\"name\":");
                crate::json::string(out, &element.name); out.push('}');
            } else {
                self.write_type_layout_json(out, &element, source, span, module, imports, active)?;
            }
            Ok(())
        });
        active.remove(&value.name);
        result
    }
    // Query stored borrows through nominal references; cleanup retains possible
    // reads by actual F. Generic arguments and signatures do not occupy storage.
    pub(crate) fn stored_borrows(&self, ty: &ir::Ty, source: &Source, span: Span, module: &str, imports: &[String], consumed: &mut BTreeSet<(String,bool)>, cleanup: bool) -> crate::Result<bool> {
        let cleanup=cleanup&&ty.cleanup.is_none();
        if !consumed.insert((ty.name.clone(),cleanup)) { return Ok(false); }
        if ty.view.is_some() || ty.is_address() { return Ok(!cleanup); }
        for nested in ty.fields.iter().map(|(_, _, ty)| ty.as_ref())
            .chain(ty.variants.iter().map(|variant| variant.ty.as_ref()))
            .chain(ty.array.as_deref().filter(|_| ty.values[0] != 0)) {
            if self.stored_borrows(nested, source, span, module, imports, consumed, cleanup)? { return Ok(true); }
        }
        if let Some(reference) = ty.dataset.as_deref().filter(|_| ty.values.first() != Some(&0)) {
            let element = self.ty_inner(reference, source, span, module, imports, &mut BTreeSet::new())?;
            return self.stored_borrows(&element, source, span, module, imports, consumed, cleanup);
        }
        Ok(false)
    }
    // An absent element model loses only actual F input requirements.
    pub(crate) fn cleanup_requirements(&self, ty: &ir::Ty, source: &Source, span: Span, module: &str, imports: &[String], consumed: &mut BTreeSet<String>) -> crate::Result<bool> {
        if !consumed.insert(ty.name.clone()) { return Ok(false); }
        if let Some(key) = &ty.cleanup {
            return Ok(self.functions[key].function.contracts().iter().any(|contract| contract.kind() == ContractKind::Requires));
        }
        for nested in ty.fields.iter().map(|(_, _, ty)| ty.as_ref())
            .chain(ty.variants.iter().map(|variant| variant.ty.as_ref()))
            .chain(ty.array.as_deref().filter(|_| ty.values[0] != 0)) {
            if self.cleanup_requirements(nested, source, span, module, imports, consumed)? { return Ok(true); }
        }
        if let Some(reference) = ty.dataset.as_deref().filter(|_| ty.values.first() != Some(&0)) {
            let element = self.ty_inner(reference, source, span, module, imports, &mut BTreeSet::new())?;
            return self.cleanup_requirements(&element, source, span, module, imports, consumed);
        }
        Ok(false)
    }
    pub(crate) fn validate_type_references(&self, ty: &ir::Ty, source: &Source, span: Span, module: &str, imports: &[String], consumed: &mut BTreeSet<String>) -> crate::Result<()> {
        if !consumed.insert(ty.name.clone()) { return Ok(()); }
        if let Some(function) = &ty.function {
            let (parameters, result, _) = self.c_function_signature(function, source, span, module, imports)?;
            crate::check::outputs::validate_function_contracts(
                self,&parameters, result.as_ref(), &function.contracts, source, span)?;
            for parameter in parameters {
                let ty = match parameter.kind { ir::CallParameterKind::Value(ty) | ir::CallParameterKind::Reference { ty, .. } => ty };
                self.validate_type_references(&ty, source, span, module, imports, consumed)?;
            }
            if let Some(result) = result { self.validate_type_references(&result, source, span, module, imports, consumed)?; }
        }
        for nested in ty.arguments.iter().chain(ty.fields.iter().map(|(_, _, ty)| ty.as_ref()))
            .chain(ty.variants.iter().map(|variant| variant.ty.as_ref()))
            .chain(ty.array.as_deref()).chain(ty.view.as_deref()) {
            self.validate_type_references(nested, source, span, module, imports, consumed)?;
        }
        if let Some(reference) = ty.dataset.as_deref() {
            let element=self.ty_inner(reference,source,span,module,imports,&mut BTreeSet::new())?;
            self.validate_type_references(&element,source,span,module,imports,consumed)?;
        }
        Ok(())
    }
    pub(crate) fn ty(
        &self,
        name: &TypeName,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
    ) -> Result<ir::Ty> {
        let ty = self.ty_inner(name, source, span, module, imports, &mut BTreeSet::new())?;
        self.validate_type_references(&ty, source, span, module, imports, &mut BTreeSet::new())?;
        Ok(ty)
    }
    pub(crate) fn ty_inner(
        &self,
        name: &TypeName,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
        stack: &mut BTreeSet<String>,
    ) -> Result<ir::Ty> {
        type_value::resolve(self, name, source, span, module, imports, stack)
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

