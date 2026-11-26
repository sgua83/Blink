//! B reads the sealed source storage and drives loading; these callbacks keep
//! the original inputs, diagnostics and declaration containers in their owner.
use super::*;
use crate::syntax::{self, wire};
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Bytes { bytes: *const u8, length: u64 }
impl Bytes {
    fn new(text: &str) -> Self { Self { bytes: text.as_ptr(), length: text.len() as u64 } }
    unsafe fn text<'a>(self) -> &'a str {
        if self.length == 0 { return ""; }
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes, self.length as usize)) }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ModuleView { owner: *const c_void, source: wire::SignatureNode }
impl ModuleView {
    fn new(module: &syntax::Module) -> Self {
        Self { owner: (module as *const syntax::Module).cast(), source: module.root.signature_node() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct LoadInput {
    interface: ModuleView, implementation: ModuleView,
    has_implementation: u32, compiled: u32,
}
#[repr(C)]
struct Imported { success: u32, requested: Bytes, name: Bytes }

#[repr(u32)]
#[derive(Clone, Copy)]
enum LoadFailure {
    ModuleName = 22, ImportName = 24, DuplicateRecord = 25, InterfaceBody = 26,
    MissingBody = 27, Signature = 28, ImplementationBody = 29, DuplicateValue = 30,
    ReservedRecord = 184, DuplicateSet = 233, MissingRequirement = 270,
}

unsafe extern "C" {
    #[link_name = "bootstrap.module_load::provider"]
    fn select_provider(loaded: *const c_void, compiled: *const c_void, source: *const c_void) -> *const c_void;
    #[link_name = "bootstrap.module_load::load"]
    fn load_b(input: LoadInput, context: *mut c_void,
        contains: unsafe extern "C" fn(*mut c_void) -> bool,
        enter: unsafe extern "C" fn(*mut c_void),
        imports: unsafe extern "C" fn(*mut c_void) -> u64,
        import_module: unsafe extern "C" fn(*mut c_void, u64) -> Imported,
        prepare: unsafe extern "C" fn(*mut c_void, Bytes, u32) -> bool,
        insert_type: unsafe extern "C" fn(*mut c_void, *const c_void, u64, u32),
        pair: unsafe extern "C" fn(*mut c_void) -> *const u64,
        signature: unsafe extern "C" fn(*mut c_void, *const c_void, u64, u64) -> bool,
        requirement: unsafe extern "C" fn(*mut c_void) -> u32,
        insert_function: unsafe extern "C" fn(*mut c_void, *const c_void, u64, wire::Optional<wire::FunctionRef>, u32, u32, u32),
        report: unsafe extern "C" fn(*mut c_void, LoadFailure, *const c_void, wire::Span, Bytes, Bytes),
    );
}

struct Loaded {
    interface: Arc<syntax::Module>,
    implementation: Option<Arc<syntax::Module>>,
    imports: Arc<[String]>,
    captured_imports: Option<Arc<[String]>>,
    pairs: Option<Vec<u64>>,
    key: String,
}
struct Context<'a> {
    root: &'a Path, world: &'a mut DeclarationInput,
    imported: Option<&'a module::Imports>,
    inputs: Option<&'a build::Inputs>, compiled: Option<&'a module::Imported>,
    data: Loaded, error: Option<Diagnostic>,
}
unsafe fn context<'a>(raw: *mut c_void) -> &'a mut Context<'a> {
    unsafe { &mut *raw.cast::<Context<'a>>() }
}
unsafe fn source_module<'a>(raw: *const c_void) -> &'a syntax::Module {
    unsafe { &*raw.cast::<syntax::Module>() }
}

unsafe extern "C" fn contains(raw: *mut c_void) -> bool {
    let context = unsafe { context(raw) };
    context.world.modules.contains_key(context.data.interface.name())
}
unsafe extern "C" fn enter(raw: *mut c_void) {
    let context = unsafe { context(raw) };
    let data = &context.data;
    context.world.modules.insert(data.interface.name(), ModuleDeclaration {
        interface: data.interface.clone(),
        implementation: data.implementation.as_ref().map(|module| module.source.clone()),
    });
}
unsafe extern "C" fn imports(raw: *mut c_void) -> u64 {
    let data = &mut unsafe { context(raw) }.data;
    data.imports = data.captured_imports.take().unwrap_or_else(||
        crate::names::module_imports(&data.interface, data.implementation.as_deref()));
    data.imports.len() as u64
}

unsafe extern "C" fn import_module(raw: *mut c_void, index: u64) -> Imported {
    let context = unsafe { context(raw) };
    let name = &context.data.imports[index as usize];
    let requested = Bytes::new(name);
    match load_import(name, context.root, context.world, context.imported, context.inputs) {
        Ok(module) => Imported { success: 1, requested, name: Bytes::new(module.name()) },
        Err(error) => {
            context.error = Some(error);
            Imported { success: 0, requested, name: Bytes::new("") }
        }
    }
}
unsafe extern "C" fn prepare(raw: *mut c_void, name: Bytes, namespace: u32) -> bool {
    let context = unsafe { context(raw) };
    let data = &mut context.data;
    data.key = format!("{}::{}", data.interface.name(), unsafe { name.text() });
    match namespace {
        0 => context.world.records.contains_key(&data.key),
        1 => context.world.records.contains_key(&data.key) || context.world.type_sets.contains_key(&data.key),
        _ => context.world.functions.contains_key(&data.key),
    }
}
unsafe extern "C" fn insert_type(raw: *mut c_void, owner: *const c_void, offset: u64, namespace: u32) {
    let context = unsafe { context(raw) };
    let data = &mut context.data;
    let module = unsafe { source_module(owner) };
    let key = std::mem::take(&mut data.key);
    if namespace == 0 {
        context.world.records.insert(&key, (module.root.child::<wire::RecordNode>(offset),
            module.source.clone(), data.interface.name().to_owned(), data.imports.clone()));
    } else {
        context.world.type_sets.insert(&key, (module.root.child::<wire::TypeSetNode>(offset),
            module.source.clone(), data.interface.name().to_owned(), data.imports.clone()));
    }
}
unsafe extern "C" fn pair(raw: *mut c_void) -> *const u64 {
    let context = unsafe { context(raw) };
    let data = &mut context.data;
    let implementations = data.implementation.as_ref().map(|module| module.functions());
    data.pairs = crate::names::function_pairs(&data.interface.functions(), implementations.as_ref());
    data.pairs.as_ref().map_or(std::ptr::null(), |pairs| pairs.as_ptr())
}
unsafe extern "C" fn signature(raw: *mut c_void, owner: *const c_void, declaration: u64, implementation: u64) -> bool {
    let context = unsafe { context(raw) };
    let data = &context.data;
    let interface = data.interface.root.child::<wire::FunctionNode>(declaration);
    let body = unsafe { source_module(owner) }.root.child::<wire::FunctionNode>(implementation);
    interface.same_signature(&body)
}
unsafe extern "C" fn requirement(raw: *mut c_void) -> u32 {
    let context = unsafe { context(raw) };
    context.compiled.and_then(|compiled| compiled.requirements.get(&context.data.key))
        .map_or(3, |requirement| match requirement {
            execution::Requirement::Invocation => 0, execution::Requirement::Domain => 1, execution::Requirement::Host => 2,
        })
}
unsafe extern "C" fn insert_function(raw: *mut c_void, owner: *const c_void, offset: u64,
    interface: wire::Optional<wire::FunctionRef>, public: u32, link_body: u32, requirement: u32) {
    let context = unsafe { context(raw) };
    let data = &mut context.data;
    let module = unsafe { source_module(owner) };
    let key = std::mem::take(&mut data.key);
    context.world.functions.insert(&key, Arc::new(Definition {
        constant: OnceLock::new(),
        function: module.root.child::<wire::FunctionNode>(offset),
        interface_declaration: (interface.present != 0).then(|| data.interface.root.child::<wire::FunctionNode>(interface.value.offset)),
        syntax_origin: if context.compiled.is_some() { DefinitionSyntax::CompiledInterface } else { DefinitionSyntax::Source },
        execution: [execution::Requirement::Invocation, execution::Requirement::Domain, execution::Requirement::Host][requirement as usize],
        source: module.source.clone(), module: data.interface.name().to_owned(), imports: data.imports.clone(),
        public: public != 0,
        compiled: (link_body != 0).then(|| context.compiled.map(|module| module.definitions.clone())).flatten(),
    }));
}
unsafe extern "C" fn report(raw: *mut c_void, failure: LoadFailure, owner: *const c_void,
    span: wire::Span, subject: Bytes, actual: Bytes) {
    let context = unsafe { context(raw) };
    let module = unsafe { source_module(owner) };
    let subject = unsafe { subject.text() };
    let (code, message) = match failure {
        LoadFailure::ModuleName => ("B022", ".h 与 .b 的模块名不一致".to_owned()),
        LoadFailure::ImportName => ("B024", format!("导入 {subject}，文件却声明 {}", unsafe { actual.text() })),
        LoadFailure::DuplicateRecord => ("B025", format!("重复类型 {}", context.data.key)),
        LoadFailure::InterfaceBody => ("B026", ".h 只声明接口，函数体放在 .b".to_owned()),
        LoadFailure::MissingBody => ("B027", format!("接口 {subject} 缺少 .b 实现")),
        LoadFailure::Signature => ("B028", format!("{subject} 的函数效果、类型参数、访问权限、类型、参数名、执行目标或前缀域约束与 .h 不一致")),
        LoadFailure::ImplementationBody => ("B029", ".b 中的函数必须有实现".to_owned()),
        LoadFailure::DuplicateValue => ("B030", format!("值名称重复 {}", context.data.key)),
        LoadFailure::ReservedRecord => ("B184", "记录名称不能覆盖内置类型".to_owned()),
        LoadFailure::DuplicateSet => ("B233", format!("类型集合名称重复或覆盖已有类型 {}", context.data.key)),
        LoadFailure::MissingRequirement => ("B270", format!("已编译模块没有接口 {subject} 的参与要求")),
    };
    context.error = Some(Diagnostic { source: module.source.clone(),
        span: Span { start: span.start as usize, end: span.end as usize }, code, message });
}

enum ModuleSource<'a> {
    Entry(&'a Path),
    Import(&'a str),
}

enum Provider<'a> {
    Loaded(&'a str),
    Compiled { name: &'a str, module: &'a module::Imported },
    Source(ModuleSource<'a>),
}

pub(super) fn load(path: &Path, root: &Path, world: &mut DeclarationInput,
    imported: Option<&module::Imports>, inputs: Option<&build::Inputs>) -> Result<String> {
    let provider = Provider::Source(ModuleSource::Entry(path));
    load_provider(&provider, root, world, imported, inputs).map(|module| module.name().to_owned())
}

fn load_import<'a>(name: &'a str, root: &'a Path, world: &'a mut DeclarationInput,
    imported: Option<&'a module::Imports>, inputs: Option<&'a build::Inputs>) -> Result<&'a syntax::Module> {
    let loaded = world.modules.contains_key(name).then_some(Provider::Loaded(name));
    let compiled = imported.and_then(|imports| imports.modules.get(name))
        .map(|module| Provider::Compiled { name, module });
    let source = Provider::Source(ModuleSource::Import(name));
    let selected = unsafe { &*select_provider(
        loaded.as_ref().map_or(std::ptr::null(), |provider| (provider as *const Provider<'_>).cast()),
        compiled.as_ref().map_or(std::ptr::null(), |provider| (provider as *const Provider<'_>).cast()),
        (&source as *const Provider<'_>).cast(),
    ).cast::<Provider<'_>>() };
    load_provider(selected, root, world, imported, inputs)
}

fn load_provider<'a>(
    selected: &Provider<'a>, root: &'a Path, world: &'a mut DeclarationInput,
    imported: Option<&'a module::Imports>, inputs: Option<&'a build::Inputs>,
) -> Result<&'a syntax::Module> {
    let (interface, implementation, captured_imports, compiled) = match selected {
        Provider::Loaded(name) => return Ok(world.modules[*name].interface.as_ref()),
        Provider::Compiled { name, module } => {
            let (interface, imports) = match inputs {
                Some(inputs) => inputs.interface(name, &module.header)?,
                None => (Arc::new(parser::parse(Source {
                    path: PathBuf::from(format!("<{name}>.h")), text: module.header.clone(),
                })?), None),
            };
            (interface, None, imports, Some(*module))
        }
        Provider::Source(source) => {
            let import_path;
            let path = match source {
                ModuleSource::Entry(path) => *path,
                ModuleSource::Import(name) => {
                    import_path = crate::source_path::module(root, name, u16::from(b'b'));
                    import_path.as_path()
                }
            };
            if let Some(inputs) = inputs {
                let (interface, implementation, imports) = inputs.source(path)?;
                (interface, implementation, Some(imports), None)
            } else {
                let interface = Arc::new(parser::parse(read(&crate::source_path::with_extension(path, u16::from(b'h')))?)?);
                let implementation_path = crate::source_path::with_extension(path, u16::from(b'b'));
                (interface, read_implementation(&implementation_path)?, None, None)
            }
        }
    };
    let mut context = Context { root, world, imported, inputs, compiled,
        data: Loaded { interface, implementation, imports: Arc::from([]), captured_imports,
            pairs: None, key: String::new() }, error: None };
    let interface = ModuleView::new(&context.data.interface);
    let input = LoadInput { interface,
        implementation: context.data.implementation.as_deref().map_or(interface, ModuleView::new),
        has_implementation: u32::from(context.data.implementation.is_some()),
        compiled: u32::from(compiled.is_some()),
    };
    unsafe { load_b(input, (&mut context as *mut Context<'_>).cast(),
        contains, enter, imports, import_module, prepare, insert_type, pair, signature, requirement, insert_function, report); }
    match context.error.take() {
        Some(error) => Err(error),
        None => Ok(context.world.modules[context.data.interface.name()].interface.as_ref()),
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WorldFunction {
    name: crate::names::Bytes,
    definition: *const c_void,
}

#[repr(C)]
struct WorldDeclaration {
    source: wire::SignatureNode,
    function: WorldFunction,
    present: u32,
    paired: u32,
}

#[allow(dead_code)] // The check kind is selected by B.
#[repr(u32)]
#[derive(Clone, Copy)]
enum WorldCheck { Constant, Interface, External, Types }

#[repr(C)]
struct WorldPreparation {
    context: *mut c_void,
    declarations: u64,
    next: unsafe extern "C" fn(*mut c_void) -> WorldDeclaration,
    signature: unsafe extern "C" fn(*mut c_void, WorldFunction) -> u32,
    track: unsafe extern "C" fn(*mut c_void, u32),
    finish_interface: unsafe extern "C" fn(*mut c_void),
    run: unsafe extern "C" fn(*mut c_void, WorldCheck, WorldFunction) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.module_load::prepare_world"]
    fn prepare_world_b(host: WorldPreparation) -> u64;
}

struct WorldChecks<'a, 'w> {
    world: &'w mut World,
    declarations: declarations::Iter<'a, Arc<Definition>>,
    bindings: Option<Arc<generics::Bindings>>,
    track_constants: bool,
    error: Option<Diagnostic>,
}

unsafe extern "C" fn next_declaration(raw: *mut c_void) -> WorldDeclaration {
    let checks = unsafe { &mut *raw.cast::<WorldChecks<'_, '_>>() };
    if let Some((name, definition)) = checks.declarations.next() {
        WorldDeclaration {
            source: definition.function.signature_node(),
            function: WorldFunction {
                name: crate::names::Bytes::new(name),
                definition: std::ptr::from_ref(definition).cast(),
            },
            present: 1,
            paired: u32::from(definition.interface_declaration.is_some()),
        }
    } else {
        WorldDeclaration {
            source: wire::SignatureNode { storage: wire::StorageView {
                nodes: std::ptr::null_mut(), node_bytes: 0,
                text: std::ptr::null_mut(), text_bytes: 0,
                source: std::ptr::null_mut(), source_bytes: 0,
            }, offset: 0 },
            function: WorldFunction { name: crate::names::Bytes::absent(), definition: std::ptr::null() },
            present: 0,
            paired: 0,
        }
    }
}

unsafe extern "C" fn check_signature(raw: *mut c_void, function: WorldFunction) -> u32 {
    let checks = unsafe { &mut *raw.cast::<WorldChecks<'_, '_>>() };
    let definition = unsafe { &*function.definition.cast::<Arc<Definition>>() };
    checks.error = generics::validate_declaration(definition).err();
    u32::from(checks.error.is_none())
}

unsafe extern "C" fn track_constants(raw: *mut c_void, active: u32) {
    let checks = unsafe { &mut *raw.cast::<WorldChecks<'_, '_>>() };
    if active == 0 {
        checks.world.build_declarations = None;
    } else if checks.track_constants {
        checks.world.build_declarations = Some(RefCell::new(BTreeSet::new()));
    }
}

unsafe extern "C" fn finish_interface(raw: *mut c_void) {
    unsafe { &mut *raw.cast::<WorldChecks<'_, '_>>() }.bindings = None;
}

impl WorldChecks<'_, '_> {
    fn run(&mut self, check: WorldCheck, function: WorldFunction) -> Result<()> {
        let world = &*self.world;
        match check {
            WorldCheck::Constant => {
                let name = unsafe { function.name.text() };
                unsafe { world.run(world.prepare_constant(name)) }.map(|_| ())
            }
            WorldCheck::Interface => {
                let definition = unsafe { &*function.definition.cast::<Arc<Definition>>() };
                let bindings = self.bindings.get_or_insert_with(|| Arc::new(generics::Bindings::default()));
                unsafe { world.run(world.check_interface_types(definition, bindings)) }
            }
            WorldCheck::Types => unsafe { world.run(super::validate_types(world)) },
            WorldCheck::External => {
                let name = unsafe { function.name.text() };
                let definition = unsafe { &*function.definition.cast::<Arc<Definition>>() };
                let instance = unsafe { world.run(generics::instantiate(world, name, definition, Vec::new(), &[])) }?;
                unsafe { world.run(check::external::check_declaration(world, &instance)) }
            }
        }
    }
}

unsafe extern "C" fn run_world_check(raw: *mut c_void, check: WorldCheck, function: WorldFunction) -> u32 {
    let checks = unsafe { &mut *raw.cast::<WorldChecks<'_, '_>>() };
    checks.error = checks.run(check, function).err();
    u32::from(checks.error.is_none())
}

// The declaration owner keeps all borrowed keys and definitions live through
// planning and checking. Each requested check enters the same World stack.
pub(super) fn prepare_world(world: &mut World, track_dependencies: bool) -> Result<()> {
    let declarations = Arc::clone(&world.declarations);
    let mut checks = WorldChecks {
        world,
        declarations: declarations.functions.iter(),
        bindings: None,
        track_constants: track_dependencies,
        error: None,
    };
    let count = declarations.functions.len() as u64;
    let failed_allocation = unsafe {
        prepare_world_b(WorldPreparation {
            context: std::ptr::from_mut(&mut checks).cast(),
            declarations: count,
            next: next_declaration,
            signature: check_signature,
            track: track_constants,
            finish_interface,
            run: run_world_check,
        })
    };
    if failed_allocation != 0 {
        std::alloc::handle_alloc_error(
            std::alloc::Layout::from_size_align(failed_allocation as usize, std::mem::align_of::<WorldFunction>()).unwrap(),
        );
    }
    checks.error.map_or(Ok(()), Err)
}

#[repr(C)]
struct ExportRange {
    arguments: wire::Slice,
    remaining: u64,
}

#[repr(C)]
struct ExportTypeHost {
    context: *mut c_void,
    save: unsafe extern "C" fn(*mut c_void, ExportRange),
    resume: unsafe extern "C" fn(*mut c_void) -> ExportRange,
    load: unsafe extern "C" fn(*mut c_void, Bytes) -> bool,
}

unsafe extern "C" {
    #[link_name = "bootstrap.module_load::load_export_types"]
    fn load_export_types(storage: wire::StorageView, arguments: wire::Slice, host: ExportTypeHost) -> u32;
}

struct ExportVisit<F> {
    visit: F,
    parents: Vec<ExportRange>,
}

unsafe extern "C" fn save_export_range<F>(context: *mut c_void, range: ExportRange) {
    unsafe { &mut *context.cast::<ExportVisit<F>>() }.parents.push(range);
}

unsafe extern "C" fn resume_export_range<F>(context: *mut c_void) -> ExportRange {
    unsafe { &mut *context.cast::<ExportVisit<F>>() }.parents.pop().unwrap_or(ExportRange {
        arguments: wire::Slice { offset: 0, count: 0 }, remaining: 0,
    })
}

unsafe extern "C" fn visit_export_owner<F: FnMut(&str) -> bool>(context: *mut c_void, name: Bytes) -> bool {
    let state = unsafe { &mut *context.cast::<ExportVisit<F>>() };
    (state.visit)(unsafe { name.text() })
}

pub(super) fn visit_export_types<F: FnMut(&str) -> bool>(
    arguments: &syntax::List<wire::ArgumentNode>, visit: F,
) -> bool {
    let mut state = ExportVisit { visit, parents: Vec::new() };
    unsafe {
        load_export_types(arguments.source_view().storage(), arguments.source_slice(), ExportTypeHost {
            context: std::ptr::from_mut(&mut state).cast(),
            save: save_export_range::<F>,
            resume: resume_export_range::<F>,
            load: visit_export_owner::<F>,
        }) != 0
    }
}

fn load_export_owner(
    name: &str, root: &Path, world: &mut DeclarationInput,
    imported: &module::Imports, inputs: Option<&build::Inputs>,
) -> Result<()> {
    let Some(module) = imported.modules.get(name) else { return Ok(()); };
    if world.modules.contains_key(name) { return Ok(()); }
    let provider = Provider::Compiled { name, module };
    load_provider(&provider, root, world, Some(imported), inputs).map(|_| ())
}

pub(super) fn prepare_exports(
    requests: &[String], root: &Path, world: &mut DeclarationInput,
    imported: &module::Imports, inputs: Option<&build::Inputs>,
) -> Result<Vec<ExportRequest>> {
    requests.iter().map(|text| {
        let request = ExportRequest::parse(text)?;
        let mut loaded = Ok(());
        request.visit_type_owners(|name| {
            loaded = load_export_owner(name, root, world, imported, inputs);
            loaded.is_ok()
        });
        loaded?;
        Ok(request)
    }).collect()
}

pub(super) fn load_captured_exports(
    requests: &[Result<CapturedExport>], root: &Path, world: &mut DeclarationInput,
    imported: &module::Imports, inputs: Option<&build::Inputs>,
) -> Result<()> {
    for request in requests {
        let request = request.as_ref().map_err(Clone::clone)?;
        for name in &request.owners {
            load_export_owner(name, root, world, imported, inputs)?;
        }
    }
    Ok(())
}
