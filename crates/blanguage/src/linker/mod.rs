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
    for (symbol_index, symbol) in object.symbols.iter().enumerate() {
        if defined.insert(symbol.name.clone(), (index, symbol_index)).is_some() {
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
    let library = matches!(objects[0].kind, Kind::Library { .. });
    let mut selected = objects.iter().map(|_| None).collect::<Vec<_>>();
    let mut pending = VecDeque::new();
    if library {
        // The original Rust wrapper references all of this algorithm object's native entries.
        // GPU-only libraries have no CPU entries; their actual archive remains the root.
        let count = objects[0].sections.len();
        selected[0] = Some(vec![true; count]);
        pending.extend((0..count).map(|section| (0, section)));
    } else {
        include(&objects, &mut selected, &mut pending, 0, 0);
    }
    while let Some((object, section)) = pending.pop_front() {
        for index in 0..objects[object].sections[section].associated.len() {
            let associated = objects[object].sections[section].associated[index];
            include(&objects, &mut selected, &mut pending, object, associated);
        }
        for index in 0..objects[object].sections[section].relocations.len() {
            let target = match &objects[object].sections[section].relocations[index].target {
                Target::Section(section, _) => (object, *section),
                Target::Import(import) => {
                    if import.dll == RUNTIME_LIBRARY {
                        if let Some(archive) = &objects[object].archive {
                            if archive.exports.contains(&import.name) {
                                if let Some(resource) = &archive.resources {
                                    for name in &resource.code {
                                        let symbol = objects[object].symbols.iter().find(|symbol| &symbol.name == name)
                                            .ok_or_else(|| format!("模块资源缺少所属CPU入口 {name}"))?;
                                        include(&objects, &mut selected, &mut pending, object, symbol.section);
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }
                Target::Symbol(name) => {
                    let name = name.clone();
                    let target = if let Some(&target) = defined.get(&name) { target } else {
                        let mut provider = None;
                        for library in &mut libraries {
                            if let Some(object) = library.extract(&name)? { provider = Some(object); break; }
                        }
                        let provider = provider.ok_or_else(|| format!("未定义模块符号 {name}"))?;
                        let next = objects.len();
                        define(&provider, next, &mut defined)?;
                        let target = defined[&name];
                        objects.push(provider);
                        selected.push(None);
                        target
                    };
                    let actual = &objects[target.0].symbols[target.1];
                    if &actual.contract != &objects[object].imports[&name] {
                        return Err(format!("模块接口不匹配 {name}；调用方与定义须使用相同的类型、布局、访问及函数事实").into());
                    }
                    (target.0, actual.section)
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
        let resources = object.archive.is_some();
        object.stack.retain_mut(|frame| match frame.coff {
            None => resources,
            Some((section, start)) => {
                frame.coff = mapping[section].map(|index| (index, start));
                frame.coff.is_some()
            }
        });
        output.push(object);
    }
    Ok(output)
}
pub(crate) fn link_object(object: Object, path: &Path) -> Result<()> {
    link_objects(vec![object], Vec::new(), path)
}
fn prepare(mut objects: Vec<Object>, libraries: Vec<crate::object::Library>) -> Result<(Vec<Object>, Vec<crate::object::Archive>)> {
    let entries = objects.iter().enumerate()
        .filter(|(_, object)| matches!(object.kind, Kind::Application { .. } | Kind::Library { .. }))
        .map(|(index, _)| index).collect::<Vec<_>>();
    if entries.len() != 1 {
        return Err("原生链接需要且只能有一个实际应用或算法库入口对象".into());
    }
    objects.swap(0, entries[0]);
    let mut objects = select(objects, libraries)?;
    let archives = objects.iter_mut().filter_map(|object| object.archive.take()).collect();
    Ok((objects, archives))
}
pub fn stack_report_inputs(inputs: &[impl AsRef<Path>], budget: Option<u64>) -> Result<String> {
    let (objects, libraries) = read_inputs(inputs)?;
    let (mut objects, _) = prepare(objects, libraries)?;
    let reserve = match objects[0].kind { Kind::Application { stack_bytes } => Some(pe::stack_reserve(stack_bytes)), _ => None };
    let object = if objects.len() == 1 { objects.remove(0) } else { combine::objects(objects)?.0 };
    Ok(crate::object::stack::report(&object, budget, reserve))
}
fn link_objects(objects: Vec<Object>, libraries: Vec<crate::object::Library>, path: &Path) -> Result<()> {
    let (objects, archives) = prepare(objects, libraries)?;
    let object = &objects[0];
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Err("本机链接当前输出 Windows x64".into());
    }
    let output = std::path::absolute(path)?;
    std::fs::create_dir_all(output.parent().unwrap())?;
    match &object.kind {
        Kind::Module { .. } => unreachable!(),
        Kind::Application { .. } => {
            if archives.is_empty() {
                let (object, _) = combine::objects(objects)?;
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
