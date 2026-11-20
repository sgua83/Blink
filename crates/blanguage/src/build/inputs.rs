//! 一次构建持有的解析输入；跨次复用仍由正式对象／映像拥有。
use crate::{CapturedExport, ExportRequest, ast::{self, Diagnostic, Source, Span}, object::{Kind, Object}, parser, syntax};
use std::{collections::{BTreeMap, BTreeSet}, path::Path, sync::Arc};
pub(super) enum ProviderInterface<'a> {
    Unit(&'a Arc<syntax::Module>),
    Object(&'a Object),
}

struct SourceInput {
    interface: Arc<syntax::Module>,
    body: Option<CapturedBody>,
}
struct CapturedBody {
    implementation: Option<Arc<syntax::Module>>,
    imports: Arc<[String]>,
}
type Sources = BTreeMap<std::path::PathBuf, SourceInput>;
type Interfaces<T> = BTreeMap<String, BTreeMap<Arc<str>, T>>;
type CapturedInterface = (Arc<syntax::Module>, Option<Arc<[String]>>);

pub(super) struct CapturedUnit {
    interfaces: Interfaces<CapturedInterface>,
    exports: Option<Vec<ast::Result<CapturedExport>>>,
}
pub(crate) struct Inputs {
    sources: Arc<Sources>,
    unit: CapturedUnit,
}
#[derive(Default)]
pub(super) struct Capture {
    sources: Sources,
    interfaces: Interfaces<Arc<syntax::Module>>,
}
impl Capture {
    pub(super) fn finish(self, units: Vec<CapturedUnit>) -> Vec<Inputs> {
        let sources = Arc::new(self.sources);
        units.into_iter().map(|unit| Inputs { sources: sources.clone(), unit }).collect()
    }
    pub(super) fn header(&mut self, path: &Path) -> ast::Result<Arc<syntax::Module>> {
        Ok(self.source_input(path)?.interface.clone())
    }

    fn source_input(&mut self, path: &Path) -> ast::Result<&mut SourceInput> {
        use std::collections::btree_map::Entry;
        Ok(match self.sources.entry(crate::source_path::with_extension(path, u16::from(b'h'))) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let interface = Arc::new(parser::parse(crate::read(entry.key())?)?);
                entry.insert(SourceInput { interface, body: None })
            }
        })
    }

    pub(super) fn capture(
        &mut self, path: &Path, root: &Path, providers: &BTreeMap<&str, ProviderInterface<'_>>,
        requests: &[String],
    ) -> ast::Result<CapturedUnit> {
        let mut interfaces = Interfaces::new();
        let mut visited = BTreeSet::new();
        self.capture_source(path, root, providers, &mut visited, &mut interfaces)?;
        let exports = (!providers.is_empty() && !requests.is_empty()).then(|| {
            let mut captured = Vec::new();
            for text in requests {
                let request = match ExportRequest::parse(text) {
                    Ok(request) => request,
                    Err(error) => {
                        captured.push(Err(error));
                        break;
                    }
                };
                let mut owners = Vec::new();
                let error = request.visit_type_owners(|name| {
                    if providers.contains_key(name) && !visited.contains(name) {
                        self.capture_import(name, root, providers, &mut visited, &mut interfaces)?;
                        owners.push(name.to_owned());
                    }
                    Ok(())
                }).err();
                let failed = error.is_some();
                captured.push(Ok(CapturedExport { request, owners, error }));
                if failed { break; }
            }
            captured
        });
        Ok(CapturedUnit { interfaces, exports })
    }

    fn capture_source(
        &mut self, path: &Path, root: &Path, providers: &BTreeMap<&str, ProviderInterface<'_>>,
        visited: &mut BTreeSet<String>, interfaces: &mut Interfaces<CapturedInterface>,
    ) -> ast::Result<()> {
        let SourceInput { interface, body } = self.source_input(path)?;
        let imports = match body {
            Some(body) => &body.imports,
            None => {
                let implementation_path = crate::source_path::with_extension(path, u16::from(b'b'));
                let implementation = crate::read_implementation(&implementation_path)?;
                let imports = crate::names::module_imports(interface, implementation.as_deref());
                &body.insert(CapturedBody { implementation, imports }).imports
            }
        };
        // Different paths can declare the same module. Only import traversal
        // is deduplicated by module name; load still consumes each opened path.
        if !visited.insert(interface.name().to_owned()) { return Ok(()); }
        let imports = imports.clone();
        self.capture_imports(&imports, root, providers, visited, interfaces)
    }

    fn capture_imports(
        &mut self, imports: &[String], root: &Path, providers: &BTreeMap<&str, ProviderInterface<'_>>,
        visited: &mut BTreeSet<String>, interfaces: &mut Interfaces<CapturedInterface>,
    ) -> ast::Result<()> {
        for name in imports {
            self.capture_import(name, root, providers, visited, interfaces)?;
        }
        Ok(())
    }

    fn capture_import(
        &mut self, name: &str, root: &Path, providers: &BTreeMap<&str, ProviderInterface<'_>>,
        visited: &mut BTreeSet<String>, interfaces: &mut Interfaces<CapturedInterface>,
    ) -> ast::Result<()> {
        if visited.contains(name) { return Ok(()); }
        let interface = match providers.get(name) {
            Some(ProviderInterface::Unit(header)) => Arc::clone(header),
            Some(ProviderInterface::Object(object)) => {
                let Kind::Module { interface, .. } = &object.kind else {
                    // Imports reports invalid explicit providers when constructing the module inputs.
                    return Ok(());
                };
                self.capture_interface(name, interface)?
            }
            None => {
                let path = crate::source_path::module(root, name, u16::from(b'b'));
                return self.capture_source(&path, root, providers, visited, interfaces);
            }
        };
        let imports = visited.insert(interface.name().to_owned())
            .then(|| crate::names::module_imports(&interface, None));
        interfaces.entry(name.to_owned()).or_default()
            .insert(interface.source.text.clone(), (interface, imports.clone()));
        if let Some(imports) = imports {
            self.capture_imports(&imports, root, providers, visited, interfaces)?;
        }
        Ok(())
    }

    fn capture_interface(
        &mut self, name: &str, text: &Arc<str>,
    ) -> ast::Result<Arc<syntax::Module>> {
        let interfaces = self.interfaces.entry(name.into()).or_default();
        if let Some(module) = interfaces.get(text) { return Ok(module.clone()); }
        let module = Arc::new(parser::parse(Source {
            path: format!("<{name}>.h").into(), text: text.clone(),
        })?);
        interfaces.insert(module.source.text.clone(), module.clone());
        Ok(module)
    }
}
impl Inputs {
    pub(crate) fn exports(&self) -> Option<&[ast::Result<CapturedExport>]> {
        self.unit.exports.as_deref()
    }

    pub(crate) fn interface(&self, name: &str, text: &str) -> ast::Result<CapturedInterface> {
        self.unit.interfaces.get(name).and_then(|interfaces| interfaces.get(text)).cloned()
            .ok_or_else(|| Diagnostic {
                source: Source { path: format!("<{name}>.h").into(), text: "".into() },
                span: Span::default(), code: "B020",
                message: "接口不在本次已捕获的构建输入中".into(),
            })
    }
    pub(crate) fn source(
        &self, path: &Path,
    ) -> ast::Result<(Arc<syntax::Module>, Option<Arc<syntax::Module>>, Arc<[String]>)> {
        let missing = |path| Diagnostic {
            source: Source { path, text: "".into() },
            span: Span::default(), code: "B020",
            message: "模块不在本次已捕获的构建输入中".into(),
        };
        let input = self.sources.get(&crate::source_path::with_extension(path, u16::from(b'h'))).ok_or_else(|| missing(crate::source_path::with_extension(path, u16::from(b'h'))))?;
        let body = input.body.as_ref().ok_or_else(|| missing(crate::source_path::with_extension(path, u16::from(b'b'))))?;
        Ok((input.interface.clone(), body.implementation.clone(), body.imports.clone()))
    }
}
