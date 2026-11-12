//! 一次构建持有的解析输入；跨次复用仍由正式对象／映像拥有。
use crate::{ast::{self, Diagnostic, Source, Span}, object::{Kind, Object}, parser, syntax};
use std::{collections::{BTreeMap, BTreeSet}, path::Path, sync::Arc};
pub(super) enum ProviderInterface<'a> {
    Unit(Arc<syntax::Module>),
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
#[derive(Default)]
pub(crate) struct Inputs {
    sources: BTreeMap<std::path::PathBuf, SourceInput>,
    interfaces: BTreeMap<String, BTreeMap<Arc<str>, Arc<syntax::Module>>>,
}
impl Inputs {
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
        &mut self, path: &Path, root: &Path, providers: &BTreeMap<String, ProviderInterface<'_>>,
    ) -> ast::Result<()> {
        self.capture_source(path, root, providers, &mut BTreeSet::new())
    }

    fn capture_source(
        &mut self, path: &Path, root: &Path, providers: &BTreeMap<String, ProviderInterface<'_>>,
        visited: &mut BTreeSet<String>,
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
        self.capture_imports(&imports, root, providers, visited)
    }

    fn capture_imports(
        &mut self, imports: &[String], root: &Path, providers: &BTreeMap<String, ProviderInterface<'_>>,
        visited: &mut BTreeSet<String>,
    ) -> ast::Result<()> {
        for name in imports {
            if visited.contains(name) {
                continue;
            }
            let interface = match providers.get(name) {
                Some(ProviderInterface::Unit(header)) => {
                    self.capture_interface(name, &header.source.text, Some(header))?
                }
                Some(ProviderInterface::Object(object)) => {
                    let Kind::Module { interface, .. } = &object.kind else {
                        // Imports reports invalid explicit providers when constructing the module inputs.
                        continue;
                    };
                    self.capture_interface(name, interface, None)?
                }
                None => {
                    let path = crate::source_path::module(root, name, u16::from(b'b'));
                    self.capture_source(&path, root, providers, visited)?;
                    continue;
                }
            };
            if visited.insert(interface.name().to_owned()) {
                let imports = crate::names::module_imports(&interface, None);
                self.capture_imports(&imports, root, providers, visited)?;
            }
        }
        Ok(())
    }

    fn capture_interface(
        &mut self, name: &str, text: &str, parsed: Option<&Arc<syntax::Module>>,
    ) -> ast::Result<Arc<syntax::Module>> {
        let interfaces = self.interfaces.entry(name.into()).or_default();
        if let Some(module) = interfaces.get(text) { return Ok(module.clone()); }
        let module = if let Some(parsed) = parsed { parsed.clone() }
        else { Arc::new(parser::parse(Source {
            path: format!("<{name}>.h").into(), text: text.into(),
        })?) };
        interfaces.insert(module.source.text.clone(), module.clone());
        Ok(module)
    }
    pub(crate) fn interface(&self, name: &str, text: &str) -> ast::Result<Arc<syntax::Module>> {
        self.interfaces.get(name).and_then(|interfaces| interfaces.get(text)).cloned()
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
