//! 已编译模块公开接口、原生定义和实际产物；导入不读取实现源码。
pub(crate) mod codec;
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
    pub fn new(objects: &[Object]) -> codec::Result<Self> {
        let mut imports = Self::default();
        for object in objects {
            let Kind::Module {
                interface,
                requirements,
                ..
            } = &object.kind
            else {
                return Err(format!("{} 不是独立模块产物", object.name).into());
            };
            let definitions = object
                .symbols
                .iter()
                .map(|symbol| (symbol.name.clone(), symbol.contract.clone()))
                .collect();
            if imports
                .modules
                .insert(
                    object.name.clone(),
                    Imported {
                        header: interface.clone(),
                        requirements: requirements.clone(),
                        definitions: Arc::new(definitions),
                    },
                )
                .is_some()
            {
                return Err(format!("模块有多个提供者 {}", object.name).into());
            }
        }
        Ok(imports)
    }
}
pub struct Module {
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
        let code = self
            .functions
            .iter()
            .map(|function| {
                native::emit_function(function).map(|code| (function.name.clone(), code))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(std::io::Error::other)?;
        let stack_bytes = code
            .iter()
            .map(|(_, code)| code.code.stack_bytes)
            .max()
            .unwrap_or(0);
        let (sections, positions) = object::module_code(
            code,
            &self.imports,
            &crate::application::resource_prefix(&self.name),
        )?;
        let symbols = self
            .exports
            .iter()
            .map(|(name, contract)| Symbol {
                name: name.clone(),
                section: 0,
                value: positions[name],
                contract: contract.clone(),
                function: true,
            })
            .collect();
        let archive =
            crate::application::compile_resources(&self.name, self.resources.as_ref(), false)?;
        Ok(Object {
            name: self.name.clone(),
            kind: Kind::Module {
                interface: self.header.clone(),
                requirements: self.requirements.clone(),
                stack_bytes,
            },
            sections,
            symbols,
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
    let imports = Imports::new(dependencies).map_err(|error| crate::ast::Diagnostic {
        source: crate::ast::Source {
            path: path.into(),
            text: "".into(),
        },
        span: crate::ast::Span::default(),
        code: "B270",
        message: error.to_string(),
    })?;
    let imported=if dependencies.is_empty(){None}else{Some(&imports)};
    let (world, entry) = crate::read_world(path, module_root, imported)?;
    crate::validate_types(&world)?;
    crate::check::compile_native_module(&world, &entry, instances)
}
