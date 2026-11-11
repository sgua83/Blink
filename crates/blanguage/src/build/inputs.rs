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
        let input = match self.sources.entry(path.with_extension("h")) {
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
        let input = self.sources.get_mut(&path.with_extension("h")).unwrap();
        if input.body.is_none() {
            let implementation_path = path.with_extension("b");
            let present = implementation_path.try_exists().map_err(|error| Diagnostic {
                source: Source { path: implementation_path.clone(), text: "".into() },
                span: Span::default(), code: "B020", message: format!("无法访问实现源：{error}"),
            })?;
            let implementation = if present {
                Some(Arc::new(parser::parse(crate::read(&implementation_path)?)?))
            } else {
                None
            };
            let imports = crate::names::module_imports(&interface, implementation.as_deref());
            input.body = Some(CapturedBody { implementation, imports });
        }
        let imports = input.body.as_ref().unwrap().imports.clone();
        for name in imports.iter() {
            if !providers.contains(name) {
                self.capture(&root.join(name.replace('.', "/")).with_extension("b"),
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
        let input = self.sources.get(&path.with_extension("h")).ok_or_else(|| missing(path.with_extension("h")))?;
        let body = input.body.as_ref().ok_or_else(|| missing(path.with_extension("b")))?;
        Ok((input.interface.clone(), body.implementation.clone(), body.imports.clone()))
    }
}
