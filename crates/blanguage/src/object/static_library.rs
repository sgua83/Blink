//! 标准 COFF 静态库保存原对象；目录归库所有，实际符号绑定仍归链接器。
use super::{Kind, Object, Result, coff};
use std::{collections::BTreeMap, path::Path};
const SIGNATURE: &[u8; 8] = b"!<arch>\n";

pub enum Input {
    Object(Object),
    Library(Library),
}
impl Input {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        if bytes.starts_with(SIGNATURE) {
            Ok(Self::Library(Library::decode(&bytes)?))
        } else {
            Ok(Self::Object(coff::decode(&bytes)?))
        }
    }
    /// 编译导入已有模块接口；不重编库成员。
    pub fn objects(self) -> Result<Vec<Object>> {
        match self {
            Self::Object(object) => Ok(vec![object]),
            Self::Library(library) => library
                .members
                .into_values()
                .flatten()
                .map(|member| member.object())
                .collect(),
        }
    }
}
struct Member {
    name: String,
    bytes: Vec<u8>,
}
impl Member {
    fn object(&self) -> Result<Object> {
        coff::decode(&self.bytes)
            .map_err(|error| format!("静态库成员 {}：{error}", self.name).into())
    }
}
pub struct Library {
    members: BTreeMap<u32, Option<Member>>,
    symbols: BTreeMap<String, u32>,
}
fn take<'a>(bytes: &'a [u8], at: &mut usize, size: usize) -> Result<&'a [u8]> {
    let end = at.checked_add(size).ok_or("静态库偏移溢出")?;
    let value = bytes.get(*at..end).ok_or("静态库被截断")?;
    *at = end;
    Ok(value)
}
fn word(bytes: &[u8], at: &mut usize) -> Result<u32> {
    Ok(u32::from_le_bytes(take(bytes, at, 4)?.try_into()?))
}
fn text<'a>(bytes: &'a [u8], at: &mut usize) -> Result<&'a str> {
    let rest = bytes.get(*at..).ok_or("静态库字符串偏移错误")?;
    let count = rest
        .iter()
        .position(|b| *b == 0)
        .ok_or("静态库字符串未终止")?;
    let value = std::str::from_utf8(take(bytes, at, count)?)?;
    take(bytes, at, 1)?;
    Ok(value)
}
impl Library {
    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut at = SIGNATURE.len();
        let mut directory = Vec::new();
        let mut longnames = &[][..];
        let mut raw = Vec::new();
        while at < bytes.len() {
            let offset = u32::try_from(at)?;
            let header = take(bytes, &mut at, 60)?;
            if &header[58..] != b"`\n" {
                return Err("静态库成员头错误".into());
            }
            let name = std::str::from_utf8(&header[..16])?.trim_end();
            let size = std::str::from_utf8(&header[48..58])?.trim().parse()?;
            let body = take(bytes, &mut at, size)?;
            if at % 2 != 0 {
                take(bytes, &mut at, 1)?;
            }
            match name {
                "/" => directory.push(body),
                "//" => longnames = body,
                _ => raw.push((offset, name, body)),
            }
        }
        // COFF 的第二链接成员直接给出排序符号与成员偏移；不重建另一套目录。
        let directory = directory.get(1).ok_or("COFF 静态库缺少第二链接成员")?;
        let mut at = 0;
        let count = word(directory, &mut at)?;
        let offsets = (0..count)
            .map(|_| word(directory, &mut at))
            .collect::<Result<Vec<_>>>()?;
        let count = word(directory, &mut at)?;
        let indices = (0..count)
            .map(|_| Ok(u16::from_le_bytes(take(directory, &mut at, 2)?.try_into()?)))
            .collect::<Result<Vec<_>>>()?;
        let mut symbols = BTreeMap::new();
        for index in indices {
            let member = index.checked_sub(1).ok_or("静态库成员索引为零")?;
            let offset = *offsets
                .get(usize::from(member))
                .ok_or("静态库成员索引越界")?;
            symbols
                .entry(text(directory, &mut at)?.into())
                .or_insert(offset);
        }
        let members = raw
            .into_iter()
            .map(|(offset, name, bytes)| {
                let name = if let Some(index) = name.strip_prefix('/') {
                    let mut at = index.parse()?;
                    text(longnames, &mut at)?
                } else {
                    name.strip_suffix('/').ok_or("静态库成员名未终止")?
                };
                Ok((
                    offset,
                    Some(Member {
                        name: name.into(),
                        bytes: bytes.into(),
                    }),
                ))
            })
            .collect::<Result<_>>()?;
        Ok(Self { members, symbols })
    }
    pub(crate) fn extract(&mut self, symbol: &str) -> Result<Option<Object>> {
        let Some(offset) = self.symbols.get(symbol) else {
            return Ok(None);
        };
        let member = self
            .members
            .get_mut(offset)
            .ok_or("静态库目录未指向对象成员")?;
        let Some(member) = member.take() else {
            return Ok(None);
        };
        let object = member.object()?;
        if !matches!(object.kind, Kind::Module { .. }) {
            return Err(format!("静态库成员 {} 不是独立模块", member.name).into());
        }
        Ok(Some(object))
    }
}
fn member(output: &mut Vec<u8>, name: &str, bytes: &[u8]) -> Result<()> {
    let mut header = [b' '; 60];
    for (begin, width, value) in [
        (0, 16, name.to_owned()),
        (16, 12, "0".into()),
        (40, 8, "0".into()),
        (48, 10, bytes.len().to_string()),
    ] {
        if value.len() > width {
            return Err("静态库成员头字段超出 COFF 格式".into());
        }
        header[begin..begin + value.len()].copy_from_slice(value.as_bytes());
    }
    header[58..].copy_from_slice(b"`\n");
    output.extend(header);
    output.extend(bytes);
    if output.len() % 2 != 0 {
        output.push(b'\n');
    }
    Ok(())
}
fn span(size: usize) -> usize {
    60 + size + size % 2
}

/// 将已编译模块保存为标准 COFF 静态库；原对象字节保持，不调用编译器。
pub fn archive(inputs: &[impl AsRef<Path>], path: &Path) -> Result<()> {
    let mut members = Vec::new();
    for input in inputs {
        let input = input.as_ref();
        let bytes = std::fs::read(input)?;
        if bytes.starts_with(SIGNATURE) {
            members.extend(Library::decode(&bytes)?.members.into_values().flatten());
        } else {
            members.push(Member {
                name: input
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or("静态库输入名不是 UTF-8")?
                    .into(),
                bytes,
            });
        }
    }
    let mut symbols = Vec::new();
    let mut longnames = Vec::new();
    let mut names = Vec::new();
    for (index, member) in members.iter().enumerate() {
        let object = member.object()?;
        if !matches!(object.kind, Kind::Module { .. }) {
            return Err(format!("{} 不是独立模块产物", member.name).into());
        }
        symbols.extend(object.symbols.into_iter().map(|s| (index, s.name)));
        names.push(longnames.len());
        longnames.extend(member.name.as_bytes());
        longnames.push(0);
    }
    let strings = symbols
        .iter()
        .map(|(_, name)| name.len() + 1)
        .sum::<usize>();
    let first = 4 + 4 * symbols.len() + strings;
    let second = 8 + 4 * members.len() + 2 * symbols.len() + strings;
    let mut at = SIGNATURE.len() + span(first) + span(second) + span(longnames.len());
    let mut offsets = Vec::new();
    for member in &members {
        offsets.push(u32::try_from(at)?);
        at = at
            .checked_add(span(member.bytes.len()))
            .ok_or("静态库大小溢出")?;
    }
    let count = u32::try_from(symbols.len())?;
    let mut directory = count.to_be_bytes().to_vec();
    for (index, _) in &symbols {
        directory.extend(offsets[*index].to_be_bytes());
    }
    for (_, name) in &symbols {
        directory.extend(name.as_bytes());
        directory.push(0);
    }
    let mut output = SIGNATURE.to_vec();
    member(&mut output, "/", &directory)?;
    directory.clear();
    directory.extend(u32::try_from(members.len())?.to_le_bytes());
    for offset in &offsets {
        directory.extend(offset.to_le_bytes());
    }
    directory.extend(count.to_le_bytes());
    symbols.sort_by(|(_, a), (_, b)| a.cmp(b));
    for (index, _) in &symbols {
        directory.extend(u16::try_from(index + 1)?.to_le_bytes());
    }
    for (_, name) in &symbols {
        directory.extend(name.as_bytes());
        directory.push(0);
    }
    member(&mut output, "/", &directory)?;
    member(&mut output, "//", &longnames)?;
    for (item, name) in members.iter().zip(names) {
        member(&mut output, &format!("/{name}"), &item.bytes)?;
    }
    let path = std::path::absolute(path)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, output)?;
    Ok(())
}
