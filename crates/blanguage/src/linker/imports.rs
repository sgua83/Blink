//! 原生宿主导入材料；实际导入槽通过COFF关联拥有其静态数据。
use super::Result;
use crate::object::{self, Import, Kind, LibraryMember, Relocation, Section, Symbol, Target};
use std::{collections::BTreeSet, path::Path};

pub(super) fn member(import: &Import) -> Result<(LibraryMember, String)> {
    let mut sections = [".idata$2", ".idata$3", ".idata$4", ".idata$5", ".idata$6", ".rdata"]
        .into_iter().enumerate().map(|(index, name)| Section {
            name: name.into(), bytes: Vec::new(),
            flags: (if index == 5 { 0x40000040 } else { 0xc0000040 })
                | match index { 0 | 1 => 0x00300000, 2 | 3 | 5 => 0x00400000, _ => 0x00200000 },
            relocations: Vec::new(), associated: Vec::new(), layout: None,
        }).collect::<Vec<_>>();
    let slot = super::resources::import_symbol(&import.dll, &import.name);
    let mut symbols = Vec::new();
    let root = if import.dll == crate::application::RUNTIME_LIBRARY {
        sections[5].bytes.extend([0; 8]);
        symbols.push(Symbol {
            name: slot, section: 5, value: 0, contract: Vec::new(), function: false,
        });
        sections[5].relocations.push(Relocation {
            offset: 0, kind: 1, target: Target::Symbol(import.name.clone()),
        });
        5
    } else {
        sections[1].bytes.resize(20, 0);
        sections[0].bytes.resize(20, 0);
        sections[4].bytes.extend(import.dll.as_bytes());
        sections[4].bytes.push(0);
        for (field, section) in [(0, 2), (12, 4), (16, 3)] {
            sections[0].relocations.push(Relocation {
                offset: field, kind: 3, target: Target::Section(section, 0),
            });
        }
        if sections[4].bytes.len() % 2 != 0 { sections[4].bytes.push(0); }
        let hint = sections[4].bytes.len();
        sections[4].bytes.extend([0, 0]);
        sections[4].bytes.extend(import.name.as_bytes());
        sections[4].bytes.push(0);
        symbols.push(Symbol {
            name: slot, section: 3, value: 0, contract: Vec::new(), function: false,
        });
        for section in [2, 3] {
            sections[section].bytes.extend(u64::try_from(hint)?.to_le_bytes());
            sections[section].relocations.push(Relocation {
                offset: 0, kind: 3, target: Target::Section(4, 0),
            });
            sections[section].bytes.extend([0; 8]);
        }
        3
    };
    let symbol = &symbols[0].name;
    let name = format!("@blink.imports@{symbol}");
    // Native host selection happens after this producer. The actual slot owns
    // the entire import contribution; the owning COMDAT precedes its children.
    sections.swap(0, root);
    let section_index = |index| if index == 0 { root } else if index == root { 0 } else { index };
    for section in &mut sections {
        section.flags |= 0x1000;
        for relocation in &mut section.relocations {
            if let Target::Section(index, _) = &mut relocation.target { *index = section_index(*index); }
        }
    }
    for symbol in &mut symbols { symbol.section = section_index(symbol.section); }
    sections[0].associated = (1..sections.len()).collect();
    let bytes = object::encode_native(&name, &sections, &symbols)?;
    let symbol = symbols.pop().unwrap().name;
    Ok((LibraryMember { name: format!("{name}.obj"), bytes }, symbol))
}

/// 保存已编译模块及其原生导入材料，供C／C++／Rust静态宿主链接。
/// 不读取B源码、不生成执行代码；原生IAT成员不作为B接口模块装入。
pub fn host_archive(inputs: &[impl AsRef<Path>], path: &Path) -> Result<()> {
    let mut members = object::library_members(inputs)?;
    let mut symbols = Vec::new();
    let mut imports = BTreeSet::new();
    for (index, member) in members.iter().enumerate() {
        let object = member.object(false)?;
        if !matches!(object.kind, Kind::Module { .. }) {
            return Err(format!("{} 不是独立模块产物", member.name).into());
        }
        symbols.extend(object.symbols.into_iter().map(|symbol| (index, symbol.name)));
        imports.extend(object.sections.into_iter().flat_map(|section| section.relocations)
            .filter_map(|relocation| match relocation.target { Target::Import(import) => Some(import), _ => None }));
    }
    for import in imports {
        let (member, symbol) = self::member(&import)?;
        symbols.push((members.len(), symbol));
        members.push(member);
    }
    let output = object::library_bytes(&members, &symbols)?;
    let path = std::path::absolute(path)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, output)?;
    Ok(())
}
