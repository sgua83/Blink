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
    root: &'a Path, world: &'a mut Declarations,
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
    context.world.sources.contains_key(context.data.interface.name())
}
unsafe extern "C" fn enter(raw: *mut c_void) {
    let context = unsafe { context(raw) };
    let data = &context.data;
    let source = data.implementation.as_ref().map_or(&data.interface.source, |module| &module.source);
    context.world.sources.insert(data.interface.name().to_owned(), source.clone());
    context.world.interfaces.insert(data.interface.name().to_owned(), data.interface.clone());
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
        context.world.records.insert(key, (module.root.child::<wire::RecordNode>(offset),
            module.source.clone(), data.interface.name().to_owned(), data.imports.clone()));
    } else {
        context.world.type_sets.insert(key, (module.root.child::<wire::TypeSetNode>(offset),
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
    context.world.functions.insert(key, Arc::new(Definition {
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

pub(super) fn load(path: &Path, root: &Path, world: &mut Declarations,
    imported: Option<&module::Imports>, inputs: Option<&build::Inputs>) -> Result<String> {
    let provider = Provider::Source(ModuleSource::Entry(path));
    load_provider(&provider, root, world, imported, inputs).map(|module| module.name().to_owned())
}

fn load_import<'a>(name: &'a str, root: &'a Path, world: &'a mut Declarations,
    imported: Option<&'a module::Imports>, inputs: Option<&'a build::Inputs>) -> Result<&'a syntax::Module> {
    let loaded = world.sources.contains_key(name).then_some(Provider::Loaded(name));
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
    selected: &Provider<'a>, root: &'a Path, world: &'a mut Declarations,
    imported: Option<&'a module::Imports>, inputs: Option<&'a build::Inputs>,
) -> Result<&'a syntax::Module> {
    let (interface, implementation, captured_imports, compiled) = match selected {
        Provider::Loaded(name) => return Ok(world.interfaces[*name].as_ref()),
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
        None => Ok(context.world.interfaces[context.data.interface.name()].as_ref()),
    }
}

type WorldFunction<'a> = (&'a String, &'a Arc<Definition>);

#[repr(C)]
struct WorldDeclaration {
    source: wire::SignatureNode,
    present: u32,
    paired: u32,
}

#[repr(u32)]
#[derive(Clone, Copy)]
enum WorldCheck { Constant, Interface, External, Types }

#[repr(C)]
struct WorldPreparation<'a, 'w> {
    context: *mut WorldChecks<'a, 'w>,
    next: unsafe extern "C" fn(*mut WorldChecks<'a, 'w>) -> WorldDeclaration,
    signature: unsafe extern "C" fn(*mut WorldChecks<'a, 'w>) -> u32,
    enqueue: unsafe extern "C" fn(*mut WorldChecks<'a, 'w>, WorldCheck),
    run: unsafe extern "C" fn(*mut WorldChecks<'a, 'w>, WorldCheck) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.module_load::prepare_world"]
    fn prepare_world_b(host: WorldPreparation<'_, '_>);
}

struct WorldChecks<'a, 'w> {
    world: &'w mut World,
    declarations: std::collections::btree_map::Iter<'a, String, Arc<Definition>>,
    current: Option<WorldFunction<'a>>,
    pending: [Vec<WorldFunction<'a>>; 3],
    track_constants: bool,
    error: Option<Diagnostic>,
}

unsafe extern "C" fn next_declaration(context: *mut WorldChecks<'_, '_>) -> WorldDeclaration {
    let checks = unsafe { &mut *context };
    checks.current = checks.declarations.next();
    if let Some((_, definition)) = checks.current {
        WorldDeclaration {
            source: definition.function.signature_node(),
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
            present: 0,
            paired: 0,
        }
    }
}

unsafe extern "C" fn check_signature(context: *mut WorldChecks<'_, '_>) -> u32 {
    let checks = unsafe { &mut *context };
    let (_, definition) = checks.current.unwrap();
    checks.error = generics::validate_declaration(definition).err();
    u32::from(checks.error.is_none())
}

unsafe extern "C" fn enqueue_declaration(context: *mut WorldChecks<'_, '_>, check: WorldCheck) {
    let checks = unsafe { &mut *context };
    checks.pending[check as usize].push(checks.current.unwrap());
}

impl WorldChecks<'_, '_> {
    fn run(&mut self, check: WorldCheck) -> Result<()> {
        let world = &mut *self.world;
        match check {
            WorldCheck::Constant => {
                if self.track_constants {
                    world.build_declarations = Some(RefCell::new(BTreeSet::new()));
                }
                for (name, _) in std::mem::take(&mut self.pending[check as usize]) {
                    unsafe { world.run(world.prepare_constant(name)) }?;
                }
                world.build_declarations = None;
            }
            WorldCheck::Interface => {
                let bindings = std::cell::LazyCell::new(|| Arc::new(generics::Bindings::default()));
                for (_, definition) in std::mem::take(&mut self.pending[check as usize]) {
                    unsafe { world.run(world.check_interface_types(definition, &bindings)) }?;
                }
            }
            WorldCheck::Types => unsafe { world.run(super::validate_types(world)) }?,
            WorldCheck::External => {
                for (name, definition) in std::mem::take(&mut self.pending[check as usize]) {
                    let instance = unsafe { world.run(generics::instantiate(world, name, definition, Vec::new(), &[])) }?;
                    unsafe { world.run(check::external::check_declaration(world, &instance)) }?;
                }
            }
        }
        Ok(())
    }
}

unsafe extern "C" fn run_world_check(context: *mut WorldChecks<'_, '_>, check: WorldCheck) -> u32 {
    let checks = unsafe { &mut *context };
    checks.error = checks.run(check).err();
    u32::from(checks.error.is_none())
}

// read_world is a synchronous entry. The original World runs each requested
// check here, with its evaluation stack empty on entry and return.
pub(super) fn prepare_world(world: &mut World, track_constants: bool) -> Result<()> {
    let declarations = Arc::clone(&world.declarations);
    let mut checks = WorldChecks {
        world,
        declarations: declarations.functions.iter(),
        current: None,
        pending: Default::default(),
        track_constants,
        error: None,
    };
    unsafe {
        prepare_world_b(WorldPreparation {
            context: &mut checks,
            next: next_declaration,
            signature: check_signature,
            enqueue: enqueue_declaration,
            run: run_world_check,
        });
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
    load: unsafe extern "C" fn(*mut c_void, Bytes) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.module_load::load_export_types"]
    fn load_export_types(storage: wire::StorageView, arguments: wire::Slice, host: ExportTypeHost);
}

struct ExportVisit<F> {
    visit: F,
    parents: Vec<ExportRange>,
    error: Option<Diagnostic>,
}

unsafe extern "C" fn save_export_range<F>(context: *mut c_void, range: ExportRange) {
    unsafe { &mut *context.cast::<ExportVisit<F>>() }.parents.push(range);
}

unsafe extern "C" fn resume_export_range<F>(context: *mut c_void) -> ExportRange {
    unsafe { &mut *context.cast::<ExportVisit<F>>() }.parents.pop().unwrap_or(ExportRange {
        arguments: wire::Slice { offset: 0, count: 0 }, remaining: 0,
    })
}

unsafe extern "C" fn visit_export_owner<F: FnMut(&str) -> Result<()>>(context: *mut c_void, name: Bytes) -> u32 {
    let state = unsafe { &mut *context.cast::<ExportVisit<F>>() };
    state.error = (state.visit)(unsafe { name.text() }).err();
    u32::from(state.error.is_none())
}

pub(super) fn visit_export_types<F: FnMut(&str) -> Result<()>>(
    arguments: &syntax::List<wire::ArgumentNode>, visit: F,
) -> Result<()> {
    let mut state = ExportVisit { visit, parents: Vec::new(), error: None };
    unsafe {
        load_export_types(arguments.source_view().storage(), arguments.source_slice(), ExportTypeHost {
            context: std::ptr::from_mut(&mut state).cast(),
            save: save_export_range::<F>,
            resume: resume_export_range::<F>,
            load: visit_export_owner::<F>,
        });
    }
    state.error.map_or(Ok(()), Err)
}

fn load_export_owner(
    name: &str, root: &Path, world: &mut Declarations,
    imported: &module::Imports, inputs: Option<&build::Inputs>,
) -> Result<()> {
    let Some(module) = imported.modules.get(name) else { return Ok(()); };
    if world.sources.contains_key(name) { return Ok(()); }
    let provider = Provider::Compiled { name, module };
    load_provider(&provider, root, world, Some(imported), inputs).map(|_| ())
}

pub(super) fn prepare_exports(
    requests: &[String], root: &Path, world: &mut Declarations,
    imported: &module::Imports, inputs: Option<&build::Inputs>,
) -> Result<Vec<ExportRequest>> {
    requests.iter().map(|text| {
        let request = ExportRequest::parse(text)?;
        request.visit_type_owners(|name| load_export_owner(name, root, world, imported, inputs))?;
        Ok(request)
    }).collect()
}

pub(super) fn load_captured_exports(
    requests: &[Result<CapturedExport>], root: &Path, world: &mut Declarations,
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
