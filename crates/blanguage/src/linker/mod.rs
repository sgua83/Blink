//! 独立链接阶段只读取 COFF 目标文件及其已编译静态库，不调用 B／Rust 编译器。
mod combine;
mod selection;
use selection::entry;
pub(crate) use selection::RootView;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod msvc;
mod imports;
pub use imports::host_archive;
mod pe;
mod resources;
pub(crate) mod native;
use crate::{
    object::{Archive, Definition, Input, Kind, Object, ReadSection, Section, read_input, stack},
};
use std::path::Path;
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn link(input: &Path, output: &Path) -> Result<()> {
    link_inputs(&[input], output)
}
pub fn link_inputs(inputs: &[impl AsRef<Path>], output: &Path) -> Result<()> {
    link_objects(read_inputs(inputs, false)?, output)
}
fn read_inputs<Bytes: ReadSection>(paths: &[impl AsRef<Path>], stack_output: bool) -> Result<Inputs<'static, Bytes>> {
    let mut inputs = Inputs { sources: Vec::new(), libraries: Vec::new() };
    for path in paths {
        read_input(path.as_ref(), stack_output,
            |object| inputs.sources.push(Source::Owned(object)),
            |library| inputs.libraries.push(library))?;
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
    kind: SelectedKind,
    sections: Vec<Section<Bytes>>,
    definitions: Vec<Definition>,
    archive: Option<Archive>,
    stack: Vec<stack::Frame>,
}

enum SelectedKind {
    Module { stack_bytes: u32 },
    Application { stack_bytes: u32 },
    Library { header: String, binding: String, prefix: String },
}

impl From<Kind> for SelectedKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Module { stack_bytes, .. } => Self::Module { stack_bytes },
            Kind::Application { stack_bytes } => Self::Application { stack_bytes },
            Kind::Library { header, binding, prefix } => Self::Library { header, binding, prefix },
        }
    }
}

impl From<&Kind> for SelectedKind {
    fn from(kind: &Kind) -> Self {
        match kind {
            Kind::Module { stack_bytes, .. } => Self::Module { stack_bytes: *stack_bytes },
            Kind::Application { stack_bytes } => Self::Application { stack_bytes: *stack_bytes },
            Kind::Library { header, binding, prefix } => Self::Library {
                header: header.clone(), binding: binding.clone(), prefix: prefix.clone(),
            },
        }
    }
}

enum Roots<'a> {
    Entry { stack_output: bool },
    Symbols(&'a [RootView<'a>], u32, u32),
}
pub(crate) fn link_object(object: Object, path: &Path) -> Result<()> {
    link_objects(Inputs { sources: vec![Source::Owned(object)], libraries: Vec::new() }, path)
}
fn prepare(inputs: Inputs<'_>) -> Result<(Vec<Selected>, Vec<Archive>)> {
    let mut objects = entry(inputs, false)?;
    let archives = objects.iter_mut().filter_map(|object| object.archive.take()).collect();
    Ok((objects, archives))
}
pub fn stack_report_inputs(inputs: &[impl AsRef<Path>], budget: Option<u64>) -> Result<String> {
    let mut objects = entry(read_inputs::<usize>(inputs, true)?, true)?;
    let runtime = objects.iter().any(|object| stack::uses_runtime(
        object.archive.as_ref(), matches!(object.kind, SelectedKind::Library { .. }), &object.sections,
    ));
    let reserve = match objects[0].kind {
        SelectedKind::Application { stack_bytes } => Some(pe::stack_reserve(stack_bytes, &objects[1..])),
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
    prepare(inputs)?;
    Err("本机链接当前输出 Windows x64".into())
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn link_objects(inputs: Inputs<'_>, path: &Path) -> Result<()> {
    use crate::{file_bytes, source_path};

    let (objects, archives) = prepare(inputs)?;
    let object = &objects[0];
    let output = source_path::absolute(path)?;
    file_bytes::create_dir_all(output.parent().unwrap())?;
    match &object.kind {
        SelectedKind::Module { .. } => unreachable!(),
        SelectedKind::Application { stack_bytes } => {
            let reserve = pe::stack_reserve(*stack_bytes, &objects[1..]);
            if archives.is_empty() {
                file_bytes::write(&output, &combine::link_pe(objects, reserve)?)?;
            } else {
                msvc::link(&archives, &objects, &output, reserve, false)?;
            }
        }
        SelectedKind::Library {
            header,
            binding,
            prefix,
        } => {
            if archives.is_empty() {
                return Err("DLL 目标文件没有静态目标库".into());
            }
            let library = source_path::file_stem(&output).ok_or("DLL 文件名不是 UTF-8")?;
            msvc::link(&archives, &objects, &output, 0, true)?;
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
