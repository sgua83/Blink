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
    let (objects, libraries) = read_inputs(inputs)?;
    link_objects(objects, libraries, output)
}
fn read_inputs(inputs: &[impl AsRef<Path>]) -> Result<(Vec<Source<'static>>, Vec<crate::object::Library>)> {
    Ok(partition(inputs.iter().map(|input| Input::read(input.as_ref())).collect::<Result<Vec<_>>>()?))
}
fn partition<'a>(inputs: Vec<Input>) -> (Vec<Source<'a>>, Vec<crate::object::Library>) {
    let mut objects = Vec::new();
    let mut libraries = Vec::new();
    for input in inputs {
        match input {
            Input::Object(object) => objects.push(Source::Owned(object)),
            Input::Library(library) => libraries.push(library),
        }
    }
    (objects, libraries)
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
    link_objects(vec![Source::Owned(object)], Vec::new(), path)
}
fn prepare(objects: Vec<Source<'_>>, libraries: Vec<crate::object::Library>) -> Result<(Vec<Object>, Vec<crate::object::Archive>)> {
    let mut objects = select(objects, libraries, Roots::Entry)?;
    let archives = objects.iter_mut().filter_map(|object| object.archive.take()).collect();
    Ok((objects, archives))
}
pub fn stack_report_inputs(inputs: &[impl AsRef<Path>], budget: Option<u64>) -> Result<String> {
    let (objects, libraries) = read_inputs(inputs)?;
    let (mut objects, _) = prepare(objects, libraries)?;
    let reserve = match objects[0].kind { Kind::Application { stack_bytes } => Some(pe::stack_reserve(stack_bytes)), _ => None };
    let object = if objects.len() == 1 { objects.remove(0) } else { combine::objects(objects,None)? };
    Ok(crate::object::stack::report(&object, budget, reserve))
}
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
fn link_objects(objects: Vec<Source<'_>>, libraries: Vec<crate::object::Library>, _: &Path) -> Result<()> {
    prepare(objects, libraries)?;
    Err("本机链接当前输出 Windows x64".into())
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn link_objects(objects: Vec<Source<'_>>, libraries: Vec<crate::object::Library>, path: &Path) -> Result<()> {
    let (objects, archives) = prepare(objects, libraries)?;
    let object = &objects[0];
    let output = std::path::absolute(path)?;
    std::fs::create_dir_all(output.parent().unwrap())?;
    match &object.kind {
        Kind::Module { .. } => unreachable!(),
        Kind::Application { .. } => {
            if archives.is_empty() {
                let object = combine::objects(objects,None)?;
                std::fs::write(&output, pe::link(&object)?)?;
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
            msvc::link(&archives, &objects.iter().collect::<Vec<_>>(), &output, true)?;
            let mut header_path = output.clone();
            header_path.set_extension("abi.h");
            std::fs::write(header_path, header)?;
            let library = output
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("DLL 文件名不是 UTF-8")?;
            let binding = binding.replace(
                &format!("#[link(name=\"{prefix}\")]"),
                &format!("#[link(name=\"{library}\")]"),
            );
            let mut binding_path = output;
            binding_path.set_extension("rs");
            std::fs::write(binding_path, binding)?;
        }
    }
    Ok(())
}
