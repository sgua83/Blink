//! 已编译模块公开接口、原生定义和实际产物；导入不读取实现源码。
pub(crate) use crate::codec;
pub(crate) mod domain;
use crate::{
    application::Resources,
    ir::FunctionBody,
    object::{self, Definition, Kind, Object, Symbol},
};
use std::{collections::BTreeMap, path::Path, sync::Arc};
pub(crate) struct Imported {
    pub header: Arc<str>,
    pub definitions: Arc<BTreeMap<String, Vec<u8>>>,
    pub requirements: Arc<BTreeMap<String, crate::execution::Requirement>>,
}
/// The compiled declarations and contracts consumed by source compilation.
pub struct CompiledInterface {
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) definitions: BTreeMap<String, Vec<u8>>,
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
    pub(crate) fn interfaces(interfaces: Vec<CompiledInterface>) -> codec::Result<Self> {
        let mut imports = Self::default();
        for interface in interfaces {
            imports.interface(interface)?;
        }
        Ok(imports)
    }
    pub(crate) fn interface(&mut self, interface: CompiledInterface) -> codec::Result<()> {
        self.insert(interface.name, interface.kind, interface.definitions)
    }
    pub(crate) fn object(&mut self, object: &Object) -> codec::Result<()> {
        self.insert(object.name.clone(), object.kind.clone(), object.symbols.iter()
            .map(|symbol| (symbol.definition.name.clone(), symbol.contract.clone()))
            .chain(object.imports.iter().map(|(name,contract)|(name.clone(),contract.clone()))).collect())
    }
    pub(crate) fn insert(&mut self, name: String, kind: Kind, definitions: BTreeMap<String, Vec<u8>>) -> codec::Result<()> {
        let Kind::Module { interface, requirements, .. } = kind else {
            return Err(format!("{name} 不是独立模块产物").into());
        };
        match self.modules.entry(name) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(Imported { header: interface, requirements, definitions: Arc::new(definitions) });
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                Err(format!("模块有多个提供者 {}", entry.key()).into())
            }
        }
    }
}

pub struct Module {
    pub(crate) compiler: crate::Compiler,
    pub(crate) name: String,
    pub(crate) header: Arc<str>,
    pub(crate) requirements: Arc<BTreeMap<String, crate::execution::Requirement>>,
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
        let (functions, task_types) = crate::application::prepare_functions(
            &self.name, None, &self.functions, &self.imports, self.resources.as_ref(),
        ).map_err(std::io::Error::other)?;
        let mut code = crate::parallel::try_map(
            worker, functions, |_| (), |_, function| function.emit(),
        ).map_err(|error| std::io::Error::other(error.to_string()))?;
        let (compiled_resources, resource_code) =
            crate::application::compile_resources(&self.name, self.resources.as_ref(), &task_types, false, worker, None)?;
        let factory_entry_count = resource_code.len();
        let resource_names: Vec<_> = resource_code.iter().map(|(name, _)| name.clone()).collect();
        code.extend(resource_code);
        let imports = self.imports.clone();
        let stack_bytes = code
            .iter()
            .map(|(_, code)| code.stack_bytes)
            .max()
            .unwrap_or(0);
        let emitted = object::module_code(
            code, factory_entry_count, &imports, &crate::application::resource_prefix(&self.name),
        )?;
        let positions = emitted.positions;
        let symbols = self
            .exports
            .iter()
            .map(|(name, contract)| Symbol {
                definition: Definition {
                    name: name.clone(), section: positions[name].0, value: positions[name].1, function: true,
                },
                contract: contract.clone(),
            })
            .chain(resource_names.into_iter().map(|name| Symbol {
                definition: Definition {
                    section: positions[&name].0, value: positions[&name].1, name, function: true,
                },
                contract: Vec::new(),
            }))
            .collect();
        let (archive, factory) = compiled_resources.unzip();
        let mut object = Object {
            name: self.name.clone(),
            kind: Kind::Module {
                interface: self.header.clone(),
                requirements: self.requirements.clone(),
                stack_bytes,
            },
            sections: emitted.sections,
            stack: emitted.stack,
            symbols,
            imports,
            archive,
        };
        if let Some(factory) = factory {
            factory.emit(&mut object, emitted.factory_entries);
        }
        Ok(object)
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
        self.compile_native_module_importing(path, module_root, instances, || Imports::new(dependencies))
    }

    /// Compiles a native module against the declared interfaces of compiled providers.
    pub fn compile_native_module_with_interfaces(
        &self, path: &Path, module_root: &Path, interfaces: Vec<CompiledInterface>, instances: &[String],
    ) -> crate::ast::Result<Module> {
        self.compile_native_module_importing(path, module_root, instances, || Imports::interfaces(interfaces))
    }

    fn compile_native_module_importing(
        &self, path: &Path, module_root: &Path, instances: &[String],
        load: impl FnOnce() -> codec::Result<Imports> + Send,
    ) -> crate::ast::Result<Module> {
        self.ast(path, |worker| {
            let imports = load().map_err(|error| crate::ast::Diagnostic {
                source: crate::ast::Source { path: path.into(), text: "".into() },
                span: crate::ast::Span::default(), code: "B270", message: error.to_string(),
            })?;
            let imported = (!imports.modules.is_empty()).then_some(&imports);
            let (world, entry, requests) = crate::read_world(path, module_root, imported, instances, worker, None, None)?;
            unsafe { world.run(crate::check::compile_native_module(&world, &entry, requests, imported)) }
        })
    }
}
