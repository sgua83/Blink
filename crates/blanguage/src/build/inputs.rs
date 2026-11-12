//! 一次构建持有的解析输入；跨次复用仍由正式对象／映像拥有。
use crate::{ast::{self, Diagnostic, Source, Span}, parser, syntax};
use std::{collections::{BTreeMap, BTreeSet}, path::Path, sync::Arc};
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
        use std::collections::btree_map::Entry;
        let input = match self.sources.entry(crate::source_path::with_extension(path, u16::from(b'h'))) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let interface = Arc::new(parser::parse(crate::read(entry.key())?)?);
                entry.insert(SourceInput { interface, body: None })
            }
        };
        Ok(input.interface.clone())
    }
    pub(super) fn capture(
        &mut self, path: &Path, root: &Path, providers: &BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> ast::Result<()> {
        let interface = self.header(path)?;
        if !visited.insert(interface.name().to_owned()) { return Ok(()); }
        let input = self.sources.get_mut(&crate::source_path::with_extension(path, u16::from(b'h'))).unwrap();
        if input.body.is_none() {
            let implementation_path = crate::source_path::with_extension(path, u16::from(b'b'));
            let implementation = crate::read_implementation(&implementation_path)?;
            let imports = crate::names::module_imports(&interface, implementation.as_deref());
            input.body = Some(CapturedBody { implementation, imports });
        }
        let imports = input.body.as_ref().unwrap().imports.clone();
        for name in imports.iter() {
            if !providers.contains(name) {
                self.capture(&crate::source_path::module(root, name, u16::from(b'b')),
                    root, providers, visited)?;
            }
        }
        Ok(())
    }
    pub(super) fn capture_interface(
        &mut self, name: &str, text: &str, parsed: Option<&Arc<syntax::Module>>,
    ) -> ast::Result<Arc<syntax::Module>> {
        let interfaces = self.interfaces.entry(name.into()).or_default();
        if let Some(module) = interfaces.get(text) { return Ok(module.clone()); }
        let path = format!("<{name}>.h").into();
        let module = if let Some(parsed) = parsed {
            let mut module = parsed.as_ref().clone();
            module.source.path = path;
            module
        } else { parser::parse(Source { path, text: text.into() })? };
        let module = Arc::new(module);
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
