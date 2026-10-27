//! 只组合已编译的节、符号和重定位；函数生成及接口分析属于编译阶段。
use super::Result;
use crate::object::{Kind, Object, Section, Target};
use std::collections::BTreeMap;

pub(super) fn objects(mut objects: Vec<Object>) -> Result<(Object, Vec<crate::object::Archive>)> {
    let entries = objects
        .iter()
        .enumerate()
        .filter(|(_, object)| matches!(object.kind, Kind::Application { .. }))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return Err("应用链接需要且只能有一个实际入口对象".into());
    }
    objects.swap(0, entries[0]);
    let name = objects[0].name.clone();
    let stack_bytes = objects
        .iter()
        .filter_map(|object| match object.kind {
            Kind::Application { stack_bytes } | Kind::Module { stack_bytes, .. } => {
                Some(stack_bytes)
            }
            _ => None,
        })
        .max()
        .unwrap();
    let mut sections: Vec<Section> = Vec::new();
    let mut groups = BTreeMap::new();
    let mut placements = Vec::new();
    let mut symbols = BTreeMap::new();
    let mut archives = Vec::new();
    for object in &mut objects {
        if !matches!(object.kind, Kind::Application { .. } | Kind::Module { .. }) {
            return Err(format!("{} 不是普通模块或应用对象", object.name).into());
        }
        if let Some(archive) = object.archive.take() {
            archives.push(archive);
        }
        let mut positions = Vec::new();
        for section in &object.sections {
            let flags = section.flags & !0x01000000;
            let key = (section.name.clone(), flags);
            let index = *groups.entry(key).or_insert_with(|| {
                let index = sections.len();
                sections.push(Section {
                    name: section.name.clone(),
                    bytes: Vec::new(),
                    flags,
                    relocations: Vec::new(),
                });
                index
            });
            let alignment = 1usize << ((flags >> 20) & 15).saturating_sub(1);
            let begin = sections[index].bytes.len().div_ceil(alignment) * alignment;
            sections[index]
                .bytes
                .resize(begin, if section.name == ".text" { 0x90 } else { 0 });
            sections[index].bytes.extend(&section.bytes);
            positions.push((index, u32::try_from(begin)?));
        }
        for symbol in &object.symbols {
            let (section, begin) = positions[symbol.section];
            if symbols
                .insert(
                    symbol.name.clone(),
                    (
                        section,
                        begin.checked_add(symbol.value).ok_or("模块符号地址溢出")?,
                        symbol.contract.clone(),
                    ),
                )
                .is_some()
            {
                return Err(format!("重复定义模块符号 {}", symbol.name).into());
            }
        }
        placements.push(positions);
    }
    for (object, positions) in objects.into_iter().zip(placements) {
        for (section, (index, base)) in object.sections.into_iter().zip(&positions) {
            for mut relocation in section.relocations {
                relocation.offset += *base as usize;
                relocation.target = match relocation.target {
                    Target::Section(old, value) => {
                        let (index, begin) = positions[old];
                        Target::Section(index, begin.checked_add(value).ok_or("模块节地址溢出")?)
                    }
                    Target::Symbol(name, expected) => {
                        let (section, value, actual) = symbols
                            .get(&name)
                            .ok_or_else(|| format!("未定义模块符号 {name}"))?;
                        if actual != &expected {
                            return Err(format!("模块接口不匹配 {name}；调用方与定义须使用相同的类型、布局、访问及函数事实").into());
                        }
                        Target::Section(*section, *value)
                    }
                    target @ Target::Import(_) => target,
                };
                sections[*index].relocations.push(relocation);
            }
        }
    }
    Ok((
        Object {
            name,
            kind: Kind::Application { stack_bytes },
            sections,
            symbols: Vec::new(),
            archive: None,
        },
        archives,
    ))
}
