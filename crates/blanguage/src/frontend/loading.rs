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
struct ImportList { names: *const Bytes, count: u64 }
#[repr(C)]
struct Imported { success: u32, name: Bytes }

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
        imports: unsafe extern "C" fn(*mut c_void) -> ImportList,
        import_module: unsafe extern "C" fn(*mut c_void, Bytes) -> Imported,
        prepare: unsafe extern "C" fn(*mut c_void, Bytes, u32) -> bool,
        insert_type: unsafe extern "C" fn(*mut c_void, *const c_void, u64, u32),
        pair: unsafe extern "C" fn(*mut c_void) -> *const u64,
        signature: unsafe extern "C" fn(*mut c_void, *const c_void, u64, u64) -> bool,
        requirement: unsafe extern "C" fn(*mut c_void) -> u32,
        insert_function: unsafe extern "C" fn(*mut c_void, *const c_void, u64, wire::Optional<wire::FunctionRef>, u32, u32, u32),
        report: unsafe extern "C" fn(*mut c_void, LoadFailure, *const c_void, wire::Span, Bytes, Bytes),
    ) -> Bytes;
}

struct Loaded {
    interface: Arc<syntax::Module>,
    implementation: Option<Arc<syntax::Module>>,
    imports: Arc<[String]>,
    captured_imports: Option<Arc<[String]>>,
    import_views: Vec<Bytes>,
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
unsafe extern "C" fn imports(raw: *mut c_void) -> ImportList {
    let context = unsafe { context(raw) };
    let data = &mut context.data;
    data.imports = data.captured_imports.take().unwrap_or_else(||
        crate::names::module_imports(&data.interface, data.implementation.as_deref()));
    data.import_views = data.imports.iter().map(|name| Bytes::new(name)).collect();
    ImportList { names: data.import_views.as_ptr(), count: data.import_views.len() as u64 }
}
unsafe extern "C" fn import_module(raw: *mut c_void, name: Bytes) -> Imported {
    let context = unsafe { context(raw) };
    let name = unsafe { name.text() };
    let path = crate::source_path::module(context.root, name, u16::from(b'b'));
    match load(&path, context.root, context.world, context.imported, Some(name), context.inputs) {
        Ok(actual) => {
            // The original interface map already retains this syntax owner;
            // no temporary String or recursive-call handle escapes the call.
            let module = &context.world.interfaces[&actual];
            Imported { success: 1, name: Bytes::new(module.name()) }
        }
        Err(error) => { context.error = Some(error); Imported { success: 0, name: Bytes::new("") } }
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

enum Provider<'a> {
    Loaded(&'a str),
    Compiled { name: &'a str, module: &'a module::Imported },
    CapturedCompiled { name: &'a str, module: &'a module::Imported, inputs: &'a build::Inputs },
    Source,
    Captured(&'a build::Inputs),
}
pub(crate) fn load(path: &Path, root: &Path, world: &mut Declarations,
    imported: Option<&module::Imports>, import: Option<&str>, inputs: Option<&build::Inputs>) -> Result<String> {
    // B selects among actual available operations, not their Rust representation.
    // Each operation already carries its typed source/capture/compiled owner.
    let loaded = import.filter(|name| world.sources.contains_key(*name)).map(Provider::Loaded);
    let compiled = import.zip(imported).and_then(|(name, imports)| imports.modules.get(name).map(|module| {
        match inputs {
            Some(inputs) => Provider::CapturedCompiled { name, module, inputs },
            None => Provider::Compiled { name, module },
        }
    }));
    let source = match inputs { Some(inputs) => Provider::Captured(inputs), None => Provider::Source };
    let selected = unsafe { &*select_provider(
        loaded.as_ref().map_or(std::ptr::null(), |provider| (provider as *const Provider<'_>).cast()),
        compiled.as_ref().map_or(std::ptr::null(), |provider| (provider as *const Provider<'_>).cast()),
        (&source as *const Provider<'_>).cast(),
    ).cast::<Provider<'_>>() };
    let (interface, implementation, captured_imports, compiled) = match selected {
        Provider::Loaded(name) => return Ok((*name).to_owned()),
        Provider::Compiled { name, module } => (Arc::new(parser::parse(Source {
            path: PathBuf::from(format!("<{name}>.h")), text: module.header.clone().into(),
        })?), None, None, Some(*module)),
        Provider::CapturedCompiled { name, module, inputs } =>
            (inputs.interface(name, &module.header)?, None, None, Some(*module)),
        Provider::Source => {
            let interface = Arc::new(parser::parse(read(&crate::source_path::with_extension(path, u16::from(b'h')))?)?);
            let implementation_path = crate::source_path::with_extension(path, u16::from(b'b'));
            (interface, read_implementation(&implementation_path)?, None, None)
        }
        Provider::Captured(inputs) => {
            let (interface, implementation, imports) = inputs.source(path)?;
            (interface, implementation, Some(imports), None)
        }
    };
    let mut context = Context { root, world, imported, inputs, compiled,
        data: Loaded { interface, implementation, imports: Arc::from([]), captured_imports,
            import_views: Vec::new(), pairs: None, key: String::new() }, error: None };
    let interface = ModuleView::new(&context.data.interface);
    let input = LoadInput { interface,
        implementation: context.data.implementation.as_deref().map_or(interface, ModuleView::new),
        has_implementation: u32::from(context.data.implementation.is_some()),
        compiled: u32::from(compiled.is_some()),
    };
    let name = unsafe { load_b(input, (&mut context as *mut Context<'_>).cast(),
        contains, enter, imports, import_module, prepare, insert_type, pair, signature, requirement, insert_function, report) };
    match context.error.take() {
        Some(error) => Err(error),
        None => Ok(unsafe { name.text() }.to_owned()),
    }
}
