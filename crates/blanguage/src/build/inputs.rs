//! 一次构建持有的解析输入；跨次复用仍由正式对象／映像拥有。
use crate::{CapturedExport, ExportRequest, ast::{self, Diagnostic, Source, Span}, object::{Kind, Object}, parser, syntax};
use std::{collections::{BTreeMap, BTreeSet}, path::Path, sync::Arc};
pub(super) enum ProviderInterface<'a> {
    Unit(&'a Arc<syntax::Module>),
    Object(&'a Object),
}

struct SourceInput {
    interface: Arc<syntax::Module>,
    body: Option<ast::Result<CapturedBody>>,
}
struct CapturedBody {
    implementation: Option<Arc<syntax::Module>>,
    imports: Arc<[String]>,
}
type Sources = BTreeMap<std::path::PathBuf, ast::Result<SourceInput>>;
type Interfaces<T> = BTreeMap<String, BTreeMap<Arc<str>, T>>;
type CapturedInterface = (Arc<syntax::Module>, Option<Arc<[String]>>);

pub(super) struct CapturedUnit {
    interfaces: Interfaces<ast::Result<CapturedInterface>>,
    exports: Option<Vec<ast::Result<CapturedExport>>>,
    checks: Option<crate::frontend::EntryChecks>,
}
pub(crate) struct Inputs {
    sources: Arc<Sources>,
    unit: CapturedUnit,
}
#[derive(Default)]
pub(super) struct Capture {
    sources: Sources,
    interfaces: Interfaces<ast::Result<Arc<syntax::Module>>>,
}
impl Capture {
    pub(super) fn finish(self, units: Vec<CapturedUnit>) -> Vec<Inputs> {
        let sources = Arc::new(self.sources);
        units.into_iter().map(|unit| Inputs { sources: sources.clone(), unit }).collect()
    }
    pub(super) fn header(&mut self, path: &Path) -> ast::Result<Arc<syntax::Module>> {
        self.source_input(path).as_ref().map(|input| input.interface.clone()).map_err(Clone::clone)
    }

    fn source_input(&mut self, path: &Path) -> &mut ast::Result<SourceInput> {
        use std::collections::btree_map::Entry;
        match self.sources.entry(crate::source_path::with_extension(path, u16::from(b'h'))) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let input = crate::read(entry.key()).and_then(parser::parse)
                    .map(|interface| SourceInput { interface: Arc::new(interface), body: None });
                entry.insert(input)
            }
        }
    }

    pub(super) fn capture(
        &mut self, path: &Path, root: &Path, providers: &BTreeMap<&str, ProviderInterface<'_>>,
        requests: &[String], share_checks: bool,
    ) -> CapturedUnit {
        let exports = (!providers.is_empty() && !requests.is_empty()).then(Vec::new);
        let count = if exports.is_some() { requests.len() } else { 0 };
        let mut state = CaptureState {
            capture: self, path, root, providers, requests,
            visited: BTreeSet::new(), interfaces: Interfaces::new(), exports,
            opened: None,
        };
        let context = std::ptr::from_mut(&mut state).cast();
        // Capture can stop at a failed read. The original loader consumes that
        // result from the snapshot when it reaches this path or provider.
        unsafe { capture_build_inputs(capture_host(context), count); }
        CapturedUnit {
            interfaces: state.interfaces, exports: state.exports,
            checks: share_checks.then(crate::frontend::EntryChecks::default),
        }
    }

    fn capture_interface(
        &mut self, name: &str, text: &Arc<str>,
    ) -> ast::Result<Arc<syntax::Module>> {
        let interfaces = self.interfaces.entry(name.into()).or_default();
        if let Some(module) = interfaces.get(text) { return module.clone(); }
        let module = parser::parse(Source {
            path: format!("<{name}>.h").into(), text: text.clone(),
        }).map(Arc::new);
        interfaces.insert(text.clone(), module.clone());
        module
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CaptureName { bytes: *const u8, length: usize }

impl CaptureName {
    fn from_str(name: &str) -> Self { Self { bytes: name.as_ptr(), length: name.len() } }
    // The loader only forwards Rust string spans held by the current open module or import list.
    unsafe fn text<'a>(self) -> &'a str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes, self.length)) }
    }
}

#[repr(u32)]
enum CaptureStatus { Failed, Ignored, Opened }

#[repr(C)]
struct CaptureOpened { name: CaptureName, status: CaptureStatus }

#[repr(C)]
#[derive(Clone, Copy)]
struct CaptureImports { data: *const String, count: usize }

impl CaptureImports {
    fn own(imports: Option<Arc<[String]>>) -> Self {
        match imports.filter(|imports| !imports.is_empty()) {
            Some(imports) => {
                let count = imports.len();
                Self { data: Arc::into_raw(imports).cast(), count }
            }
            None => Self { data: std::ptr::null(), count: 0 },
        }
    }
}

#[repr(C)]
struct CaptureHost {
    context: *mut std::ffi::c_void,
    known: unsafe extern "C" fn(*const std::ffi::c_void, CaptureName) -> u32,
    enter: unsafe extern "C" fn(*mut std::ffi::c_void, CaptureName) -> u32,
    open: unsafe extern "C" fn(*mut std::ffi::c_void, CaptureName) -> CaptureOpened,
    bind: unsafe extern "C" fn(*mut std::ffi::c_void, CaptureName, u32) -> CaptureImports,
    name: unsafe extern "C" fn(CaptureImports, usize) -> CaptureName,
    close: unsafe extern "C" fn(CaptureImports),
    provider: unsafe extern "C" fn(*const std::ffi::c_void, CaptureName) -> u32,
    export: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.module_load::capture_build_inputs"]
    fn capture_build_inputs(host: CaptureHost, requests: usize);
    #[link_name = "bootstrap.module_load::capture_export_owner"]
    fn capture_export_owner(host: CaptureHost, name: CaptureName) -> u32;
}

enum OpenedModule {
    Source(Arc<[String]>),
    Provider(Arc<syntax::Module>),
}

struct CaptureState<'a, 'provider> {
    capture: &'a mut Capture,
    path: &'a Path,
    root: &'a Path,
    providers: &'a BTreeMap<&'provider str, ProviderInterface<'provider>>,
    requests: &'a [String],
    visited: BTreeSet<String>,
    interfaces: Interfaces<ast::Result<CapturedInterface>>,
    exports: Option<Vec<ast::Result<CapturedExport>>>,
    opened: Option<OpenedModule>,
}

impl CaptureState<'_, '_> {
    fn source(&mut self, path: &Path) -> Option<(CaptureName, OpenedModule)> {
        let SourceInput { interface, body } = self.capture.source_input(path).as_mut().ok()?;
        let current = body.get_or_insert_with(|| {
            let implementation_path = crate::source_path::with_extension(path, u16::from(b'b'));
            let implementation = crate::read_implementation(&implementation_path)?;
            let imports = crate::names::module_imports(interface, implementation.as_deref());
            Ok(CapturedBody { implementation, imports })
        }).as_ref().ok()?;
        Some((CaptureName::from_str(interface.name()), OpenedModule::Source(current.imports.clone())))
    }

    fn open(&mut self, requested: Option<&str>) -> Result<Option<(CaptureName, OpenedModule)>, ()> {
        let Some(requested) = requested else {
            let path = self.path;
            return self.source(path).map(Some).ok_or(());
        };
        let module = match self.providers.get(requested) {
            Some(ProviderInterface::Unit(header)) => Arc::clone(header),
            Some(ProviderInterface::Object(object)) => {
                let Kind::Module { interface, .. } = &object.kind else {
                    // The actual module input builder diagnoses invalid explicit providers.
                    return Ok(None);
                };
                match self.capture.capture_interface(requested, interface) {
                    Ok(module) => module,
                    Err(error) => {
                        self.interfaces.entry(requested.to_owned()).or_default()
                            .insert(interface.clone(), Err(error));
                        return Err(());
                    }
                }
            }
            None => {
                let path = crate::source_path::module(self.root, requested, u16::from(b'b'));
                return self.source(&path).map(Some).ok_or(());
            }
        };
        Ok(Some((CaptureName::from_str(module.name()), OpenedModule::Provider(module))))
    }
}

fn capture_host(context: *mut std::ffi::c_void) -> CaptureHost {
    CaptureHost { context, known: capture_known, enter: capture_enter, open: capture_open,
        bind: capture_bind, name: capture_name, close: capture_close, provider: capture_provider,
        export: capture_export }
}

unsafe extern "C" fn capture_known(context: *const std::ffi::c_void, name: CaptureName) -> u32 {
    let state = unsafe { &*context.cast::<CaptureState<'_, '_>>() };
    u32::from(state.visited.contains(unsafe { name.text() }))
}
unsafe extern "C" fn capture_enter(context: *mut std::ffi::c_void, name: CaptureName) -> u32 {
    let state = unsafe { &mut *context.cast::<CaptureState<'_, '_>>() };
    u32::from(state.visited.insert(unsafe { name.text() }.to_owned()))
}
unsafe extern "C" fn capture_open(context: *mut std::ffi::c_void, name: CaptureName) -> CaptureOpened {
    let state = unsafe { &mut *context.cast::<CaptureState<'_, '_>>() };
    let requested = (!name.bytes.is_null()).then(|| unsafe { name.text() });
    let status = match state.open(requested) {
        Ok(Some((name, module))) => {
            state.opened = Some(module);
            return CaptureOpened { name, status: CaptureStatus::Opened };
        }
        Ok(None) => CaptureStatus::Ignored,
        Err(()) => CaptureStatus::Failed,
    };
    CaptureOpened { name, status }
}
unsafe extern "C" fn capture_bind(context: *mut std::ffi::c_void, name: CaptureName, expand: u32) -> CaptureImports {
    let state = unsafe { &mut *context.cast::<CaptureState<'_, '_>>() };
    let imports = match state.opened.take().unwrap() {
        OpenedModule::Source(imports) => (expand != 0).then_some(imports),
        OpenedModule::Provider(interface) => {
            let imports = (expand != 0).then(|| crate::names::module_imports(&interface, None));
            state.interfaces.entry(unsafe { name.text() }.to_owned()).or_default()
                .insert(interface.source.text.clone(), Ok((interface, imports.clone())));
            imports
        }
    };
    CaptureImports::own(imports)
}
unsafe extern "C" fn capture_name(imports: CaptureImports, index: usize) -> CaptureName {
    CaptureName::from_str(unsafe { &*imports.data.add(index) })
}
unsafe extern "C" fn capture_close(imports: CaptureImports) {
    let slice = std::ptr::slice_from_raw_parts(imports.data, imports.count);
    drop(unsafe { Arc::from_raw(slice) });
}
unsafe extern "C" fn capture_provider(context: *const std::ffi::c_void, name: CaptureName) -> u32 {
    let state = unsafe { &*context.cast::<CaptureState<'_, '_>>() };
    u32::from(state.providers.contains_key(unsafe { name.text() }))
}
unsafe extern "C" fn capture_export(context: *mut std::ffi::c_void, index: usize) -> u32 {
    let state = context.cast::<CaptureState<'_, '_>>();
    let requests = unsafe { (*state).requests };
    let request = match ExportRequest::parse(&requests[index]) {
        Ok(request) => request,
        Err(error) => {
            unsafe { (*state).exports.as_mut().unwrap().push(Err(error)); }
            return 0;
        }
    };
    let mut owners = Vec::new();
    // The type-owner walk calls the same B capture routine. Do not borrow state across that call.
    let complete = request.visit_type_owners(|name| {
        match unsafe { capture_export_owner(capture_host(context), CaptureName::from_str(name)) } {
            0 => {
                owners.push(name.to_owned());
                false
            }
            1 => true,
            _ => {
                owners.push(name.to_owned());
                true
            }
        }
    });
    unsafe { (*state).exports.as_mut().unwrap().push(Ok(CapturedExport { request, owners })); }
    u32::from(complete)
}

impl Inputs {
    pub(super) fn entry_checks(&self) -> Option<&crate::frontend::EntryChecks> {
        self.unit.checks.as_ref()
    }

    pub(crate) fn exports(&self) -> Option<&[ast::Result<CapturedExport>]> {
        self.unit.exports.as_deref()
    }

    pub(crate) fn interface(&self, name: &str, text: &str) -> ast::Result<CapturedInterface> {
        self.unit.interfaces.get(name).and_then(|interfaces| interfaces.get(text)).cloned()
            .unwrap_or_else(|| Err(Diagnostic {
                source: Source { path: format!("<{name}>.h").into(), text: "".into() },
                span: Span::default(), code: "B020",
                message: "接口不在本次已捕获的构建输入中".into(),
            }))
    }
    pub(crate) fn source(
        &self, path: &Path,
    ) -> ast::Result<(Arc<syntax::Module>, Option<Arc<syntax::Module>>, Arc<[String]>)> {
        let missing = |path| Diagnostic {
            source: Source { path, text: "".into() },
            span: Span::default(), code: "B020",
            message: "模块不在本次已捕获的构建输入中".into(),
        };
        let header = crate::source_path::with_extension(path, u16::from(b'h'));
        let input = self.sources.get(&header).ok_or_else(|| missing(header))?
            .as_ref().map_err(Clone::clone)?;
        let body = input.body.as_ref().ok_or_else(|| missing(crate::source_path::with_extension(path, u16::from(b'b'))))?
            .as_ref().map_err(Clone::clone)?;
        Ok((input.interface.clone(), body.implementation.clone(), body.imports.clone()))
    }
}
