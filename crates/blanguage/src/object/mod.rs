//! 编译完成的 Windows x64 COFF 目标文件；机器码、重定位和运行库均在编译阶段确定。
mod archive;
mod static_library;
pub use static_library::{Input, archive};
pub(crate) use static_library::{Member as LibraryMember, members as library_members, encode as library_bytes};
pub(crate) use static_library::Library;
mod coff;
pub(crate) use coff::encode_native;
pub(crate) mod stack;
pub(crate) use coff::Reader as ObjectReader;
mod emit;
pub(crate) use archive::{compile as compile_archive, inputs as native_inputs};
#[cfg(feature = "installed")]
pub(crate) use archive::sdk_root;
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
    pub associated: Vec<usize>,
    pub layout: Option<coff::facts::observations::Layout>,
}
pub(crate) struct Symbol {
    pub name: String,
    pub section: usize,
    pub value: u32,
    pub contract: Vec<u8>,
    pub function: bool,
}
pub(crate) enum Target {
    Symbol(String),
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
    pub code: Vec<String>,
}
pub(crate) struct Archive {
    pub bytes: std::sync::Arc<Vec<u8>>,
    pub libraries: Vec<String>,
    pub exports: Vec<String>,
    pub resources: Option<ResourceBinding>,
}
#[derive(Clone)]
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
    pub(crate) imports: std::collections::BTreeMap<String, Vec<u8>>,
    pub(crate) archive: Option<Archive>,
    pub(crate) stack: Vec<stack::Frame>,
}
impl Object {
    pub fn stack_report(&self, budget: Option<u64>) -> String { stack::report(self, budget, None) }
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

pub use coff::facts::{CodeSelection, code_facts, code_facts_with_observations, source_facts};

impl Object {
    /// Captures original generated code ranges as independent material, without compiling code.
    /// This explicit request uses one temporary COFF serialization; execution bytes stay unchanged.
    /// A restored or cache-only Object without retained producer facts returns an error.
    pub fn code_observations(&self) -> Result<Vec<u8>> {
        coff::facts::observations::capture(self)
    }
}
