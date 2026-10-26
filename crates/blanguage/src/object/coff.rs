//! COFF 节、符号与 AMD64 重定位；Blink 元数据不替代标准重定位表。
use super::*;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
const META: &str = ".blmeta";
const ARCHIVE: &str = ".blrt";
fn u16_at(b: &mut [u8], p: usize, v: u16) {
    b[p..p + 2].copy_from_slice(&v.to_le_bytes());
}
fn u32_at(b: &mut [u8], p: usize, v: u32) {
    b[p..p + 4].copy_from_slice(&v.to_le_bytes());
}
fn word(b: &mut Vec<u8>, v: u32) {
    b.extend(v.to_le_bytes());
}
fn string(b: &mut Vec<u8>, s: &str) -> Result<()> {
    word(b, u32::try_from(s.len())?);
    b.extend(s.as_bytes());
    Ok(())
}
fn get16(b: &[u8], p: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(slice(b, p, 2)?.try_into()?))
}
fn get32(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(slice(b, p, 4)?.try_into()?))
}
fn slice(b: &[u8], p: usize, n: usize) -> Result<&[u8]> {
    Ok(b.get(p..p.checked_add(n).ok_or("目标文件偏移溢出")?)
        .ok_or("目标文件被截断")?)
}
fn read_word(c: &mut Cursor<&[u8]>) -> Result<u32> {
    let mut b = [0; 4];
    c.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}
fn read_string(c: &mut Cursor<&[u8]>) -> Result<String> {
    let n = read_word(c)? as usize;
    let p = c.position() as usize;
    let s = std::str::from_utf8(slice(c.get_ref(), p, n)?)?.to_owned();
    c.set_position((p + n) as u64);
    Ok(s)
}
fn import_name(i: &Import) -> String {
    format!("__blink_import${}${}{}", i.dll.len(), i.dll, i.name)
}
fn read_import(s: &str) -> Result<Import> {
    let s = s
        .strip_prefix("__blink_import$")
        .ok_or_else(|| format!("目标文件有未解析的符号 {s}"))?;
    let (n, rest) = s.split_once('$').ok_or("DLL 符号缺少长度")?;
    let n: usize = n.parse()?;
    Ok(Import {
        dll: rest.get(..n).ok_or("DLL 符号长度错误")?.into(),
        name: rest.get(n..).ok_or("DLL 符号名称错误")?.into(),
    })
}
struct Symbol {
    name: String,
    section: i16,
    value: u32,
    function: bool,
}
pub(super) fn encode(o: &Object) -> Result<Vec<u8>> {
    let mut meta = Vec::from(*b"BLCO");
    word(&mut meta, 1);
    string(&mut meta, &o.name)?;
    match &o.kind {
        Kind::Application { stack_bytes } => {
            word(&mut meta, 0);
            word(&mut meta, *stack_bytes);
        }
        Kind::Library {
            prefix,
            header,
            binding,
        } => {
            word(&mut meta, 1);
            string(&mut meta, prefix)?;
            string(&mut meta, header)?;
            string(&mut meta, binding)?;
        }
    }
    word(
        &mut meta,
        u32::try_from(o.archive.as_ref().map_or(0, |a| a.libraries.len()))?,
    );
    if let Some(a) = &o.archive {
        for l in &a.libraries {
            string(&mut meta, l)?;
        }
    }
    word(
        &mut meta,
        u32::try_from(o.archive.as_ref().map_or(0, |a| a.exports.len()))?,
    );
    if let Some(a) = &o.archive {
        for e in &a.exports {
            string(&mut meta, e)?;
        }
    }
    let mut sections: Vec<(&str, &[u8], u32, &[Relocation])> = o
        .sections
        .iter()
        .map(|s| {
            (
                s.name.as_str(),
                s.bytes.as_slice(),
                s.flags,
                s.relocations.as_slice(),
            )
        })
        .collect();
    sections.push((META, &meta, 0x840, &[]));
    if let Some(a) = &o.archive {
        sections.push((ARCHIVE, &a.bytes, 0x840, &[]));
    }
    let mut symbols: Vec<Symbol> = o
        .sections
        .iter()
        .enumerate()
        .map(|(i, s)| Symbol {
            name: s.name.clone(),
            section: i as i16 + 1,
            value: 0,
            function: false,
        })
        .collect();
    if matches!(o.kind, Kind::Application { .. }) {
        symbols.push(Symbol {
            name: "mainCRTStartup".into(),
            section: 1,
            value: 0,
            function: true,
        });
    }
    let mut imports = BTreeMap::new();
    for s in &o.sections {
        for r in &s.relocations {
            if let Target::Import(i) = &r.target {
                imports.insert(i.clone(), 0);
            }
        }
    }
    for (i, index) in &mut imports {
        *index = symbols.len();
        symbols.push(Symbol {
            name: import_name(i),
            section: 0,
            value: 0,
            function: true,
        });
    }
    let mut out = vec![0; 20 + 40 * sections.len()];
    u16_at(&mut out, 0, 0x8664);
    u16_at(&mut out, 2, u16::try_from(sections.len())?);
    for (i, (name, bytes, flags, relocs)) in sections.iter().enumerate() {
        let h = 20 + i * 40;
        out[h..h + name.len()].copy_from_slice(name.as_bytes());
        u32_at(&mut out, h + 16, u32::try_from(bytes.len())?);
        let p = out.len();
        u32_at(&mut out, h + 20, u32::try_from(p)?);
        out.extend(*bytes);
        let p = out.len();
        u32_at(&mut out, h + 24, u32::try_from(p)?);
        let overflow = relocs.len() >= u16::MAX as usize;
        u16_at(
            &mut out,
            h + 32,
            if overflow {
                u16::MAX
            } else {
                relocs.len() as u16
            },
        );
        u32_at(
            &mut out,
            h + 36,
            *flags | if overflow { 0x01000000 } else { 0 },
        );
        if overflow {
            word(&mut out, u32::try_from(relocs.len() + 1)?);
            out.extend([0; 6]);
        }
        for r in *relocs {
            word(&mut out, u32::try_from(r.offset)?);
            let symbol = match &r.target {
                Target::Section(i, _) => *i,
                Target::Import(i) => imports[i],
            };
            word(&mut out, u32::try_from(symbol)?);
            out.extend(r.kind.to_le_bytes());
        }
    }
    let p = out.len();
    u32_at(&mut out, 8, u32::try_from(p)?);
    u32_at(&mut out, 12, u32::try_from(symbols.len())?);
    let mut strings = vec![0; 4];
    for symbol in symbols {
        let mut row = [0; 18];
        if symbol.name.len() <= 8 {
            row[..symbol.name.len()].copy_from_slice(symbol.name.as_bytes());
        } else {
            u32_at(&mut row, 4, u32::try_from(strings.len())?);
            strings.extend(symbol.name.as_bytes());
            strings.push(0);
        }
        u32_at(&mut row, 8, symbol.value);
        u16_at(&mut row, 12, symbol.section as u16);
        u16_at(&mut row, 14, if symbol.function { 0x20 } else { 0 });
        row[16] = if symbol.function { 2 } else { 3 };
        out.extend(row);
    }
    let n = u32::try_from(strings.len())?;
    u32_at(&mut strings, 0, n);
    out.extend(strings);
    Ok(out)
}
pub(super) fn decode(b: &[u8]) -> Result<Object> {
    if get16(b, 0)? != 0x8664 || get16(b, 16)? != 0 {
        return Err("需要 Windows x64 COFF 目标文件".into());
    }
    let n = get16(b, 2)? as usize;
    let sym = get32(b, 8)? as usize;
    let count = get32(b, 12)? as usize;
    let strings_at = sym + count * 18;
    let strings = slice(b, strings_at, get32(b, strings_at)? as usize)?;
    let mut symbols = Vec::new();
    for i in 0..count {
        let r = slice(b, sym + i * 18, 18)?;
        let name = if get32(r, 0)? == 0 {
            let p = get32(r, 4)? as usize;
            let tail = strings.get(p..).ok_or("COFF 字符串偏移错误")?;
            let n = tail
                .iter()
                .position(|&c| c == 0)
                .ok_or("COFF 符号缺少结尾")?;
            std::str::from_utf8(&tail[..n])?.to_owned()
        } else {
            std::str::from_utf8(&r[..r[..8].iter().position(|&c| c == 0).unwrap_or(8)])?.to_owned()
        };
        symbols.push((name, get16(r, 12)? as i16, get32(r, 8)?));
    }
    let mut raw = Vec::new();
    for i in 0..n {
        let h = slice(b, 20 + i * 40, 40)?;
        let name =
            std::str::from_utf8(&h[..h[..8].iter().position(|&c| c == 0).unwrap_or(8)])?.to_owned();
        raw.push((
            name,
            slice(b, get32(h, 20)? as usize, get32(h, 16)? as usize)?.to_vec(),
            get32(h, 36)?,
            get32(h, 24)? as usize,
            get16(h, 32)? as usize,
        ));
    }
    let meta = &raw
        .iter()
        .find(|s| s.0 == META)
        .ok_or("缺少 Blink 目标文件元数据")?
        .1;
    let mut c = Cursor::new(meta.as_slice());
    let mut tag = [0; 4];
    c.read_exact(&mut tag)?;
    if tag != *b"BLCO" || read_word(&mut c)? != 1 {
        return Err("不支持的 Blink 目标文件版本".into());
    }
    let name = read_string(&mut c)?;
    let kind = match read_word(&mut c)? {
        0 => Kind::Application {
            stack_bytes: read_word(&mut c)?,
        },
        1 => Kind::Library {
            prefix: read_string(&mut c)?,
            header: read_string(&mut c)?,
            binding: read_string(&mut c)?,
        },
        _ => return Err("未知目标文件种类".into()),
    };
    let mut libraries = Vec::new();
    for _ in 0..read_word(&mut c)? {
        libraries.push(read_string(&mut c)?);
    }
    let mut exports = Vec::new();
    for _ in 0..read_word(&mut c)? {
        exports.push(read_string(&mut c)?);
    }
    let archive = raw.iter().find(|s| s.0 == ARCHIVE).map(|s| Archive {
        bytes: s.1.clone(),
        libraries,
        exports,
    });
    let mut mapping = BTreeMap::new();
    let mut sections = Vec::new();
    for (i, (name, bytes, flags, _, _)) in raw.iter().enumerate() {
        if name != META && name != ARCHIVE {
            mapping.insert(i, sections.len());
            sections.push(Section {
                name: name.clone(),
                bytes: bytes.clone(),
                flags: *flags,
                relocations: Vec::new(),
            });
        }
    }
    for (old, index) in &mapping {
        let (_, _, flags, p, count) = &raw[*old];
        let (mut p, mut count) = (*p, *count);
        if flags & 0x01000000 != 0 {
            count = (get32(b, p)? as usize)
                .checked_sub(1)
                .ok_or("COFF 重定位数量错误")?;
            p += 10;
        }
        for i in 0..count {
            let r = slice(b, p + i * 10, 10)?;
            let s = symbols
                .get(get32(r, 4)? as usize)
                .ok_or("COFF 重定位符号越界")?;
            let target = if s.1 > 0 {
                Target::Section(
                    *mapping
                        .get(&(s.1 as usize - 1))
                        .ok_or("COFF 引用了不可装载的节")?,
                    s.2,
                )
            } else {
                Target::Import(read_import(&s.0)?)
            };
            sections[*index].relocations.push(Relocation {
                offset: get32(r, 0)? as usize,
                kind: get16(r, 8)?,
                target,
            });
        }
    }
    Ok(Object {
        name,
        kind,
        sections,
        archive,
    })
}
