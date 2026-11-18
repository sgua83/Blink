//! 编译完成的 Windows x64 COFF 目标文件；机器码、重定位和运行库均在编译阶段确定。
mod archive;
mod static_library;
pub use static_library::{Input, archive};
pub(crate) use static_library::{Member as LibraryMember, members as library_members, encode as library_bytes};
pub(crate) use static_library::{Library, ObjectBytes};
mod coff;
pub(crate) use coff::{encode_native, rust_link_binding};
pub(crate) mod stack;
pub(crate) use coff::Reader as ObjectReader;
mod emit;
pub(crate) use archive::{compile as compile_archive, inputs as native_inputs};
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub(crate) use archive::{SDK_RUNTIME, hold_sdk, installed_sdk_root};
#[cfg(feature = "installed")]
pub(crate) use archive::sdk_root;
pub(crate) use emit::{application as application_code, function_sections, module as module_code};
use std::{path::Path, sync::Arc};
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Import {
    pub dll: String,
    pub name: String,
}
pub(crate) struct Section<Bytes = Vec<u8>> {
    pub name: String,
    pub bytes: Bytes,
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
// COFF stack recovery borrows the offset at the start of each owning record.
#[repr(C)]
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
    pub bytes: ObjectBytes,
    pub libraries: Vec<String>,
    pub exports: Vec<String>,
    pub resources: Option<ResourceBinding>,
}

impl Archive {
    pub(crate) fn local_libraries_available(&self) -> std::io::Result<bool> {
        for library in &self.libraries {
            let path = Path::new(library);
            if path.is_absolute() && !path.try_exists()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
#[derive(Clone)]
pub(crate) enum Kind {
    Module {
        interface: Arc<str>,
        requirements: Arc<std::collections::BTreeMap<String, crate::execution::Requirement>>,
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
pub struct Object<Bytes = Vec<u8>> {
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) sections: Vec<Section<Bytes>>,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) imports: std::collections::BTreeMap<String, Vec<u8>>,
    pub(crate) archive: Option<Archive>,
    pub(crate) stack: Vec<stack::Frame>,
}
impl Object {
    pub fn stack_report(&self, budget: Option<u64>) -> String { stack::report(self, budget, None, stack::uses_runtime(self)) }
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            crate::file_bytes::create_dir_all(parent)?;
        }
        crate::file_bytes::write(path, &self.coff_bytes()?)?;
        Ok(())
    }
    pub(crate) fn coff_bytes(&self) -> Result<Vec<u8>> {
        coff::encode(self)
    }
    pub fn read(path: &Path) -> Result<Self> {
        coff::decode(&ObjectBytes::read(path)?)
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
