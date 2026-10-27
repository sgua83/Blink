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
    pub(crate) name: String,
    pub(crate) body: ValueCode,
    pub(crate) functions: Vec<crate::ir::FunctionBody>,
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
        let code = native::emit_application(&self.body).map_err(std::io::Error::other)?;
        let stack_bytes = code.code.stack_bytes;
        let functions = self
            .functions
            .iter()
            .map(|function| {
                native::emit_function(function).map(|code| (function.name.clone(), code))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(std::io::Error::other)?;
        let sections = object::application_code(code, functions)?;
        let archive = if let Some(resources) = &self.resources {
            let source = crate::dll::application_source(resources)?;
            let mut exports = [
                "blink_create",
                "blink_destroy",
                "blink_release",
                "blink_upload",
                "blink_download",
                "blink_address",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
            exports.extend((0..resources.types.len()).map(|i| format!("blink_allocate_{i}")));
            exports.extend((0..resources.programs.len()).map(|i| format!("blink_run_{i}")));
            Some(object::compile_archive(&source, exports)?)
        } else {
            None
        };
        Ok(Object {
            name: self.name.clone(),
            kind: Kind::Application { stack_bytes },
            sections,
            archive,
        })
    }
    pub fn write_object(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        self.compile()?.write(path)
    }
}
