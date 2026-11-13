//! 独立链接阶段只读取 COFF 目标文件及其已编译静态库，不调用 B／Rust 编译器。
mod combine;
mod selection;
use selection::select;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod msvc;
mod imports;
pub use imports::host_archive;
mod pe;
mod resources;
pub(crate) mod native;
use crate::{
    object::{Input, Kind, Object},
};
use std::{
    collections::BTreeMap,
    path::Path,
};
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn link(input: &Path, output: &Path) -> Result<()> {
    link_inputs(&[input], output)
}
pub fn link_inputs(inputs: &[impl AsRef<Path>], output: &Path) -> Result<()> {
    link_objects(read_inputs(inputs)?, output)
}
fn read_inputs(paths: &[impl AsRef<Path>]) -> Result<Inputs<'static>> {
    let mut inputs = Inputs::default();
    for path in paths {
        inputs.push(Input::read(path.as_ref())?);
    }
    Ok(inputs)
}

#[derive(Default)]
struct Inputs<'a> {
    sources: Vec<Source<'a>>,
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
enum Source<'a> {
    Owned(Object),
    Borrowed(&'a Object),
}
impl std::ops::Deref for Source<'_> {
    type Target = Object;
    fn deref(&self) -> &Object {
        match self { Self::Owned(object) => object, Self::Borrowed(object) => object }
    }
}
enum Roots<'a> {
    Entry,
    Symbols(&'a BTreeMap<String, Vec<u8>>, u32, u32),
}
pub(crate) fn link_object(object: Object, path: &Path) -> Result<()> {
    link_objects(Inputs { sources: vec![Source::Owned(object)], libraries: Vec::new() }, path)
}
fn prepare(inputs: Inputs<'_>) -> Result<(Vec<Object>, Vec<crate::object::Archive>)> {
    let mut objects = select(inputs, Roots::Entry)?;
    let archives = objects.iter_mut().filter_map(|object| object.archive.take()).collect();
    Ok((objects, archives))
}
pub fn stack_report_inputs(inputs: &[impl AsRef<Path>], budget: Option<u64>) -> Result<String> {
    let (mut objects, _) = prepare(read_inputs(inputs)?)?;
    let reserve = match objects[0].kind { Kind::Application { stack_bytes } => Some(pe::stack_reserve(stack_bytes)), _ => None };
    let object = if objects.len() == 1 { objects.remove(0) } else { combine::objects(objects,None)? };
    Ok(crate::object::stack::report(&object, budget, reserve))
}
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
fn link_objects(inputs: Inputs<'_>, _: &Path) -> Result<()> {
    prepare(inputs)?;
    Err("本机链接当前输出 Windows x64".into())
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn link_objects(inputs: Inputs<'_>, path: &Path) -> Result<()> {
    use crate::file_bytes;

    let (objects, archives) = prepare(inputs)?;
    let object = &objects[0];
    let output = std::path::absolute(path)?;
    file_bytes::create_dir_all(output.parent().unwrap())?;
    match &object.kind {
        Kind::Module { .. } => unreachable!(),
        Kind::Application { .. } => {
            if archives.is_empty() {
                let object = combine::objects(objects,None)?;
                file_bytes::write(&output, &pe::link(&object)?)?;
            } else {
                msvc::link(&archives, &objects.iter().collect::<Vec<_>>(), &output, false)?;
            }
        }
        Kind::Library {
            header,
            binding,
            prefix,
        } => {
            if archives.is_empty() {
                return Err("DLL 目标文件没有静态目标库".into());
            }
            let library = output
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("DLL 文件名不是 UTF-8")?;
            msvc::link(&archives, &objects.iter().collect::<Vec<_>>(), &output, true)?;
            let mut header_path = output.clone();
            header_path.set_extension("abi.h");
            file_bytes::write(&header_path, header.as_bytes())?;
            let binding = binding.replace(
                &format!("#[link(name=\"{prefix}\")]"),
                &format!("#[link(name=\"{library}\")]"),
            );
            let mut binding_path = output;
            binding_path.set_extension("rs");
            file_bytes::write(&binding_path, binding.as_bytes())?;
        }
    }
    Ok(())
}
