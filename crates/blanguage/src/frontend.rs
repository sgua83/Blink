use crate::*;
use std::{
    cell::RefCell,
    fmt::Write,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

#[repr(C)]
struct FieldPlacement {
    offset: u64,
    end: u64,
    alignment: u32,
}

#[repr(C)]
struct ArrayLayout {
    size: u64,
    stride: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.type_layout::fixed_array"]
    fn fixed_array_layout(element_size: u32, alignment: u32, count: u32) -> ArrayLayout;
    #[link_name = "bootstrap.type_layout::place"]
    fn place_field(size: u32, alignment: u32, field_size: u32, field_alignment: u32) -> FieldPlacement;
    #[link_name = "bootstrap.type_layout::natural_size"]
    fn natural_record_size(size: u32, alignment: u32) -> u64;
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
pub(crate) fn read(path: &Path) -> Result<Source> {
    let text = std::fs::read_to_string(path).map_err(|e| Diagnostic {
        source: Source {
            path: path.into(),
            text: "".into(),
        },
        span: Span::default(),
        code: "B020",
        message: format!("无法读取模块：{e}"),
    })?;
    Ok(Source {
        path: path.into(),
        text: text.into(),
    })
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
                        load(&module_root.join(owner.replace('.', "/")).with_extension("b"), module_root,
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
        world.set_members(name, &mut BTreeSet::new())?;
    }
    for (name, (record, source, module, imports)) in &world.records {
        generics::validate_parameters(&record.types(), &record.name(), source, record.span())?;
        value_parameters::validate(&record.values(), record.name(), std::iter::empty::<crate::syntax::Param>(), source, record.span())?;
        generics::for_each_combination(
            world,
            &record.types(),
            &record.values(),
            (source, record.span(), module, imports),
            |arguments, row| {
                let mut ty = TypeName::named(name);
                ty.arguments = arguments.into_iter().map(GenericArgument::Type)
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
pub(crate) fn load(
    path: &Path,
    root: &Path,
    world: &mut Declarations,
    imported: Option<&module::Imports>,
    import: Option<&str>,
    inputs: Option<&build::Inputs>,
) -> Result<String> {
    if let Some(name) = import
        && world.sources.contains_key(name)
    {
        return Ok(name.to_owned());
    }
    let compiled = imported.and_then(|imports| import.and_then(|name| imports.modules.get(name)));
    let (interface, implementation, captured_imports) = if let Some(compiled) = compiled {
        let name = import.unwrap();
        let interface = if let Some(inputs) = inputs { inputs.interface(name, &compiled.header)? }
        else { Arc::new(parser::parse(Source {
            path: PathBuf::from(format!("<{name}>.h")),
            text: compiled.header.clone().into(),
        })?) };
        (interface, None, None)
    } else if let Some(inputs) = inputs {
        let (interface, implementation, imports) = inputs.source(path)?;
        (interface, implementation, Some(imports))
    } else {
        let interface = Arc::new(parser::parse(read(&path.with_extension("h"))?)?);
        let implementation_path = path.with_extension("b");
        let present = implementation_path.try_exists().map_err(|error| Diagnostic {
            source: interface.source.clone(),
            span: Span::default(),
            code: "B020",
            message: format!("无法访问实现源 {}：{error}", implementation_path.display()),
        })?;
        let implementation = if present {
            Some(Arc::new(parser::parse(read(&implementation_path)?)?))
        } else {
            None
        };
        (interface, implementation, None)
    };
    if let Some(module) = &implementation
        && module.name() != interface.name()
    {
        return module
            .source
            .fail(Span::default(), "B022", ".h 与 .b 的模块名不一致");
    }
    let name = interface.name().to_owned();
    let source = implementation
        .as_ref()
        .map_or(&interface.source, |module| &module.source);
    if world.sources.contains_key(&name) {
        return Ok(name);
    }
    world.sources.insert(name.clone(), source.clone());
    world
        .interfaces
        .insert(name.clone(), Arc::clone(&interface));
    let imports = match captured_imports {
        Some(imports) => imports,
        None => crate::names::module_imports(&interface, implementation.as_deref()),
    };
    for import in imports.iter() {
        let import_path: PathBuf = root.join(import.replace('.', "/")).with_extension("b");
        let actual = load(
            &import_path,
            root,
            world,
            imported,
            Some(import),
            inputs,
        )?;
        if actual != *import {
            return source.fail(
                Span::default(),
                "B024",
                format!("导入 {import}，文件却声明 {actual}"),
            );
        }
    }
    for (record, source) in interface.records().into_iter()
        .map(|r| (r, &interface.source))
        .chain(implementation.iter().flat_map(|module| {
            module.records().into_iter()
                .map(move |record| (record, &module.source))
        }))
    {
        if matches!(
            record.name(),
            "acceleration"
                | "dataset"
                | "view"
                | "tile"
                | "array"
                | "result"
                | "texture2d" | "texture3d"
                | "void"
                | "never"
        ) || ir::Ty::builtin(&record.name()).is_some()
        {
            return source.fail(record.span(), "B184", "记录名称不能覆盖内置类型");
        }
        let key = format!("{name}::{}", record.name());
        if world.records.contains_key(&key) {
            return source.fail(record.span(), "B025", format!("重复类型 {key}"));
        }
        world.records.insert(
            key,
            (
                record.clone(),
                source.clone(),
                name.clone(),
                imports.clone(),
            ),
        );
    }
    for (set, source) in interface.type_sets().into_iter()
        .map(|s| (s, &interface.source))
        .chain(implementation.iter().flat_map(|module| {
            module.type_sets().into_iter()
                .map(move |set| (set, &module.source))
        }))
    {
        let key = format!("{name}::{}", set.name());
        if world.records.contains_key(&key)
            || world.type_sets.contains_key(&key)
            || ir::Ty::builtin(&set.name()).is_some()
            || matches!(
                set.name(),
                "void"
                    | "never"
                    | "dataset"
                    | "view"
                    | "tile"
                    | "array"
                    | "result"
                    | "acceleration"
                    | "texture2d" | "texture3d"
            )
            || matches!(
                set.name(),
                "numeric" | "floating" | "integer" | "scalar_numeric" | "scalar_integer"
            )
        {
            return source.fail(
                set.span(),
                "B233",
                format!("类型集合名称重复或覆盖已有类型 {key}"),
            );
        }
        world.type_sets.insert(
            key,
            (set.clone(), source.clone(), name.clone(), imports.clone()),
        );
    }
    let declarations = interface.functions();
    let implementations = implementation.as_ref().map(|module| module.functions());
    let pairs = crate::names::function_pairs(&declarations, implementations.as_ref());
    for (index, f) in declarations.iter().enumerate()
        .filter(|(_, f)| f.kind() != FunctionKind::Constant && f.external().is_none())
    {
        if f.body().is_some() {
            return interface
                .source
                .fail(f.span(), "B026", ".h 只声明接口，函数体放在 .b");
        }
        if compiled.is_some() {
            continue;
        }
        let body = pairs.as_ref().and_then(|pairs| implementations.as_ref().unwrap().get(pairs[index] as usize))
            .ok_or_else(|| Diagnostic {
                source: interface.source.clone(),
                span: f.span(),
                code: "B027",
                message: format!("接口 {} 缺少 .b 实现", f.name()),
            })?;
        if !f.same_signature(&body) {
            return implementation.as_ref().unwrap().source.fail(
                body.span(),
                "B028",
                format!(
                    "{} 的函数效果、类型参数、访问权限、类型、参数名、执行目标或前缀域约束与 .h 不一致",
                    f.name()
                ),
            );
        }
    }
    let declaration_count = declarations.len();
    let pairs = pairs.as_deref();
    for (f, source, public, interface_declaration) in declarations.iter()
        .filter(|f| f.kind() == FunctionKind::Constant || f.external().is_some() || compiled.is_some())
        .map(|f| (f, &interface.source, true, None))
        .chain(implementation.iter().zip(implementations).flat_map(|(module, functions)| {
            let declarations = &declarations;
            functions.into_iter().enumerate().map(move |(index, function)| {
                let declaration = pairs.and_then(|pairs| declarations.get(pairs[declaration_count + index] as usize));
                (function, &module.source, declaration.is_some(), declaration)
            })
        }))
    {
        if f.body().is_none() && f.external().is_none() && compiled.is_none() {
            return source.fail(f.span(), "B029", ".b 中的函数必须有实现");
        }
        let key = format!("{name}::{}", f.name());
        if world.functions.contains_key(&key) {
            return source.fail(f.span(), "B030", format!("值名称重复 {key}"));
        }
        world.functions.insert(
            key,
            Arc::new(Definition {
                constant: OnceLock::new(),
                function: f.clone(),
                interface_declaration,
                syntax_origin: if compiled.is_some() { DefinitionSyntax::CompiledInterface } else { DefinitionSyntax::Source },
                execution: if let Some(compiled) = compiled {
                    compiled
                        .requirements
                        .get(&format!("{name}::{}", f.name()))
                        .copied()
                        .ok_or_else(|| Diagnostic {
                            source: source.clone(),
                            span: f.span(),
                            code: "B270",
                            message: format!("已编译模块没有接口 {} 的参与要求", f.name()),
                        })?
                } else {
                    execution::Requirement::Invocation
                },
                source: source.clone(),
                module: name.clone(),
                imports: imports.clone(),
                public,
                compiled: if f.kind() != FunctionKind::Constant && f.external().is_none() {
                    compiled.map(|module| module.definitions.clone())
                } else {
                    None
                },
            }),
        );
    }
    Ok(name)
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
        if name.name == "cfn" && !name.dataset && !name.view && !name.tile {
            return Ok(self.c_function_type(name, source, span, module, imports)?.syntax());
        }
        let mut syntax = name.clone();
        syntax.canonical = false;
        if syntax.is_texture() || syntax.is_acceleration() {
            return Ok(syntax);
        }
        let (types, count, code) = match name.name.as_str() {
            "array" => (1, 2, "B238"),
            "result" => (2, 2, "B269"),
            "owned" => (1, 2, "B224"),
            "task" | "task_start_error" => (1, 1, "B224"),
            "void" | "never" => (0, 0, "B228"),
            _ => {
                if let Some(builtin) = ir::Ty::builtin(&name.name) {
                    syntax.name = builtin.name;
                    (0, 0, "B232")
                } else {
                    let key = if name.canonical {
                        self.records.get_key_value(&name.name).map(|(key, _)| key.clone())
                    } else { self.resolve(&name.name, module, imports, false).unique() };
                    let Some(key) = key else { return source.fail(span, "B031", format!("未知或不明确的类型 {}", name.name)); };
                    let record = &self.record(&key).0;
                    syntax.name = key;
                    syntax.canonical = true;
                    (record.types().len(), record.types().len() + record.values().len(), "B232")
                }
            }
        };
        if name.arguments.len() != count {
            return source.fail(span, code, format!("类型 {} 需要 {count} 个类型及const实参，实际为 {}", syntax.name, name.arguments.len()));
        }
        syntax.arguments.clear();
        for (index, argument) in name.arguments.iter().enumerate() {
            let argument = if name.name == "owned" && index == 1 {
                let GenericArgument::Type(function) = argument else {
                    return source.fail(span, "B224", "owned<T,F>需要载荷类型与静态释放函数");
                };
                if function.dataset || function.tile || function.view || !function.arguments.is_empty() {
                    return source.fail(span, "B224", "释放函数由载荷推导泛型，声明处使用函数名称");
                }
                let key = if function.canonical && self.functions.contains_key(&function.name) { Some(function.name.clone()) }
                    else { self.resolve(&function.name, module, imports, true).unique() };
                let Some(key) = key else { return source.fail(span, "B158", format!("未知或未导出的释放函数 {}", function.name)); };
                let mut function = TypeName::named(key);
                function.canonical = true;
                GenericArgument::Type(function)
            } else if index < types {
                let GenericArgument::Type(ty) = argument else { return source.fail(span, code, "此位置需要类型实参"); };
                GenericArgument::Type(self.type_reference(ty, source, span, module, imports)?)
            } else {
                let value = argument.shape().ok_or_else(|| crate::Diagnostic {
                    source: source.clone(), span, code: "B235", message: "const实参需要u32编译期表达式".into(),
                })?;
                let value = self.shape(&Some(value), source, span, module, imports)?.unwrap();
                GenericArgument::Value(Shape::Literal(value))
            };
            syntax.arguments.push(argument);
        }
        if let Some(count) = &name.count {
            syntax.count = Some(Shape::Literal(self.shape(&Some(count.clone()), source, span, module, imports)?.unwrap()));
        }
        Ok(syntax)
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
        if name.contracts.is_some() && name.name != "cfn" {
            return source.fail(span, "B225", "输入／输出合同属于cfn函数签名");
        }
        if name.name == "cfn" && !name.dataset && !name.view && !name.tile {
            return self.c_function_type(name, source, span, module, imports);
        }
        if matches!(name.name.as_str(), "texture2d" | "texture3d") {
            return source.fail(
                span,
                "B239",
                "texture2d／texture3d 只能作为 read 资源参数，不能作为普通值存储或返回",
            );
        }
        if name.name == "void" {
            return source.fail(span, "B228", "void 只能表示函数无返回值，不能作为数据类型");
        }
        if name.name == "never" {
            return source.fail(
                span,
                "B228",
                "never 表示函数没有正常返回路径，不提供可存储的数据值",
            );
        }
        if name.tile {
            return source.fail(
                span,
                "B207",
                "tile<T> 只能作为 cooperative 函数的只读借用参数，不能存储或返回",
            );
        }
        if name.view {
            let mut element = name.clone();
            element.view = false;
            let element = self.ty_inner(&element, source, span, module, imports, stack)?;
            return Ok(ir::Ty {
                name: format!("view<{}>", element.name),
                arguments: Vec::new(),
                values: Vec::new(),
                size: 24,
                align: 8,
                components: vec![
                    (ir::Scalar::U32, 0),
                    (ir::Scalar::U32, 4),
                    (ir::Scalar::U32, 8),
                    (ir::Scalar::U32, 12),
                    (ir::Scalar::U32, 16),
                ],
                fields: Vec::new(),
                view: Some(Box::new(element)),
                array: None,
                enumeration: false,
                result: false,
                variants: Vec::new(),
                dataset: None, cleanup: None, function: None,
            });
        }
        if matches!(name.name.as_str(),"task"|"task_start_error") {
            let [GenericArgument::Type(payload)] = name.arguments.as_slice() else {
                return source.fail(span,"B224","任务类型需要一个类型实参");
            };
            let mut ty=self.ty_inner(payload,source,span,module,imports,stack)?;
            if payload.dataset {
                ty=ir::Ty::owned_dataset(ty.syntax(),self.shape(&payload.count,source,span,module,imports)?);
            }
            return if name.name=="task" {
                Ok(ir::Ty::task(ty))
            } else {
                ir::Ty::task_start_error(ty).ok_or_else(||Diagnostic{
                    source:source.clone(),span,code:"B224",message:"任务错误载荷布局过大".into(),
                })
            };
        }
        if name.name == "owned" {
            if name.arguments.len()!=2{return source.fail(span,"B224","owned<T,F>需要载荷类型与静态释放函数");}
            let (GenericArgument::Type(payload),GenericArgument::Type(function))=(&name.arguments[0],&name.arguments[1])else{return source.fail(span,"B224","owned<T,F>需要载荷类型与静态释放函数");};
            if function.dataset||function.tile||function.view||!function.arguments.is_empty(){return source.fail(span,"B224","释放函数由载荷推导泛型，声明处使用函数名称");}
            let cleanup=if function.canonical&&self.functions.contains_key(&function.name){function.name.clone()}else{self.resolve(&function.name,module,imports,true).unique().ok_or_else(||Diagnostic{source:source.clone(),span,code:"B158",message:format!("未知或未导出的释放函数 {}",function.name)})?};
            let f=&self.functions[&cleanup].function;
            if f.kind()!=FunctionKind::Value||f.target()!=Target::Cpu||f.params().len()!=1||f.params().at(0).access()!=Access::Value{return source.fail(span,"B224","释放函数必须是接收一个按值载荷并返回void的普通on cpu函数");}
            let mut ty=self.ty_inner(payload,source,span,module,imports,stack)?;
            if payload.dataset {if ty.contains_views(){return source.fail(span,"B033","dataset元素不能保存view借用");}ty=ir::Ty::owned_dataset((ty).syntax(),self.shape(&payload.count,source,span,module,imports)?);}
            let owner=ir::Ty::owned(ty,cleanup.clone());let identity=format!("owned contract:{}",owner.name);
            if !stack.insert(identity.clone()){return source.fail(span,"B224","释放函数的载荷合同不能递归要求同一拥有包装");}
            let definition=&self.functions[&cleanup];
            let instance=check::generics::specialize(self,&cleanup,&[check::generics::Actual::value(&owner.arguments[0])],None,(&definition.source,&definition.module,&definition.imports),definition.function.span()).map_err(|error|Diagnostic{source:source.clone(),span,code:error.code,message:format!("释放函数 {}：{}",cleanup,error.message)})?;
            let result_name=instance.bindings.type_name(self,&instance.definition.function.result(),
                (&instance.definition.source,&instance.definition.module,&instance.definition.imports),instance.definition.function.span())?;
            if result_name!=TypeName::named("void"){return source.fail(span,"B224","释放函数必须返回void");}
            let parameter=&instance.definition.function.params().at(0);
            let parameter_name=instance.bindings.type_name(self,&parameter.ty(),
                (&instance.definition.source,&instance.definition.module,&instance.definition.imports),parameter.span())?;
            let mut expected=self.ty_inner(&parameter_name,&instance.definition.source,parameter.span(),&instance.definition.module,&instance.definition.imports,stack)?;
            if parameter_name.dataset{expected=ir::Ty::owned_dataset((expected).syntax(),self.shape(&parameter_name.count,&instance.definition.source,parameter.span(),&instance.definition.module,&instance.definition.imports)?);}
            stack.remove(&identity);
            let payload=&owner.arguments[0];
            let compatible=if let(Some(expected),Some(actual))=(&expected.dataset,&payload.dataset){expected==actual}else{expected==*payload};
            if !compatible{return source.fail(span,"B124",format!("释放函数 {} 需要 {}，实际拥有载荷为 {}",cleanup,expected.name,payload.name));}
            return Ok(owner);
        }
        if name.name == "result" {
            if name.arguments.len() != 2 {
                return source.fail(span, "B269", "result<T,E> 需要成功及错误两个类型");
            }
            let mut payloads = Vec::new();
            for argument in &name.arguments {
                let GenericArgument::Type(payload) = argument else {
                    return source.fail(span, "B269", "结果实参需要类型");
                };
                if payload.tile {
                    return source.fail(span, "B269", "tile 是执行域借用，不能作为拥有结果载荷");
                }
                let mut ty = if *payload == TypeName::named("void") {
                    ir::Ty::unit()
                } else {
                    self.ty_inner(payload, source, span, module, imports, stack)?
                };
                if payload.dataset {
                    if ty.contains_views() {
                        return source.fail(span, "B033", "dataset 元素不能保存view借用");
                    }
                    let count = self.shape(&payload.count, source, span, module, imports)?;
                    ty = ir::Ty::owned_dataset((ty).syntax(), count);
                }
                payloads.push(ty);
            }
            let error = payloads.pop().unwrap();
            let ok = payloads.pop().unwrap();
            return ir::Ty::result(ok, error).ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B269",
                message: "结果布局超过 u32 字节地址范围".into(),
            });
        }
        if name.name == "array" {
            if name.arguments.len() != 2 {
                return source.fail(span, "B238", "array<T,N> 需要元素类型和固定长度");
            }
            let GenericArgument::Type(element) = &name.arguments[0] else {
                return source.fail(span, "B238", "array 首参数需要值类型");
            };
            if element.tile {
                return source.fail(span, "B238", "tile 是执行域借用，不能作为拥有数组元素");
            }
            let mut element_ty = self.ty_inner(element, source, span, module, imports, stack)?;
            if element.dataset {
                if element_ty.contains_views() {
                    return source.fail(span, "B033", "dataset 元素不能保存view借用");
                }
                let count = self.shape(&element.count, source, span, module, imports)?;
                element_ty = ir::Ty::owned_dataset((element_ty).syntax(), count);
            }
            let element = element_ty;
            let length = name.arguments[1].shape().ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B238",
                message: "array 长度需要 u32 编译期表达式".into(),
            })?;
            let count = self
                .shape(&Some(length), source, span, module, imports)?
                .unwrap();
            // Bound element types carry nonzero alignment.
            let layout = unsafe { fixed_array_layout(element.size, element.align, count) };
            let size = u32::try_from(layout.size).map_err(|_| Diagnostic {
                source: source.clone(),
                span,
                code: "B238",
                message: "array 布局超过 u32 字节地址范围".into(),
            })?;
            let components = if element.components.is_empty() {
                Vec::new()
            } else {
                // The checked extent bounds the stride for every present element.
                let stride = layout.stride as u32;
                (0..count).flat_map(|index| {
                    element.components.iter().map(move |&(scalar, offset)| {
                        (scalar, index * stride + offset)
                    })
                }).collect()
            };
            return Ok(ir::Ty {
                name: format!("array<{},{}>", element.name, count),
                arguments: Vec::new(),
                values: vec![count],
                size,
                align: element.align,
                components,
                fields: Vec::new(),
                view: None,
                array: Some(Box::new(element)),
                enumeration: false,
                result: false,
                variants: Vec::new(),
                dataset: None, cleanup: None, function: None,
            });
        }
        if let Some(ty) = ir::Ty::builtin(&name.name) {
            if !name.arguments.is_empty() {
                return source.fail(span, "B232", "内置数值类型不接受记录类型实参");
            }
            return Ok(ty);
        }
        let key = if name.canonical {
            self.records
                .get_key_value(&name.name)
                .map(|(key, _)| key.clone())
        } else {
            self.resolve(&name.name, module, imports, false).unique()
        };
        let Some(key) = key else {
            return source.fail(span, "B031", format!("未知或不明确的类型 {}", name.name));
        };
        let (record, s, m, i) = self.record(&key);
        if record.types().len() + record.values().len() != name.arguments.len() {
            return source.fail(
                span,
                "B232",
                format!(
                    "类型 {key} 需要 {} 个类型及 const 实参，实际为 {}",
                    record.types().len() + record.values().len(),
                    name.arguments.len()
                ),
            );
        }
        let mut arguments = Vec::new();
        let mut replacements = BTreeMap::new();
        for (parameter, arg) in record.types().iter().zip(&name.arguments) {
            let GenericArgument::Type(arg) = arg else {
                return source.fail(span, "B232", "此记录位置需要普通值类型");
            };
            if arg.tile { return source.fail(span, "B232", "tile借用不能作为拥有类型实参"); }
            let mut actual = if *arg == TypeName::named("void") {
                ir::Ty::unit()
            } else {
                self.ty_inner(arg, source, span, module, imports, stack)?
            };
            if arg.dataset {
                if actual.contains_views() { return source.fail(span, "B033", "dataset元素不能保存view借用"); }
                actual = ir::Ty::owned_dataset((actual).syntax(), self.shape(&arg.count, source, span, module, imports)?);
            }
            if !self.family_accepts(&parameter.family(), &actual, s, record.span(), m, i, stack)? {
                return source.fail(
                    span,
                    "B232",
                    format!("类型 {} 不属于 {} 的允许集合", actual.name, parameter.name()),
                );
            }
            replacements.insert(parameter.name().to_owned(), actual.syntax());
            arguments.push(actual);
        }
        let mut values = Vec::new();
        let mut value_map = BTreeMap::new();
        for (parameter, arg) in record
            .values()
            .iter()
            .zip(&name.arguments[record.types().len()..])
        {
            let shape = arg.shape().ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B235",
                message: "记录 const 实参需要 u32 编译期表达式".into(),
            })?;
            let value = self
                .shape(&Some(shape), source, span, module, imports)?
                .unwrap();
            if !parameter.accepts_const(value) {
                return source.fail(
                    span,
                    "B235",
                    format!(
                        "记录 const 实参 {value} 不属于 {} 的允许值域",
                        parameter.name()
                    ),
                );
            }
            values.push(value);
            value_map.insert(parameter.name().to_owned(), value);
        }
        let bindings = Arc::new(generics::Bindings { types: replacements, values: value_map });
        let key = if arguments.is_empty() && values.is_empty() {
            key
        } else {
            generics::key(
                &key,
                arguments.iter().map(|ty| ty.name.as_str()),
                &values,
            )
        };
        if !stack.insert(key.clone()) {
            return source.fail(span, "B032", "结构体不能递归包含自身");
        }
        if let Some(representation) = &record.enumeration() {
            let representation=bindings.type_name(self,representation,(s,m,i),record.span())?;
            let mut ty = self.ty_inner(&representation, s, record.span(), m, i, stack)?;
            if !ty.is_numeric() || ty.components.len() != 1 || !ty.components[0].0.is_integer() {
                return s.fail(record.span(), "B268", "编号枚举的表示类型需要整数标量");
            }
            if !record.variants().is_empty() {
                if ty != ir::Ty::scalar(ir::Scalar::U32) && ty != ir::Ty::scalar(ir::Scalar::I32) {
                    return s.fail(record.span(), "B268", "带载荷变体的表示类型需要 u32 或 i32");
                }
                let representation = ty;
                let mut alternatives = Vec::new();
                let mut names = BTreeSet::new(); let mut tags = BTreeSet::new();
                for variant in &record.variants() {
                    if !names.insert(variant.name().to_owned()) { return s.fail(variant.span(), "B268", "同一变体名称不能重复"); }
                    let expression = variant.discriminant();
                    let tag = check::variant_discriminant(self, &format!("{key}::{}",variant.name()), s, m, i, &representation, &expression, &bindings)?;
                    if !tags.insert(tag) { return s.fail(variant.span(), "B268", "带载荷变体的编号必须唯一，才能确定活动载荷"); }
                    let syntax = bindings.type_name(self,&variant.payload(),(s,m,i),variant.span())?;
                    if syntax.tile { return s.fail(variant.span(), "B269", "tile是执行域借用，不能作为变体载荷"); }
                    let mut payload = if syntax == TypeName::named("void") { ir::Ty::unit() }
                        else { self.ty_inner(&syntax, s, variant.span(), m, i, stack)? };
                    if syntax.dataset {
                        if payload.contains_views() { return s.fail(variant.span(), "B033", "dataset元素不能保存view借用"); }
                        payload = ir::Ty::owned_dataset((payload).syntax(), self.shape(&syntax.count, s, variant.span(), m, i)?);
                    }
                    alternatives.push(ir::Variant { name: variant.name().to_owned(), discriminant: tag, ty: Box::new(payload) });
                }
                let ty = ir::Ty::variant(key.clone(), arguments, values, representation.components[0].0, alternatives)
                    .ok_or_else(|| Diagnostic { source: s.clone(), span: record.span(), code: "B238", message: "变体布局超过u32字节地址范围".into() })?;
                stack.remove(&key); return Ok(ty);
            }
            ty.name = key.clone();
            ty.enumeration = true;
            stack.remove(&key);
            return Ok(ty);
        }
        let mut ty = ir::Ty {
            name: key.clone(),
            arguments,
            values,
            size: 0,
            align: 1,
            components: Vec::new(),
            fields: Vec::new(),
            view: None,
            array: None,
            enumeration: false,
            result: false,
            variants: Vec::new(),
            dataset: None, cleanup: None, function: None,
        };
        let mut names = BTreeSet::new();
        for field in &record.fields() {
            if !names.insert(field.name().to_owned()) {
                return s.fail(field.span(), "B034", format!("重复字段 {}", field.name()));
            }
            let field_type = bindings.type_name(self,&field.ty(),(s,m,i),field.span())?;
            let f = if field_type.dataset {
                let mut element=field_type.clone();element.dataset=false;element.count=None;
                let reference=self.type_reference(&element,s,field.span(),m,i)?;
                ir::Ty::owned_dataset(reference,self.shape(&field_type.count,s,field.span(),m,i)?)
            } else { self.ty_inner(&field_type,s,field.span(),m,i,stack)? };
            // Bound field types carry nonzero alignment.
            let placement = unsafe { place_field(ty.size, ty.align, f.size, f.align) };
            let Ok(size) = u32::try_from(placement.end) else {
                return s.fail(field.span(), "B238", "记录布局超过 u32 字节地址范围");
            };
            let offset = placement.offset as u32;
            ty.components
                .extend(f.components.iter().map(|(s, o)| (*s, o + offset)));
            ty.size = size;
            ty.align = placement.alignment;
            ty.fields.push((field.name().to_owned(), offset, Box::new(f)));
        }
        let size = unsafe { natural_record_size(ty.size, ty.align) };
        ty.size = u32::try_from(size).map_err(|_| Diagnostic {
            source: source.clone(),
            span,
            code: "B238",
            message: "记录对齐后超过 u32 字节地址范围".into(),
        })?;
        if let Some(shape) = &record.stride() {
            let stride=self.source_shape(shape,&bindings,s,record.span(),m,i)?;
            if !unsafe { accepts_record_stride(ty.size, ty.align, stride) } {
                return s.fail(
                    record.span(),
                    "B241",
                    format!(
                        "记录 stride {stride} 必须容纳自然布局 {} 且是对齐 {} 的倍数",
                        ty.size, ty.align
                    ),
                );
            }
            ty.size = stride;
        }
        stack.remove(&key);
        Ok(ty)
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

