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
    object::{Archive, Definition, Input, Object, ObjectBytes, ReadSection, Section, stack},
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

#[derive(Default)]
struct Inputs<'a, Bytes = Vec<u8>> {
    sources: Vec<Source<'a, Bytes>>,
    libraries: Vec<crate::object::Library>,
}

impl Inputs<'_> {
    fn push(&mut self, input: Input) {
        match input {
            Input::Object(object) => self.sources.push(Source::Owned(object)),
            Input::Library(library) => self.libraries.push(library),
        }
    }
}

/// One original object owner; graph consumers borrow the same compiled value.
enum Source<'a, Bytes = Vec<u8>> {
    Owned(Object<Bytes>),
    Borrowed(&'a Object<Bytes>),
}
impl<Bytes> std::ops::Deref for Source<'_, Bytes> {
    type Target = Object<Bytes>;
    fn deref(&self) -> &Self::Target {
        match self { Self::Owned(object) => object, Self::Borrowed(object) => object }
    }
}
struct Selected<Bytes = Vec<u8>> {
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
    link_objects(Inputs { sources: vec![Source::Owned(object)], libraries: Vec::new() }, path)
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
    let Selection { mut objects, plan } = entry(inputs, true)?;
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
fn link_objects(inputs: Inputs<'_>, _: &Path) -> Result<()> {
    entry(inputs, false)?;
    Err("本机链接当前输出 Windows x64".into())
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn link_objects(inputs: Inputs<'_>, path: &Path) -> Result<()> {
    use crate::{file_bytes, source_path};

    let Selection { objects, plan } = entry(inputs, false)?;
    let output = source_path::absolute(path)?;
    file_bytes::create_dir_all(output.parent().unwrap())?;
    match plan.product {
        EntryProduct::None => unreachable!(),
        EntryProduct::Pe => {
            file_bytes::write(&output, &combine::link_pe(objects, plan.stack_reserve)?)?;
        }
        EntryProduct::HostApplication => {
            msvc::link(&objects, &output, plan.stack_reserve, false)?;
        }
        EntryProduct::DllWithoutArchive => return Err("DLL 目标文件没有静态目标库".into()),
        EntryProduct::Dll => {
            let LibraryBinding { header, binding, prefix } = objects[0].library.as_ref().unwrap();
            let library = source_path::file_stem(&output).ok_or("DLL 文件名不是 UTF-8")?;
            msvc::link(&objects, &output, 0, true)?;
            let header_path = source_path::with_extension_units(
                &output, &[b'a' as u16, b'b' as u16, b'i' as u16, b'.' as u16, b'h' as u16],
            );
            file_bytes::write(&header_path, header.as_bytes())?;
            let binding = crate::object::rust_link_binding(binding, prefix, &library);
            let binding_path = source_path::with_extension_units(&output, &[b'r' as u16, b's' as u16]);
            file_bytes::write(&binding_path, &binding)?;
        }
    }
    Ok(())
}
