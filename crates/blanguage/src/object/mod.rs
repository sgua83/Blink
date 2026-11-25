//! 编译完成的 Windows x64 COFF 目标文件；机器码、重定位和运行库均在编译阶段确定。
mod archive;
mod static_library;
pub use static_library::{Input, archive};
pub(crate) use static_library::{ArchiveSymbol, HostMember, member_sources as library_member_sources, encode as library_bytes};
pub(crate) use static_library::{Library, ObjectBytes};
mod coff;
pub(crate) use coff::{ImportKey, encode_native, encode_native_with_imports, native_import_member};
pub(crate) mod stack;
pub(crate) use coff::Reader as ObjectReader;
mod emit;
pub(crate) use archive::{compile as compile_archive, runtime_fingerprint, runtime_libraries};
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub(crate) use archive::{InstalledSdk, SDK_RUNTIME};
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
impl<Bytes> Section<Bytes> {
    pub(crate) fn map_bytes<Output>(self, map: impl FnOnce(Bytes) -> Output) -> Section<Output> {
        let Self { name, bytes, flags, relocations, associated, layout } = self;
        Section { name, bytes: map(bytes), flags, relocations, associated, layout }
    }
}

pub(crate) trait ReadSection: Sized {
    fn from_coff(source: &ObjectBytes, bytes: &[u8], zero_fill: usize) -> Self;
    fn from_generated_bytes(bytes: Vec<u8>) -> Self;
    fn from_generated(object: Object) -> Object<Self> {
        let Object { name, kind, sections, symbols, imports, archive, stack } = object;
        let sections = sections.into_iter().map(|section| section.map_bytes(Self::from_generated_bytes)).collect();
        Object { name, kind, sections, symbols, imports, archive, stack }
    }
}

impl ReadSection for Vec<u8> {
    fn from_coff(_: &ObjectBytes, bytes: &[u8], zero_fill: usize) -> Self {
        if zero_fill == 0 { bytes.to_vec() } else { vec![0; zero_fill] }
    }

    fn from_generated_bytes(bytes: Vec<u8>) -> Self { bytes }
    fn from_generated(object: Object) -> Object<Self> { object }
}

// The report only needs the original logical length for section layout.
impl ReadSection for usize {
    fn from_coff(_: &ObjectBytes, bytes: &[u8], zero_fill: usize) -> Self {
        if zero_fill == 0 { bytes.len() } else { zero_fill }
    }

    fn from_generated_bytes(bytes: Vec<u8>) -> Self { bytes.len() }
}

pub(crate) enum LinkSectionBytes<'a> {
    Captured(ObjectBytes),
    Owned(Vec<u8>),
    Borrowed(&'a [u8]),
    ZeroFill(usize),
}

pub(crate) enum LinkSectionData<'a> {
    Initialized(&'a [u8]),
    ZeroFill(usize),
}

impl ReadSection for LinkSectionBytes<'_> {
    fn from_coff(source: &ObjectBytes, bytes: &[u8], zero_fill: usize) -> Self {
        if zero_fill != 0 {
            return Self::ZeroFill(zero_fill);
        }
        if bytes.is_empty() {
            return Self::Borrowed(&[]);
        }
        // The COFF reader validated this range in the original snapshot.
        let start = bytes.as_ptr().addr() - source.as_ptr().addr();
        Self::Captured(source.slice(start..start + bytes.len()))
    }

    fn from_generated_bytes(bytes: Vec<u8>) -> Self { Self::Owned(bytes) }
}

impl LinkSectionBytes<'_> {
    pub(crate) fn data(&self) -> LinkSectionData<'_> {
        match self {
            Self::Captured(bytes) => LinkSectionData::Initialized(bytes),
            Self::Owned(bytes) => LinkSectionData::Initialized(bytes),
            Self::Borrowed(bytes) => LinkSectionData::Initialized(bytes),
            Self::ZeroFill(length) => LinkSectionData::ZeroFill(*length),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Definition {
    pub name: String,
    pub section: usize,
    pub value: u32,
    pub function: bool,
}

pub(crate) struct Symbol {
    pub definition: Definition,
    pub contract: Vec<u8>,
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
    pub fn stack_report(&self, budget: Option<u64>) -> String {
        let code = stack::Code {
            relocations: &|section| self.sections[section].relocations.as_slice(),
            frames: &self.stack,
            definitions: stack::Definitions::Compiled(&self.symbols),
        };
        let runtime = stack::uses_runtime(
            self.archive.as_ref(), matches!(self.kind, Kind::Library { .. }), &self.sections,
        );
        stack::report(&self.name, code, budget, None, runtime)
    }
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
        coff::decode(&ObjectBytes::read(path)?, true)
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
