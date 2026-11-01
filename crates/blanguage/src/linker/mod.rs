//! 独立链接阶段只读取 COFF 目标文件及其已编译静态库，不调用 B／Rust 编译器。
mod combine;
mod msvc;
mod pe;
mod resources;
use crate::{
    application::RUNTIME_LIBRARY,
    object::{Input, Kind, Object, Target},
};
use std::{
    collections::{BTreeMap, VecDeque},
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
fn read_inputs(inputs: &[impl AsRef<Path>]) -> Result<(Vec<Object>, Vec<crate::object::Library>)> {
    let mut objects = Vec::new();
    let mut libraries = Vec::new();
    for input in inputs {
        match Input::read(input.as_ref())? {
            Input::Object(object) => objects.push(object),
            Input::Library(library) => libraries.push(library),
        }
    }
    Ok((objects, libraries))
}
// 符号与节的存活只属于本次原生链接；展开节沿编译产物的真实关联跟随代码。
fn define(object: &Object, index: usize, defined: &mut BTreeMap<String, (usize, usize)>) -> Result<()> {
    for symbol in &object.symbols {
        if defined.insert(symbol.name.clone(), (index, symbol.section)).is_some() {
            return Err(format!("重复定义模块符号 {}", symbol.name).into());
        }
    }
    Ok(())
}
fn include(objects: &[Object], selected: &mut [Option<Vec<bool>>], pending: &mut VecDeque<(usize, usize)>, object: usize, section: usize) {
    if selected[object].is_none() {
        let mut live = vec![false; objects[object].sections.len()];
        for (index, section) in objects[object].sections.iter().enumerate() {
            if section.flags & 0x1000 == 0 {
                live[index] = true;
                pending.push_back((object, index));
            }
        }
        selected[object] = Some(live);
    }
    let live = selected[object].as_mut().unwrap();
    if !live[section] {
        live[section] = true;
        pending.push_back((object, section));
    }
}
fn select(mut objects: Vec<Object>, mut libraries: Vec<crate::object::Library>) -> Result<Vec<Object>> {
    let mut defined = BTreeMap::new();
    for (index, object) in objects.iter().enumerate() { define(object, index, &mut defined)?; }
    if matches!(objects[0].kind, Kind::Library { .. }) { return Ok(objects); }
    let mut selected = objects.iter().map(|_| None).collect::<Vec<_>>();
    let mut pending = VecDeque::new();
    include(&objects, &mut selected, &mut pending, 0, 0);
    while let Some((object, section)) = pending.pop_front() {
        for index in 0..objects[object].sections[section].associated.len() {
            let associated = objects[object].sections[section].associated[index];
            include(&objects, &mut selected, &mut pending, object, associated);
        }
        for index in 0..objects[object].sections[section].relocations.len() {
            let target = match &objects[object].sections[section].relocations[index].target {
                Target::Section(section, _) => (object, *section),
                Target::Import(_) => continue,
                Target::Symbol(name) => {
                    if let Some(&target) = defined.get(name) { target } else {
                        let mut provider = None;
                        for library in &mut libraries {
                            if let Some(object) = library.extract(name)? { provider = Some(object); break; }
                        }
                        let Some(provider) = provider else { continue; };
                        let next = objects.len();
                        define(&provider, next, &mut defined)?;
                        let target = defined[name];
                        objects.push(provider);
                        selected.push(None);
                        target
                    }
                }
            };
            include(&objects, &mut selected, &mut pending, target.0, target.1);
        }
    }
    let mut output = Vec::new();
    for (mut object, live) in objects.into_iter().zip(selected) {
        let Some(live) = live else { continue; };
        let mut count = 0;
        let mapping = live.iter().map(|&keep| if keep { let index = count; count += 1; Some(index) } else { None }).collect::<Vec<_>>();
        object.sections = object.sections.into_iter().enumerate().filter_map(|(index, mut section)| {
            if !live[index] { return None; }
            for relocation in &mut section.relocations {
                if let Target::Section(index, _) = &mut relocation.target { *index = mapping[*index].unwrap(); }
            }
            for associated in &mut section.associated { *associated = mapping[*associated].unwrap(); }
            Some(section)
        }).collect();
        object.stack.retain(|frame| live[frame.section]);
        for frame in &mut object.stack { frame.section = mapping[frame.section].unwrap(); }
        object.symbols.retain(|symbol| live[symbol.section]);
        for symbol in &mut object.symbols { symbol.section = mapping[symbol.section].unwrap(); }
        if matches!(object.kind, Kind::Module { .. }) {
            let used = object.sections.iter().flat_map(|section| &section.relocations).filter_map(|relocation| {
                if let Target::Import(import) = &relocation.target && import.dll == RUNTIME_LIBRARY { Some(import.name.as_str()) } else { None }
            }).collect::<std::collections::BTreeSet<_>>();
            if let Some(archive) = &mut object.archive {
                if archive.exports.iter().any(|name| used.contains(name.as_str())) {
                    let factory = archive.resources.as_ref().map(|resource| format!("{}_resources", resource.prefix));
                    archive.exports.retain(|name| used.contains(name.as_str()) || factory.as_ref() == Some(name));
                } else { object.archive = None; }
            }
        }
        output.push(object);
    }
    Ok(output)
}
pub(crate) fn link_object(object: Object, path: &Path) -> Result<()> {
    link_objects(vec![object], Vec::new(), path)
}
fn prepare(mut objects: Vec<Object>, libraries: Vec<crate::object::Library>) -> Result<(Object, Vec<crate::object::Archive>)> {
    let root = if objects.len() == 1 && matches!(objects[0].kind, Kind::Library { .. }) {
        0
    } else {
        let entries = objects.iter().enumerate()
            .filter(|(_, object)| matches!(object.kind, Kind::Application { .. }))
            .map(|(index, _)| index).collect::<Vec<_>>();
        if entries.len() != 1 {
            return Err("应用链接需要且只能有一个实际入口对象".into());
        }
        for object in &objects {
            if !matches!(object.kind, Kind::Application { .. } | Kind::Module { .. }) {
                return Err(format!("{} 不是普通模块或应用对象", object.name).into());
            }
        }
        entries[0]
    };
    objects.swap(0, root);
    let mut objects = select(objects, libraries)?;
    let (object, archives) =
        if objects.len() == 1 && matches!(objects[0].kind, Kind::Library { .. }) {
            let mut object = objects.remove(0);
            let archives = object.archive.take().into_iter().collect::<Vec<_>>();
            (object, archives)
        } else {
            combine::objects(objects)?
        };
    Ok((object, archives))
}
pub fn stack_report_inputs(inputs: &[impl AsRef<Path>], budget: Option<u64>) -> Result<String> {
    let (objects, libraries) = read_inputs(inputs)?;
    let (object, _) = prepare(objects, libraries)?;
    let reserve = match object.kind { Kind::Application { stack_bytes } => Some(pe::stack_reserve(stack_bytes)), _ => None };
    Ok(crate::object::stack::report(&object, budget, reserve))
}
fn link_objects(objects: Vec<Object>, libraries: Vec<crate::object::Library>, path: &Path) -> Result<()> {
    let (object, archives) = prepare(objects, libraries)?;
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Err("本机链接当前输出 Windows x64".into());
    }
    let output = std::path::absolute(path)?;
    std::fs::create_dir_all(output.parent().unwrap())?;
    match &object.kind {
        Kind::Module { .. } => unreachable!(),
        Kind::Application { .. } => {
            let library = if !archives.is_empty() {
                use std::hash::{Hash, Hasher};
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                output.file_name().hash(&mut hash);
                for archive in &archives {
                    archive.bytes.hash(&mut hash);
                }
                let name = format!("blink_app_{:016x}.dll", hash.finish());
                msvc::library(&archives, &output.with_file_name(&name), false)?;
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
            if archives.is_empty() {
                return Err("DLL 目标文件没有静态目标库".into());
            }
            msvc::library(&archives, &output, true)?;
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
