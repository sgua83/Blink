//! 一次构建持有的解析输入；跨次复用仍由正式对象／映像拥有。
use crate::{ast::{self, Diagnostic, Source, Span}, parser};
use std::{collections::{BTreeMap, BTreeSet}, path::Path, sync::Arc};
#[derive(Default)]
pub(crate) struct Inputs {
    files: BTreeMap<std::path::PathBuf, Option<Arc<ast::Module>>>,
    interfaces: BTreeMap<String, BTreeMap<Arc<str>, Arc<ast::Module>>>,
}
impl Inputs {
    fn file(&mut self, path: &Path, optional: bool) -> ast::Result<Option<Arc<ast::Module>>> {
        if !self.files.contains_key(path) {
            let present = !optional || path.try_exists().map_err(|error| Diagnostic {
                source: Source { path: path.into(), text: "".into() },
                span: Span::default(), code: "B020", message: format!("无法访问实现源：{error}"),
            })?;
            let module = if present { Some(Arc::new(parser::parse(crate::read(path)?)?)) } else { None };
            self.files.insert(path.into(), module);
        }
        Ok(self.files[path].clone())
    }
    pub(super) fn header(&mut self, path: &Path) -> ast::Result<Arc<ast::Module>> {
        self.file(&path.with_extension("h"), false)?;
        self.read(&path.with_extension("h"))
    }
    pub(super) fn capture(
        &mut self, path: &Path, root: &Path, providers: &BTreeSet<String>,
        imported: bool, visited: &mut BTreeSet<String>,
    ) -> ast::Result<()> {
        let interface = self.header(path)?;
        if !visited.insert(interface.name.clone()) { return Ok(()); }
        let implementation = if imported && !providers.is_empty() { None }
            else { self.file(&path.with_extension("b"), true)? };
        let imports = interface.imports.iter()
            .chain(implementation.iter().flat_map(|module| &module.imports))
            .collect::<BTreeSet<_>>();
        for name in imports {
            if !providers.contains(name) {
                self.capture(&root.join(name.replace('.', "/")).with_extension("b"),
                    root, providers, true, visited)?;
            }
        }
        Ok(())
    }
    pub(super) fn capture_interface(
        &mut self, name: &str, text: &str, parsed: Option<&Arc<ast::Module>>,
    ) -> ast::Result<Arc<ast::Module>> {
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
    pub(crate) fn interface(&self, name: &str, text: &str) -> ast::Result<Arc<ast::Module>> {
        self.interfaces.get(name).and_then(|interfaces| interfaces.get(text)).cloned()
            .ok_or_else(|| Diagnostic {
                source: Source { path: format!("<{name}>.h").into(), text: "".into() },
                span: Span::default(), code: "B020",
                message: "接口不在本次已捕获的构建输入中".into(),
            })
    }
    pub(crate) fn read(&self, path: &Path) -> ast::Result<Arc<ast::Module>> {
        self.files.get(path).and_then(|module| module.clone()).ok_or_else(|| Diagnostic {
            source: Source { path: path.into(), text: "".into() },
            span: Span::default(), code: "B020",
            message: "模块不在本次已捕获的构建输入中".into(),
        })
    }
    pub(crate) fn present(&self, path: &Path) -> bool {
        self.files[path].is_some()
    }
}
