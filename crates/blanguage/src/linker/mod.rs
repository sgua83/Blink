//! 独立链接阶段只读取 COFF 目标文件及其已编译静态库，不调用 B／Rust 编译器。
mod combine;
mod msvc;
mod imports;
pub use imports::host_archive;
mod pe;
mod resources;
pub(crate) mod native;
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
struct Selected {
    live: Vec<bool>,
    resources: bool,
}
fn materialize(source: Source<'_>, live: &[bool], native: bool) -> Object {
    let rewrite = |section: &mut crate::object::Section, archive: Option<&crate::object::Archive>| {
        if native {
            for relocation in &mut section.relocations {
                if let Target::Import(import) = &relocation.target && import.dll == RUNTIME_LIBRARY
                    && archive.is_some_and(|archive| archive.exports.contains(&import.name)) {
                    relocation.target = Target::Symbol(resources::import_symbol(&import.dll, &import.name));
                }
            }
        }
    };
    match source {
        Source::Owned(mut object) => {
            object.sections = object.sections.into_iter().zip(live).filter_map(|(mut section, &keep)| {
                if !keep { return None; }
                rewrite(&mut section, object.archive.as_ref());
                Some(section)
            }).collect();
            if native { object.archive = None; }
            object
        }
        Source::Borrowed(object) => Object {
            name: object.name.clone(), kind: object.kind.clone(),
            sections: object.sections.iter().zip(live).filter_map(|(section, &keep)| {
                if !keep { return None; }
                let mut section = crate::object::Section {
                    name: section.name.clone(), bytes: section.bytes.clone(), flags: section.flags,
                    associated: section.associated.clone(), layout: None,
                    relocations: section.relocations.iter().map(|relocation| crate::object::Relocation {
                        offset: relocation.offset, kind: relocation.kind, target: match &relocation.target {
                            Target::Symbol(name) => Target::Symbol(name.clone()),
                            Target::Section(section, value) => Target::Section(*section, *value),
                            Target::Import(import) => Target::Import(import.clone()),
                        },
                    }).collect(),
                };
                rewrite(&mut section, object.archive.as_ref());
                Some(section)
            }).collect(),
            symbols: object.symbols.iter().filter(|symbol| live[symbol.section]).map(|symbol| crate::object::Symbol {
                name: symbol.name.clone(), section: symbol.section, value: symbol.value,
                contract: symbol.contract.clone(), function: symbol.function,
            }).collect(),
            imports: object.imports.clone(),
            // The final native image owns selected code and unwind sections, not the original archive or compiler reports.
            archive: None, stack: Vec::new(),
        },
    }
}
enum Roots<'a> {
    Entry,
    Symbols(&'a BTreeMap<String, Vec<u8>>, u32, u32),
}
fn resolve_symbol(
    name: &str, contract: &[u8], objects: &mut Vec<Source<'_>>,
    libraries: &mut [crate::object::Library], defined: &mut BTreeMap<String, (usize, usize)>,
) -> Result<(usize, usize)> {
    let target = if let Some(&target) = defined.get(name) { target } else {
        let mut provider = None;
        for library in libraries {
            if let Some(object) = library.extract(name)? { provider = Some(object); break; }
        }
        let provider = provider.ok_or_else(|| format!("未定义模块符号 {name}"))?;
        let next = objects.len();
        define(&provider, next, defined)?;
        let target = *defined.get(name).ok_or_else(|| format!("静态库目录选中的模块 {} 未定义请求符号 {name}", provider.name))?;
        objects.push(Source::Owned(provider));
        target
    };
    let actual = &objects[target.0].symbols[target.1];
    if actual.contract != contract {
        return Err(format!("模块接口不匹配 {name}；调用方与定义须使用相同的类型、布局、访问及函数事实").into());
    }
    Ok((target.0, actual.section))
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
fn include(objects: &[Source<'_>], selected: &mut [Option<Selected>], pending: &mut VecDeque<(usize, usize)>, object: usize, section: usize) {
    if selected[object].is_none() {
        let mut live = vec![false; objects[object].sections.len()];
        for (index, section) in objects[object].sections.iter().enumerate() {
            if section.flags & 0x1000 == 0 {
                live[index] = true;
                pending.push_back((object, index));
            }
        }
        selected[object] = Some(Selected { live, resources: false });
    }
    let live = &mut selected[object].as_mut().unwrap().live;
    if !live[section] {
        live[section] = true;
        pending.push_back((object, section));
    }
}
fn select(mut objects: Vec<Source<'_>>, mut libraries: Vec<crate::object::Library>, roots: Roots<'_>) -> Result<Vec<Object>> {
    let mut resource_base = match roots { Roots::Symbols(_, types, programs) => Some((types, programs)), Roots::Entry => None };
    let native = resource_base.is_some();
    let mut local_imports = BTreeMap::new();
    let mut defined = BTreeMap::new();
    for (index, object) in objects.iter().enumerate() { define(object, index, &mut defined)?; }
    let mut selected = objects.iter().map(|_| None).collect::<Vec<_>>();
    let mut pending = VecDeque::new();
    match roots {
        Roots::Entry if matches!(objects[0].kind, Kind::Library { .. }) => {
            let count = objects[0].sections.len();
            selected[0] = Some(Selected { live: vec![true; count], resources: false });
            pending.extend((0..count).map(|section| (0, section)));
        }
        Roots::Entry => include(&objects, &mut selected, &mut pending, 0, 0),
        Roots::Symbols(roots, _, _) => {
            for (name, contract) in roots {
                let (object, section) = resolve_symbol(name, contract, &mut objects, &mut libraries, &mut defined)?;
                selected.resize_with(objects.len(), || None);
                include(&objects, &mut selected, &mut pending, object, section);
            }
        }
    }
    loop {
        while let Some((object, section)) = pending.pop_front() {
            for index in 0..objects[object].sections[section].associated.len() {
                let associated = objects[object].sections[section].associated[index];
                include(&objects, &mut selected, &mut pending, object, associated);
            }
            for index in 0..objects[object].sections[section].relocations.len() {
                let resource = match &objects[object].sections[section].relocations[index].target {
                    Target::Import(import) if import.dll == RUNTIME_LIBRARY => Some(import.name.clone()),
                    _ => None,
                };
                if let Some(name) = resource && let Some(archive) = &objects[object].archive
                    && archive.exports.contains(&name) {
                    if let Some(resource) = &archive.resources {
                        for name in &resource.code {
                            let symbol = objects[object].symbols.iter().find(|symbol| &symbol.name == name)
                                .ok_or_else(|| format!("模块资源缺少所属CPU入口 {name}"))?;
                            include(&objects, &mut selected, &mut pending, object, symbol.section);
                        }
                    }
                    if native {
                        let selected_object = selected[object].as_mut().unwrap();
                        if !selected_object.resources {
                            libraries.push(crate::object::Library::native(&archive.bytes)?);
                            selected_object.resources = true;
                        }
                        let (target, section_index) = resolve_symbol(&name, &[], &mut objects, &mut libraries, &mut defined)?;
                        selected.resize_with(objects.len(), || None);
                        include(&objects, &mut selected, &mut pending, target, section_index);
                        let slot = resources::import_symbol(RUNTIME_LIBRARY, &name);
                        local_imports.insert(slot.clone(), name);

                    }
                }
                let target = match &objects[object].sections[section].relocations[index].target {
                    Target::Section(section, _) => (object, *section),
                    Target::Import(_) => continue,
                    Target::Symbol(name) => {
                        // 基址的数据定义由本次已选模块共同形成；其余符号照常立即解析。
                        if native && !defined.contains_key(name) && (local_imports.contains_key(name) || crate::resource::MODULE_RUNTIME_SYMBOLS.contains(&name.as_str()) || objects.iter().any(|object|
                            object.archive.as_ref().and_then(|archive| archive.resources.as_ref()).is_some_and(|resource|
                                name == &format!("{}_type_base", resource.prefix) || name == &format!("{}_program_base", resource.prefix)))) {
                            continue;
                        }
                        let name = name.clone();
                        let contract = objects[object].imports[&name].clone();
                        let target = resolve_symbol(&name, &contract, &mut objects, &mut libraries, &mut defined)?;
                        selected.resize_with(objects.len(), || None);
                        target
                    }
                };
                include(&objects, &mut selected, &mut pending, target.0, target.1);
            }
        }
        let Some((types, programs)) = resource_base.take() else { break; };
        let mut runtime = std::collections::BTreeSet::new();
        for (object, live) in objects.iter().zip(&selected) {
            if let Some(live) = live {
                for (section, &keep) in object.sections.iter().zip(&live.live) {
                    if keep {
                        for relocation in &section.relocations {
                            if let Target::Symbol(name) = &relocation.target
                                && crate::resource::MODULE_RUNTIME_SYMBOLS.contains(&name.as_str()) {
                                runtime.insert(name.clone());
                            }
                        }
                    }
                }
            }
        }
        let archives = objects.iter().zip(&selected).filter_map(|(object, selected)| {
            selected.as_ref().filter(|selected| selected.resources).and_then(|_| object.archive.as_ref())
        }).collect::<Vec<_>>();
        if let Some(binding) = resources::resources(&archives, types, programs, &runtime.into_iter().collect::<Vec<_>>(), &local_imports, Some(crate::resource::ABI))? {
            let index = objects.len();
            define(&binding, index, &mut defined)?;
            let count = binding.sections.len();
            objects.push(Source::Owned(binding));
            selected.push(Some(Selected { live: vec![true; count], resources: false }));
            pending.extend((0..count).map(|section| (index, section)));
        }
    }
    let mut output = Vec::new();
    for (source, selection) in objects.into_iter().zip(selected) {
        let Some(selection) = selection else { continue; };
        let live = selection.live;
        let mut object = materialize(source, &live, native);
        let mut count = 0;
        let mapping = live.iter().map(|&keep| if keep { let index = count; count += 1; Some(index) } else { None }).collect::<Vec<_>>();
        for section in &mut object.sections {
            for relocation in &mut section.relocations {
                if let Target::Section(index, _) = &mut relocation.target { *index = mapping[*index].unwrap(); }
            }
            for associated in &mut section.associated { *associated = mapping[*associated].unwrap(); }
        }
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
    link_objects(vec![Source::Owned(object)], Vec::new(), path)
}
fn prepare(mut objects: Vec<Source<'_>>, libraries: Vec<crate::object::Library>) -> Result<(Vec<Object>, Vec<crate::object::Archive>)> {
    let entries = objects.iter().enumerate()
        .filter(|(_, object)| matches!(object.kind, Kind::Application { .. } | Kind::Library { .. }))
        .map(|(index, _)| index).collect::<Vec<_>>();
    if entries.len() != 1 {
        return Err("原生链接需要且只能有一个实际应用或算法库入口对象".into());
    }
    objects.swap(0, entries[0]);
    let mut objects = select(objects, libraries, Roots::Entry)?;
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
fn link_objects(objects: Vec<Source<'_>>, libraries: Vec<crate::object::Library>, path: &Path) -> Result<()> {
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
                let (object, _, _) = combine::objects(objects)?;
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
