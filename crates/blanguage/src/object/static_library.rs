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
            Ok(Self::Library(Library::decode(&bytes, false)?))
        } else {
            Ok(Self::Object(coff::decode(&bytes)?))
        }
    }
    pub(crate) fn imports(&self, imports: &mut crate::module::Imports) -> Result<()> {
        match self {
            Self::Object(object) => imports.object(object)?,
            Self::Library(library) => {
                for member in library.members.values().flatten() {
                    let (name, kind, definitions) = coff::Reader::new(&member.bytes)?.interface()
                        .map_err(|error| format!("静态库成员 {}：{error}", member.name))?;
                    imports.insert(&name, &kind, definitions)?;
                }
            }
        }
        Ok(())
    }
    /// 编译导入已有模块接口；不重编库成员。
    pub fn objects(self) -> Result<Vec<Object>> {
        match self {
            Self::Object(object) => Ok(vec![object]),
            Self::Library(library) => library
                .members
                .into_values()
                .flatten()
                .map(|member| member.object(library.native))
                .collect(),
        }
    }
}
struct Member {
    name: String,
    bytes: Vec<u8>,
}
impl Member {
    fn object(&self, native: bool) -> Result<Object> {
        (if native { coff::Reader::new(&self.bytes)?.native(&self.name) } else { coff::decode(&self.bytes) })
            .map_err(|error| format!("静态库成员 {}：{error}", self.name).into())
    }
}
pub struct Library {
    members: BTreeMap<u32, Option<Member>>,
    symbols: BTreeMap<String, u32>,
    native: bool,
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
    pub(crate) fn native(bytes: &[u8]) -> Result<Self> { Self::decode(bytes, true) }
    fn decode(bytes: &[u8], native: bool) -> Result<Self> {
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
        Ok(Self { members, symbols, native })
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
        let object = member.object(self.native)?;
        if !matches!(object.kind, Kind::Module { .. }) {
            return Err(format!("静态库成员 {} 不是独立模块", member.name).into());
        }
        Ok(Some(object))
    }
}
// The C layout borrows the existing names/objects for one synchronous B call.
// Only the caller owns the views and final Vec; B retains no pointer.
#[repr(C)]
struct ArchiveMember {
    name: *const u8,
    name_length: usize,
    bytes: *const u8,
    byte_length: usize,
    offset: usize,
    name_offset: usize,
}
#[repr(C)]
struct ArchiveSymbol {
    name: *const u8,
    name_length: usize,
    member: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ArchiveInput {
    members: *mut ArchiveMember,
    member_count: usize,
    symbols: *mut ArchiveSymbol,
    symbol_count: usize,
}
// Discriminants and field layout are shared with bootstrap/archive.h.
#[repr(u32)]
#[derive(Clone, Copy)]
enum ArchiveError { None, MemberField, MemberOffset, SymbolCount, MemberCount, MemberIndex }
#[repr(C)]
#[derive(Clone, Copy)]
struct ArchiveLayout {
    length: usize,
    first: usize,
    second: usize,
    names: usize,
    error: ArchiveError,
}
unsafe extern "C" {
    #[link_name = "bootstrap.archive::layout"]
    fn archive_layout(input: ArchiveInput) -> ArchiveLayout;
    #[link_name = "bootstrap.archive::encode"]
    fn archive_encode(input: ArchiveInput, layout: ArchiveLayout, output: *mut u8);
}

/// 将已编译模块保存为标准 COFF 静态库；原对象字节保持，不调用编译器。
pub fn archive(inputs: &[impl AsRef<Path>], path: &Path) -> Result<()> {
    let mut members = Vec::new();
    for input in inputs {
        let input = input.as_ref();
        let bytes = std::fs::read(input)?;
        if bytes.starts_with(SIGNATURE) {
            members.extend(Library::decode(&bytes, false)?.members.into_values().flatten());
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
    for (index, member) in members.iter().enumerate() {
        let object = member.object(false)?;
        if !matches!(object.kind, Kind::Module { .. }) {
            return Err(format!("{} 不是独立模块产物", member.name).into());
        }
        symbols.extend(object.symbols.into_iter().map(|s| (index, s.name)));
    }
    let mut member_views: Vec<_> = members.iter().map(|member| ArchiveMember {
        name: member.name.as_ptr(), name_length: member.name.len(),
        bytes: member.bytes.as_ptr(), byte_length: member.bytes.len(),
        offset: 0, name_offset: 0,
    }).collect();
    let mut symbol_views: Vec<_> = symbols.iter().map(|(member, name)| ArchiveSymbol {
        name: name.as_ptr(), name_length: name.len(), member: *member,
    }).collect();
    let input = ArchiveInput {
        members: member_views.as_mut_ptr(), member_count: member_views.len(),
        symbols: symbol_views.as_mut_ptr(), symbol_count: symbol_views.len(),
    };
    let layout = unsafe { archive_layout(input) };
    match layout.error {
        ArchiveError::None => {}
        ArchiveError::MemberField => return Err("静态库成员头字段超出 COFF 格式".into()),
        ArchiveError::MemberOffset => return Err("静态库成员偏移超出 COFF u32 范围".into()),
        ArchiveError::SymbolCount => return Err("静态库符号数量超出 COFF u32 范围".into()),
        ArchiveError::MemberCount => return Err("静态库成员数量超出 COFF u32 范围".into()),
        ArchiveError::MemberIndex => return Err("静态库符号的成员编号超出 COFF u16 范围".into()),
    }
    let mut output = Vec::<u8>::with_capacity(layout.length);
    // Layout initialized every member position. Encode initializes the complete
    // final buffer, including headers and padding, before the Vec takes its length.
    unsafe {
        archive_encode(input, layout, output.as_mut_ptr());
        output.set_len(layout.length);
    }
    let path = std::path::absolute(path)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, output)?;
    Ok(())
}
