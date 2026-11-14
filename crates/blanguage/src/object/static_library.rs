//! 标准 COFF 静态库保存原对象；目录归库所有，实际符号绑定仍归链接器。
use super::{Kind, Object, Result, coff};
use crate::file_bytes::FileBytes;
use std::{fs::File, ops::{Deref, Range}, path::Path, ptr::NonNull, sync::Arc};
fn has_signature(bytes: &[u8]) -> bool {
    unsafe { archive_has_signature(bytes.as_ptr(), bytes.len()) }
}

pub enum Input {
    Object(Object),
    Library(Library),
}
impl Input {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = ObjectBytes::read(path)?;
        if has_signature(&bytes) {
            Ok(Self::Library(Library::decode(bytes, false)?))
        } else {
            Ok(Self::Object(coff::decode(&bytes)?))
        }
    }
    pub(crate) fn imports(&self, imports: &mut crate::module::Imports) -> Result<()> {
        match self {
            Self::Object(object) => imports.object(object)?,
            Self::Library(library) => {
                for index in 0..library.member_count() {
                    let member=library.member(index);
                    if member.available==0 { continue; }
                    let (name,bytes)=library.member_bytes(member.member);
                    let (module,kind,definitions)=coff::Reader::new(&bytes)?.interface()
                        .map_err(|error| format!("静态库成员 {name}：{error}"))?;
                    imports.insert(&module,&kind,definitions)?;
                }
            }
        }
        Ok(())
    }
    /// 编译导入已有模块接口；不重编库成员。
    pub fn objects(self) -> Result<Vec<Object>> {
        match self {
            Self::Object(object) => Ok(vec![object]),
            Self::Library(library) => (0..library.member_count())
                .filter_map(|index| {
                    let member=library.member(index);
                    (member.available!=0).then(|| {
                        let (name,bytes)=library.member_bytes(member.member);
                        member_object(name,&bytes,library.native)
                    })
                }).collect(),
        }
    }
}
#[derive(Clone)]
enum Storage {
    Captured(Arc<FileBytes>),
    Generated(Arc<Vec<u8>>),
}

// The original snapshot owns storage. Each consumer holds the range it reads.
#[derive(Clone)]
pub(crate) struct ObjectBytes {
    storage: Storage,
    range: Range<usize>,
}

impl ObjectBytes {
    pub(crate) fn read(path: &Path) -> Result<Self> {
        Self::capture(&File::open(path)?)
    }

    pub(crate) fn capture(file: &File) -> Result<Self> {
        let bytes = FileBytes::read(file)?;
        let range = 0..bytes.len();
        Ok(Self { storage: Storage::Captured(Arc::new(bytes)), range })
    }

    fn generated(bytes: Vec<u8>) -> Self {
        let range = 0..bytes.len();
        Self { storage: Storage::Generated(Arc::new(bytes)), range }
    }

    pub(super) fn slice(&self, range: Range<usize>) -> Self {
        let start = self.range.start;
        Self { storage: self.storage.clone(), range: start + range.start..start + range.end }
    }
}

impl Deref for ObjectBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        let bytes: &[u8] = match &self.storage {
            Storage::Captured(bytes) => bytes,
            Storage::Generated(bytes) => bytes,
        };
        &bytes[self.range.clone()]
    }
}

impl AsRef<[u8]> for ObjectBytes {
    fn as_ref(&self) -> &[u8] { self }
}

pub(crate) struct Member {
    pub(crate) name: String,
    bytes: ObjectBytes,
}
impl Member {
    pub(crate) fn new(name:String,bytes:Vec<u8>)->Self {
        Self { name, bytes: ObjectBytes::generated(bytes) }
    }
    fn bytes(&self)->&[u8] { &self.bytes }
    pub(crate) fn host_links(&self)->Result<Option<(Vec<String>,Vec<super::Import>)>> {
        coff::Reader::new(&self.bytes).and_then(|reader|reader.host_links())
            .map_err(|error|format!("静态库成员 {}：{error}",self.name).into())
    }
    pub(crate) fn archive_symbols(&self)->Result<Option<Vec<String>>> {
        coff::Reader::new(&self.bytes).and_then(|reader|reader.archive_symbols())
            .map_err(|error|format!("静态库成员 {}：{error}",self.name).into())
    }
}
fn member_object(name:&str,bytes:&ObjectBytes,native:bool)->Result<Object> {
    (if native {coff::Reader::new(bytes)?.native(name)} else {coff::decode(bytes)})
        .map_err(|error|format!("静态库成员 {name}：{error}").into())
}
pub struct Library {
    directory: NonNull<std::ffi::c_void>,
    bytes: ObjectBytes,
    native: bool,
}
// The compiler moves this unique directory into its native worker. B keeps
// only CRT allocations and immutable input ranges; mutation requires &mut Self.
unsafe impl Send for Library {}
impl Drop for Library {
    fn drop(&mut self) { unsafe {library_destroy(self.directory.as_ptr());} }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ArchiveRange { start: usize, length: usize }
impl ArchiveRange {
    fn range(self) -> Range<usize> { self.start..self.start + self.length }
    fn bytes<'a>(&self, bytes: &'a [u8]) -> &'a [u8] { &bytes[self.range()] }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ArchiveReadMember { offset: u32, name: ArchiveRange, body: ArchiveRange }
#[repr(u32)]
#[derive(Clone, Copy)]
enum ArchiveReadError {
    None, Truncated, Header, Decimal, MissingDirectory, ZeroIndex,
    IndexRange, StringOffset, UnterminatedString, UnterminatedName, MemberOffset, Receiver, Allocation,
}
#[repr(C)]
struct ArchiveReadResult { error: ArchiveReadError, at: usize }
#[repr(C)]
struct ArchiveLoad {
    library: *mut std::ffi::c_void,
    read: ArchiveReadResult,
}
unsafe extern "C" {
    #[link_name = "bootstrap.archive::has_signature"]
    fn archive_has_signature(bytes: *const u8, length: usize) -> bool;
    #[link_name = "bootstrap.archive::load"]
    fn archive_load(bytes: *const u8, length: usize, context: *mut std::ffi::c_void,
        validate: unsafe extern "C" fn(*mut std::ffi::c_void, ArchiveRange) -> u32) -> ArchiveLoad;
    #[link_name="bootstrap.archive::library_member_count"]
    fn library_member_count(library:*mut std::ffi::c_void)->usize;
    #[link_name="bootstrap.archive::library_member"]
    fn library_member(library:*mut std::ffi::c_void,index:usize)->ArchiveMemberState;
    #[link_name="bootstrap.archive::library_take"]
    fn library_take(library:*mut std::ffi::c_void,name:*const u8,length:usize)->ArchiveSelection;
    #[link_name="bootstrap.archive::library_destroy"]
    fn library_destroy(library:*mut std::ffi::c_void);
}
#[repr(C)]
struct ArchiveMemberState {member:ArchiveReadMember,available:u32}
#[repr(C)]
struct ArchiveSelection {result:u32,index:usize}
struct ArchiveNames<'a> {
    bytes: &'a [u8],
    error: Option<std::str::Utf8Error>,
}

unsafe extern "C" fn validate_name(context: *mut std::ffi::c_void, name: ArchiveRange) -> u32 {
    let names = unsafe { &mut *context.cast::<ArchiveNames<'_>>() };
    match std::str::from_utf8(name.bytes(names.bytes)) {
        Ok(_) => 1,
        Err(error) => {
            names.error = Some(error);
            0
        }
    }
}
fn archive_read_result(result: ArchiveReadResult) -> Result<()> {
    let message=match result.error {
        ArchiveReadError::None => return Ok(()),
        ArchiveReadError::Truncated => "静态库被截断",
        ArchiveReadError::Header => "静态库成员头错误",
        ArchiveReadError::Decimal => "静态库十进制成员大小或名称偏移错误",
        ArchiveReadError::MissingDirectory => "COFF 静态库缺少第二链接成员",
        ArchiveReadError::ZeroIndex => "静态库成员索引为零",
        ArchiveReadError::IndexRange => "静态库成员索引越界",
        ArchiveReadError::StringOffset => "静态库字符串偏移错误",
        ArchiveReadError::UnterminatedString => "静态库字符串未终止",
        ArchiveReadError::UnterminatedName => "静态库成员名未终止",
        ArchiveReadError::MemberOffset => "静态库成员偏移超出 COFF 格式",
        ArchiveReadError::Receiver => "静态库输出接收失败",
        ArchiveReadError::Allocation => return Err("静态库目录内存分配失败".into()),
    };
    Err(format!("{message}（归档字节偏移 {}）",result.at).into())
}
impl Library {
    pub(crate) fn native(bytes:ObjectBytes)->Result<Self> {
        Self::decode(bytes, true)
    }
    fn decode(bytes: ObjectBytes, native: bool) -> Result<Self> {
        let mut names = ArchiveNames { bytes: &bytes, error: None };
        let result = unsafe {
            archive_load(bytes.as_ptr(), bytes.len(), (&mut names as *mut ArchiveNames<'_>).cast(), validate_name)
        };
        if let Some(error) = names.error { return Err(error.into()); }
        archive_read_result(result.read)?;
        // Successful load transfers the directory while the original bytes stay owned here.
        let directory = unsafe { NonNull::new_unchecked(result.library) };
        Ok(Self { directory, bytes, native })
    }
    fn member_count(&self)->usize {unsafe{library_member_count(self.directory.as_ptr())}}
    fn member(&self,index:usize)->ArchiveMemberState {unsafe{library_member(self.directory.as_ptr(),index)}}
    fn member_bytes(&self,member:ArchiveReadMember)->(&str,ObjectBytes) {
        // The original receiver validated this name before B retained its range.
        (unsafe{std::str::from_utf8_unchecked(member.name.bytes(&self.bytes))},self.bytes.slice(member.body.range()))
    }
    pub(crate) fn extract(&mut self,symbol:&str)->Result<Option<Object>> {
        let selected=unsafe{library_take(self.directory.as_ptr(),symbol.as_ptr(),symbol.len())};
        match selected.result {
            0=>return Ok(None),
            1=>{},
            _=>return Err("静态库目录未指向对象成员".into()),
        }
        let member=self.member(selected.index).member;
        let (name,bytes)=self.member_bytes(member);
        let object=member_object(name,&bytes,self.native)?;
        if !matches!(object.kind,Kind::Module{..}) {
            return Err(format!("静态库成员 {name} 不是独立模块").into());
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
    let members = members(inputs)?;
    let mut symbols = Vec::new();
    for (index, member) in members.iter().enumerate() {
        let names = member.archive_symbols()?
            .ok_or_else(||format!("{} 不是独立模块产物",member.name))?;
        symbols.extend(names.into_iter().map(|name| (index, name)));
    }
    let output = encode(&members, &symbols)?;
    if let Some(parent) = path.parent() {
        crate::file_bytes::create_dir_all(parent)?;
    }
    crate::file_bytes::write(&path, &output)?;
    Ok(())
}

pub(crate) fn members(inputs: &[impl AsRef<Path>]) -> Result<Vec<Member>> {
    let mut members = Vec::new();
    for input in inputs {
        let input = input.as_ref();
        let bytes = ObjectBytes::read(input)?;
        if has_signature(&bytes) {
            let library=Library::decode(bytes, false)?;
            for index in 0..library.member_count() {
                let member=library.member(index).member;
                let (name,bytes)=library.member_bytes(member);
                members.push(Member{name:name.into(),bytes});
            }
        } else {
            let name = input.file_name()
                .and_then(|name| name.to_str())
                .ok_or("静态库输入名不是 UTF-8")?
                .into();
            members.push(Member { name, bytes });
        }
    }
    Ok(members)
}

pub(crate) fn encode(members: &[Member], symbols: &[(usize, String)]) -> Result<Vec<u8>> {
    let mut member_views: Vec<_> = members.iter().map(|member| ArchiveMember {
        name: member.name.as_ptr(), name_length: member.name.len(),
        bytes: member.bytes().as_ptr(), byte_length: member.bytes().len(),
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
    Ok(output)
}

