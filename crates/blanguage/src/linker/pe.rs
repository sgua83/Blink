//! 组合已编译 COFF 节、解析符号与重定位，写出 PE；不生成函数机器码。
use super::{RUNTIME_LIBRARY, Result};
use crate::object::{Import, Kind, Object, Target};
use std::collections::{BTreeMap, BTreeSet};
const BASE: u64 = 0x140000000;
const TEXT: u32 = 0x1000;
fn align(n: usize, a: usize) -> usize {
    n.div_ceil(a) * a
}
fn put16(v: &mut [u8], at: usize, n: u16) {
    v[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn put32(v: &mut [u8], at: usize, n: u32) {
    v[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn put64(v: &mut [u8], at: usize, n: u64) {
    v[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
struct ImportTable {
    bytes: Vec<u8>,
    targets: BTreeMap<Import, u32>,
    directory_size: u32,
    iat_start: u32,
    iat_size: u32,
}
fn imports(values: &BTreeSet<Import>, rva: u32) -> ImportTable {
    let mut groups: BTreeMap<&str, Vec<Import>> = BTreeMap::new();
    for value in values {
        groups.entry(&value.dll).or_default().push(value.clone());
    }
    let directory_size = (groups.len() + 1) * 20;
    let mut bytes = vec![0; align(directory_size, 8)];
    let mut lookups = Vec::new();
    for group in groups.values() {
        let at = bytes.len();
        bytes.resize(at + (group.len() + 1) * 8, 0);
        lookups.push(at);
    }
    let iat_start = bytes.len();
    let mut tables = Vec::new();
    for group in groups.values() {
        let at = bytes.len();
        bytes.resize(at + (group.len() + 1) * 8, 0);
        tables.push(at);
    }
    let iat_end = bytes.len();
    let mut targets = BTreeMap::new();
    for (i, (dll, group)) in groups.iter().enumerate() {
        let name = bytes.len();
        bytes.extend(dll.as_bytes());
        bytes.push(0);
        put32(&mut bytes, i * 20, rva + lookups[i] as u32);
        put32(&mut bytes, i * 20 + 12, rva + name as u32);
        put32(&mut bytes, i * 20 + 16, rva + tables[i] as u32);
        for (j, value) in group.iter().enumerate() {
            bytes.resize(align(bytes.len(), 2), 0);
            let hint = bytes.len();
            bytes.extend([0, 0]);
            bytes.extend(value.name.as_bytes());
            bytes.push(0);
            put64(&mut bytes, lookups[i] + j * 8, (rva + hint as u32) as u64);
            put64(&mut bytes, tables[i] + j * 8, (rva + hint as u32) as u64);
            targets.insert(value.clone(), rva + (tables[i] + j * 8) as u32);
        }
    }
    ImportTable {
        bytes,
        targets,
        directory_size: directory_size as u32,
        iat_start: rva + iat_start as u32,
        iat_size: (iat_end - iat_start) as u32,
    }
}
fn relocation_table(locations: &[u32]) -> Vec<u8> {
    let mut pages: BTreeMap<u32, Vec<u16>> = BTreeMap::new();
    for &location in locations {
        pages
            .entry(location & !4095)
            .or_default()
            .push(0xa000 | (location & 4095) as u16);
    }
    let mut bytes = Vec::new();
    for (page, mut entries) in pages {
        if entries.len() % 2 != 0 {
            entries.push(0);
        }
        bytes.extend(page.to_le_bytes());
        bytes.extend((8 + entries.len() as u32 * 2).to_le_bytes());
        for entry in entries {
            bytes.extend(entry.to_le_bytes());
        }
    }
    bytes
}

struct Section {
    name: Vec<u8>,
    rva: u32,
    bytes: Vec<u8>,
    flags: u32,
}
pub(super) fn link(object: &Object, runtime: Option<&str>) -> Result<Vec<u8>> {
    let stack_bytes = match object.kind {
        Kind::Application { stack_bytes } => stack_bytes,
        _ => return Err("需要应用目标文件".into()),
    };
    let physical = |i: &Import| -> Result<Import> {
        Ok(Import {
            dll: if i.dll == RUNTIME_LIBRARY {
                runtime.ok_or("应用缺少已链接执行库")?.into()
            } else {
                i.dll.clone()
            },
            name: i.name.clone(),
        })
    };
    let mut values = BTreeSet::new();
    for section in &object.sections {
        for r in &section.relocations {
            if let Target::Import(i) = &r.target {
                values.insert(physical(i)?);
            }
        }
    }
    let mut sections = Vec::new();
    let mut rva = TEXT;
    for section in &object.sections {
        sections.push(Section {
            name: section.name.as_bytes().to_vec(),
            rva,
            bytes: section.bytes.clone(),
            flags: section.flags & !0x01f00000,
        });
        rva += u32::try_from(align(section.bytes.len(), 4096))?;
    }
    let idata_rva = rva;
    let table = imports(&values, idata_rva);
    let mut locations = Vec::new();
    for (index, section) in object.sections.iter().enumerate() {
        for relocation in &section.relocations {
            let target = match &relocation.target {
                Target::Section(i, value) => {
                    sections.get(*i).ok_or("重定位引用未知节")?.rva as u64 + *value as u64
                }
                Target::Import(i) => table.targets[&physical(i)?] as u64,
                Target::Symbol(name, _) => return Err(format!("未定义模块符号 {name}").into()),
            };
            let at = relocation.offset;
            let source = sections[index].rva as u64 + at as u64;
            let bytes = &mut sections[index].bytes;
            let width = if relocation.kind == 1 { 8 } else { 4 };
            let old = bytes.get(at..at + width).ok_or("COFF 重定位位置越界")?;
            match relocation.kind {
                1 => {
                    let add = u64::from_le_bytes(old.try_into()?);
                    put64(bytes, at, add.wrapping_add(BASE + target));
                    locations.push(source as u32);
                }
                3 => {
                    let add = u32::from_le_bytes(old.try_into()?);
                    put32(bytes, at, u32::try_from(target)?.wrapping_add(add));
                }
                4 => {
                    let add = i32::from_le_bytes(old.try_into()?);
                    let value = i32::try_from(target as i64 - source as i64 - 4 + add as i64)?;
                    put32(bytes, at, value as u32);
                }
                kind => return Err(format!("不支持 AMD64 重定位类型 {kind}").into()),
            }
        }
    }
    let pdata = sections
        .iter()
        .find(|s| s.name == b".pdata")
        .ok_or("应用目标文件缺少展开信息")?;
    let pdata_rva = pdata.rva;
    let directory_size = pdata.bytes.len();
    sections.push(Section {
        name: b".idata".to_vec(),
        rva: idata_rva,
        bytes: table.bytes.clone(),
        flags: 0xc0000040,
    });
    let reloc = relocation_table(&locations);
    let reloc_size = reloc.len() as u32;
    let reloc_rva = idata_rva + u32::try_from(align(table.bytes.len(), 4096))?;
    if !reloc.is_empty() {
        sections.push(Section {
            name: b".reloc".to_vec(),
            rva: reloc_rva,
            bytes: reloc,
            flags: 0x42000040,
        });
    }
    let header_size = align(0x80 + 24 + 240 + sections.len() * 40, 512);
    let mut output = vec![0; header_size];
    output[0..2].copy_from_slice(b"MZ");
    put32(&mut output, 0x3c, 0x80);
    output[0x80..0x84].copy_from_slice(b"PE\0\0");
    let file = 0x84;
    put16(&mut output, file, 0x8664);
    put16(&mut output, file + 2, sections.len() as u16);
    put16(&mut output, file + 16, 240);
    put16(&mut output, file + 18, 0x22);
    let optional = file + 20;
    put16(&mut output, optional, 0x20b);
    put32(
        &mut output,
        optional + 4,
        align(sections[0].bytes.len(), 512) as u32,
    );
    put32(
        &mut output,
        optional + 8,
        sections
            .iter()
            .skip(1)
            .map(|section| align(section.bytes.len(), 512) as u32)
            .sum(),
    );
    put32(&mut output, optional + 16, TEXT);
    put32(&mut output, optional + 20, TEXT);
    put64(&mut output, optional + 24, BASE);
    put32(&mut output, optional + 32, 4096);
    put32(&mut output, optional + 36, 512);
    put16(&mut output, optional + 40, 6);
    put16(&mut output, optional + 48, 6);
    let last = sections.last().unwrap();
    put32(
        &mut output,
        optional + 56,
        last.rva + u32::try_from(align(last.bytes.len(), 4096))?,
    );
    put32(&mut output, optional + 60, header_size as u32);
    put16(&mut output, optional + 68, 3);
    put16(
        &mut output,
        optional + 70,
        0x100 | if reloc_size != 0 { 0x60 } else { 0 },
    );
    put64(
        &mut output,
        optional + 72,
        align(stack_bytes as usize + 1024 * 1024, 65536) as u64,
    );
    put64(&mut output, optional + 80, 4096);
    put64(&mut output, optional + 88, 1024 * 1024);
    put64(&mut output, optional + 96, 4096);
    put32(&mut output, optional + 108, 16);
    for (index, rva, size) in [
        (1, idata_rva, table.directory_size),
        (3, pdata_rva, directory_size as u32),
        (5, if reloc_size == 0 { 0 } else { reloc_rva }, reloc_size),
        (12, table.iat_start, table.iat_size),
    ] {
        put32(&mut output, optional + 112 + index * 8, rva);
        put32(&mut output, optional + 116 + index * 8, size);
    }
    for (i, section) in sections.iter().enumerate() {
        let header = optional + 240 + i * 40;
        output[header..header + section.name.len()].copy_from_slice(&section.name);
        put32(&mut output, header + 8, section.bytes.len() as u32);
        put32(&mut output, header + 12, section.rva);
        put32(
            &mut output,
            header + 16,
            align(section.bytes.len(), 512) as u32,
        );
        let raw = output.len();
        put32(&mut output, header + 20, raw as u32);
        put32(&mut output, header + 36, section.flags);
        output.extend(&section.bytes);
        output.resize(align(output.len(), 512), 0);
    }
    Ok(output)
}
