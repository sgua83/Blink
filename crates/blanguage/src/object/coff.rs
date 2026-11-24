//! COFF 节、符号与 AMD64 重定位；Blink 元数据不替代标准重定位表。
use super::*;
use std::{collections::{BTreeMap, BTreeSet}, ffi::c_void, sync::Arc};
const META: &str = ".blmeta";
const ARCHIVE: &str = ".blrt";
const BUILD: &str = ".blbuild";
const FORMAT: u32 = 93;
// Caller-owned views; original names and bodies stay in Object until encoding ends.
#[repr(C)]
struct CoffSectionView {
    name: *const u8, name_length: usize,
    byte_length: usize, flags: u64,
    relocations: *const CoffRelocationView, relocation_count: usize,
    offset: usize, relocation_offset: usize,
}
#[repr(C)]
struct CoffSymbolView {
    name: *const u8, name_length: usize, section: u64, value: u64,
    function: u64, external: u64, auxiliary: u64, owner: u64, generated: u64, source: u64,
}
#[repr(C)]
struct CoffRelocationView { offset: u64, symbol: u64, kind: u64 }
#[repr(C)]
#[derive(Clone, Copy)]
struct CoffInput {
    sections: *mut CoffSectionView, section_count: usize,
    symbols: *const CoffSymbolView, symbol_count: usize,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum CoffError { None, SectionCount, SectionSize, Offset, RelocationCount, SymbolCount, Strings }
#[repr(C)]
#[derive(Clone, Copy)]
struct CoffLayout { length: usize, symbols: usize, strings: usize, symbol_count: usize, error: CoffError }
unsafe extern "C" {
    #[link_name = "bootstrap.coff::layout"]
    fn coff_layout(input: CoffInput) -> CoffLayout;
    #[link_name = "bootstrap.coff::encode"]
    fn coff_encode(input: CoffInput, layout: CoffLayout, output: *mut u8);
}

// Synchronous borrowed fields; Object and caller-owned storage outlive both B phases.
#[repr(C)]
#[derive(Clone, Copy)]
struct BindingBytes { bytes: *const u8, length: usize }
impl BindingBytes {
    fn new(bytes: &[u8]) -> Self { Self { bytes: bytes.as_ptr(), length: bytes.len() } }
}
// The key borrows the typed names in the selected relocation owner.
#[repr(C)]
pub(crate) struct ImportKey<'a> {
    dll: BindingBytes,
    symbol: BindingBytes,
    source: std::marker::PhantomData<&'a str>,
}

impl<'a> ImportKey<'a> {
    fn new(dll: &'a str, symbol: &'a str) -> Self {
        Self {
            dll: BindingBytes::new(dll.as_bytes()),
            symbol: BindingBytes::new(symbol.as_bytes()),
            source: std::marker::PhantomData,
        }
    }

    fn text(bytes: BindingBytes) -> &'a str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(bytes.bytes, bytes.length)) }
    }

    pub(crate) fn dll(&self) -> &'a str { Self::text(self.dll) }
    pub(crate) fn symbol(&self) -> &'a str { Self::text(self.symbol) }
}

#[repr(C)]
struct BindingSection {
    name: BindingBytes, flags: u64, relocations: *const BindingRelocation, relocation_count: usize,
    associated: *const usize, associated_count: usize, owner: u64, symbol: u64,
}
#[repr(C)]
struct BindingDefinition { name: BindingBytes, section: u64, value: u64, function: u64 }
#[repr(C)]
struct BindingRelocation { offset: u64, kind: u64, target: u64, section: u64, name: BindingBytes, dll: BindingBytes }
#[repr(C)]
#[derive(Clone, Copy)]
struct BindingInput {
    name: BindingBytes, sections: *mut BindingSection, section_count: usize,
    definitions: *const BindingDefinition, definition_count: usize,
}
type EmitImport<'a> = unsafe extern "C" fn(*mut c_void, ImportKey<'a>);

#[repr(C)]
#[derive(Clone, Copy)]
struct BindingWork<'a> {
    symbols: *mut CoffSymbolView,
    nodes: *mut [u64; 9],
    relocations: *mut CoffRelocationView,
    import_context: *mut c_void,
    emit_import: Option<EmitImport<'a>>,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum BindingError { None = 0, SectionCount = 2, RelocationOffset = 5 }
#[repr(C)]
#[derive(Clone, Copy)]
struct BindingPlan { symbols: usize, names: usize, relocations: usize, error: BindingError }
unsafe extern "C" {
    #[link_name = "bootstrap.bindings::prepare"]
    fn binding_prepare(input: BindingInput, work: BindingWork<'_>) -> BindingPlan;
    #[link_name = "bootstrap.bindings::materialize"]
    fn binding_materialize(input: BindingInput, work: BindingWork<'_>, plan: BindingPlan, names: *mut u8);
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ReadBytes { bytes: *const u8, length: usize }
impl ReadBytes {
    fn new(bytes: &[u8]) -> Self { Self { bytes: bytes.as_ptr(), length: bytes.len() } }
    unsafe fn borrowed<'a>(self) -> &'a [u8] { unsafe { std::slice::from_raw_parts(self.bytes, self.length) } }
}
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
enum ReadError { None = 0, Machine = 1, Truncated = 2, StringOffset = 4, Unterminated = 5, RelocationCount = 6, AuxiliaryCount = 7 }
impl ReadError {
    fn check(self) -> Result<()> {
        match self {
            Self::None => Ok(()),
            Self::Machine => Err("需要 Windows x64 COFF 目标文件".into()),
            Self::Truncated => Err("目标文件被截断".into()),
            Self::StringOffset => Err("COFF 字符串偏移错误".into()),
            Self::Unterminated => Err("COFF 符号缺少结尾".into()),
            Self::RelocationCount => Err("COFF 重定位数量错误".into()),
            Self::AuxiliaryCount => Err("COFF auxiliary records exceed the symbol table".into()),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ReadRange { bytes: ReadBytes, error: ReadError }
#[repr(C)]
#[derive(Clone, Copy)]
struct ReadBuild { bytes: ReadBytes, present: u64, error: ReadError }
#[repr(C)]
#[derive(Clone, Copy)]
struct ReadSymbols { strings: ReadBytes, offset: usize, count: usize, error: ReadError }
#[repr(C)]
#[derive(Clone, Copy)]
struct ReadSymbol {
    name: ReadBytes, section: i64, value: u64, storage: u64, function: u64,
    auxiliary: u64, selection: u64, owner: u64, next: usize, error: ReadError,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ReadRelocations { offset: usize, count: usize, error: ReadError }
#[repr(C)]
#[derive(Clone, Copy)]
struct ReadRelocation { offset: usize, symbol: usize, kind: u64, error: ReadError }
unsafe extern "C" {
    #[link_name = "bootstrap.coff::host_headers"]
    fn host_headers(output: *mut u8, sections: usize, symbols: usize, cache_end: usize, cache_length: usize);
    #[link_name = "blink.windows.coff::headers"]
    fn read_headers(input: ReadBytes) -> ReadRange;
    #[link_name = "blink.windows.coff::build_inputs"]
    fn read_build_inputs(input: ReadBytes, headers: ReadBytes) -> ReadBuild;
    #[link_name = "blink.windows.coff::symbols"]
    fn read_symbols(input: ReadBytes) -> ReadSymbols;
    #[link_name = "blink.windows.coff::symbol"]
    fn read_symbol(input: ReadBytes, table: ReadSymbols, index: usize) -> ReadSymbol;
    #[link_name = "blink.windows.coff::next_export"]
    fn next_export(input: ReadBytes, table: ReadSymbols, index: &mut usize) -> ReadRange;
    #[link_name = "blink.windows.coff::relocations"]
    fn read_relocations(input: ReadBytes, offset: usize, count: usize, flags: u64) -> ReadRelocations;
    #[link_name = "blink.windows.coff::relocation"]
    fn read_relocation(input: ReadBytes, offset: usize, index: usize) -> ReadRelocation;
}
// B owns logical section identity and COFF target interpretation; rows borrow the original input.
#[repr(C)]
#[derive(Clone, Copy)]
struct ObjectSection {
    name: ReadBytes, bytes: ReadBytes, flags: u64, relocations: usize, relocation_count: usize, mapped: usize, zero_fill: usize,
    decoded_relocations: *const u8, decoded_count: usize,
}
#[repr(C)]
#[derive(Default)]
struct ObjectParts {
    metadata: ReadBytes,
    runtime: ReadBytes,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct ObjectRows { values: *const ObjectSection, count: usize }
impl ObjectRows {
    fn new(rows: &[ObjectSection]) -> Self { Self { values: rows.as_ptr(), count: rows.len() } }
}
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
enum ObjectReadError {
    None, OwnerNumber, OwnerUnavailable, OwnerNotComdat, Selection, SectionUnavailable,
    DllLengthMissing, DllLengthEmpty, DllLengthDigit, DllLengthOverflow, DllLength,
    StackSection, StackPosition, StackNoLocation, StackRelocation, RelocationSymbol,
    MissingSectionDefinition, RelocationBounds,
}
impl ObjectReadError {
    fn check(self) -> Result<()> {
        match self {
            Self::None => Ok(()),
            Self::OwnerNumber => Err("COFF关联节编号错误".into()),
            Self::OwnerUnavailable => Err("COFF关联节不可装载".into()),
            Self::OwnerNotComdat => Err("COFF关联所有者不是COMDAT节".into()),
            Self::Selection => Err("不支持的Blink COFF COMDAT选择".into()),
            Self::SectionUnavailable => Err("COFF 引用了不可装载的节".into()),
            Self::DllLengthMissing => Err("DLL 符号缺少长度".into()),
            Self::DllLengthEmpty => Err("cannot parse integer from empty string".into()),
            Self::DllLengthDigit => Err("invalid digit found in string".into()),
            Self::DllLengthOverflow => Err("number too large to fit in target type".into()),
            Self::DllLength => Err("DLL 符号长度错误".into()),
            Self::StackSection => Err("栈信息引用不可装载的节".into()),
            Self::StackPosition => Err("栈信息的函数位置无效或重复".into()),
            Self::StackNoLocation => Err("没有COFF位置的入口不能引用COFF调用".into()),
            Self::StackRelocation => Err("栈调用信息缺少对应的原生重定位".into()),
            Self::RelocationSymbol => Err("COFF 重定位符号越界".into()),
            Self::MissingSectionDefinition => Err("COFF COMDAT节缺少定义辅助记录".into()),
            Self::RelocationBounds => Err("COFF 重定位位置越界".into()),
        }
    }
}
// One slot per physical symbol record. Auxiliary slots have no primary name.
#[repr(C)]
#[derive(Clone, Copy)]
struct ObjectSymbol {
    name: ReadBytes,
    // Section value, or the DLL split/error for an undefined import.
    value: u32,
    section: i16,
    external: u8,
    role: ObjectSymbolRole,
}

#[repr(u8)]
#[derive(Clone, Copy)]
enum ObjectSymbolRole { Data, Function, Import, InvalidImport }

impl ObjectSymbol {
    fn is_function(&self) -> bool { matches!(self.role, ObjectSymbolRole::Function) }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ObjectSymbols { values: *const ObjectSymbol, count: usize }


#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SectionDefinition { owner: u16, selection: u8, present: u8 }

#[repr(C)]
struct ObjectAssociation { present: u64, owner: usize, error: ObjectReadError }
#[repr(u64)]
enum ObjectTargetKind { Section, Import, Symbol }
#[repr(C)]
struct ObjectTarget {
    kind: ObjectTargetKind, section: usize, value: u64, dll: ReadBytes, name: ReadBytes, error: ObjectReadError,
}
#[repr(C)]
struct ObjectRelocationStatus { format: ReadError, object: ObjectReadError }

unsafe extern "C" {
    #[link_name = "bootstrap.object_read::object_sections"]
    fn object_sections(input: ReadBytes, headers: ReadBytes, rows: *mut ObjectSection,
        parts: &mut ObjectParts, context: *mut c_void, validate: ValidateObjectName) -> ReadError;
    #[link_name = "bootstrap.object_read::object_symbols"]
    fn object_symbols(input: ReadBytes, table: ReadSymbols, rows: *mut ObjectSymbol,
        definitions: *mut SectionDefinition, definition_count: usize, context: *mut c_void,
        validate: ValidateObjectName) -> ReadError;
    #[link_name = "bootstrap.object_read::association"]
    fn object_association(rows: ObjectRows, definitions: *const SectionDefinition,
        index: usize) -> ObjectAssociation;
    #[link_name = "bootstrap.object_read::object_relocations"]
    fn object_relocations(input: ReadBytes, rows: ObjectRows, symbols: ObjectSymbols,
        section: &ObjectSection, context: *mut c_void,
        consume: unsafe extern "C" fn(*mut c_void, usize, u32, ObjectTarget) -> u32,
    ) -> ObjectRelocationStatus;
}
struct RelocationConsumer<F> {
    consume: F,
    result: Result<()>,
}

unsafe extern "C" fn consume_relocation<F>(
    context: *mut c_void, offset: usize, kind: u32, target: ObjectTarget,
) -> u32
where
    F: FnMut(usize, u16, ObjectTarget) -> Result<()>,
{
    let receiver = unsafe { &mut *context.cast::<RelocationConsumer<F>>() };
    receiver.result = (receiver.consume)(offset, kind as u16, target);
    u32::from(receiver.result.is_ok())
}

type ValidateObjectName = unsafe extern "C" fn(*mut c_void, ReadBytes) -> u32;

unsafe extern "C" fn validate_object_name(context: *mut c_void, bytes: ReadBytes) -> u32 {
    match std::str::from_utf8(unsafe { bytes.borrowed() }) {
        Ok(_) => 1,
        Err(error) => {
            unsafe { *context.cast::<Option<std::str::Utf8Error>>() = Some(error) };
            0
        }
    }
}

fn read_object_names(read: impl FnOnce(*mut c_void) -> ReadError) -> Result<()> {
    let mut invalid = None::<std::str::Utf8Error>;
    read(std::ptr::from_mut(&mut invalid).cast()).check()?;
    invalid.map_or(Ok(()), |error| Err(error.into()))
}

fn object_index(rows: &[ObjectSection], index: usize) -> Option<usize> {
    rows.get(index)?.mapped.checked_sub(1)
}
fn export_names(rows: &[ObjectSection], symbols: &[ObjectSymbol]) -> Result<Vec<String>> {
    symbols.iter().filter(|symbol| symbol.external != 0 && symbol.section > 0)
        .map(|symbol| {
            object_index(rows, symbol.section as usize - 1).ok_or("导出符号引用不可装载的节")?;
            Ok(unsafe { std::str::from_utf8_unchecked(symbol.name.borrowed()) }.to_owned())
        }).collect()
}

fn read_text(bytes: ReadBytes) -> Result<String> {
    Ok(std::str::from_utf8(unsafe { bytes.borrowed() })?.to_owned())
}

// One metadata cursor; B owns byte interpretation, Rust owns the recovered values.
#[repr(C)]
struct MetadataReadCursor { input: ReadBytes, position: usize }
#[repr(u32)]
#[derive(Clone, Copy)]
enum MetadataReadError { None, Word, Field, Version, Kind, Requirement, Coff, Call, ModuleTruncated, ModuleLength, ResourceFlag }
impl MetadataReadError {
    fn check(self) -> Result<()> {
        match self {
            Self::None => Ok(()),
            Self::Word => Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "failed to fill whole buffer").into()),
            Self::Field => Err("目标文件被截断".into()),
            Self::Version => Err("不支持的 Blink 目标文件版本".into()),
            Self::Kind => Err("未知目标文件种类".into()),
            Self::Requirement => Err("未知的模块参与要求".into()),
            Self::Coff => Err("未知的栈代码位置种类".into()),
            Self::Call => Err("未知的栈调用引用种类".into()),
            Self::ModuleTruncated => Err("模块接口被截断".into()),
            Self::ModuleLength => Err("模块接口长度溢出".into()),
            Self::ResourceFlag => Err("模块布尔值编码错误".into()),
        }
    }
}
#[repr(u64)]
#[derive(Clone, Copy)]
enum ReadObjectKind { Application, Library, Module }
#[repr(u64)]
#[derive(Clone, Copy)]
enum ReadRequirementKind { Invocation, Domain, Host }
#[repr(u64)]
#[derive(Clone, Copy)]
enum ReadCallKind { Reference, Tail, Indirect, Unreported }
#[repr(C)]
struct MetadataReadWord { value: u64, error: MetadataReadError }
#[repr(C)]
struct MetadataReadField { value: ReadBytes, error: MetadataReadError }
#[repr(C)]
#[derive(Clone, Copy)]
struct MetadataReadHeader {
    name: ReadBytes, kind: ReadObjectKind, interface: ReadBytes, prefix: ReadBytes, header: ReadBytes, binding: ReadBytes,
    requirements: u64, stack: u64, error: MetadataReadError,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct MetadataReadPrefix {
    header: MetadataReadHeader, requirements: ReadBytes, libraries: ReadBytes, exports: ReadBytes,
    resource: ReadBytes, binding_count: u64,
}
#[repr(C)]
struct MetadataReadBinding { name: ReadBytes, value: ReadBytes, error: MetadataReadError }
#[repr(C)]
struct MetadataReadFrame { name: ReadBytes, has_coff: u64, section: u64, start: u64, local: u64, calls: u64, error: MetadataReadError }
#[repr(C)]
struct MetadataReadCall { kind: ReadCallKind, offset: u64, reason: ReadBytes, error: MetadataReadError }
#[repr(C)]
struct MetadataReadResourceHead { present: u64, prefix: ReadBytes, error: MetadataReadError }
#[repr(C)]
struct MetadataReadResourceBody {
    types: u64,
    programs: u64,
    abi: ReadBytes,
    code_count: usize,
    error: MetadataReadError,
}
unsafe extern "C" {
    #[link_name = "bootstrap.metadata_read::resource_head"]
    fn metadata_read_resource_head(cursor: *mut MetadataReadCursor) -> MetadataReadResourceHead;
    #[link_name = "bootstrap.metadata_read::resource_body"]
    fn metadata_read_resource_body(cursor: *mut MetadataReadCursor) -> MetadataReadResourceBody;
    #[link_name = "bootstrap.metadata_read::resource_text"]
    fn metadata_read_resource_text(cursor: *mut MetadataReadCursor) -> MetadataReadField;
    #[link_name = "bootstrap.metadata_read::word"]
    fn metadata_read_word(cursor: *mut MetadataReadCursor) -> MetadataReadWord;
    #[link_name = "bootstrap.metadata_read::prefix"]
    fn metadata_read_prefix(
        cursor: *mut MetadataReadCursor, format: u64, sink: MetadataNameSink,
    ) -> MetadataReadPrefix;
    #[link_name = "bootstrap.metadata_read::project_names"]
    fn metadata_project_names(prefix: MetadataReadPrefix, sink: MetadataNameSink) -> MetadataReadError;
    #[link_name = "bootstrap.metadata_read::binding"]
    fn metadata_read_binding(cursor: *mut MetadataReadCursor) -> MetadataReadBinding;
    #[link_name = "bootstrap.metadata_read::frame"]
    fn metadata_read_frame(cursor: *mut MetadataReadCursor) -> MetadataReadFrame;
    #[link_name = "bootstrap.metadata_read::call"]
    fn metadata_read_call(cursor: *mut MetadataReadCursor) -> MetadataReadCall;
    #[link_name = "bootstrap.object_read::recover_stack"]
    fn object_recover_stack(
        rows: ObjectRows, frames: *mut MetadataReadFrame, count: usize,
        calls: *mut MetadataReadCall, nodes: *mut [u64; 9], stride: usize, stack_output: u32,
    ) -> ObjectReadError;
}

#[derive(Default)]
struct StackMetadata {
    frames: Vec<MetadataReadFrame>,
    calls: Vec<MetadataReadCall>,
}

impl StackMetadata {
    fn restore(mut self, rows: ObjectRows, stack_output: bool) -> Result<Vec<stack::Frame>> {
        let mut nodes = vec![[0; 9]; self.frames.len()];
        unsafe {
            object_recover_stack(rows, self.frames.as_mut_ptr(), self.frames.len(),
                self.calls.as_mut_ptr(), nodes.as_mut_ptr(), size_of::<Relocation>(), u32::from(stack_output))
        }.check()?;
        if !stack_output { return Ok(Vec::new()); }
        // The reader checked UTF-8. Recovery replaced each direct call offset with its relocation index.
        let text = |field: ReadBytes| unsafe { std::str::from_utf8_unchecked(field.borrowed()) }.to_owned();
        let mut calls = self.calls.into_iter();
        Ok(self.frames.into_iter().map(|frame| {
            let calls = calls.by_ref().take(frame.calls as usize).map(|call| match call.kind {
                ReadCallKind::Reference => stack::Call::Reference { relocation: call.offset as usize, tail: false },
                ReadCallKind::Tail => stack::Call::Reference { relocation: call.offset as usize, tail: true },
                ReadCallKind::Indirect => stack::Call::Indirect,
                ReadCallKind::Unreported => stack::Call::Unreported(text(call.reason)),
            }).collect();
            stack::Frame {
                name: text(frame.name),
                coff: (frame.has_coff != 0).then_some((frame.section as usize, frame.start as u32)),
                local: frame.local as u32,
                calls,
            }
        }).collect())
    }
}
impl MetadataReadCursor {
    fn new(input: &[u8]) -> Self { Self { input: ReadBytes::new(input), position: 0 } }
    fn word(&mut self) -> Result<u32> {
        let value = unsafe { metadata_read_word(self) };
        value.error.check()?;
        Ok(value.value as u32)
    }
}

#[repr(u32)]
enum MetadataNameGroup { Requirement = 1, Library = 2, Export = 4 }

#[repr(C)]
struct MetadataNameSink {
    context: *mut c_void,
    emit: unsafe extern "C" fn(*mut c_void, MetadataNameGroup, ReadBytes, ReadRequirementKind),
    groups: u32,
}

#[derive(Default)]
struct MetadataNames {
    requirements: BTreeMap<String, crate::execution::Requirement>,
    libraries: Vec<String>,
    exports: Vec<String>,
    archive: bool,
    error: Option<std::str::Utf8Error>,
}

impl MetadataNames {
    fn sink(&mut self, groups: u32) -> MetadataNameSink {
        MetadataNameSink { context: std::ptr::from_mut(self).cast(), emit: consume_metadata_name, groups }
    }

    fn check(&mut self) -> Result<()> {
        match self.error.take() {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }
}

const REQUIREMENTS: u32 = MetadataNameGroup::Requirement as u32;
const LIBRARIES: u32 = MetadataNameGroup::Library as u32;
const EXPORTS: u32 = MetadataNameGroup::Export as u32;

unsafe extern "C" fn consume_metadata_name(
    context: *mut c_void, group: MetadataNameGroup, name: ReadBytes, requirement: ReadRequirementKind,
) {
    let names = unsafe { &mut *context.cast::<MetadataNames>() };
    if names.error.is_some() { return; }
    // Check borrowed text once, while its original COFF snapshot is live.
    let name = match std::str::from_utf8(unsafe { name.borrowed() }) {
        Ok(name) => name,
        Err(error) => { names.error = Some(error); return; }
    };
    match group {
        MetadataNameGroup::Requirement => {
            let requirement = match requirement {
                ReadRequirementKind::Invocation => crate::execution::Requirement::Invocation,
                ReadRequirementKind::Domain => crate::execution::Requirement::Domain,
                ReadRequirementKind::Host => crate::execution::Requirement::Host,
            };
            names.requirements.insert(name.to_owned(), requirement);
        }
        MetadataNameGroup::Library if names.archive => names.libraries.push(name.to_owned()),
        MetadataNameGroup::Export if names.archive => names.exports.push(name.to_owned()),
        _ => {}
    }
}

fn read_strings<'a>(
    count: usize, keep: bool, mut read: impl FnMut() -> Result<&'a str>,
) -> Result<Vec<String>> {
    let mut strings = Vec::new();
    for _ in 0..count {
        let text = read()?;
        if keep { strings.push(text.to_owned()); }
    }
    Ok(strings)
}

// Synchronous borrowed metadata views; the original Object owns every field.
#[repr(C)]
#[derive(Clone, Copy)]
struct MetadataBytes { bytes: *const u8, length: usize }
impl MetadataBytes {
    fn new(bytes: &[u8]) -> Self { Self { bytes: bytes.as_ptr(), length: bytes.len() } }
}
#[repr(C)]
struct MetadataRequirement { name: MetadataBytes, kind: u64 }
#[repr(C)]
struct MetadataBinding { name: MetadataBytes, value: MetadataBytes }
#[repr(C)]
struct BindingOrder { count: usize, conflict: usize }
#[repr(C)]
struct MetadataCall { kind: u64, offset: u64, reason: MetadataBytes }
#[repr(C)]
struct MetadataFrame {
    name: MetadataBytes, has_coff: u64, section: u64, start: u64, local: u64,
    call_count: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct MetadataResource {
    present: u64,
    prefix: MetadataBytes,
    types: u64,
    programs: u64,
    abi: MetadataBytes,
    code: *const MetadataBytes,
    code_count: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct MetadataInput {
    format_version: u64, name: MetadataBytes, kind: u64,
    interface: MetadataBytes, stack_bytes: u64,
    prefix: MetadataBytes, header: MetadataBytes, binding: MetadataBytes,
    requirements: *const MetadataRequirement, requirement_count: usize,
    libraries: *const MetadataBytes, library_count: usize,
    exports: *const MetadataBytes, export_count: usize,
    resource: MetadataResource,
    bindings: *const MetadataBinding, binding_order: *const usize, binding_count: usize,
    frames: *const MetadataFrame, frame_count: usize, calls: *const MetadataCall,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum MetadataError { None, Count, Field, SectionSize }
#[repr(C)]
#[derive(Clone, Copy)]
struct MetadataLayout { length: usize, error: MetadataError }
unsafe extern "C" {
    #[link_name = "bootstrap.metadata::order_bindings"]
    fn metadata_order_bindings(
        bindings: *const MetadataBinding, definitions: usize, count: usize, order: *mut usize,
    ) -> BindingOrder;
    #[link_name = "bootstrap.metadata::layout"]
    fn metadata_layout(input: MetadataInput) -> MetadataLayout;
    #[link_name = "bootstrap.metadata::encode"]
    fn metadata_encode(input: MetadataInput, output: *mut u8);
    #[link_name = "bootstrap.metadata::rust_link_binding"]
    fn bind_rust_library(
        binding: MetadataBytes,
        prefix: MetadataBytes,
        library: MetadataBytes,
        output: *mut u8,
    ) -> usize;
}

pub(crate) fn rust_link_binding<'a>(
    binding: &'a str,
    prefix: &str,
    library: &str,
) -> std::borrow::Cow<'a, [u8]> {
    use std::borrow::Cow;

    let input = MetadataBytes::new(binding.as_bytes());
    let prefix = MetadataBytes::new(prefix.as_bytes());
    let library = MetadataBytes::new(library.as_bytes());
    let length = unsafe { bind_rust_library(input, prefix, library, std::ptr::null_mut()) };
    if length == 0 {
        return Cow::Borrowed(binding.as_bytes());
    }
    let mut output = Vec::with_capacity(length);
    unsafe {
        let written = bind_rust_library(input, prefix, library, output.as_mut_ptr());
        output.set_len(written);
    }
    Cow::Owned(output)
}

fn metadata<T>(o: &Object, consume: impl FnOnce(MetadataInput, usize) -> Result<T>) -> Result<T> {
    let binding = o.archive.as_ref().and_then(|archive| archive.resources.as_ref());
    let empty = MetadataBytes::new(&[]);
    let code = binding.iter().flat_map(|binding| &binding.code)
        .map(|name| MetadataBytes::new(name.as_bytes())).collect::<Vec<_>>();
    let resource = MetadataResource {
        present: u64::from(binding.is_some()),
        prefix: binding.map_or(empty, |binding| MetadataBytes::new(binding.prefix.as_bytes())),
        types: binding.map_or(0, |binding| u64::from(binding.types)),
        programs: binding.map_or(0, |binding| u64::from(binding.programs)),
        abi: binding.map_or(empty, |binding| MetadataBytes::new(&binding.abi)),
        code: code.as_ptr(),
        code_count: code.len(),
    };
    // B sorts the definition prefix and merges the original ordered, unique imports.
    let bindings = o.symbols.iter().map(|symbol| (symbol.definition.name.as_str(), symbol.contract.as_slice()))
        .chain(o.imports.iter().map(|(name, contract)| (name.as_str(), contract.as_slice())))
        .map(|(name, value)| MetadataBinding {
            name: MetadataBytes::new(name.as_bytes()), value: MetadataBytes::new(value),
        }).collect::<Vec<_>>();
    let mut binding_order = Vec::with_capacity(bindings.len());
    let order = unsafe {
        metadata_order_bindings(bindings.as_ptr(), o.symbols.len(), bindings.len(), binding_order.as_mut_ptr())
    };
    if order.conflict != bindings.len() {
        let name = bindings[order.conflict].name;
        // The binding name borrows the original symbol or import String.
        let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name.bytes, name.length)) };
        return Err(format!("对象内模块接口冲突 {name}").into());
    }
    let libraries = o.archive.iter().flat_map(|a| &a.libraries)
        .map(|name| MetadataBytes::new(name.as_bytes())).collect::<Vec<_>>();
    let exports = o.archive.iter().flat_map(|a| &a.exports)
        .map(|name| MetadataBytes::new(name.as_bytes())).collect::<Vec<_>>();
    let requirements = if let Kind::Module { requirements, .. } = &o.kind {
        requirements.iter().map(|(name, requirement)| MetadataRequirement {
            name: MetadataBytes::new(name.as_bytes()),
            kind: match requirement {
                crate::execution::Requirement::Invocation => 0,
                crate::execution::Requirement::Domain => 1,
                crate::execution::Requirement::Host => 2,
            },
        }).collect::<Vec<_>>()
    } else { Vec::new() };
    let mut calls = Vec::with_capacity(o.stack.iter().map(|frame| frame.calls.len()).sum());
    let frames = o.stack.iter().map(|frame| {
        calls.extend(frame.calls.iter().map(|call| {
            let (kind, offset, reason) = match call {
                stack::Call::Reference { relocation, tail } => {
                    let (section, _) = frame.coff.unwrap();
                    let offset = o.sections[section].relocations[*relocation].offset;
                    (u64::from(*tail), offset as u64, empty)
                }
                stack::Call::Indirect => (2, 0, empty),
                stack::Call::Unreported(reason) => (3, 0, MetadataBytes::new(reason.as_bytes())),
            };
            MetadataCall { kind, offset, reason }
        }));
        let (has_coff, section, start) = match frame.coff {
            Some((section, start)) => (1, section as u64, u64::from(start)),
            None => (0, 0, 0),
        };
        MetadataFrame {
            name: MetadataBytes::new(frame.name.as_bytes()), has_coff, section, start,
            local: u64::from(frame.local), call_count: frame.calls.len(),
        }
    }).collect::<Vec<_>>();
    let mut input = MetadataInput {
        format_version: u64::from(FORMAT), name: MetadataBytes::new(o.name.as_bytes()), kind: 0,
        interface: empty, stack_bytes: 0, prefix: empty, header: empty, binding: empty,
        requirements: requirements.as_ptr(), requirement_count: requirements.len(),
        libraries: libraries.as_ptr(), library_count: libraries.len(),
        exports: exports.as_ptr(), export_count: exports.len(), resource,
        bindings: bindings.as_ptr(), binding_order: binding_order.as_ptr(), binding_count: order.count,
        frames: frames.as_ptr(), frame_count: frames.len(), calls: calls.as_ptr(),
    };
    match &o.kind {
        Kind::Application { stack_bytes } => input.stack_bytes = u64::from(*stack_bytes),
        Kind::Library { prefix, header, binding } => {
            input.kind = 1;
            input.prefix = MetadataBytes::new(prefix.as_bytes());
            input.header = MetadataBytes::new(header.as_bytes());
            input.binding = MetadataBytes::new(binding.as_bytes());
        }
        Kind::Module { interface, stack_bytes, .. } => {
            input.kind = 2;
            input.interface = MetadataBytes::new(interface.as_bytes());
            input.stack_bytes = u64::from(*stack_bytes);
        }
    }
    let layout = unsafe { metadata_layout(input) };
    match layout.error {
        MetadataError::None => (),
        MetadataError::Count => return Err("对象元数据条目数量超过32位字段".into()),
        MetadataError::Field => return Err("对象元数据字段超过32位编码".into()),
        MetadataError::SectionSize => return Err("COFF元数据节超过32位长度字段".into()),
    }
    consume(input, layout.length)
}

pub(super) fn encode(o: &Object) -> Result<Vec<u8>> {
    encode_build(o, None)
}
pub(super) fn encode_build(o: &Object, build: Option<&[u8]>) -> Result<Vec<u8>> {
    metadata(o, |input, length| {
        let extra = std::iter::once((META, CoffPayload::Metadata(&input, length), 0x840, &[][..]))
            .chain(build.map(|bytes| (BUILD, CoffPayload::Borrowed(bytes), 0x840, &[][..])))
            .chain(o.archive.as_ref().map(|archive| (ARCHIVE, CoffPayload::Borrowed(archive.bytes.as_ref()), 0x840, &[][..])));
        encode_sections(&o.name, &o.sections, o.symbols.iter().map(|symbol| &symbol.definition), extra, None)
    })
}

pub(crate) fn encode_native<Name: AsRef<str>>(
    name: &str, source: &[Section], declared: &[Definition<Name>],
) -> Result<Vec<u8>> {
    encode_sections(name, source, declared.iter(), std::iter::empty(), None)
}

pub(crate) fn encode_native_with_imports<'a>(
    name: &str, source: &'a [Section], declared: &'a [Definition],
    imports: &mut Vec<ImportKey<'a>>,
) -> Result<Vec<u8>> {
    encode_sections(name, source, declared.iter(), std::iter::empty(), Some(imports))
}

unsafe extern "C" fn collect_import<'a>(context: *mut c_void, key: ImportKey<'a>) {
    unsafe { &mut *context.cast::<Vec<ImportKey<'a>>>() }.push(key);
}

enum CoffPayload<'a> {
    Borrowed(&'a [u8]),
    ZeroFill(usize),
    Metadata(&'a MetadataInput, usize),
}
impl CoffPayload<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Borrowed(bytes) => bytes.len(),
            Self::ZeroFill(length) | Self::Metadata(_, length) => *length,
        }
    }
    unsafe fn write(&self, destination: *mut u8) {
        match self {
            Self::Borrowed(bytes) => unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len()); },
            Self::ZeroFill(length) => unsafe { destination.write_bytes(0, *length); },
            Self::Metadata(input, _) => unsafe { metadata_encode(**input, destination); },
        }
    }
}
fn encode_sections<'a, Name: AsRef<str> + 'a>(
    name: &str, source: &'a [Section], declared: impl Iterator<Item = &'a Definition<Name>>,
    extra: impl Iterator<Item = (&'a str, CoffPayload<'a>, u32, &'a [Relocation])>,
    imports: Option<&mut Vec<ImportKey<'a>>>,
) -> Result<Vec<u8>> {
    let sections = source.iter().map(|section|
        (section.name.as_str(), CoffPayload::Borrowed(section.bytes.as_slice()), section.flags, section.relocations.len())
    ).chain(extra.map(|(name, payload, flags, relocations)| (name, payload, flags, relocations.len()))).collect::<Vec<_>>();
    let input_relocations = source.iter().map(|section| {
        section.relocations.iter().map(|relocation| {
            let empty = BindingBytes::new(&[]);
            let (target, section, name, dll) = match &relocation.target {
                Target::Section(section, _) => (0, *section as u64, empty, empty),
                Target::Symbol(name) => (1, 0, BindingBytes::new(name.as_bytes()), empty),
                Target::Import(import) => (2, 0, BindingBytes::new(import.name.as_bytes()), BindingBytes::new(import.dll.as_bytes())),
            };
            BindingRelocation { offset: relocation.offset as u64, kind: u64::from(relocation.kind), target, section, name, dll }
        }).collect::<Vec<_>>()
    }).collect::<Vec<_>>();
    let binding_sections = source.iter().zip(&input_relocations).map(|(section, relocations)| BindingSection {
        name: BindingBytes::new(section.name.as_bytes()), flags: u64::from(section.flags),
        relocations: relocations.as_ptr(), relocation_count: relocations.len(),
        associated: section.associated.as_ptr(), associated_count: section.associated.len(), owner: 0, symbol: 0,
    }).collect::<Vec<_>>();
    let definitions = declared.map(|symbol| BindingDefinition {
        name: BindingBytes::new(symbol.name.as_ref().as_bytes()), section: symbol.section as u64,
        value: u64::from(symbol.value), function: u64::from(symbol.function),
    }).collect::<Vec<_>>();
    encode_bound(name, sections, binding_sections, &definitions, imports)
}

fn encode_bound<'a>(
    name: &str,
    sections: Vec<(&str, CoffPayload<'_>, u32, usize)>,
    mut binding_sections: Vec<BindingSection>,
    definitions: &[BindingDefinition],
    imports: Option<&mut Vec<ImportKey<'a>>>,
) -> Result<Vec<u8>> {
    let relocation_count: usize = binding_sections.iter().map(|section| section.relocation_count).sum();
    // At most two symbols per section and one per definition/reference.
    // Each definition/reference contributes at most one shared tree node.
    let mut symbols = Vec::<CoffSymbolView>::with_capacity(2 * binding_sections.len() + definitions.len() + relocation_count);
    let mut nodes = Vec::<[u64; 9]>::with_capacity(definitions.len() + relocation_count);
    let mut relocations = Vec::<CoffRelocationView>::with_capacity(relocation_count);
    let binding_input = BindingInput {
        name: BindingBytes::new(name.as_bytes()),
        sections: binding_sections.as_mut_ptr(), section_count: binding_sections.len(),
        definitions: definitions.as_ptr(), definition_count: definitions.len(),
    };
    // Only the MSVC input producer keeps names across objects. B borrows the
    // source and emits each object's existing unique keys into that same list.
    let (import_context, emit_import): (*mut c_void, Option<EmitImport<'a>>) = match imports {
        Some(imports) => (std::ptr::from_mut(imports).cast(), Some(collect_import)),
        None => (std::ptr::null_mut(), None),
    };
    let work = BindingWork {
        symbols: symbols.as_mut_ptr(), nodes: nodes.as_mut_ptr(), relocations: relocations.as_mut_ptr(),
        import_context, emit_import,
    };
    let plan = unsafe { binding_prepare(binding_input, work) };
    match plan.error {
        BindingError::None => (),
        BindingError::SectionCount => return Err("COFF符号节号超过有符号16位字段".into()),
        BindingError::RelocationOffset => return Err("COFF重定位偏移超过32位字段".into()),
    }
    unsafe {
        symbols.set_len(plan.symbols);
        relocations.set_len(plan.relocations);
    }
    let mut at = 0;
    let mut physical = sections.iter().map(|(name, payload, flags, count)| {
        let bound = &relocations[at..at + count];
        at += count;
        CoffSectionView {
            name: name.as_ptr(), name_length: name.len(),
            byte_length: payload.len(), flags: u64::from(*flags),
            relocations: bound.as_ptr(), relocation_count: bound.len(), offset: 0, relocation_offset: 0,
        }
    }).collect::<Vec<_>>();
    let input = CoffInput {
        sections: physical.as_mut_ptr(), section_count: physical.len(),
        symbols: symbols.as_ptr(), symbol_count: symbols.len(),
    };
    let layout = unsafe { coff_layout(input) };
    match layout.error {
        CoffError::None => (),
        CoffError::SectionCount => return Err("COFF节数量超过16位字段".into()),
        CoffError::SectionSize => return Err("COFF节长度超过32位字段".into()),
        CoffError::Offset => return Err("COFF文件偏移超过32位字段".into()),
        CoffError::RelocationCount => return Err("COFF重定位数量超过32位字段".into()),
        CoffError::SymbolCount => return Err("COFF符号数量超过32位字段".into()),
        CoffError::Strings => return Err("COFF字符串表超过32位字段".into()),
    }
    let mut names = Vec::<u8>::with_capacity(plan.names);
    unsafe {
        binding_materialize(binding_input, work, plan, names.as_mut_ptr());
        names.set_len(plan.names);
    }
    let mut output = Vec::with_capacity(layout.length);
    unsafe {
        coff_encode(input, layout, output.as_mut_ptr());
        for ((_, payload, _, _), section) in sections.iter().zip(&physical) {
            payload.write(output.as_mut_ptr().add(section.offset));
        }
        output.set_len(layout.length);
    }
    Ok(output)

}

pub(super) fn decode<Bytes: super::ReadSection>(b: &super::ObjectBytes, stack_output: bool) -> Result<Object<Bytes>> {
    decode_object(&Reader::new(b)?, None, None, stack_output)
}
struct Metadata<'a> {
    input: &'a [u8],
    prefix: MetadataReadPrefix,
    binding_start: usize,
}

impl<'a> Metadata<'a> {
    fn read(input: &'a [u8], sink: MetadataNameSink) -> Result<Self> {
        let mut cursor = MetadataReadCursor::new(input);
        let prefix = unsafe { metadata_read_prefix(&mut cursor, u64::from(FORMAT), sink) };
        prefix.header.error.check()?;
        Ok(Self { input, prefix, binding_start: cursor.position })
    }

    fn project_names(&self, sink: MetadataNameSink) -> Result<()> {
        unsafe { metadata_project_names(self.prefix, sink) }.check()
    }

    fn bindings_cursor(&self) -> MetadataReadCursor {
        MetadataReadCursor { input: ReadBytes::new(self.input), position: self.binding_start }
    }
}

pub(crate) struct ReuseInputs<'a> {
    pub(crate) imports: BTreeSet<&'a str>,
    reader: Reader<'a, super::ObjectBytes>,
    metadata: Metadata<'a>,
    has_archive: bool,
}

impl ReuseInputs<'_> {
    pub(crate) fn restore(self) -> Result<Option<Object>> {
        drop(self.imports);
        let mut names = MetadataNames { archive: self.has_archive, ..Default::default() };
        self.metadata.project_names(names.sink(LIBRARIES))?;
        names.check()?;
        for library in &names.libraries {
            let path = Path::new(library);
            if path.is_absolute() && !path.try_exists()? {
                return Ok(None);
            }
        }
        decode_object(&self.reader, None, Some((self.metadata, names.libraries)), true).map(Some)
    }
}

pub(super) struct HostBuildSection {
    section: usize,
    sections: usize,
    record: std::ops::Range<usize>,
    symbols: usize,
}

pub(super) enum HostArchiveMember {
    Original(Vec<String>, Option<HostBuildSection>),
    Groups(Vec<(Vec<u8>, Vec<String>)>),
}

impl HostBuildSection {
    pub(super) fn length(&self, bytes: &[u8]) -> usize {
        bytes.len() - 40 - self.record.len()
    }

    // The final archive already owns this member's output range.
    pub(super) unsafe fn write(&self, bytes: &[u8], output: *mut u8) {
        let header = &bytes[..self.section];
        let body = &bytes[self.section + 40..self.record.start];
        let tail = &bytes[self.record.end..];
        unsafe {
            std::ptr::copy_nonoverlapping(header.as_ptr(), output, header.len());
            std::ptr::copy_nonoverlapping(body.as_ptr(), output.add(header.len()), body.len());
            std::ptr::copy_nonoverlapping(tail.as_ptr(), output.add(header.len() + body.len()), tail.len());
            host_headers(
                output, self.sections, self.symbols - 40 - self.record.len(), self.record.end, self.record.len(),
            );
        }
    }
}
/// 同一COFF包络的借用视图；构建调用先消费输入记录，命中后才消费代码ABI。
pub(crate) struct Reader<'a, Input: ?Sized = [u8]> {
    bytes: &'a Input,
    headers: &'a [u8],
}
impl<'a, Input: AsRef<[u8]> + ?Sized> Reader<'a, Input> {
    pub(crate) fn new(bytes: &'a Input) -> Result<Self> {
        let headers = unsafe { read_headers(ReadBytes::new(bytes.as_ref())) };
        headers.error.check()?;
        Ok(Self { bytes, headers: unsafe { headers.bytes.borrowed() } })
    }
    pub(crate) fn build_inputs(&self) -> Result<Option<&'a [u8]>> {
        let input = unsafe { read_build_inputs(ReadBytes::new(self.bytes.as_ref()), ReadBytes::new(self.headers)) };
        input.error.check()?;
        Ok(if input.present == 0 { None } else { Some(unsafe { input.bytes.borrowed() }) })
    }
    fn host_build_section(
        &self, index: usize, build: &ObjectSection, symbols: usize,
    ) -> Result<HostBuildSection> {
        let bytes = self.bytes.as_ref();
        // A build record is physical data. Section decoding treats an
        // uninitialized row at offset zero as zero-fill, retaining its size here.
        if build.zero_fill > bytes.len() {
            ReadError::Truncated.check()?;
        }
        let start = build.bytes.bytes.addr() - bytes.as_ptr().addr();
        let end = start + build.bytes.length + build.zero_fill;
        let following = &self.headers[(index + 1) * 40..];
        if start < 20 + self.headers.len() || end > symbols
            || following.chunks_exact(40).any(|section| &section[..8] != b".blrt\0\0\0")
        {
            return Err("宿主模块的构建记录不在原COFF元数据区".into());
        }
        Ok(HostBuildSection {
            section: 20 + index * 40,
            sections: self.headers.len() / 40 - 1,
            record: start..end,
            symbols,
        })
    }

    fn sections(&self, materialize: bool) -> Result<(ObjectParts, Vec<ObjectSection>)> {
        let count = self.headers.len() / 40;
        let mut parts = ObjectParts::default();
        let mut rows = Vec::with_capacity(if materialize { count } else { 0 });
        let output = if materialize { rows.as_mut_ptr() } else { std::ptr::null_mut() };
        read_object_names(|context| unsafe {
            object_sections(
                ReadBytes::new(self.bytes.as_ref()), ReadBytes::new(self.headers), output,
                &mut parts, context, validate_object_name,
            )
        })?;
        if materialize {
            unsafe { rows.set_len(count) };
        }
        Ok((parts, rows))
    }
    fn metadata(&self, bytes: ReadBytes) -> Result<&'a [u8]> {
        // B borrows this range from the same validated section rows.
        if bytes.bytes.is_null() { return Err("缺少 Blink 目标文件元数据".into()); }
        Ok(unsafe { bytes.borrowed() })
    }
    fn symbol_rows(&self, definitions: Option<&mut [SectionDefinition]>) -> Result<(Vec<ObjectSymbol>, usize)> {
        let input = ReadBytes::new(self.bytes.as_ref());
        let table = unsafe { read_symbols(input) };
        table.error.check()?;
        let mut symbols = Vec::with_capacity(table.count);
        let (definitions, count) = definitions.map_or((std::ptr::null_mut(), 0), |rows| (rows.as_mut_ptr(), rows.len()));
        read_object_names(|context| unsafe {
            object_symbols(
                input, table, symbols.as_mut_ptr(), definitions, count,
                context, validate_object_name,
            )
        })?;
        unsafe { symbols.set_len(table.count) };
        Ok((symbols, table.offset))
    }
    fn relocations<F>(&self, raw: &[ObjectSection], symbols: &[ObjectSymbol],
        row: &ObjectSection, consume: F,
    ) -> Result<()>
    where
        F: FnMut(usize, u16, ObjectTarget) -> Result<()>,
    {
        let input = ReadBytes::new(self.bytes.as_ref());
        let rows = ObjectRows::new(raw);
        let symbols = ObjectSymbols { values: symbols.as_ptr(), count: symbols.len() };
        let mut receiver = RelocationConsumer { consume, result: Ok(()) };
        let status = unsafe {
            object_relocations(input, rows, symbols, row,
                std::ptr::from_mut(&mut receiver).cast(), consume_relocation::<F>)
        };
        status.format.check()?;
        status.object.check()?;
        receiver.result
    }
    fn module_symbols(&self, definitions: Option<&mut [SectionDefinition]>) -> Result<Option<(Vec<ObjectSection>, Vec<ObjectSymbol>, usize)>> {
        let (parts, raw) = self.sections(true)?;
        let metadata = Metadata::read(self.metadata(parts.metadata)?, MetadataNames::default().sink(0))?;
        if !matches!(metadata.prefix.header.kind, ReadObjectKind::Module) { return Ok(None); }
        let (symbols, offset) = self.symbol_rows(definitions)?;
        Ok(Some((raw, symbols, offset)))
    }
    /// B archives preserve member bytes and only produce the physical export directory.
    pub(crate) fn archive_symbols(&self) -> Result<Option<Vec<String>>> {
        self.module_symbols(None)?.map(|(raw, symbols, _)| export_names(&raw, &symbols)).transpose()
    }
    /// Native archives additionally consume the actual DLL import relocations.
    pub(super) fn host_links(
        &self, imports: &mut Vec<ImportKey<'a>>,
    ) -> Result<Option<HostArchiveMember>> {
        let mut definitions = vec![SectionDefinition::default(); self.headers.len() / 40];
        let Some((raw, symbols, offset)) = self.module_symbols(Some(&mut definitions))? else { return Ok(None); };
        let mut groups = if host::needs_groups(&symbols) {
            host::Groups::new(&raw, &definitions)?
        } else {
            None
        };
        let names = if groups.is_none() { export_names(&raw, &symbols)? } else { Vec::new() };
        let mut build = None;
        for (index, row) in raw.iter().enumerate() {
            if row.mapped == 0 {
                if build.is_none() && unsafe { row.name.borrowed() } == BUILD.as_bytes() {
                    build = Some((index, row));
                }
                continue;
            }
            self.relocations(&raw, &symbols, row, |offset, kind, target| {
                if matches!(target.kind, ObjectTargetKind::Import) {
                    // B returns two ranges of a symbol name in this COFF input.
                    let dll = std::str::from_utf8(unsafe { target.dll.borrowed() })?;
                    let symbol = std::str::from_utf8(unsafe { target.name.borrowed() })?;
                    imports.push(ImportKey::new(dll, symbol));
                }
                if let Some(groups) = &mut groups {
                    groups.relocation(row.mapped - 1, offset, kind, target);
                }
                Ok(())
            })?;
        }
        let build = build.map(|(index, row)| self.host_build_section(index, row, offset)).transpose()?;
        let member = match groups {
            Some(groups) => HostArchiveMember::Groups(
                groups.encode(self.bytes.as_ref(), &raw, &symbols, build.as_ref())?,
            ),
            None => HostArchiveMember::Original(names, build),
        };
        Ok(Some(member))
    }

    /// 从库成员借用公开接口与合同。完整对象负责代码、重定位和诊断材料。
    pub(crate) fn interface(&self) -> Result<(String, Kind, BTreeMap<String, Vec<u8>>)> {
        let (parts, _) = self.sections(false)?;
        let mut names = MetadataNames::default();
        let metadata = Metadata::read(self.metadata(parts.metadata)?, names.sink(REQUIREMENTS))?;
        let (name, kind) = read_metadata_kind(&metadata.prefix.header, &mut names)?;
        let mut bindings = read_metadata_bindings(
            &mut metadata.bindings_cursor(), metadata.prefix.binding_count, false,
        )?;
        let mut definitions = BTreeMap::new();
        self.for_each_definition(|symbol| {
            let (name, contract) = bindings.remove_entry(symbol).ok_or("导出符号缺少模块接口合同")?;
            definitions.insert(name, contract);
            Ok(())
        })?;
        definitions.extend(bindings);
        Ok((name, kind, definitions))
    }

    pub(super) fn for_each_definition(&self, mut consume: impl FnMut(&str) -> Result<()>) -> Result<()> {
        let input = ReadBytes::new(self.bytes.as_ref());
        let table = unsafe { read_symbols(input) };
        table.error.check()?;
        let mut index = 0;
        loop {
            let row = unsafe { next_export(input, table, &mut index) };
            row.error.check()?;
            if row.bytes.bytes.is_null() { return Ok(()); }
            consume(std::str::from_utf8(unsafe { row.bytes.borrowed() })?)?;
        }
    }
}
impl<'a> Reader<'a, super::ObjectBytes> {
    /// 借用判等所需的名称，等采用后再恢复完整对象。
    pub(crate) fn reuse_inputs(self) -> Result<ReuseInputs<'a>> {
        let (parts, _) = self.sections(false)?;
        let metadata = Metadata::read(self.metadata(parts.metadata)?, MetadataNames::default().sink(0))?;
        let mut cursor = metadata.bindings_cursor();
        let mut names = BTreeSet::new();
        for _ in 0..metadata.prefix.binding_count {
            let value = unsafe { metadata_read_binding(&mut cursor) };
            value.error.check()?;
            names.insert(std::str::from_utf8(unsafe { value.name.borrowed() })?);
        }
        let input = ReadBytes::new(self.bytes.as_ref());
        let table = unsafe { read_symbols(input) };
        table.error.check()?;
        let mut index = 0;
        loop {
            let row = unsafe { next_export(input, table, &mut index) };
            row.error.check()?;
            if row.bytes.bytes.is_null() { break; }
            names.remove(std::str::from_utf8(unsafe { row.bytes.borrowed() })?);
        }
        Ok(ReuseInputs {
            imports: names,
            reader: self,
            metadata,
            has_archive: !parts.runtime.bytes.is_null(),
        })
    }
    /// 非根资源侧库来自原编译产物；其C符号沿真实ABI恢复，不消费B公开合同。
    pub(crate) fn native<Bytes: super::ReadSection>(&self, name: &str) -> Result<Object<Bytes>> {
        decode_object(self, Some(name), None, false)
    }
}
fn read_metadata_kind(header: &MetadataReadHeader, names: &mut MetadataNames) -> Result<(String, Kind)> {
    let name = read_text(header.name)?;
    let kind = match header.kind {
        ReadObjectKind::Module => {
            let interface: Arc<str> = std::str::from_utf8(unsafe { header.interface.borrowed() })?.into();
            Kind::Module {
                interface,
                requirements: Arc::new(std::mem::take(&mut names.requirements)),
                stack_bytes: header.stack as u32,
            }
        }
        ReadObjectKind::Application => Kind::Application { stack_bytes: header.stack as u32 },
        ReadObjectKind::Library => Kind::Library {
            prefix: read_text(header.prefix)?, header: read_text(header.header)?, binding: read_text(header.binding)?,
        },
    };
    // Preserve format and header errors ahead of the first name error.
    names.check()?;
    Ok((name, kind))
}

fn read_metadata_bindings(
    cursor: &mut MetadataReadCursor, count: u64, same_producer: bool,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut bindings = BTreeMap::new();
    for _ in 0..count {
        let value = unsafe { metadata_read_binding(cursor) };
        value.error.check()?;
        bindings.insert(read_text(value.name)?, crate::check::read_function_contract(unsafe { value.value.borrowed() }, same_producer)?);
    }
    Ok(bindings)
}

fn read_metadata(
    meta: Metadata<'_>, archive: Option<super::ObjectBytes>, mut names: MetadataNames, same_producer: bool,
) -> Result<(String, Kind, BTreeMap<String, Vec<u8>>, StackMetadata, Option<Archive>)> {
    let (name, kind) = read_metadata_kind(&meta.prefix.header, &mut names)?;
    let mut c = meta.bindings_cursor();
    let prefix = meta.prefix;
    let keep_archive = archive.is_some();
    let mut resource = MetadataReadCursor::new(unsafe { prefix.resource.borrowed() });
    let head = unsafe { metadata_read_resource_head(&mut resource) };
    head.error.check()?;
    let resources = if head.present != 0 {
        let prefix = std::str::from_utf8(unsafe { head.prefix.borrowed() })?;
        let body = unsafe { metadata_read_resource_body(&mut resource) };
        body.error.check()?;
        let code = read_strings(body.code_count, keep_archive, || {
            let value = unsafe { metadata_read_resource_text(&mut resource) };
            value.error.check()?;
            Ok(std::str::from_utf8(unsafe { value.value.borrowed() })?)
        })?;
        keep_archive.then(|| ResourceBinding {
            prefix: prefix.to_owned(),
            types: body.types as u32,
            programs: body.programs as u32,
            abi: unsafe { body.abi.borrowed() }.to_vec(),
            code,
        })
    } else {
        None
    };
    let bindings = read_metadata_bindings(&mut c, prefix.binding_count, same_producer)?;
    let mut stack = StackMetadata::default();
    for _ in 0..c.word()? {
        let frame = unsafe { metadata_read_frame(&mut c) };
        frame.error.check()?;
        std::str::from_utf8(unsafe { frame.name.borrowed() })?;
        for _ in 0..frame.calls {
            let call = unsafe { metadata_read_call(&mut c) };
            call.error.check()?;
            if matches!(call.kind, ReadCallKind::Unreported) {
                std::str::from_utf8(unsafe { call.reason.borrowed() })?;
            }
            stack.calls.push(call);
        }
        stack.frames.push(frame);
    }
    let archive = archive.map(|bytes| Archive {
        bytes,
        libraries: names.libraries,
        exports: names.exports,
        resources,
    });
    Ok((name, kind, bindings, stack, archive))
}
fn decode_object<Bytes: super::ReadSection>(
    reader: &Reader<'_, super::ObjectBytes>, native: Option<&str>, reuse: Option<(Metadata<'_>, Vec<String>)>,
    stack_output: bool,
) -> Result<Object<Bytes>> {
    let b = reader.bytes;
    let mut definitions = vec![SectionDefinition::default(); reader.headers.len() / 40];
    let (symbols, _) = reader.symbol_rows(Some(&mut definitions))?;
    let (parts, mut raw) = reader.sections(true)?;
    let (name, kind, mut bindings, stack, archive) = match native {
        Some(name) => (name.to_owned(), Kind::Module { interface: "".into(), requirements: Arc::default(), stack_bytes: 0 }, BTreeMap::new(), StackMetadata::default(), None),
        None => {
            let archive = (!parts.runtime.bytes.is_null()).then(|| {
                let start = parts.runtime.bytes.addr() - b.as_ptr().addr();
                b.slice(start..start + parts.runtime.length)
            });
            let mut names = MetadataNames { archive: archive.is_some(), ..Default::default() };
            let same_producer = reuse.is_some();
            let metadata = match reuse {
                Some((metadata, libraries)) => {
                    names.libraries = libraries;
                    metadata.project_names(names.sink(REQUIREMENTS | EXPORTS))?;
                    metadata
                }
                None => Metadata::read(reader.metadata(parts.metadata)?, names.sink(REQUIREMENTS | LIBRARIES | EXPORTS))?,
            };
            read_metadata(metadata, archive, names, same_producer)?
        }
    };
    let mut sections = Vec::new();
    for row in &raw {
        if row.mapped != 0 {
            let bytes = if row.zero_fill == 0 { unsafe { row.bytes.borrowed() } } else { &[] };
            sections.push(Section {
                associated: Vec::new(),
                layout: None,
                name: unsafe { std::str::from_utf8_unchecked(row.name.borrowed()) }.to_owned(),
                bytes: Bytes::from_coff(bytes, row.zero_fill),
                flags: row.flags as u32,
                relocations: Vec::new(),
            });
        }
    }
    let rows = ObjectRows::new(&raw);
    for (old, row) in raw.iter().enumerate() {
        if row.mapped == 0 || row.flags & 0x1000 == 0 { continue; }
        let value = unsafe { object_association(rows, definitions.as_ptr(), old) };
        value.error.check()?;
        if value.present != 0 { sections[value.owner].associated.push(row.mapped - 1); }
    }
    for row in &raw {
        if row.mapped == 0 { continue; }
        let relocations = &mut sections[row.mapped - 1].relocations;
        reader.relocations(&raw, &symbols, row, |offset, kind, value| {
            let target = match value.kind {
                ObjectTargetKind::Section => Target::Section(value.section, value.value as u32),
                ObjectTargetKind::Import => Target::Import(Import { dll: read_text(value.dll)?, name: read_text(value.name)? }),
                ObjectTargetKind::Symbol => {
                    // The primary symbol's borrowed name was checked before target resolution.
                    let name = unsafe { std::str::from_utf8_unchecked(value.name.borrowed()) };
                    if native.is_some() { bindings.entry(name.to_owned()).or_default(); }
                    else { bindings.get(name).ok_or("未定义符号缺少模块接口合同")?; }
                    Target::Symbol(name.to_owned())
                }
            };
            relocations.push(Relocation { offset, kind, target });
            Ok(())
        })?;
    }
    let symbols = symbols
            .into_iter()
            .filter(|symbol| symbol.external != 0 && symbol.section > 0)
            .map(|symbol| {
                let name = unsafe { std::str::from_utf8_unchecked(symbol.name.borrowed()) };
                let (name, contract) = if native.is_some() {
                    (name.to_owned(), Vec::new())
                } else {
                    bindings.remove_entry(name).ok_or("导出符号缺少模块接口合同")?
                };
                Ok(Symbol {
                    definition: Definition {
                        name,
                        section: object_index(&raw, symbol.section as usize - 1)
                            .ok_or("导出符号引用不可装载的节")?,
                        value: symbol.value,
                        function: symbol.is_function(),
                    },
                    contract,
                })
            })
            .collect::<Result<_>>()?;
    // Relocation decoding is complete and the owning buffers are stable.
    for row in raw.iter_mut().filter(|row| row.mapped != 0) {
        let relocations = &sections[row.mapped - 1].relocations;
        row.decoded_relocations = relocations.as_ptr().cast();
        row.decoded_count = relocations.len();
    }
    Ok(Object {
        stack: stack.restore(ObjectRows::new(&raw), stack_output)?,
        name,
        symbols,
        imports: bindings,
        sections,
        archive,
        kind,
    })
}

pub(super) mod facts;
mod host;
