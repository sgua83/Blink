//! 独立链接阶段只读取 COFF 目标文件及其已编译静态库，不调用 B／Rust 编译器。
mod combine;
mod selection;
use selection::entry;
pub(crate) use selection::{RootView, bind_root_contracts};
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod msvc;
mod imports;
pub use imports::host_archive;
mod pe;
mod resources;
pub(crate) mod native;
use crate::{
    object::{Archive, Definition, Input, LinkSectionBytes, Object, ObjectBytes, ReadSection, Section, stack},
};
use std::path::Path;
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn link(input: &Path, output: &Path) -> Result<()> {
    link_inputs(&[input], output)
}
pub fn link_inputs(inputs: &[impl AsRef<Path>], output: &Path) -> Result<()> {
    let bytes = inputs.iter().map(|path| ObjectBytes::read(path.as_ref()));
    link_objects(read_inputs(bytes, false)?, output)
}

fn read_inputs<Bytes: ReadSection>(
    bytes: impl IntoIterator<Item = Result<ObjectBytes>>, stack_output: bool,
) -> Result<Inputs<'static, Bytes>> {
    let mut inputs = Inputs { sources: Vec::new(), libraries: Vec::new() };
    for source in bytes {
        source?.decode_input(
            stack_output,
            |object| inputs.sources.push(Source::Owned(object)),
            |library| inputs.libraries.push(library),
        )?;
    }
    Ok(inputs)
}

struct Inputs<'a, Bytes> {
    sources: Vec<Source<'a, Bytes>>,
    libraries: Vec<crate::object::Library>,
}

impl<Payload: ReadSection> Inputs<'_, Payload> {
    fn push(&mut self, input: Input) {
        match input {
            Input::Object(object) => self.sources.push(Source::Owned(Payload::from_generated(object))),
            Input::Library(library) => self.libraries.push(library),
        }
    }
}

/// One original object owner; graph consumers borrow the same compiled value.
enum Source<'a, Bytes> {
    Owned(Object<Bytes>),
    Borrowed(&'a Object),
}
impl<Bytes> Source<'_, Bytes> {
    fn archive(&self) -> Option<&Archive> {
        match self {
            Self::Owned(object) => object.archive.as_ref(),
            Self::Borrowed(object) => object.archive.as_ref(),
        }
    }
}
struct Selected<Bytes> {
    name: String,
    library: Option<LibraryBinding>,
    sections: Vec<Section<Bytes>>,
    definitions: Vec<Definition>,
    archive: Option<Archive>,
    stack: Vec<stack::Frame>,
}

struct LibraryBinding {
    header: String,
    binding: String,
    prefix: String,
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum EntryProduct {
    None,
    Pe,
    HostApplication,
    Dll,
    DllWithoutArchive,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct EntryPlan {
    product: EntryProduct,
    uses_runtime: u32,
    stack_reserve: u64,
}

struct Selection<Bytes> {
    objects: Vec<Selected<Bytes>>,
    plan: EntryPlan,
}

enum Roots<'a> {
    Entry { stack_output: bool },
    Symbols(&'a [RootView<'a>], u32, u32),
}
pub(crate) fn link_object(object: Object, path: &Path) -> Result<()> {
    link_objects(Inputs { sources: vec![Source::Owned(LinkSectionBytes::from_generated(object))], libraries: Vec::new() }, path)
}
pub fn stack_report_inputs(inputs: &[impl AsRef<Path>], budget: Option<u64>) -> Result<String> {
    let bytes = inputs.iter().map(|path| ObjectBytes::read(path.as_ref()));
    stack_report(read_inputs(bytes, true)?, budget)
}

/// Read each input through the handle held until the report has been written.
pub fn write_stack_report(inputs: &[impl AsRef<Path>], output: &Path, budget: Option<u64>) -> Result<()> {
    use crate::file_bytes;

    let input_files = inputs.iter().map(|path| file_bytes::open_shared_read(path.as_ref()))
        .collect::<std::io::Result<Vec<_>>>()?;
    let report = stack_report(read_inputs(input_files.iter().map(ObjectBytes::capture), true)?, budget)?;
    if let Some(parent) = output.parent() {
        file_bytes::create_dir_all(parent)?;
    }
    file_bytes::write(output, report.as_bytes())?;
    Ok(())
}

fn stack_report(inputs: Inputs<'_, usize>, budget: Option<u64>) -> Result<String> {
    let Selection { mut objects, plan } = entry(inputs, true, Vec::len)?;
    let runtime = plan.uses_runtime != 0;
    let reserve = match plan.product {
        EntryProduct::Pe | EntryProduct::HostApplication => Some(plan.stack_reserve),
        _ => None,
    };
    let report = if let [object] = objects.as_slice() {
        let code = stack::Code {
            relocations: &|section| object.sections[section].relocations.as_slice(),
            frames: &object.stack,
            definitions: stack::Definitions::Linked(&object.definitions),
        };
        stack::report(&object.name, code, budget, reserve, runtime)
    } else {
        let name = std::mem::take(&mut objects[0].name);
        let (relocations, frames) = combine::report(objects)?;
        let code = stack::Code {
            relocations: &|section| relocations[section].as_slice(),
            frames: &frames,
            definitions: stack::Definitions::Linked(&[]),
        };
        stack::report(&name, code, budget, reserve, runtime)
    };
    Ok(report)
}
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
fn link_objects<'a>(inputs: Inputs<'a, LinkSectionBytes<'a>>, _: &Path) -> Result<()> {
    entry(inputs, false, |bytes| LinkSectionBytes::Borrowed(bytes))?;
    Err("本机链接当前输出 Windows x64".into())
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
use windows_output::link_objects;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod windows_output {
    use super::*;
    use crate::{file_bytes::{HostWriteResult, host_path}, source_path};

    #[repr(C)]
    struct PathUnits { data: *const u16, length: usize }

    impl PathUnits {
        fn terminated(units: &[u16]) -> Self {
            Self { data: units.as_ptr(), length: units.len() - 1 }
        }
    }

    #[repr(C)]
    struct Bytes { data: *const u8, length: usize }

    impl Bytes {
        fn of(bytes: &[u8]) -> Self {
            Self { data: bytes.as_ptr(), length: bytes.len() }
        }
    }

    #[repr(C)]
    struct Bindings { header: Bytes, rust: Bytes, prefix: Bytes }

    #[repr(u32)]
    enum Failure { None, Native, MissingArchive, LibraryName, NoProduct }

    #[repr(C)]
    struct OutputResult { written: HostWriteResult, failure: Failure }

    struct Link<'a> {
        objects: Vec<Selected<LinkSectionBytes<'a>>>,
        output: &'a Path,
        pe: Vec<u8>,
        error: Result<()>,
    }

    unsafe extern "C" {
        #[link_name = "bootstrap.host_sdk::deliver_link_output"]
        fn deliver_link_output(
            path: PathUnits, parent: PathUnits, plan: EntryPlan, bindings: Bindings, context: *mut (),
            pe: unsafe extern "C" fn(*mut (), u64) -> Bytes,
            msvc: unsafe extern "C" fn(*mut (), u64, u32) -> u32,
        ) -> OutputResult;
    }

    unsafe extern "C" fn link_pe(context: *mut (), reserve: u64) -> Bytes {
        let link = unsafe { &mut *context.cast::<Link<'_>>() };
        match combine::link_pe(std::mem::take(&mut link.objects), reserve) {
            Ok(pe) => {
                link.pe = pe;
                Bytes::of(&link.pe)
            }
            Err(error) => {
                link.error = Err(error);
                Bytes { data: std::ptr::null(), length: 0 }
            }
        }
    }

    unsafe extern "C" fn link_msvc(context: *mut (), reserve: u64, dll: u32) -> u32 {
        let link = unsafe { &mut *context.cast::<Link<'_>>() };
        link.error = msvc::link(&link.objects, link.output, reserve, dll != 0);
        u32::from(link.error.is_ok())
    }

    pub(super) fn link_objects<'a>(inputs: Inputs<'a, LinkSectionBytes<'a>>, path: &Path) -> Result<()> {
        let Selection { objects, plan } = entry(inputs, false, |bytes| LinkSectionBytes::Borrowed(bytes))?;
        let output = source_path::absolute(path)?;
        let path = host_path(&output)?;
        let parent = host_path(output.parent().unwrap())?;
        let mut link = Link { objects, output: &output, pe: Vec::new(), error: Ok(()) };
        let context = std::ptr::from_mut(&mut link).cast();
        let (header, rust, prefix) = link.objects[0].library.as_ref()
            .map_or(("", "", ""), |binding| (binding.header.as_str(), binding.binding.as_str(), binding.prefix.as_str()));
        let bindings = Bindings {
            header: Bytes::of(header.as_bytes()), rust: Bytes::of(rust.as_bytes()), prefix: Bytes::of(prefix.as_bytes()),
        };
        // The DLL branch borrows the original strings through the native link and writes.
        // The PE branch alone consumes the selected objects and exposes the produced image.
        let result = unsafe {
            deliver_link_output(PathUnits::terminated(&path), PathUnits::terminated(&parent),
                plan, bindings, context, link_pe, link_msvc)
        };
        match result.failure {
            Failure::None => Ok(result.written.into_result()?),
            Failure::Native => link.error,
            Failure::MissingArchive => Err("DLL 目标文件没有静态目标库".into()),
            Failure::LibraryName => Err("DLL 文件名不是 UTF-8".into()),
            Failure::NoProduct => unreachable!(),
        }
    }
}
