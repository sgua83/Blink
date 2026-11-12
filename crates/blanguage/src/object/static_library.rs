//! 标准 COFF 静态库保存原对象；目录归库所有，实际符号绑定仍归链接器。
use super::{Kind, Object, Result, coff};
use crate::file_bytes::FileBytes;
use std::{fs::File, ops::{Deref, Range}, path::Path, ptr::NonNull, sync::Arc};
const SIGNATURE: &[u8; 8] = b"!<arch>\n";

pub enum Input {
    Object(Object),
    Library(Library),
}
impl Input {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = FileBytes::read(&File::open(path)?)?;
        if bytes.starts_with(SIGNATURE) {
            Ok(Self::Library(Library::decode(ArchiveBytes::Captured(Arc::new(bytes)), false)?))
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
                    let (module,kind,definitions)=coff::Reader::new(bytes)?.interface()
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
                        member_object(name,bytes,library.native)
                    })
                }).collect(),
        }
    }
}
#[derive(Clone)]
enum ArchiveBytes {
    Captured(Arc<FileBytes>),
    Generated(Arc<Vec<u8>>),
}

impl Deref for ArchiveBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Self::Captured(bytes) => bytes,
            Self::Generated(bytes) => bytes,
        }
    }
}

// A writer queue keeps the original input alive and borrows its actual range.
// Newly generated native members own their own complete input through new().
pub(crate) struct Member {
    pub(crate) name: String,
    bytes: ArchiveBytes,
    range: Range<usize>,
}
impl Member {
    pub(crate) fn new(name:String,bytes:Vec<u8>)->Self {
        Self::whole(name, ArchiveBytes::Generated(Arc::new(bytes)))
    }
    fn whole(name: String, bytes: ArchiveBytes) -> Self {
        let range = 0..bytes.len();
        Self { name, bytes, range }
    }
    fn bytes(&self)->&[u8] { &self.bytes[self.range.clone()] }
    pub(crate) fn object(&self,native:bool)->Result<Object> {
        member_object(&self.name,self.bytes(),native)
    }
}
fn member_object(name:&str,bytes:&[u8],native:bool)->Result<Object> {
    (if native {coff::Reader::new(bytes)?.native(name)} else {coff::decode(bytes)})
        .map_err(|error|format!("静态库成员 {name}：{error}").into())
}
pub struct Library {
    directory: NonNull<std::ffi::c_void>,
    bytes: ArchiveBytes,
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
    fn bytes<'a>(&self, bytes: &'a [u8]) -> &'a [u8] { &bytes[self.start..self.start+self.length] }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ArchiveReadMember { offset: u32, name: ArchiveRange, body: ArchiveRange }
#[repr(C)]
struct ArchiveReadSymbol { member: u32, name: ArchiveRange }
#[repr(u32)]
#[derive(Clone, Copy)]
enum ArchiveReadError {
    None, Truncated, Header, Decimal, MissingDirectory, ZeroIndex,
    IndexRange, StringOffset, UnterminatedString, UnterminatedName, MemberOffset, Receiver,
}
#[repr(C)]
struct ArchiveReadResult { error: ArchiveReadError, at: usize }
unsafe extern "C" {
    #[link_name = "bootstrap.archive::decode"]
    fn archive_decode(bytes: *const u8, length: usize, context: *mut std::ffi::c_void,
        member: unsafe extern "C" fn(*mut std::ffi::c_void, ArchiveReadMember) -> u32,
        symbol: unsafe extern "C" fn(*mut std::ffi::c_void, ArchiveReadSymbol) -> u32) -> ArchiveReadResult;
    #[link_name="bootstrap.archive::library_create"]
    fn library_create(bytes:*const u8)->*mut std::ffi::c_void;
    #[link_name="bootstrap.archive::library_add_member"]
    fn library_add_member(library:*mut std::ffi::c_void,member:ArchiveReadMember)->u32;
    #[link_name="bootstrap.archive::library_add_symbol"]
    fn library_add_symbol(library:*mut std::ffi::c_void,symbol:ArchiveReadSymbol)->u32;
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
// The synchronous receiver interprets UTF-8. B retains only the ranges into
// Library's immutable input, and owns directory insertion and release.
struct ArchiveRead<'a> {library:&'a mut Library,received:Result<()>}
unsafe extern "C" fn read_member(context:*mut std::ffi::c_void,member:ArchiveReadMember)->u32 {
    let read=unsafe{&mut *(context as *mut ArchiveRead<'_>)};
    let result=(||->Result<()>{
        std::str::from_utf8(member.name.bytes(&read.library.bytes))?;
        if unsafe{library_add_member(read.library.directory.as_ptr(),member)}==0 {
            return Err("静态库目录内存分配失败".into());
        }
        Ok(())
    })();
    match result {Ok(())=>1,Err(error)=>{read.received=Err(error);0}}
}
unsafe extern "C" fn read_symbol(context:*mut std::ffi::c_void,symbol:ArchiveReadSymbol)->u32 {
    let read=unsafe{&mut *(context as *mut ArchiveRead<'_>)};
    let result=(||->Result<()>{
        std::str::from_utf8(symbol.name.bytes(&read.library.bytes))?;
        if unsafe{library_add_symbol(read.library.directory.as_ptr(),symbol)}==0 {
            return Err("静态库目录内存分配失败".into());
        }
        Ok(())
    })();
    match result {Ok(())=>1,Err(error)=>{read.received=Err(error);0}}
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
    };
    Err(format!("{message}（归档字节偏移 {}）",result.at).into())
}
impl Library {
    pub(crate) fn native(bytes:Arc<Vec<u8>>)->Result<Self> {
        Self::decode(ArchiveBytes::Generated(bytes), true)
    }
    fn decode(bytes: ArchiveBytes, native: bool) -> Result<Self> {
        let directory=NonNull::new(unsafe{library_create(bytes.as_ptr())})
            .ok_or("静态库目录内存分配失败")?;
        let mut library=Self{directory,bytes,native};
        let mut read=ArchiveRead{library:&mut library,received:Ok(())};
        let result=unsafe{archive_decode(read.library.bytes.as_ptr(),read.library.bytes.len(),
            &mut read as *mut ArchiveRead<'_> as *mut std::ffi::c_void,read_member,read_symbol)};
        read.received?;
        archive_read_result(result)?;
        Ok(library)
    }
    fn member_count(&self)->usize {unsafe{library_member_count(self.directory.as_ptr())}}
    fn member(&self,index:usize)->ArchiveMemberState {unsafe{library_member(self.directory.as_ptr(),index)}}
    fn member_bytes(&self,member:ArchiveReadMember)->(&str,&[u8]) {
        // The original receiver validated this name before B retained its range.
        (unsafe{std::str::from_utf8_unchecked(member.name.bytes(&self.bytes))},member.body.bytes(&self.bytes))
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
        let object=member_object(name,bytes,self.native)?;
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
        let object = member.object(false)?;
        if !matches!(object.kind, Kind::Module { .. }) {
            return Err(format!("{} 不是独立模块产物", member.name).into());
        }
        symbols.extend(object.symbols.into_iter().map(|s| (index, s.name)));
    }
    let output = encode(&members, &symbols)?;
    let path = std::path::absolute(path)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, output)?;
    Ok(())
}

pub(crate) fn members(inputs: &[impl AsRef<Path>]) -> Result<Vec<Member>> {
    let mut members = Vec::new();
    for input in inputs {
        let input = input.as_ref();
        let bytes = ArchiveBytes::Captured(Arc::new(FileBytes::read(&File::open(input)?)?));
        if bytes.starts_with(SIGNATURE) {
            let library=Library::decode(bytes, false)?;
            for index in 0..library.member_count() {
                let member=library.member(index).member;
                let (name,_)=library.member_bytes(member);
                members.push(Member{name:name.into(),bytes:library.bytes.clone(),
                    range:member.body.start..member.body.start+member.body.length});
            }
        } else {
            members.push(Member::whole(input
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or("静态库输入名不是 UTF-8")?
                    .into(),
                bytes,
            ));
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

