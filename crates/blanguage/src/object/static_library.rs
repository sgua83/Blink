//! 标准 COFF 静态库保存原对象；目录归库所有，实际符号绑定仍归链接器。
use super::{Kind, Object, ReadSection, Result, coff};
use crate::{file_bytes::FileBytes, module::CompiledInterface};
use std::{borrow::Cow, fs::File, ops::{Deref, Range}, path::Path, ptr::NonNull, sync::Arc};
fn has_signature(bytes: &[u8]) -> bool {
    unsafe { archive_has_signature(bytes.as_ptr(), bytes.len()) }
}

pub enum Input {
    Object(Object),
    Library(Library),
}
impl Input {
    pub fn read(path: &Path) -> Result<Self> {
        ObjectBytes::read(path)?.decode_input(true, Self::Object, Self::Library)
    }
    pub(crate) fn imports(&self, imports: &mut crate::module::Imports) -> Result<()> {
        match self {
            Self::Object(object) => imports.object(object)?,
            Self::Library(library) => {
                for interface in library.interfaces() {
                    imports.interface(interface?)?;
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
                        member_object(name,&bytes,library.native,true)
                    })
                }).collect(),
        }
    }
}
impl CompiledInterface {
    /// Reads source-facing interfaces in the original COFF input order.
    pub fn read(path: &Path) -> Result<Vec<Self>> {
        let bytes = FileBytes::read_path(path)?;
        if !has_signature(&bytes) {
            return Ok(vec![read_interface(&bytes)?]);
        }
        let mut interfaces = Vec::new();
        let mut member_result = Ok(());
        visit_archive(&bytes, |member| {
            // The whole archive must pass format checks before a member error wins.
            if member_result.is_ok() {
                member_result = read_member_interface(
                    member.name.validated_name(&bytes), member.body.bytes(&bytes),
                )
                    .map(|interface| interfaces.push(interface));
            }
        })?;
        member_result?;
        Ok(interfaces)
    }
}

fn read_interface(bytes: &[u8]) -> Result<CompiledInterface> {
    let (name, kind, definitions) = coff::Reader::new(bytes)?.interface()?;
    Ok(CompiledInterface { name, kind, definitions })
}

fn read_member_interface(name: &str, bytes: &[u8]) -> Result<CompiledInterface> {
    read_interface(bytes).map_err(|error| format!("静态库成员 {name}：{error}").into())
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

    pub(crate) fn decode_input<Bytes: ReadSection, Output>(
        self,
        stack_output: bool,
        object: impl FnOnce(Object<Bytes>) -> Output,
        library: impl FnOnce(Library) -> Output,
    ) -> Result<Output> {
        if has_signature(&self) {
            Ok(library(Library::decode(self, false)?))
        } else {
            Ok(object(coff::decode(&self, stack_output)?))
        }
    }

    pub(super) fn generated(bytes: Vec<u8>) -> Self {
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

#[derive(Clone, Copy)]
pub(crate) struct Member<'a> {
    name: &'a str,
    bytes: &'a [u8],
}

pub(crate) struct MemberView<'a> {
    name: &'a str,
    bytes: &'a [u8],
    build: Option<&'a coff::HostBuildSection>,
}

impl<'a> MemberView<'a> {
    pub(crate) fn new(name: &'a str, bytes: &'a [u8]) -> Self {
        Self { name, bytes, build: None }
    }

    fn output_length(&self) -> usize {
        self.build.map_or(self.bytes.len(), |build| build.length(self.bytes))
    }

    unsafe fn write(&self, output: *mut u8) {
        unsafe {
            if let Some(build) = self.build {
                build.write(self.bytes, output);
            } else {
                std::ptr::copy_nonoverlapping(self.bytes.as_ptr(), output, self.bytes.len());
            }
        }
    }
}

pub(crate) struct HostMember<'a> {
    name: Cow<'a, str>,
    bytes: Cow<'a, [u8]>,
    build: Option<coff::HostBuildSection>,
    names: Vec<Cow<'a, str>>,
}

impl HostMember<'_> {
    pub(crate) fn symbols(&self, index: usize) -> impl Iterator<Item = ArchiveSymbol<'_>> {
        self.names.iter().map(move |name| ArchiveSymbol::new(index, name))
    }

    pub(crate) fn view(&self) -> MemberView<'_> {
        MemberView { name: &self.name, bytes: &self.bytes, build: self.build.as_ref() }
    }
}

impl<'a> Member<'a> {
    fn view(self) -> MemberView<'a> {
        MemberView::new(self.name, self.bytes)
    }

    pub(crate) fn emit_host_members(
        self, imports: &mut Vec<coff::ImportKey<'a>>,
        mut emit: impl FnMut(HostMember<'a>),
    ) -> Result<()> {
        let output = coff::Reader::new(self.bytes).and_then(|reader| reader.host_links(imports))
            .map_err(|error| format!("静态库成员 {}：{error}", self.name))?
            .ok_or_else(|| format!("{} 不是独立模块产物", self.name))?;
        match output {
            coff::HostArchiveMember::Original(names, build) => {
                emit(HostMember {
                    name: Cow::Borrowed(self.name),
                    bytes: Cow::Borrowed(self.bytes),
                    build,
                    names,
                });
            }
            coff::HostArchiveMember::Groups(groups) => {
                for (index, (bytes, names)) in groups.into_iter().enumerate() {
                    emit(HostMember {
                        name: Cow::Owned(format!("{}.{}.obj", self.name, index + 1)),
                        bytes: Cow::Owned(bytes),
                        build: None,
                        names,
                    });
                }
            }
        }
        Ok(())
    }
    fn archive_symbols(self, emit: impl FnMut(&'a str)) -> Result<bool> {
        coff::Reader::new(self.bytes).and_then(|reader| reader.archive_symbols(emit))
            .map_err(|error|format!("静态库成员 {}：{error}",self.name).into())
    }
}
fn member_object<Bytes: ReadSection>(name: &str, bytes: &ObjectBytes, native: bool, stack_output: bool) -> Result<Object<Bytes>> {
    (if native {coff::Reader::new(bytes)?.native(name)} else {coff::decode(bytes, stack_output)})
        .map_err(|error|format!("静态库成员 {name}：{error}").into())
}
pub struct Library {
    directory: NonNull<std::ffi::c_void>,
    bytes: ObjectBytes,
    native: bool,
}
impl Library {
    fn interfaces(&self) -> impl Iterator<Item = Result<CompiledInterface>> + '_ {
        (0..self.member_count()).filter_map(move |index| {
            let member = self.member(index);
            (member.available != 0).then(|| {
                let member = member.member;
                read_member_interface(member.name.validated_name(&self.bytes), member.body.bytes(&self.bytes))
            })
        })
    }
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
    fn validated_name<'a>(&self, bytes: &'a [u8]) -> &'a str {
        // B hands off this name only after the UTF-8 receiver accepts it.
        unsafe { std::str::from_utf8_unchecked(self.bytes(bytes)) }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ArchiveReadMember { name: ArchiveRange, body: ArchiveRange }
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
    #[link_name = "bootstrap.archive::visit_members"]
    fn archive_visit_members(bytes: *const u8, length: usize, name_context: *mut std::ffi::c_void,
        validate: unsafe extern "C" fn(*mut std::ffi::c_void, ArchiveRange) -> u32,
        member_context: *mut std::ffi::c_void,
        emit: unsafe extern "C" fn(*mut std::ffi::c_void, ArchiveRange, ArchiveRange)) -> ArchiveReadResult;
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
struct ArchiveSelection {member:ArchiveReadMember,result:u32}
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
        (member.name.validated_name(&self.bytes),self.bytes.slice(member.body.range()))
    }
    pub(crate) fn extract<Bytes: ReadSection>(&mut self, symbol: &str, stack_output: bool) -> Result<Option<Object<Bytes>>> {
        let selected=unsafe{library_take(self.directory.as_ptr(),symbol.as_ptr(),symbol.len())};
        match selected.result {
            0=>return Ok(None),
            1=>{},
            _=>return Err("静态库目录未指向对象成员".into()),
        }
        let (name,bytes)=self.member_bytes(selected.member);
        let object=member_object(name,&bytes,self.native,stack_output)?;
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
    byte_length: usize,
    offset: usize,
    name_offset: usize,
}
#[repr(C)]
pub(crate) struct ArchiveSymbol<'a> {
    name: *const u8,
    name_length: usize,
    member: usize,
    borrowed_name: std::marker::PhantomData<&'a str>,
}
impl<'a> ArchiveSymbol<'a> {
    pub(crate) fn new(member: usize, name: &'a str) -> Self {
        Self {
            name: name.as_ptr(), name_length: name.len(), member,
            borrowed_name: std::marker::PhantomData,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ArchiveInput<'a> {
    members: *mut ArchiveMember,
    member_count: usize,
    symbols: *mut ArchiveSymbol<'a>,
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
    fn archive_layout(input: ArchiveInput<'_>) -> ArchiveLayout;
    #[link_name = "bootstrap.archive::encode"]
    fn archive_encode(input: ArchiveInput<'_>, layout: ArchiveLayout, output: *mut u8);
}

unsafe extern "C" fn visit_member<Emit: FnMut(ArchiveReadMember)>(
    context: *mut std::ffi::c_void, name: ArchiveRange, body: ArchiveRange,
) {
    let emit = unsafe { &mut *context.cast::<Emit>() };
    emit(ArchiveReadMember { name, body });
}

fn visit_archive<Emit: FnMut(ArchiveReadMember)>(source: &[u8], mut emit: Emit) -> Result<()> {
    let mut names = ArchiveNames { bytes: source, error: None };
    let result = unsafe {
        archive_visit_members(source.as_ptr(), source.len(),
            (&mut names as *mut ArchiveNames<'_>).cast(), validate_name,
            std::ptr::from_mut(&mut emit).cast(), visit_member::<Emit>)
    };
    if let Some(error) = names.error { return Err(error.into()); }
    archive_read_result(result)
}

/// 将已编译模块保存为标准 COFF 静态库；原对象字节保持，不调用编译器。
pub fn archive(inputs: &[impl AsRef<Path>], path: &Path) -> Result<()> {
    let output = {
        let sources = member_sources(inputs)?;
        let members = sources.iter().flat_map(MemberSource::members);
        let mut symbols = Vec::new();
        for (index, member) in members.clone().enumerate() {
            if !member.archive_symbols(|name| symbols.push(ArchiveSymbol::new(index, name)))? {
                return Err(format!("{} 不是独立模块产物", member.name).into());
            }
        }
        encode(members.map(Member::view), symbols)?
    };
    if let Some(parent) = path.parent() {
        crate::file_bytes::create_dir_all(parent)?;
    }
    crate::file_bytes::write(&path, &output)?;
    Ok(())
}

pub(crate) struct MemberSource<'a> {
    bytes: FileBytes,
    members: MemberRanges<'a>,
}

enum MemberRanges<'a> {
    Object(&'a str),
    Archive(Vec<ArchiveReadMember>),
}

impl MemberSource<'_> {
    pub(crate) fn members(&self) -> impl Iterator<Item = Member<'_>> + Clone {
        let bytes = &*self.bytes;
        let (object, archive) = match &self.members {
            MemberRanges::Object(name) => (Some(Member { name, bytes }), &[][..]),
            MemberRanges::Archive(members) => (None, members.as_slice()),
        };
        object.into_iter().chain(archive.iter().map(move |member| Member {
            name: member.name.validated_name(bytes),
            bytes: member.body.bytes(bytes),
        }))
    }
}

pub(crate) fn member_sources(inputs: &[impl AsRef<Path>]) -> Result<Vec<MemberSource<'_>>> {
    let mut sources = Vec::new();
    for input in inputs {
        let input = input.as_ref();
        let bytes = FileBytes::read_path(input)?;
        let members = if has_signature(&bytes) {
            let mut ranges = Vec::new();
            visit_archive(&bytes, |member| ranges.push(member))?;
            if ranges.is_empty() { continue; }
            MemberRanges::Archive(ranges)
        } else {
            let name = input.file_name()
                .and_then(|name| name.to_str())
                .ok_or("静态库输入名不是 UTF-8")?;
            MemberRanges::Object(name)
        };
        sources.push(MemberSource { bytes, members });
    }
    Ok(sources)
}

pub(crate) fn encode<'a>(
    members: impl Iterator<Item = MemberView<'a>> + Clone,
    mut symbols: Vec<ArchiveSymbol<'a>>,
) -> Result<Vec<u8>> {
    let mut member_views: Vec<_> = members.clone().map(|member| ArchiveMember {
        name: member.name.as_ptr(), name_length: member.name.len(),
        byte_length: member.output_length(),
        offset: 0, name_offset: 0,
    }).collect();
    let input = ArchiveInput {
        members: member_views.as_mut_ptr(), member_count: member_views.len(),
        symbols: symbols.as_mut_ptr(), symbol_count: symbols.len(),
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
    // B initializes the archive envelope and the layout supplies each member's
    // final span. Its owner writes the payload there before the Vec takes length.
    unsafe {
        archive_encode(input, layout, output.as_mut_ptr());
        for (member, view) in members.zip(member_views) {
            member.write(output.as_mut_ptr().add(view.offset + 60));
        }
        output.set_len(layout.length);
    }
    Ok(output)
}

