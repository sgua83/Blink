//! 已编译模块公开接口、原生定义和实际产物；导入不读取实现源码。
pub(crate) use crate::codec;
pub(crate) mod domain;
use crate::{
    application::Resources,
    ir::FunctionBody,
    native,
    object::{self, Kind, Object, Symbol},
};
use std::{collections::BTreeMap, path::Path, sync::Arc};
pub(crate) struct Imported {
    pub header: String,
    pub definitions: Arc<BTreeMap<String, Vec<u8>>>,
    pub requirements: BTreeMap<String, crate::execution::Requirement>,
}
#[derive(Default)]
pub(crate) struct Imports {
    pub modules: BTreeMap<String, Imported>,
}
impl Imports {
    pub fn new<'a>(objects: impl IntoIterator<Item = &'a Object>) -> codec::Result<Self> {
        let mut imports = Self::default();
        for object in objects { imports.object(object)?; }
        Ok(imports)
    }
    pub(crate) fn inputs(inputs: &[object::Input]) -> codec::Result<Self> {
        let mut imports = Self::default();
        for input in inputs { input.imports(&mut imports)?; }
        Ok(imports)
    }
    pub(crate) fn object(&mut self, object: &Object) -> codec::Result<()> {
        self.insert(&object.name, &object.kind, object.symbols.iter()
            .map(|symbol| (symbol.name.clone(), symbol.contract.clone()))
            .chain(object.imports.iter().map(|(name,contract)|(name.clone(),contract.clone()))).collect())
    }
    pub(crate) fn insert(&mut self, name: &str, kind: &Kind, definitions: BTreeMap<String, Vec<u8>>) -> codec::Result<()> {
        let Kind::Module { interface, requirements, .. } = kind else {
            return Err(format!("{name} 不是独立模块产物").into());
        };
        if self.modules.insert(name.into(), Imported {
            header: interface.clone(), requirements: requirements.clone(), definitions: Arc::new(definitions),
        }).is_some() {
            return Err(format!("模块有多个提供者 {name}").into());
        }
        Ok(())
    }
}

pub struct Module {
    pub(crate) compiler: crate::Compiler,
    pub(crate) name: String,
    pub(crate) header: String,
    pub(crate) requirements: BTreeMap<String, crate::execution::Requirement>,
    pub(crate) functions: Vec<FunctionBody>,
    pub(crate) imports: BTreeMap<String, Vec<u8>>,
    pub(crate) exports: BTreeMap<String, Vec<u8>>,
    pub(crate) resources: Option<Resources>,
}
impl Module {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn compile(&self) -> codec::Result<Object> {
        self.compiler.execute(|worker| self.compile_in(worker).map_err(|error| error.to_string()))
            .map_err(|error| std::io::Error::other(error.to_string()).into())
    }
    pub(crate) fn compile_in(&self, worker: &crate::compiler::Worker) -> codec::Result<Object> {
        let mut code = crate::parallel::try_map(
            worker, self.functions.iter().collect(), |_| (),
            |_, function| native::emit_function(function).map(|code| (function.name.clone(), code)),
        ).map_err(|error| std::io::Error::other(error.to_string()))?;
        let (archive, resource_code, resource_imports) =
            crate::application::compile_resources(&self.name, self.resources.as_ref(), false, worker)?;
        let resource_names: Vec<_> = resource_code.iter().map(|(name, _)| name.clone()).collect();
        code.extend(resource_code);
        let mut imports = self.imports.clone();
        imports.extend(resource_imports);
        let stack_bytes = code
            .iter()
            .map(|(_, code)| code.stack_bytes)
            .max()
            .unwrap_or(0);
        let (sections, positions, stack) = object::module_code(
            code,
            &imports,
            &crate::application::resource_prefix(&self.name),
        )?;
        let symbols = self
            .exports
            .iter()
            .map(|(name, contract)| Symbol {
                name: name.clone(),
                section: positions[name].0,
                value: positions[name].1,
                contract: contract.clone(),
                function: true,
            })
            .chain(resource_names.into_iter().map(|name| Symbol {
                section: positions[&name].0, value: positions[&name].1,
                name, contract: Vec::new(), function: true,
            }))
            .collect();
        Ok(Object {
            name: self.name.clone(),
            kind: Kind::Module {
                interface: self.header.clone(),
                requirements: self.requirements.clone(),
                stack_bytes,
            },
            sections,
            stack,
            symbols,
            imports,
            archive,
        })
    }
}
/// 编译一个普通原生模块；dependencies 是实际已编译模块，instances 选择开放泛型的具体导出。
pub fn compile(
    path: &Path,
    module_root: &Path,
    dependencies: &[Object],
    instances: &[String],
) -> crate::ast::Result<Module> {
    let compiler = crate::Compiler::available().map_err(|error| crate::compiler::diagnostic(path, error))?;
    compiler.compile_native_module(path, module_root, dependencies, instances)
}
impl crate::Compiler {
    pub fn compile_native_module(
        &self, path: &Path, module_root: &Path, dependencies: &[Object], instances: &[String],
    ) -> crate::ast::Result<Module> {
        self.ast(path, |worker| {
            let imports = Imports::new(dependencies).map_err(|error| crate::ast::Diagnostic {
                source: crate::ast::Source { path: path.into(), text: "".into() },
                span: crate::ast::Span::default(), code: "B270", message: error.to_string(),
            })?;
            let imported = if dependencies.is_empty() { None } else { Some(&imports) };
            let (world, entry) = crate::read_world(path, module_root, imported, instances, worker, None, None)?;
            crate::check::compile_native_module(&world, &entry, instances, imported)
        })
    }
}
