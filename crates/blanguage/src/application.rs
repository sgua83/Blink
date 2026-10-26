//! Windows x64 应用的原生启动、PE 链接及平台导入；不携带源码、IR或解释器。
mod pe;
use crate::{ir::ValueCode, native};
use std::path::Path;
pub struct Application {
    pub(crate) name: String,
    pub(crate) body: ValueCode,
}
impl Application {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn build(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let code = native::emit_application(&self.body).map_err(std::io::Error::other)?;
        let output = std::path::absolute(path)?;
        std::fs::create_dir_all(output.parent().unwrap())?;
        std::fs::write(output, pe::link(code)?)?;
        Ok(())
    }
}
