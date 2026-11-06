//! Windows x64 应用编译为 COFF 目标文件；PE 链接由独立 linker 模块负责。
pub(crate) const RUNTIME_LIBRARY: &str = "@blink.application@";
use crate::{
    ast::Target,
    ir::{Program, Ty, ValueCode},
    native,
    object::{self, Kind, Object},
};
use std::path::Path;
pub struct Application {
    pub(crate) compiler: crate::Compiler,
    pub(crate) name: String,
    pub(crate) body: ValueCode,
    pub(crate) functions: Vec<crate::ir::FunctionBody>,
    pub(crate) imports: std::collections::BTreeMap<String, Vec<u8>>,
    pub(crate) resources: Option<Resources>,
}
#[derive(Default)]
pub(crate) struct Resources {
    pub types: Vec<Ty>,
    pub programs: Vec<(Target, Program)>,
}
impl Application {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn compile(&self) -> Result<Object, Box<dyn std::error::Error>> {
        self.compiler.execute(|worker| self.compile_in(worker).map_err(|error| error.to_string()))
            .map_err(|error| std::io::Error::other(error.to_string()).into())
    }
    pub(crate) fn compile_in(&self, worker: &crate::compiler::Worker) -> Result<Object, Box<dyn std::error::Error>> {
        let code = native::emit_application(&self.body).map_err(std::io::Error::other)?;
        let mut stack_bytes = code.stack_bytes;
        let mut functions = crate::parallel::try_map(worker, self.functions.iter().collect(), |_| (),
            |_, function| native::emit_function(function).map(|code| (function.name.clone(), code)))
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let (archive, resource_code, resource_imports) = compile_resources(&self.name, self.resources.as_ref(), true, worker)?;
        let resource_names: Vec<_> = resource_code.iter().map(|(name, _)| name.clone()).collect();
        functions.extend(resource_code);
        let mut imports = self.imports.clone();
        imports.extend(resource_imports);
        for (_, function) in &functions {
            stack_bytes = stack_bytes.max(function.stack_bytes);
        }
        let (sections, positions, stack) =
            object::application_code(code, functions, &imports, &resource_prefix(&self.name), &self.name)?;
        let symbols = std::iter::once("main".to_owned()).chain(resource_names).map(|name| object::Symbol {
            section: positions[&name].0, value: positions[&name].1,
            name, contract: Vec::new(), function: true,
        }).collect();
        Ok(Object {
            name: self.name.clone(),
            kind: Kind::Application { stack_bytes },
            sections,
            stack,
            symbols,
            imports,
            archive,
        })
    }
    pub fn write_object(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        self.compile()?.write(path)
    }
}

pub(crate) fn resource_prefix(name: &str) -> String {
    use std::fmt::Write;
    let mut prefix = String::from("blink_m");
    for byte in name.bytes() {
        write!(prefix, "{byte:02x}").unwrap();
    }
    prefix
}
pub(crate) fn compile_resources(
    name: &str,
    resources: Option<&Resources>,
    root: bool,
    worker: &crate::compiler::Worker,
) -> Result<(Option<object::Archive>, Vec<(String, native::NativeCode)>, std::collections::BTreeMap<String, Vec<u8>>), Box<dyn std::error::Error>> {
    Ok(if let Some(resources) = resources {
        let prefix = resource_prefix(name);
        let (image, code, imports) = crate::resource::compile(resources, worker)?;
        let code: Vec<_> = code.into_iter().enumerate()
            .map(|(index, code)| (format!("__blink_range_{prefix}_{index}"), code)).collect();
        let source = crate::dll::application_source(resources, &prefix, root, &image, &code)?;
        let mut exports = if root {
            [
                "blink_destroy",
                "blink_release",
                "blink_upload",
                "blink_download",
                "blink_address",
                "blink_storage_address",
                "blink_allocate",
                "blink_resize",
                "blink_run",
                "blink_error_message",
                "blink_error_destroy",
                "blink_error_join",
                "blink_application_failed",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        exports.extend((0..resources.types.len()).map(|i| format!("{prefix}_allocate_{i}")));
        exports.extend((0..resources.programs.len()).map(|i| format!("{prefix}_run_{i}")));
        if root {
            exports.extend(["blink_create".into(), "blink_application_error".into()]);
        }
        exports.push(format!("{prefix}_resources"));
        let dependencies: &[&str] = if root {
            &["blanguage", "blcompute"]
        } else {
            &[]
        };
        let mut archive = object::compile_archive(&source, exports, dependencies, worker)?;
        archive.resources = Some(object::ResourceBinding {
            prefix,
            types: u32::try_from(resources.types.len())?,
            programs: u32::try_from(resources.programs.len())?,
            abi: crate::resource::ABI.to_vec(),
            code: code.iter().map(|(name, _)| name.clone()).collect(),
        });
        (Some(archive), code, imports)
    } else {
        (None, Vec::new(), std::collections::BTreeMap::new())
    })
}
