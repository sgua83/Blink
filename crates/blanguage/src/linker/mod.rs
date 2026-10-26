//! 独立链接阶段只读取 COFF 目标文件及其已编译静态库，不调用 B／Rust 编译器。
mod msvc;
mod pe;
use crate::{
    application::RUNTIME_LIBRARY,
    object::{Kind, Object},
};
use std::path::Path;
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn link(input: &Path, output: &Path) -> Result<()> {
    link_object(Object::read(input)?, output)
}
pub(crate) fn link_object(object: Object, path: &Path) -> Result<()> {
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Err("本机链接当前输出 Windows x64".into());
    }
    let output = std::path::absolute(path)?;
    std::fs::create_dir_all(output.parent().unwrap())?;
    match &object.kind {
        Kind::Application { .. } => {
            let library = if let Some(archive) = &object.archive {
                use std::hash::{Hash, Hasher};
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                output.file_name().hash(&mut hash);
                archive.bytes.hash(&mut hash);
                let name = format!("blink_app_{:016x}.dll", hash.finish());
                msvc::library(archive, &output.with_file_name(&name), false)?;
                Some(name)
            } else {
                None
            };
            std::fs::write(&output, pe::link(&object, library.as_deref())?)?;
        }
        Kind::Library {
            header,
            binding,
            prefix,
        } => {
            let archive = object
                .archive
                .as_ref()
                .ok_or("DLL 目标文件没有静态目标库")?;
            msvc::library(archive, &output, true)?;
            std::fs::write(output.with_extension("abi.h"), header)?;
            let library = output
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("DLL 文件名不是 UTF-8")?;
            let binding = binding.replace(
                &format!("#[link(name=\"{prefix}\")]"),
                &format!("#[link(name=\"{library}\")]"),
            );
            std::fs::write(output.with_extension("rs"), binding)?;
        }
    }
    Ok(())
}
