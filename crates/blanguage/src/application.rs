//! Windows x64 应用的原生启动、PE 链接及平台导入；不携带源码、IR或解释器。
mod pe;
pub(crate) const RUNTIME_LIBRARY: &str = "@blink.application@";
use crate::{
    ast::Target,
    ir::{Program, Ty, ValueCode},
    native,
};
use std::path::Path;
pub struct Application {
    pub(crate) name: String,
    pub(crate) body: ValueCode,
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
    pub fn build(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let mut code = native::emit_application(&self.body).map_err(std::io::Error::other)?;
        let output = std::path::absolute(path)?;
        std::fs::create_dir_all(output.parent().unwrap())?;
        if let Some(resources) = &self.resources {
            use std::hash::{Hash, Hasher};
            let source = crate::dll::application_source(resources)?;
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            output.file_name().hash(&mut hash);
            source.hash(&mut hash);
            let prefix = format!("blink_app_{:016x}", hash.finish());
            let library = format!("{prefix}.dll");
            crate::dll::link_rust(&source, &prefix, &output.with_file_name(&library), false)?;
            for relocation in &mut code.code.relocations {
                if let native::NativeSymbol::External(name) = &mut relocation.target
                    && name.library == RUNTIME_LIBRARY
                {
                    name.library = library.clone();
                }
            }
        }
        std::fs::write(output, pe::link(code)?)?;
        Ok(())
    }
}
