use crate::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub(crate) enum DefinitionSyntax { Source, CompiledInterface, Generated(Option<Arc<ir::SourceSite>>) }

#[derive(Clone)]
pub(crate) struct Definition {
    pub(crate) function: Function,
    pub(crate) syntax_origin: DefinitionSyntax,
    pub(crate) execution: execution::Requirement,
    pub(crate) source: Source,
    pub(crate) module: String,
    pub(crate) imports: Vec<String>,
    pub(crate) public: bool,
    pub(crate) compiled: Option<Arc<BTreeMap<String, Vec<u8>>>>,
}
impl Definition {
    pub(crate) fn source_origin(&self) -> Option<ir::SourceOrigin> {
        match (&self.syntax_origin, &self.function.body) {
            (DefinitionSyntax::Source, Some(_)) => Some(ir::SourceOrigin {
                source: self.source.clone(), span: self.function.span,
                module: self.module.clone(), definition: self.function.name.clone(),
            }),
            _ => None,
        }
    }
    pub(crate) fn declaration_origin(&self) -> Option<ir::SourceDeclaration> {
        if self.function.body.is_some() { return None; }
        let representation = match &self.syntax_origin {
            DefinitionSyntax::Source => ir::DeclarationRepresentation::Source,
            DefinitionSyntax::CompiledInterface => ir::DeclarationRepresentation::CompiledInterface,
            DefinitionSyntax::Generated(_) => return None,
        };
        Some(ir::SourceDeclaration { source: self.source.clone(), span: self.function.span,
            module: self.module.clone(), declaration: self.function.name.clone(), representation })
    }
    pub(crate) fn syntax_context(&self, instance: Option<&str>) -> (Option<Arc<ir::SourceOrigin>>, Arc<ir::SourceContext>) {
        match &self.syntax_origin {
            DefinitionSyntax::Generated(Some(site)) => (Some(site.origin.clone()), site.context.clone()),
            DefinitionSyntax::Source | DefinitionSyntax::CompiledInterface | DefinitionSyntax::Generated(None) => (
                self.source_origin().map(Arc::new),
                Arc::new(ir::SourceContext { instance: instance.map(Arc::from), inlined_at: Box::default(), domain_path: Arc::from([]) }),
            ),
        }
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
    pub(crate) interfaces: BTreeMap<String, Arc<ast::Module>>,
    pub(crate) records: BTreeMap<String, (Record, Source, String, Vec<String>)>,
    pub(crate) functions: BTreeMap<String, Arc<Definition>>,
    pub(crate) type_sets: BTreeMap<String, (TypeSet, Source, String, Vec<String>)>,
}
pub(crate) struct World {
    pub(crate) declarations: Arc<Declarations>,
    pub(crate) worker: compiler::Worker,
    pub(crate) evaluating_constants: RefCell<BTreeSet<String>>,
    pub(crate) build_declarations: Option<RefCell<BTreeSet<build::Declaration>>>,
}
impl std::ops::Deref for World {
    type Target = Declarations;
    fn deref(&self) -> &Declarations { &self.declarations }
}
impl World {
    pub(crate) fn new(declarations: Arc<Declarations>, worker: compiler::Worker) -> Self {
        Self { declarations, worker, evaluating_constants: RefCell::new(BTreeSet::new()), build_declarations: None }
    }
    pub(crate) fn resolve(&self, name: &str, module: &str, imports: &[String], functions: bool) -> Option<String> {
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
    pub(crate) fn record(&self, key: &str) -> &(Record, Source, String, Vec<String>) {
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
pub(crate) fn parse_input(path: &Path, inputs: Option<&build::Inputs>) -> Result<Arc<ast::Module>> {
    if let Some(inputs) = inputs { inputs.read(path) }
    else { parser::parse(read(path)?).map(Arc::new) }
}
pub(crate) fn contract_signature(f: &Function) -> BTreeSet<(ContractKind, &ContractPredicate)> {
    f.contracts.iter().map(|c| (c.kind, &c.predicate)).collect()
}
pub(crate) fn same_signature(a: &Function, b: &Function) -> bool {
    a.kind == b.kind
        && a.c_abi == b.c_abi
        && a.external == b.external
        && a.types == b.types
        && a.values == b.values
        && a.target == b.target
        && contract_signature(a) == contract_signature(b)
        && a.params.len() == b.params.len()
        && a.params
            .iter()
            .zip(&b.params)
            .all(|(a, b)| a.name == b.name && a.access == b.access)
}
pub fn compile(path: &Path, module_root: &Path) -> Result<Compilation> {
    compile_module(path, module_root, true, None, None)
}
/// 选择公开 program 的具体静态入口，例如 shade<13> 或 run<float3,17>。
pub fn compile_entry(path: &Path, module_root: &Path, entry: &str) -> Result<Compilation> {
    compile_module(
        path,
        module_root,
        true,
        Some((EntryKind::Domain, entry)),
        None,
    )
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
        None,
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
    compile_module(path, module_root, false, None, None)
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
    imported: Option<&module::Imports>,
) -> Result<Compilation> {
    let compiler = Compiler::available().map_err(|error| compiler::diagnostic(path, error))?;
    compile_module_on(&compiler, path, module_root, require_program, requested, imported)
}
pub(crate) fn compile_module_on(
    compiler: &Compiler, path: &Path, module_root: &Path, require_program: bool,
    requested: Option<(EntryKind, &str)>, imported: Option<&module::Imports>,
) -> Result<Compilation> {
    compiler.ast(path, |worker| {
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
            let imports = &world.interfaces[&module].imports;
            let value = world.ty(&name, &source, span, &module, imports)?;
            if name.dataset {
                let count = world.shape(&name.count, &source, span, &module, imports)?;
                Ok(ir::Ty::owned_dataset(value, count))
            } else { Ok(value) }
        })
    }
    pub fn compile(&self, path: &Path, root: &Path) -> Result<Compilation> {
        compile_module_on(self, path, root, true, None, None)
    }
    pub fn check(&self, path: &Path, root: &Path) -> Result<Compilation> {
        compile_module_on(self, path, root, false, None, None)
    }
    pub fn compile_entry(&self, path: &Path, root: &Path, entry: &str) -> Result<Compilation> {
        compile_module_on(self, path, root, true, Some((EntryKind::Domain, entry)), None)
    }
    pub fn compile_application(&self, path: &Path, root: &Path, entry: &str) -> Result<application::Application> {
        Ok(compile_module_on(self, path, root, false, Some((EntryKind::Application, entry)), None)?.applications.remove(0))
    }
    pub fn compile_application_with_modules(
        &self, path: &Path, root: &Path, entry: &str, dependencies: &[object::Object],
    ) -> Result<application::Application> {
        self.ast(path, |worker| {
            let imports = module::Imports::new(dependencies).map_err(|error| Diagnostic {
                source: Source { path: path.into(), text: "".into() },
                span: Span::default(), code: "B270", message: error.to_string(),
            })?;
            let (world, module) = read_world(path, root, Some(&imports), &[], worker, None, None)?;
            Ok(compile_world(&world, module, false, Some((EntryKind::Application, entry)))?
                .applications.remove(0))
        })
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
            .resolve(&name, &entry, &[], true)
            .ok_or_else(|| Diagnostic {
                source: source.clone(),
                span: Span::default(),
                code: "B240",
                message: format!("没有公开入口 {name}"),
            })?;
        let definition = &world.functions[&key];
        if !definition.public
            || definition.module != entry
            || definition.function.kind != FunctionKind::Value
        {
            return source.fail(
                Span::default(),
                "B240",
                "入口选择需要本模块对应种类的公开函数",
            );
        }
        Some((key, arguments, kind))
    } else {
        None
    };
    enum Job<'a> {
        Instance(generics::Instance, bool),
        Failed(Diagnostic),
        Selected(&'a str, &'a Option<Vec<GenericArgument>>, EntryKind),
    }
    enum Output {
        Programs(Vec<ir::Program>),
        Application(application::Application),
    }
    let for_application = matches!(selected.as_ref(), Some((_, _, EntryKind::Application)));
    let mut jobs = Vec::new();
    let planned = (|| -> Result<()> {
        for (base, definition) in &world.functions {
            if definition.compiled.is_some() || definition.function.kind == FunctionKind::Constant
                || definition.function.external.is_some() { continue; }
            generics::validate_declaration(definition)?;
            if selected.as_ref().is_some_and(|(key, _, _)| key == base) {
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
    } else if let Some((key, arguments, kind)) = &selected {
        jobs.push(Job::Selected(key, arguments, *kind));
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
                    && (d.function.target != Target::Cpu || check::kernel_interface(world, d)?)
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
            Job::Selected(key, arguments, kind) => {
                let definition = &world.functions[key];
                let instance = check::generics::specialize(world, key, &[], arguments.as_deref(), (&definition.source, &definition.module, &definition.imports), definition.function.span)?;
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
                if program.prefix_requirements.is_empty() { programs.push(ExecutableProgram(program)); }
                else { templates.push(TemplateProgram(program)); }
            },
        }
    }
    let open_generics: Vec<_> = world
        .records
        .iter()
        .filter(|(_, (record, ..))| generics::is_open(&record.types, &record.values))
        .map(|(name, _)| name.clone())
        .chain(
            world
                .functions
                .iter()
                .filter(|(_, d)| generics::is_open(&d.function.types, &d.function.values))
                .map(|(name, _)| name.clone()),
        )
        .collect();
    if require_program && programs.is_empty() && templates.is_empty() {
        if world.functions.values().any(|d| {
            d.module == entry
                && d.public
                && d.execution == execution::Requirement::Domain
                && generics::is_open(&d.function.types, &d.function.values)
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
            let mut arguments = arguments.unwrap_or_default();
            while let Some(argument) = arguments.pop() {
                if let GenericArgument::Type(ty) = argument {
                    if let Some((owner, _)) = ty.name.split_once("::")
                        && imported.modules.contains_key(owner)
                    {
                        load(&module_root.join(owner.replace('.', "/")).with_extension("b"), module_root,
                            &mut world, Some(imported), Some(owner), inputs)?;
                    }
                    arguments.extend(ty.arguments);
                }
            }
        }
    }
    execution::infer(&mut world, build_declarations.as_ref());
    for definition in world.functions.values().filter(|definition| definition.compiled.is_some()) {
        generics::validate_declaration(definition)?;
    }
    let mut world = World::new(Arc::new(world), worker.clone());
    for (key, definition) in &world.functions {
        if definition.function.kind == FunctionKind::Constant {
            generics::validate_declaration(definition)?;
            check::constant_value(&world, key)?;
        }
    }
    for definition in world.functions.values().filter(|definition| {
        definition.function.types.is_empty() && definition.function.values.is_empty()
    }) {
        world.check_interface_types(definition, &definition.function, &BTreeMap::new(), &BTreeMap::new())?;
    }
    validate_types(&world)?;
    for definition in world.functions.values().filter(|definition| definition.function.c_abi) {
        check::external::check_declaration(&world, definition)?;
    }
    world.build_declarations = build_declarations;
    Ok((world, entry))
}
pub(crate) fn validate_types(world: &World) -> Result<()> {
    for name in world.type_sets.keys() {
        world.set_members(name, &mut BTreeSet::new())?;
    }
    for (name, (record, source, module, imports)) in &world.records {
        generics::validate_parameters(&record.types, &record.name, source, record.span)?;
        value_parameters::validate(&record.values, &record.name, &[], source, record.span)?;
        let values = value_parameters::combinations(&record.values);
        for arguments in
            generics::combinations(&world, &record.types, source, record.span, module, imports)?
        {
            for values in &values {
                let mut ty = TypeName::named(name);
                ty.arguments = arguments
                    .iter()
                    .cloned()
                    .map(GenericArgument::Type)
                    .chain(
                        values
                            .iter()
                            .map(|value| GenericArgument::Value(Shape::Literal(*value))),
                    )
                    .collect();
                world.ty(&ty, source, record.span, module, imports)?;
            }
        }
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
    let interface = if let Some(compiled) = compiled {
        if let Some(inputs) = inputs { inputs.interface(import.unwrap(), &compiled.header)? }
        else { Arc::new(parser::parse(Source {
            path: PathBuf::from(format!("<{}>.h", import.unwrap())),
            text: compiled.header.clone().into(),
        })?) }
    } else {
        let mut header_path = path.to_path_buf();
        header_path.set_extension("h");
        parse_input(&header_path, inputs)?
    };
    let mut implementation_path = path.to_path_buf();
    implementation_path.set_extension("b");
    let present = if compiled.is_some() {
        false
    } else if let Some(inputs) = inputs {
        inputs.present(&implementation_path)
    } else {
        implementation_path
            .try_exists()
            .map_err(|error| Diagnostic {
                source: interface.source.clone(),
                span: Span::default(),
                code: "B020",
                message: format!("无法访问实现源 {}：{error}", implementation_path.display()),
            })?
    };
    let implementation = if present {
        Some(parse_input(&implementation_path, inputs)?)
    } else {
        None
    };
    if let Some(module) = &implementation
        && module.name != interface.name
    {
        return module
            .source
            .fail(Span::default(), "B022", ".h 与 .b 的模块名不一致");
    }
    let name = interface.name.clone();
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
    let imports: Vec<_> = interface
        .imports
        .iter()
        .chain(implementation.iter().flat_map(|module| &module.imports))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for import in &imports {
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
    for (record, source) in interface
        .records
        .iter()
        .map(|r| (r, &interface.source))
        .chain(implementation.iter().flat_map(|module| {
            module
                .records
                .iter()
                .map(move |record| (record, &module.source))
        }))
    {
        if matches!(
            record.name.as_str(),
            "acceleration"
                | "dataset"
                | "view"
                | "tile"
                | "array"
                | "result"
                | "texture2d"
                | "void"
                | "never"
        ) || ir::Ty::builtin(&record.name).is_some()
        {
            return source.fail(record.span, "B184", "记录名称不能覆盖内置类型");
        }
        let key = format!("{name}::{}", record.name);
        if world.records.contains_key(&key) {
            return source.fail(record.span, "B025", format!("重复类型 {key}"));
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
    for (set, source) in interface
        .type_sets
        .iter()
        .map(|s| (s, &interface.source))
        .chain(implementation.iter().flat_map(|module| {
            module
                .type_sets
                .iter()
                .map(move |set| (set, &module.source))
        }))
    {
        let key = format!("{name}::{}", set.name);
        if world.records.contains_key(&key)
            || world.type_sets.contains_key(&key)
            || ir::Ty::builtin(&set.name).is_some()
            || matches!(
                set.name.as_str(),
                "void"
                    | "never"
                    | "dataset"
                    | "view"
                    | "tile"
                    | "array"
                    | "result"
                    | "acceleration"
                    | "texture2d"
            )
            || matches!(
                set.name.as_str(),
                "numeric" | "floating" | "integer" | "scalar_numeric" | "scalar_integer"
            )
        {
            return source.fail(
                set.span,
                "B233",
                format!("类型集合名称重复或覆盖已有类型 {key}"),
            );
        }
        world.type_sets.insert(
            key,
            (set.clone(), source.clone(), name.clone(), imports.clone()),
        );
    }
    for f in interface
        .functions
        .iter()
        .filter(|f| f.kind != FunctionKind::Constant && f.external.is_none())
    {
        if f.body.is_some() {
            return interface
                .source
                .fail(f.span, "B026", ".h 只声明接口，函数体放在 .b");
        }
        if compiled.is_some() {
            continue;
        }
        let (body, body_source) = implementation
            .iter()
            .flat_map(|module| {
                module
                    .functions
                    .iter()
                    .map(move |function| (function, &module.source))
            })
            .find(|(function, _)| function.name == f.name)
            .ok_or_else(|| Diagnostic {
                source: interface.source.clone(),
                span: f.span,
                code: "B027",
                message: format!("接口 {} 缺少 .b 实现", f.name),
            })?;
        if !same_signature(f, body) {
            return body_source.fail(
                body.span,
                "B028",
                format!(
                    "{} 的函数效果、类型参数、访问权限、类型、参数名、执行目标或前缀域约束与 .h 不一致",
                    f.name
                ),
            );
        }
    }
    let declarations = &interface.functions;
    for (f, source, public) in interface
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Constant || f.external.is_some() || compiled.is_some())
        .map(|f| (f, &interface.source, true))
        .chain(implementation.iter().flat_map(|module| {
            module.functions.iter().map(move |function| {
                (
                    function,
                    &module.source,
                    declarations.iter().any(|decl| decl.name == function.name),
                )
            })
        }))
    {
        if f.body.is_none() && f.external.is_none() && compiled.is_none() {
            return source.fail(f.span, "B029", ".b 中的函数必须有实现");
        }
        let key = format!("{name}::{}", f.name);
        if world.functions.contains_key(&key) {
            return source.fail(f.span, "B030", format!("值名称重复 {key}"));
        }
        world.functions.insert(
            key,
            Arc::new(Definition {
                function: f.clone(),
                syntax_origin: if compiled.is_some() { DefinitionSyntax::CompiledInterface } else { DefinitionSyntax::Source },
                execution: if let Some(compiled) = compiled {
                    compiled
                        .requirements
                        .get(&format!("{name}::{}", f.name))
                        .copied()
                        .ok_or_else(|| Diagnostic {
                            source: source.clone(),
                            span: f.span,
                            code: "B270",
                            message: format!("已编译模块没有接口 {} 的参与要求", f.name),
                        })?
                } else {
                    execution::Requirement::Invocation
                },
                source: source.clone(),
                module: name.clone(),
                imports: imports.clone(),
                public,
                compiled: if f.kind != FunctionKind::Constant && f.external.is_none() {
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
    ) -> Option<String> {
        resolve_name(name, module, imports, |n| {
            if functions {
                self.functions.get(n).is_some_and(|d| {
                    d.module == module || (d.public && imports.contains(&d.module))
                })
            } else {
                self.records.get(n).is_some_and(|(_, source, owner, _)| {
                    type_visible(source, owner, module, imports)
                })
            }
        })
    }
}
impl World {
    pub(crate) fn ty(
        &self,
        name: &TypeName,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
    ) -> Result<ir::Ty> {
        let ty = self.ty_inner(name, source, span, module, imports, &mut BTreeSet::new())?;
        self.validate_c_functions(&ty, source, span, module, imports, &mut BTreeSet::new())?;
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
        if name.name == "texture2d" {
            return source.fail(
                span,
                "B239",
                "texture2d 只能作为 read 资源参数，不能作为普通值存储或返回",
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
        if name.name == "owned" {
            if name.arguments.len()!=2{return source.fail(span,"B224","owned<T,F>需要载荷类型与静态释放函数");}
            let (GenericArgument::Type(payload),GenericArgument::Type(function))=(&name.arguments[0],&name.arguments[1])else{return source.fail(span,"B224","owned<T,F>需要载荷类型与静态释放函数");};
            if function.dataset||function.tile||function.view||!function.arguments.is_empty(){return source.fail(span,"B224","释放函数由载荷推导泛型，声明处使用函数名称");}
            let cleanup=if function.canonical&&self.functions.contains_key(&function.name){function.name.clone()}else{self.resolve(&function.name,module,imports,true).ok_or_else(||Diagnostic{source:source.clone(),span,code:"B158",message:format!("未知或未导出的释放函数 {}",function.name)})?};
            let f=&self.functions[&cleanup].function;
            if f.kind!=FunctionKind::Value||f.target!=Target::Cpu||f.params.len()!=1||f.params[0].access!=Access::Value||f.result!=TypeName::named("void"){return source.fail(span,"B224","释放函数必须是接收一个按值载荷并返回void的普通on cpu函数");}
            let mut ty=self.ty_inner(payload,source,span,module,imports,stack)?;
            if payload.dataset {if ty.contains_views(){return source.fail(span,"B033","dataset元素不能保存view借用");}ty=ir::Ty::owned_dataset(ty,self.shape(&payload.count,source,span,module,imports)?);}
            let owner=ir::Ty::owned(ty,cleanup.clone());let identity=format!("owned contract:{}",owner.name);
            if !stack.insert(identity.clone()){return source.fail(span,"B224","释放函数的载荷合同不能递归要求同一拥有包装");}
            let definition=&self.functions[&cleanup];
            let instance=check::generics::specialize(self,&cleanup,&[check::generics::Actual::value(&owner.arguments[0])],None,(&definition.source,&definition.module,&definition.imports),definition.function.span).map_err(|error|Diagnostic{source:source.clone(),span,code:error.code,message:format!("释放函数 {}：{}",cleanup,error.message)})?;
            let parameter=&instance.definition.function.params[0];
            let mut expected=self.ty_inner(&parameter.ty,&instance.definition.source,parameter.span,&instance.definition.module,&instance.definition.imports,stack)?;
            if parameter.ty.dataset{expected=ir::Ty::owned_dataset(expected,self.shape(&parameter.ty.count,&instance.definition.source,parameter.span,&instance.definition.module,&instance.definition.imports)?);}
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
                    ty = ir::Ty::owned_dataset(ty, count);
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
                element_ty = ir::Ty::owned_dataset(element_ty, count);
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
            let stride = ir::align_up(element.size, element.align);
            let size = stride.checked_mul(count).ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B238",
                message: "array 布局超过 u32 字节地址范围".into(),
            })?;
            return Ok(ir::Ty {
                name: format!("array<{},{}>", element.name, count),
                arguments: Vec::new(),
                values: vec![count],
                size,
                align: element.align,
                components: (0..count)
                    .flat_map(|i| {
                        element
                            .components
                            .iter()
                            .map(move |(s, o)| (*s, i * stride + o))
                    })
                    .collect(),
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
            self.resolve(&name.name, module, imports, false)
        };
        let Some(key) = key else {
            return source.fail(span, "B031", format!("未知或不明确的类型 {}", name.name));
        };
        let (record, s, m, i) = self.record(&key);
        if record.types.len() + record.values.len() != name.arguments.len() {
            return source.fail(
                span,
                "B232",
                format!(
                    "类型 {key} 需要 {} 个类型及 const 实参，实际为 {}",
                    record.types.len() + record.values.len(),
                    name.arguments.len()
                ),
            );
        }
        let mut arguments = Vec::new();
        let mut replacements = BTreeMap::new();
        for (parameter, arg) in record.types.iter().zip(&name.arguments) {
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
                actual = ir::Ty::owned_dataset(actual, self.shape(&arg.count, source, span, module, imports)?);
            }
            if !self.family_accepts(&parameter.family, &actual, s, record.span, m, i, stack)? {
                return source.fail(
                    span,
                    "B232",
                    format!("类型 {} 不属于 {} 的允许集合", actual.name, parameter.name),
                );
            }
            replacements.insert(parameter.name.clone(), actual.syntax());
            arguments.push(actual);
        }
        let mut values = Vec::new();
        let mut value_map = BTreeMap::new();
        for (parameter, arg) in record
            .values
            .iter()
            .zip(&name.arguments[record.types.len()..])
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
            if parameter
                .choices
                .as_ref()
                .is_some_and(|choices| !choices.contains(&value))
            {
                return source.fail(
                    span,
                    "B235",
                    format!(
                        "记录 const 实参 {value} 不属于 {} 的允许值域",
                        parameter.name
                    ),
                );
            }
            values.push(value);
            value_map.insert(parameter.name.clone(), value);
        }
        let key = if arguments.is_empty() && values.is_empty() {
            key
        } else {
            generics::key(
                &key,
                &arguments
                    .iter()
                    .map(|t| t.name.clone())
                    .chain(values.iter().map(u32::to_string))
                    .collect::<Vec<_>>(),
            )
        };
        if !stack.insert(key.clone()) {
            return source.fail(span, "B032", "结构体不能递归包含自身");
        }
        if let Some(representation) = &record.enumeration {
            let mut ty = self.ty_inner(representation, s, record.span, m, i, stack)?;
            if !ty.is_numeric() || ty.components.len() != 1 || !ty.components[0].0.is_integer() {
                return s.fail(record.span, "B268", "编号枚举的表示类型需要整数标量");
            }
            if !record.variants.is_empty() {
                if ty != ir::Ty::scalar(ir::Scalar::U32) && ty != ir::Ty::scalar(ir::Scalar::I32) {
                    return s.fail(record.span, "B268", "带载荷变体的表示类型需要 u32 或 i32");
                }
                let representation = ty;
                let mut alternatives = Vec::new();
                let mut names = BTreeSet::new(); let mut tags = BTreeSet::new();
                for variant in &record.variants {
                    if !names.insert(variant.name.clone()) { return s.fail(variant.span, "B268", "同一变体名称不能重复"); }
                    let mut expression = variant.discriminant.clone();
                    generics::substitute_expr(&mut expression, &replacements, &value_map, s)?;
                    let tag = check::variant_discriminant(self, &format!("{key}::{}",variant.name), s, m, i, &representation, &expression)?;
                    if !tags.insert(tag) { return s.fail(variant.span, "B268", "带载荷变体的编号必须唯一，才能确定活动载荷"); }
                    let mut syntax = variant.payload.clone();
                    generics::substitute_type(&mut syntax, &replacements, s, variant.span)?;
                    value_parameters::substitute_type(&mut syntax, &value_map,s)?;
                    if syntax.tile { return s.fail(variant.span, "B269", "tile是执行域借用，不能作为变体载荷"); }
                    let mut payload = if syntax == TypeName::named("void") { ir::Ty::unit() }
                        else { self.ty_inner(&syntax, s, variant.span, m, i, stack)? };
                    if syntax.dataset {
                        if payload.contains_views() { return s.fail(variant.span, "B033", "dataset元素不能保存view借用"); }
                        payload = ir::Ty::owned_dataset(payload, self.shape(&syntax.count, s, variant.span, m, i)?);
                    }
                    alternatives.push(ir::Variant { name: variant.name.clone(), discriminant: tag, ty: Box::new(payload) });
                }
                let ty = ir::Ty::variant(key.clone(), arguments, values, representation.components[0].0, alternatives)
                    .ok_or_else(|| Diagnostic { source: s.clone(), span: record.span, code: "B238", message: "变体布局超过u32字节地址范围".into() })?;
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
        for field in &record.fields {
            if !names.insert(field.name.clone()) {
                return s.fail(field.span, "B034", format!("重复字段 {}", field.name));
            }
            let mut field_type = field.ty.clone();
            generics::substitute_type(&mut field_type, &replacements, s, field.span)?;
            value_parameters::substitute_type(&mut field_type, &value_map,s)?;
            let mut f = self.ty_inner(&field_type, s, field.span, m, i, stack)?;
            if field_type.dataset {
                f = ir::Ty::owned_dataset(f, self.shape(&field_type.count, s, field.span, m, i)?);
            }
            let offset = (ty.size as u64).div_ceil(f.align as u64) * f.align as u64;
            let end = offset + f.size as u64;
            if end > u32::MAX as u64 {
                return s.fail(field.span, "B238", "记录布局超过 u32 字节地址范围");
            }
            let offset = offset as u32;
            ty.components
                .extend(f.components.iter().map(|(s, o)| (*s, o + offset)));
            ty.size = end as u32;
            ty.align = ty.align.max(f.align);
            ty.fields.push((field.name.clone(), offset, Box::new(f)));
        }
        let size = (ty.size as u64).div_ceil(ty.align as u64) * ty.align as u64;
        ty.size = u32::try_from(size).map_err(|_| Diagnostic {
            source: source.clone(),
            span,
            code: "B238",
            message: "记录对齐后超过 u32 字节地址范围".into(),
        })?;
        if let Some(shape) = &record.stride {
            let mut shape=shape.clone();
            value_parameters::substitute_shape(&mut shape,&value_map,s)?;
            let stride=self.shape(&Some(shape),s,record.span,m,i)?.unwrap();
            if stride < ty.size || stride % ty.align != 0 {
                return s.fail(
                    record.span,
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

pub(crate) fn type_visible(source: &Source, owner: &str, module: &str, imports: &[String]) -> bool {
    owner == module
        || (source
            .path
            .extension()
            .is_some_and(|extension| extension == "h")
            && imports.iter().any(|import| import == owner))
}
pub(crate) fn resolve_name(
    name: &str,
    module: &str,
    imports: &[String],
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    if name.contains("::") && exists(name) {
        return Some(name.into());
    }
    let local = format!("{module}::{name}");
    if exists(&local) {
        return Some(local);
    }
    let candidates: Vec<_> = imports
        .iter()
        .map(|m| format!("{m}::{name}"))
        .filter(|n| exists(n))
        .collect();
    if candidates.len() == 1 {
        Some(candidates[0].clone())
    } else {
        None
    }
}
