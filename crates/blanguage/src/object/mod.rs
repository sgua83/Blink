//! 编译完成的 Windows x64 COFF 目标文件；机器码、重定位和运行库均在编译阶段确定。
mod archive;
mod static_library;
pub use static_library::{Input, archive};
mod coff;
pub(crate) use coff::Reader as ObjectReader;
mod emit;
pub(crate) use archive::{compile as compile_archive, inputs as native_inputs};
pub(crate) use emit::{application as application_code, module as module_code};
use std::path::Path;
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Import {
    pub dll: String,
    pub name: String,
}
pub(crate) struct Section {
    pub name: String,
    pub bytes: Vec<u8>,
    pub flags: u32,
    pub relocations: Vec<Relocation>,
}
pub(crate) struct Symbol {
    pub name: String,
    pub section: usize,
    pub value: u32,
    pub contract: Vec<u8>,
    pub function: bool,
}
pub(crate) enum Target {
    Symbol(String, Vec<u8>),
    Section(usize, u32),
    Import(Import),
}
pub(crate) struct Relocation {
    pub offset: usize,
    pub kind: u16,
    pub target: Target,
}
pub(crate) struct ResourceBinding {
    pub prefix: String,
    pub types: u32,
    pub programs: u32,
    pub abi: Vec<u8>,
}
pub(crate) struct Archive {
    pub bytes: Vec<u8>,
    pub libraries: Vec<String>,
    pub exports: Vec<String>,
    pub resources: Option<ResourceBinding>,
}
pub(crate) enum Kind {
    Module {
        interface: String,
        requirements: std::collections::BTreeMap<String, crate::execution::Requirement>,
        stack_bytes: u32,
    },
    Application {
        stack_bytes: u32,
    },
    Library {
        prefix: String,
        header: String,
        binding: String,
    },
}
pub struct Object {
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) sections: Vec<Section>,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) archive: Option<Archive>,
}
impl Object {
    pub fn write(&self, path: &Path) -> Result<()> {
        let path = std::path::absolute(path)?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, coff::encode(self)?)?;
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self> {
        coff::decode(&std::fs::read(path)?)
    }
    pub(crate) fn build_bytes(&self, inputs: &[u8]) -> Result<Vec<u8>> {
        coff::encode_build(self, Some(inputs))
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}
