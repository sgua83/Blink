//! PE32+ 的节、平台导入、DIR64 重定位与 x64 展开信息均由本机链接器生产。
use crate::native::{ApplicationCode, NativeIntrinsic, NativeSymbol, application_unwind};
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
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
fn relative(v: &mut [u8], at: usize, target: usize) -> Result<()> {
    put32(v, at, i32::try_from(target as i64 - at as i64 - 4)? as u32);
    Ok(())
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Import {
    dll: String,
    name: String,
}
fn kernel(name: &'static str) -> Import {
    Import {
        dll: "KERNEL32.dll".into(),
        name: name.into(),
    }
}
fn ucrt(name: &'static str) -> Import {
    Import {
        dll: "ucrtbase.dll".into(),
        name: name.into(),
    }
}
fn math(op: NativeIntrinsic) -> Option<Import> {
    let name = match op {
        NativeIntrinsic::Sin => "sinf",
        NativeIntrinsic::Cos => "cosf",
        NativeIntrinsic::Floor => "floorf",
        NativeIntrinsic::Exp => "expf",
        NativeIntrinsic::Exp2 => "exp2f",
        NativeIntrinsic::Log => "logf",
        NativeIntrinsic::Log2 => "log2f",
        NativeIntrinsic::Pow => "powf",
        NativeIntrinsic::PrintU32
        | NativeIntrinsic::PrintI32
        | NativeIntrinsic::PrintBool
        | NativeIntrinsic::PrintF32 => {
            return None;
        }
    };
    Some(ucrt(name))
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
struct Function {
    begin: usize,
    end: usize,
    unwind: Vec<u8>,
}
struct Helper {
    bytes: Vec<u8>,
    patches: Vec<(usize, Import)>,
    unwind: Vec<u8>,
}
fn jmp(v: &mut Vec<u8>, condition: Option<u8>) -> usize {
    if let Some(c) = condition {
        v.extend([0x0f, c]);
    } else {
        v.push(0xe9);
    }
    let at = v.len();
    v.extend([0; 4]);
    at
}
fn call(v: &mut Vec<u8>, patches: &mut Vec<(usize, Import)>, import: Import) {
    v.extend([0xff, 0x15]);
    let at = v.len();
    v.extend([0; 4]);
    patches.push((at, import));
}
fn print(op: NativeIntrinsic) -> Result<Helper> {
    let signed = op == NativeIntrinsic::PrintI32;
    let boolean = op == NativeIntrinsic::PrintBool;
    let float = op == NativeIntrinsic::PrintF32;
    let saved = if signed { vec![3, 6, 7] } else { vec![3, 6] };
    let frame = if float {
        120u8
    } else if signed {
        80u8
    } else {
        88u8
    };
    let mut v = Vec::new();
    let mut operations = Vec::new();
    let mut patches = Vec::new();
    let mut failures = Vec::new();
    for &reg in &saved {
        v.push(0x50 + reg);
        let at = v.len() as u8;
        operations.push((at, vec![at, reg << 4]));
    }
    v.extend([0x48, 0x83, 0xec, frame]);
    let at = v.len() as u8;
    operations.push((at, vec![at, ((frame / 8 - 1) << 4) | 2]));
    let unwind = application_unwind(at, operations);
    if float {
        // intrinsic 的参数仍是原始 32 位；只为 C 的 %g 参数提升到 double。
        v.extend([
            0x66, 0x0f, 0x6e, 0xc1, 0xf3, 0x0f, 0x5a, 0xc0, 0xf2, 0x0f, 0x11, 0x44, 0x24, 0x38,
            0x66, 0xc7, 0x44, 0x24, 0x40, b'C', 0, 0xc7, 0x44, 0x24, 0x48, b'%', b'.', b'9', b'g',
            0x66, 0xc7, 0x44, 0x24, 0x4c, 10, 0, 0xb9, 4, 0, 0, 0, 0x48, 0x8d, 0x54, 0x24, 0x40,
        ]);
        call(&mut v, &mut patches, ucrt("_create_locale"));
        v.extend([0x48, 0x85, 0xc0]);
        failures.push(jmp(&mut v, Some(0x84)));
        v.extend([
            0x48, 0x89, 0xc6, 0xb9, 2, 0, 0, 0, 0x48, 0x8d, 0x54, 0x24, 0x50, 0x41, 0xb8, 32, 0, 0,
            0, 0x4c, 0x8d, 0x4c, 0x24, 0x48, 0x48, 0x89, 0x74, 0x24, 0x20, 0x48, 0x8d, 0x44, 0x24,
            0x38, 0x48, 0x89, 0x44, 0x24, 0x28,
        ]);
        call(&mut v, &mut patches, ucrt("__stdio_common_vsprintf"));
        v.extend([0x89, 0xc3, 0x48, 0x89, 0xf1]);
        call(&mut v, &mut patches, ucrt("_free_locale"));
        v.extend([0x85, 0xdb]);
        failures.push(jmp(&mut v, Some(0x88)));
        v.extend([0x48, 0x8d, 0x74, 0x24, 0x50]);
    } else if boolean {
        v.extend([0x85, 0xc9]);
        let no = jmp(&mut v, Some(0x84));
        v.extend([0xc7, 0x44, 0x24, 0x30]);
        v.extend(u32::from_le_bytes(*b"true").to_le_bytes());
        v.extend([0xc6, 0x44, 0x24, 0x34, 10, 0xbb, 5, 0, 0, 0]);
        let end = jmp(&mut v, None);
        let target = v.len();
        relative(&mut v, no, target)?;
        v.extend([0xc7, 0x44, 0x24, 0x30]);
        v.extend(u32::from_le_bytes(*b"fals").to_le_bytes());
        v.extend([0x66, 0xc7, 0x44, 0x24, 0x34, b'e', 10, 0xbb, 6, 0, 0, 0]);
        let target = v.len();
        relative(&mut v, end, target)?;
        v.extend([0x48, 0x8d, 0x74, 0x24, 0x30]);
    } else {
        v.extend([0x89, 0xc8]);
        if signed {
            v.extend([0x31, 0xff, 0x85, 0xc0]);
            let positive = jmp(&mut v, Some(0x89));
            v.extend([0xf7, 0xd8, 0xbf, 1, 0, 0, 0]);
            let target = v.len();
            relative(&mut v, positive, target)?;
        }
        v.extend([
            0x48, 0x8d, 0x74, 0x24, 0x40, 0xc6, 0x06, 10, 0xbb, 1, 0, 0, 0, 0xb9, 10, 0, 0, 0,
        ]);
        let begin = v.len();
        v.extend([
            0x31, 0xd2, 0xf7, 0xf1, 0x80, 0xc2, b'0', 0x48, 0xff, 0xce, 0x88, 0x16, 0xff, 0xc3,
            0x85, 0xc0,
        ]);
        let back = jmp(&mut v, Some(0x85));
        relative(&mut v, back, begin)?;
        if signed {
            v.extend([0x85, 0xff]);
            let positive = jmp(&mut v, Some(0x84));
            v.extend([0x48, 0xff, 0xce, 0xc6, 0x06, b'-', 0xff, 0xc3]);
            let target = v.len();
            relative(&mut v, positive, target)?;
        }
    }
    v.extend([0xb9]);
    v.extend((-11i32).to_le_bytes());
    call(&mut v, &mut patches, kernel("GetStdHandle"));
    v.extend([
        0x48, 0x89, 0xc1, 0x48, 0x89, 0xf2, 0x41, 0x89, 0xd8, 0x4c, 0x8d, 0x4c, 0x24, 0x2c, 0x48,
        0xc7, 0x44, 0x24, 0x20, 0, 0, 0, 0,
    ]);
    call(&mut v, &mut patches, kernel("WriteFile"));
    v.extend([0x85, 0xc0]);
    failures.push(jmp(&mut v, Some(0x84)));
    v.extend([0x39, 0x5c, 0x24, 0x2c, 0x0f, 0x94, 0xc0, 0x0f, 0xb6, 0xc0]);
    let finish = jmp(&mut v, None);
    let target = v.len();
    for failed in failures {
        relative(&mut v, failed, target)?;
    }
    v.extend([0x31, 0xc0]);
    let target = v.len();
    relative(&mut v, finish, target)?;
    v.extend([0x48, 0x83, 0xc4, frame]);
    for &reg in saved.iter().rev() {
        v.push(0x58 + reg);
    }
    v.push(0xc3);
    Ok(Helper {
        bytes: v,
        patches,
        unwind,
    })
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
    name: &'static [u8],
    rva: u32,
    bytes: Vec<u8>,
    flags: u32,
}
fn thunk(text: &mut Vec<u8>, patches: &mut Vec<(usize, Import)>, import: Import) {
    text.extend([0xff, 0x25]);
    let at = text.len();
    text.extend([0; 4]);
    patches.push((at, import));
}
pub(super) fn link(native: ApplicationCode) -> Result<Vec<u8>> {
    let mut text = vec![0x48, 0x83, 0xec, 0x28, 0xe8, 0, 0, 0, 0, 0x89, 0xc1];
    let mut patches = Vec::new();
    call(&mut text, &mut patches, kernel("ExitProcess"));
    let mut functions = vec![Function {
        begin: 0,
        end: text.len(),
        unwind: application_unwind(4, vec![(4, vec![4, 0x42])]),
    }];
    text.resize(align(text.len(), 16), 0x90);
    let main = text.len();
    relative(&mut text, 5, main)?;
    text.extend(&native.code.bytes);
    functions.push(Function {
        begin: main,
        end: main + native.function_end as usize,
        unwind: native.unwind,
    });
    let mut targets = BTreeMap::new();
    let operations: BTreeSet<_> = native
        .code
        .relocations
        .iter()
        .map(|r| r.target.clone())
        .collect();
    for op in operations {
        text.resize(align(text.len(), 16), 0x90);
        let begin = text.len();
        targets.insert(op.clone(), begin);
        match op {
            NativeSymbol::External(name) => thunk(
                &mut text,
                &mut patches,
                Import {
                    dll: name.library,
                    name: name.symbol,
                },
            ),
            NativeSymbol::Intrinsic(op) => {
                if let Some(import) = math(op) {
                    thunk(&mut text, &mut patches, import);
                } else {
                    let helper = print(op)?;
                    text.extend(helper.bytes);
                    for (at, import) in helper.patches {
                        patches.push((begin + at, import));
                    }
                    functions.push(Function {
                        begin,
                        end: text.len(),
                        unwind: helper.unwind,
                    });
                }
            }
        }
    }
    let mut locations = Vec::new();
    for relocation in &native.code.relocations {
        let at = main + relocation.offset;
        put64(
            &mut text,
            at,
            BASE + TEXT as u64 + targets[&relocation.target] as u64,
        );
        locations.push(TEXT + at as u32);
    }
    let idata_rva = TEXT + u32::try_from(align(text.len(), 4096))?;
    let table = imports(
        &patches.iter().map(|(_, import)| import.clone()).collect(),
        idata_rva,
    );
    for (at, import) in patches {
        relative(&mut text, at, (table.targets[&import] - TEXT) as usize)?;
    }
    let pdata_rva = idata_rva + u32::try_from(align(table.bytes.len(), 4096))?;
    functions.sort_by_key(|function| function.begin);
    let directory_size = functions.len() * 12;
    let mut pdata = vec![0; directory_size];
    for (i, function) in functions.iter().enumerate() {
        pdata.resize(align(pdata.len(), 4), 0);
        let unwind = pdata.len();
        pdata.extend(&function.unwind);
        put32(&mut pdata, i * 12, TEXT + function.begin as u32);
        put32(&mut pdata, i * 12 + 4, TEXT + function.end as u32);
        put32(&mut pdata, i * 12 + 8, pdata_rva + unwind as u32);
    }
    let reloc = relocation_table(&locations);
    let reloc_rva = pdata_rva + u32::try_from(align(pdata.len(), 4096))?;
    let reloc_size = reloc.len() as u32;
    let mut sections = vec![
        Section {
            name: b".text",
            rva: TEXT,
            bytes: text,
            flags: 0x60000020,
        },
        Section {
            name: b".idata",
            rva: idata_rva,
            bytes: table.bytes,
            flags: 0xc0000040,
        },
        Section {
            name: b".pdata",
            rva: pdata_rva,
            bytes: pdata,
            flags: 0x40000040,
        },
    ];
    if !reloc.is_empty() {
        sections.push(Section {
            name: b".reloc",
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
        align(native.code.stack_bytes as usize + 1024 * 1024, 65536) as u64,
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
        output[header..header + section.name.len()].copy_from_slice(section.name);
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
