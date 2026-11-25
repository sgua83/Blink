//! COFF 节、符号与 AMD64 重定位；Blink 元数据不替代标准重定位表。
use super::*;
use std::{borrow::Cow, collections::{BTreeMap, BTreeSet}, ffi::c_void, sync::Arc};
const META: &str = ".blmeta";
const ARCHIVE: &str = ".blrt";
const BUILD: &str = ".blbuild";
const FORMAT: u32 = 94;

#[repr(u64)]
#[derive(Clone, Copy)]
enum CoffBody { Bytes, Metadata, NativeImport }

#[repr(C)]
// B holds this physical row through binding, layout and final write.
struct CoffSectionView {
    name: *const u8, name_length: usize,
    byte_length: usize, flags: u64,
    relocation_count: usize, uninitialized: u64,
    offset: usize, relocation_offset: usize,
    relocations: *const c_void, associated: *const usize, associated_count: usize,
    owner: u64, symbol: u64,
    body: *const c_void, body_kind: CoffBody,
}
#[repr(C)]
struct CoffSymbolView {
    name: *const u8, name_length: usize, section: u64, value: u64,
    function: u64, external: u64, auxiliary: u64, owner: u64, generated: u64, source: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CoffInput {
    sections: *mut CoffSectionView, section_count: usize,
    symbols: *const CoffSymbolView, symbol_count: usize,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum CoffError {
    None, SectionCount, SectionSize, Offset, RelocationCount, SymbolCount, Strings,
    BindingSectionCount, BindingRelocationOffset,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CoffLayout { length: usize, symbols: usize, strings: usize, symbol_count: usize, error: CoffError }
type ReadRelocationView = unsafe extern "C" fn(*const c_void, usize) -> BindingRelocation;
#[repr(C)]
#[derive(Clone, Copy)]
struct CoffBindingSource {
    module: BindingBytes, sections: *const CoffSectionView, nodes: *const [u64; 9],
    names_root: u64, imports_root: u64, read_relocation: Option<ReadRelocationView>,
}
#[repr(C)]
struct CoffSource {
    name: BindingBytes,
    sections: *mut CoffSectionView,
    section_count: usize,
    bound_count: usize,
    definition_count: usize,
    next_section: Option<unsafe extern "C" fn(*mut c_void) -> CoffSectionView>,
    next_definition: unsafe extern "C" fn(*mut c_void) -> BindingDefinition,
    read_relocation: Option<ReadRelocationView>,
}
#[repr(C)]
struct CoffWork<'a> {
    context: *mut c_void,
    reserve: unsafe extern "C" fn(*mut c_void, usize, usize) -> BindingStorage,
    data_name: *const u8,
    name: unsafe extern "C" fn(*mut c_void, usize) -> *mut u8,
    consume: unsafe extern "C" fn(*mut c_void, CoffOutput<'_>),
    import_context: *mut c_void,
    emit_import: Option<EmitImport<'a>>,
}

unsafe extern "C" {
    #[link_name = "bootstrap.coff::with_output"]
    fn coff_with_output(source: CoffSource, work: CoffWork<'_>) -> CoffError;
    #[link_name = "bootstrap.coff::needs_body"]
    fn coff_needs_body(flags: u64, relocation_count: usize) -> u32;
    #[link_name = "bootstrap.coff::write"]
    fn coff_write(plan: &CoffOutput<'_>, output: *mut u8);
    #[link_name = "bootstrap.bindings::relocation_symbol"]
    fn relocation_symbol(binding: &CoffBindingSource, section: usize, relocation: usize) -> usize;
    #[link_name = "bootstrap.bindings::write_generated_name"]
    fn write_generated_name(binding: &CoffBindingSource, kind: u64, source: u64, output: *mut u8);
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
    fn text(bytes: BindingBytes) -> &'a str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(bytes.bytes, bytes.length)) }
    }

    pub(crate) fn dll(&self) -> &'a str { Self::text(self.dll) }
    pub(crate) fn symbol(&self) -> &'a str { Self::text(self.symbol) }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BindingDefinition { name: BindingBytes, section: u64, value: u64, function: u64 }

impl From<&Definition> for BindingDefinition {
    fn from(value: &Definition) -> Self {
        Self {
            name: BindingBytes::new(value.name.as_bytes()), section: value.section as u64,
            value: u64::from(value.value), function: u64::from(value.function),
        }
    }
}

#[repr(u16)]
enum BindingTarget { Section, Symbol, Import }

#[repr(C)]
struct BindingRelocation {
    offset: u64, kind: u16, target: BindingTarget,
    value: u32, section: u64, name: BindingBytes, dll: BindingBytes,
}

unsafe extern "C" fn borrow_relocation(records: *const c_void, index: usize) -> BindingRelocation {
    let relocation = unsafe { &*records.cast::<Relocation>().add(index) };
    let empty = BindingBytes::new(&[]);
    let (target, section, value, name, dll) = match &relocation.target {
        // The binding tree borrows the original numeric key until the synchronous write ends.
        Target::Section(section, value) => (
            BindingTarget::Section, *section as u64, *value,
            BindingBytes { bytes: std::ptr::from_ref(section).cast(), length: size_of::<usize>() },
            BindingBytes { bytes: std::ptr::from_ref(value).cast(), length: size_of::<u32>() },
        ),
        Target::Symbol(name) => (BindingTarget::Symbol, 0, 0, BindingBytes::new(name.as_bytes()), empty),
        Target::Import(import) => (
            BindingTarget::Import, 0, 0,
            BindingBytes::new(import.name.as_bytes()), BindingBytes::new(import.dll.as_bytes()),
        ),
    };
    BindingRelocation { offset: relocation.offset as u64, kind: relocation.kind, target, value, section, name, dll }
}

type EmitImport<'a> = unsafe extern "C" fn(*mut c_void, ImportKey<'a>);

#[repr(C)]
struct BindingStorage { symbols: *mut CoffSymbolView, nodes: *mut [u64; 9] }

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ReadBytes { bytes: *const u8, length: usize }
impl ReadBytes {
    fn new(bytes: &[u8]) -> Self { Self { bytes: bytes.as_ptr(), length: bytes.len() } }
    unsafe fn borrowed<'a>(self) -> &'a [u8] { unsafe { std::slice::from_raw_parts(self.bytes, self.length) } }
    fn as_binding(self) -> BindingBytes { BindingBytes { bytes: self.bytes, length: self.length } }
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
impl Default for ReadSymbols {
    fn default() -> Self {
        Self { strings: ReadBytes::default(), offset: 0, count: 0, error: ReadError::None }
    }
}
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
    build_section: usize,
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
    MissingSectionDefinition, RelocationBounds, MetadataMissing, ExportContract, ExportSection, ImportContract,
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
            Self::MetadataMissing => Err("缺少 Blink 目标文件元数据".into()),
            Self::ExportContract => Err("导出符号缺少模块接口合同".into()),
            Self::ExportSection => Err("导出符号引用不可装载的节".into()),
            Self::ImportContract => Err("未定义符号缺少模块接口合同".into()),
        }
    }
}
// One slot per physical symbol record. Auxiliary slots have no primary name.
#[repr(C)]
#[derive(Clone, Copy)]
struct ObjectSymbol<'a> {
    name: ReadBytes,
    // Section value, or the DLL split/error for an undefined import.
    value: u32,
    section: i16,
    external: u8,
    role: ObjectSymbolRole,
    source: std::marker::PhantomData<&'a [u8]>,
}

#[repr(u8)]
#[derive(Clone, Copy)]
enum ObjectSymbolRole { Data, Function, Import, InvalidImport }

impl<'a> ObjectSymbol<'a> {
    fn is_function(&self) -> bool { matches!(self.role, ObjectSymbolRole::Function) }

    // Primary names were checked by the original symbol reader and borrow
    // its input. Auxiliary rows have no name.
    unsafe fn validated_name(&self) -> &'a str {
        unsafe { std::str::from_utf8_unchecked(self.name.borrowed()) }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ObjectSymbols<'a> { values: *const ObjectSymbol<'a>, count: usize }


#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SectionDefinition { owner: u16, selection: u8, present: u8 }

#[repr(C)]
struct ObjectAssociation { present: u64, owner: usize, error: ObjectReadError }
#[repr(u64)]
enum ObjectTargetKind { Section, Import, Symbol }
#[repr(C)]
struct ObjectTarget<'a> {
    kind: ObjectTargetKind, section: usize, value: u64, dll: ReadBytes, name: ReadBytes, error: ObjectReadError,
    source: std::marker::PhantomData<&'a [u8]>,
}

impl<'a> ObjectTarget<'a> {
    // Only import targets have both ranges. B splits the already validated
    // primary name at a UTF-8 boundary, and both ranges borrow the reader input.
    unsafe fn import_key(&self) -> ImportKey<'a> {
        ImportKey {
            dll: self.dll.as_binding(),
            symbol: self.name.as_binding(),
            source: std::marker::PhantomData,
        }
    }
}
#[repr(C)]
struct ObjectRelocationStatus { format: ReadError, object: ObjectReadError }
#[repr(C)]
struct ObjectScanStatus { error: ReadError, complete: u32 }

unsafe extern "C" {
    #[link_name = "bootstrap.object_read::object_sections"]
    fn object_sections(input: ReadBytes, headers: ReadBytes, rows: *mut ObjectSection, loadable: *mut u8,
        parts: &mut ObjectParts, table: &mut ReadSymbols, context: *mut c_void, validate: ValidateObjectName) -> ObjectScanStatus;
    #[link_name = "bootstrap.object_read::object_symbols"]
    fn object_symbols<'a>(input: ReadBytes, table: ReadSymbols, rows: *mut ObjectSymbol<'a>,
        definitions: *mut SectionDefinition, definition_count: usize, context: *mut c_void,
        consume: ValidateSymbolName) -> ObjectScanStatus;
    #[link_name = "bootstrap.object_read::association"]
    fn object_association(rows: ObjectRows, definitions: *const SectionDefinition,
        index: usize) -> ObjectAssociation;
    #[link_name = "bootstrap.object_read::object_relocations"]
    fn object_relocations<'a>(input: ReadBytes, rows: ObjectRows, symbols: ObjectSymbols<'a>,
        section: &ObjectSection, context: *mut c_void,
        consume: unsafe extern "C" fn(*mut c_void, usize, usize, u32, ObjectTarget<'a>),
    ) -> ObjectRelocationStatus;
}
unsafe extern "C" fn consume_relocation<'a, F>(
    context: *mut c_void, section: usize, offset: usize, kind: u32, target: ObjectTarget<'a>,
)
where
    F: FnMut(usize, usize, u16, ObjectTarget<'a>),
{
    let consume = unsafe { &mut *context.cast::<F>() };
    consume(section, offset, kind as u16, target);
}

type ValidateObjectName = unsafe extern "C" fn(*mut c_void, ReadBytes) -> u32;
type ValidateSymbolName = unsafe extern "C" fn(*mut c_void, ReadBytes, usize) -> u32;

unsafe extern "C" fn validate_object_name(context: *mut c_void, bytes: ReadBytes) -> u32 {
    match std::str::from_utf8(unsafe { bytes.borrowed() }) {
        Ok(_) => 1,
        Err(error) => {
            unsafe { *context.cast::<Option<std::str::Utf8Error>>() = Some(error) };
            0
        }
    }
}

unsafe extern "C" fn validate_symbol_name(context: *mut c_void, bytes: ReadBytes, _: usize) -> u32 {
    unsafe { validate_object_name(context, bytes) }
}

struct ArchiveExports<'sections, F> {
    loadable: &'sections [u8],
    emit: F,
    invalid_name: Option<std::str::Utf8Error>,
    invalid_section: bool,
}

unsafe extern "C" fn collect_archive_export<'a, F: FnMut(&'a str)>(
    context: *mut c_void, bytes: ReadBytes, section: usize,
) -> u32 {
    let exports = unsafe { &mut *context.cast::<ArchiveExports<'_, F>>() };
    let name = match std::str::from_utf8(unsafe { bytes.borrowed() }) {
        Ok(name) => name,
        Err(error) => {
            exports.invalid_name = Some(error);
            return 0;
        }
    };
    // Keep scanning names and physical rows even after an invalid export target.
    if section != 0 && !exports.invalid_section {
        if exports.loadable.get(section - 1) == Some(&1) {
            (exports.emit)(name);
        } else {
            exports.invalid_section = true;
        }
    }
    1
}

fn read_object_names(read: impl FnOnce(*mut c_void) -> ObjectScanStatus) -> Result<()> {
    let mut invalid = None::<std::str::Utf8Error>;
    read(std::ptr::from_mut(&mut invalid).cast()).error.check()?;
    invalid.map_or(Ok(()), |error| Err(error.into()))
}

fn object_index(rows: &[ObjectSection], index: usize) -> Option<usize> {
    rows.get(index)?.mapped.checked_sub(1)
}
fn export_names<'a>(rows: &[ObjectSection], symbols: &[ObjectSymbol<'a>]) -> Result<Vec<Cow<'a, str>>> {
    symbols.iter().filter(|symbol| symbol.external != 0 && symbol.section > 0)
        .map(|symbol| {
            object_index(rows, symbol.section as usize - 1).ok_or("导出符号引用不可装载的节")?;
            Ok(Cow::Borrowed(unsafe { symbol.validated_name() }))
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
struct MetadataReadResourceBody {
    types: u64,
    programs: u64,
    abi: ReadBytes,
    code_count: usize,
    error: MetadataReadError,
}
unsafe extern "C" {
    #[link_name = "bootstrap.metadata_read::prefix"]
    fn metadata_read_prefix(
        cursor: *mut MetadataReadCursor, format: u64, sink: MetadataNameSink,
    ) -> MetadataReadPrefix;
    #[link_name = "bootstrap.metadata_read::project_names"]
    fn metadata_project_names(prefix: MetadataReadPrefix, sink: MetadataNameSink) -> MetadataReadError;
    #[link_name = "bootstrap.metadata_read::binding"]
    fn metadata_read_binding(cursor: *mut MetadataReadCursor) -> MetadataReadBinding;
    #[link_name = "bootstrap.metadata_read::restore_kind"]
    fn metadata_restore_kind(header: MetadataReadHeader, output: MetadataReceiver) -> u32;
    #[link_name = "bootstrap.metadata_read::restore_bindings"]
    fn metadata_restore_bindings(cursor: &mut MetadataReadCursor, count: u64, same_producer: u32,
        context: *mut c_void, consume: unsafe extern "C" fn(*mut c_void, MetadataReadBinding, u32) -> u32,
    ) -> MetadataRecovery;
}

#[derive(Default)]
struct StackMetadata {
    frames: Vec<MetadataReadFrame>,
    calls: Vec<MetadataReadCall>,
}

impl MetadataReadCursor {
    fn new(input: &[u8]) -> Self { Self { input: ReadBytes::new(input), position: 0 } }
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
        let extra = std::iter::once(CoffSectionView::extra(META, CoffPayload::Metadata(&input, length), 0x840))
            .chain(build.map(|bytes| CoffSectionView::extra(BUILD, CoffPayload::Borrowed(bytes), 0x840)))
            .chain(o.archive.as_ref().map(|archive|
                CoffSectionView::extra(ARCHIVE, CoffPayload::Borrowed(archive.bytes.as_ref()), 0x840)));
        let extra_count = 1 + usize::from(build.is_some()) + usize::from(o.archive.is_some());
        with_object_sections(o, extra, extra_count, owned_output)
    })
}

fn with_object_sections<'a>(
    object: &'a Object,
    extra: impl Iterator<Item = CoffSectionView>,
    extra_count: usize,
    finish: impl FnOnce(CoffOutput<'_>) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    with_sections(
        &object.name, object.sections.len(), extra_count,
        object.sections.iter().map(|section| CoffSectionView::from_section(section, coff_payload(&section.bytes)))
            .chain(extra),
        object.symbols.len(), object.symbols.iter().map(|symbol| BindingDefinition::from(&symbol.definition)),
        Some(borrow_relocation), None, finish,
    )
}

pub(crate) fn encode_native(name: &str, source: &[Section], declared: &[Definition]) -> Result<Vec<u8>> {
    with_native(name, source, declared, owned_output)
}

pub(super) fn with_native(
    name: &str, source: &[Section], declared: &[Definition],
    finish: impl FnOnce(CoffOutput<'_>) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    with_sections(
        name, source.len(), 0,
        source.iter().map(|section| CoffSectionView::from_section(section, CoffPayload::Borrowed(&section.bytes))),
        declared.len(), declared.iter().map(BindingDefinition::from),
        Some(borrow_relocation), None, finish,
    )
}

pub(crate) fn encode_native_with_imports<'a>(
    name: &str, source: &'a [Section<SectionData<LinkSectionBytes<'_>>>], declared: &'a [Definition],
    imports: &mut Vec<ImportKey<'a>>,
) -> Result<Vec<u8>> {
    with_sections(
        name, source.len(), 0,
        source.iter().map(|section| CoffSectionView::from_section(section, coff_payload(&section.bytes))),
        declared.len(), declared.iter().map(BindingDefinition::from),
        Some(borrow_relocation), Some(imports), owned_output,
    )
}

unsafe extern "C" fn collect_import<'a>(context: *mut c_void, key: ImportKey<'a>) {
    unsafe { &mut *context.cast::<Vec<ImportKey<'a>>>() }.push(key);
}

enum CoffPayload<'a> {
    Borrowed(&'a [u8]),
    ZeroFill(usize),
    Metadata(&'a MetadataInput, usize),
}
fn coff_payload<Bytes: AsRef<[u8]>>(data: &SectionData<Bytes>) -> CoffPayload<'_> {
    match data {
        SectionData::Initialized(bytes) => CoffPayload::Borrowed(bytes.as_ref()),
        SectionData::ZeroFill(length) => CoffPayload::ZeroFill(*length),
    }
}

impl CoffSectionView {
    fn extra(name: &str, payload: CoffPayload<'_>, flags: u32) -> Self {
        let (body, byte_length, uninitialized, body_kind) = match payload {
            CoffPayload::Borrowed(bytes) => (bytes.as_ptr().cast(), bytes.len(), 0, CoffBody::Bytes),
            CoffPayload::ZeroFill(length) => (std::ptr::null(), length, 1, CoffBody::Bytes),
            CoffPayload::Metadata(input, length) => (std::ptr::from_ref(input).cast(), length, 0, CoffBody::Metadata),
        };
        Self {
            name: name.as_ptr(), name_length: name.len(), byte_length, flags: u64::from(flags),
            relocation_count: 0, uninitialized, offset: 0, relocation_offset: 0,
            relocations: std::ptr::null(), associated: std::ptr::null(), associated_count: 0,
            owner: 0, symbol: 0, body, body_kind,
        }
    }

    fn with_relocations<R>(mut self, relocations: &[R], associated: &[usize]) -> Self {
        self.relocations = relocations.as_ptr().cast();
        self.relocation_count = relocations.len();
        self.associated = associated.as_ptr();
        self.associated_count = associated.len();
        self
    }

    fn from_section<Bytes>(section: &Section<Bytes>, payload: CoffPayload<'_>) -> Self {
        Self::extra(&section.name, payload, section.flags)
            .with_relocations(&section.relocations, &section.associated)
    }
}

fn with_sections<'a>(
    name: &str, bound_count: usize, extra_count: usize, sections: impl Iterator<Item = CoffSectionView>,
    definition_count: usize, definitions: impl Iterator<Item = BindingDefinition>,
    read_relocation: Option<ReadRelocationView>, imports: Option<&mut Vec<ImportKey<'a>>>,
    finish: impl FnOnce(CoffOutput<'_>) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    let section_count = bound_count + extra_count;
    let mut physical = Vec::<CoffSectionView>::with_capacity(section_count);
    encode_coff(
        name, physical.as_mut_ptr(), section_count, bound_count, Some(sections),
        definition_count, definitions, read_relocation, imports, finish,
    )
}

struct CoffContext<S, D, F> {
    sections: Option<S>,
    definitions: D,
    finish: Option<F>,
    symbols: Vec<CoffSymbolView>,
    nodes: Vec<[u64; 9]>,
    names: Vec<Vec<u8>>,
    result: Option<Result<Vec<u8>>>,
}

impl<S, D, F> CoffContext<S, D, F>
where
    S: Iterator<Item = CoffSectionView>,
    D: Iterator<Item = BindingDefinition>,
    F: FnOnce(CoffOutput<'_>) -> Result<Vec<u8>>,
{
    unsafe extern "C" fn section(context: *mut c_void) -> CoffSectionView {
        unsafe { &mut *context.cast::<Self>() }.sections.as_mut().unwrap().next().unwrap()
    }

    unsafe extern "C" fn definition(context: *mut c_void) -> BindingDefinition {
        unsafe { &mut *context.cast::<Self>() }.definitions.next().unwrap()
    }

    unsafe extern "C" fn reserve(context: *mut c_void, symbols: usize, nodes: usize) -> BindingStorage {
        let context = unsafe { &mut *context.cast::<Self>() };
        context.symbols.reserve_exact(symbols);
        context.nodes.reserve_exact(nodes);
        BindingStorage { symbols: context.symbols.as_mut_ptr(), nodes: context.nodes.as_mut_ptr() }
    }

    unsafe extern "C" fn name(context: *mut c_void, length: usize) -> *mut u8 {
        // B initializes and borrows these bytes while this allocation remains live.
        let mut name = Vec::with_capacity(length);
        let bytes = name.as_mut_ptr();
        unsafe { &mut *context.cast::<Self>() }.names.push(name);
        bytes
    }

    unsafe extern "C" fn consume(context: *mut c_void, plan: CoffOutput<'_>) {
        // The destination and observers borrow the B plan during this call.
        let finish = unsafe { &mut *context.cast::<Self>() }.finish.take().unwrap();
        let result = finish(plan);
        unsafe { &mut *context.cast::<Self>() }.result = Some(result);
    }
}

fn encode_coff<'a, S, D, F>(
    name: &str, physical: *mut CoffSectionView, section_count: usize, bound_count: usize, sections: Option<S>,
    definition_count: usize, definitions: D, read_relocation: Option<ReadRelocationView>,
    imports: Option<&mut Vec<ImportKey<'a>>>, finish: F,
) -> Result<Vec<u8>>
where
    S: Iterator<Item = CoffSectionView>,
    D: Iterator<Item = BindingDefinition>,
    F: FnOnce(CoffOutput<'_>) -> Result<Vec<u8>>,
{
    let next_section = sections.as_ref()
        .map(|_| CoffContext::<S, D, F>::section as unsafe extern "C" fn(*mut c_void) -> CoffSectionView);
    let mut context = CoffContext {
        sections, definitions, finish: Some(finish),
        symbols: Vec::new(), nodes: Vec::new(), names: Vec::new(), result: None,
    };
    let source = CoffSource {
        name: BindingBytes::new(name.as_bytes()), sections: physical, section_count, bound_count, definition_count,
        next_section, next_definition: CoffContext::<S, D, F>::definition, read_relocation,
    };
    let (import_context, emit_import): (*mut c_void, Option<EmitImport<'a>>) = match imports {
        Some(imports) => (std::ptr::from_mut(imports).cast(), Some(collect_import)),
        None => (std::ptr::null_mut(), None),
    };
    let work = CoffWork {
        context: std::ptr::from_mut(&mut context).cast(),
        reserve: CoffContext::<S, D, F>::reserve, data_name: b".data".as_ptr(), name: CoffContext::<S, D, F>::name,
        consume: CoffContext::<S, D, F>::consume, import_context, emit_import,
    };
    match unsafe { coff_with_output(source, work) } {
        CoffError::None => context.result.unwrap(),
        CoffError::SectionCount => Err("COFF节数量超过16位字段".into()),
        CoffError::SectionSize => Err("COFF节长度超过32位字段".into()),
        CoffError::Offset => Err("COFF文件偏移超过32位字段".into()),
        CoffError::RelocationCount => Err("COFF重定位数量超过32位字段".into()),
        CoffError::SymbolCount => Err("COFF符号数量超过32位字段".into()),
        CoffError::Strings => Err("COFF字符串表超过32位字段".into()),
        CoffError::BindingSectionCount => Err("COFF符号节号超过有符号16位字段".into()),
        CoffError::BindingRelocationOffset => Err("COFF重定位偏移超过32位字段".into()),
    }
}

#[repr(C)]
pub(super) struct CoffOutput<'a> {
    input: CoffInput,
    layout: CoffLayout,
    binding: CoffBindingSource,
    storage: std::marker::PhantomData<&'a [u8]>,
}

impl CoffOutput<'_> {
    pub(super) fn len(&self) -> usize { self.layout.length }

    /// The caller owns this exclusive, non-overlapping range of len() bytes.
    pub(super) unsafe fn write(&self, output: *mut u8) {
        unsafe { coff_write(self, output); }
    }
}

fn owned_output(plan: CoffOutput<'_>) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(plan.len());
    unsafe {
        plan.write(output.as_mut_ptr());
        output.set_len(plan.len());
    }
    Ok(output)
}

pub(super) fn decode<Bytes: super::ReadSection>(b: &super::ObjectBytes, stack_output: bool) -> Result<Object<Bytes>> {
    let reader = Reader::new(b)?;
    reader.restore(None, None, reader.symbol_table(ReadSymbols::default())?, stack_output)
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
    table: ReadSymbols,
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
        self.reader.restore(None, Some((self.metadata, names.libraries)), self.table, true).map(Some)
    }
}

pub(super) struct HostBuildSection {
    index: usize,
    sections: usize,
    record: std::ops::Range<usize>,
    symbols: usize,
}

impl HostBuildSection {
    pub(super) fn length(&self, bytes: &[u8]) -> usize {
        bytes.len() - 40 - self.record.len()
    }

    // The final archive already owns this member's output range.
    pub(super) unsafe fn write(&self, bytes: &[u8], output: *mut u8) {
        let section = 20 + self.index * 40;
        let header = &bytes[..section];
        let body = &bytes[section + 40..self.record.start];
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
            index,
            sections: self.headers.len() / 40 - 1,
            record: start..end,
            symbols,
        })
    }

    fn read_sections(
        &self, rows: *mut ObjectSection, loadable: *mut u8, table: &mut ReadSymbols,
    ) -> Result<ObjectParts> {
        let mut parts = ObjectParts::default();
        read_object_names(|context| unsafe {
            object_sections(
                ReadBytes::new(self.bytes.as_ref()), ReadBytes::new(self.headers), rows, loadable,
                &mut parts, table, context, validate_object_name,
            )
        })?;
        Ok(parts)
    }

    fn sections(&self, materialize: bool) -> Result<(ObjectParts, Vec<ObjectSection>, ReadSymbols)> {
        let count = self.headers.len() / 40;
        let mut rows = Vec::with_capacity(if materialize { count } else { 0 });
        let output = if materialize { rows.as_mut_ptr() } else { std::ptr::null_mut() };
        let mut table = ReadSymbols::default();
        let parts = self.read_sections(output, std::ptr::null_mut(), &mut table)?;
        if materialize {
            unsafe { rows.set_len(count) };
        }
        Ok((parts, rows, table))
    }
    fn metadata(&self, bytes: ReadBytes) -> Result<&'a [u8]> {
        // B borrows this range from the same validated section rows.
        if bytes.bytes.is_null() { return Err("缺少 Blink 目标文件元数据".into()); }
        Ok(unsafe { bytes.borrowed() })
    }
    fn symbol_table(&self, table: ReadSymbols) -> Result<ReadSymbols> {
        let table = if table.strings.bytes.is_null() {
            unsafe { read_symbols(ReadBytes::new(self.bytes.as_ref())) }
        } else {
            table
        };
        table.error.check()?;
        Ok(table)
    }
    fn symbol_rows(&self, definitions: &mut [SectionDefinition], table: ReadSymbols) -> Result<(Vec<ObjectSymbol<'a>>, usize)> {
        let input = ReadBytes::new(self.bytes.as_ref());
        let table = self.symbol_table(table)?;
        let mut symbols = Vec::with_capacity(table.count);
        read_object_names(|context| unsafe {
            object_symbols(
                input, table, symbols.as_mut_ptr(), definitions.as_mut_ptr(), definitions.len(),
                context, validate_symbol_name,
            )
        })?;
        unsafe { symbols.set_len(table.count) };
        Ok((symbols, table.offset))
    }
    fn relocations<F>(&self, raw: &[ObjectSection], symbols: &[ObjectSymbol<'a>],
        row: &ObjectSection, mut consume: F,
    ) -> Result<()>
    where
        F: FnMut(usize, usize, u16, ObjectTarget<'a>),
    {
        let input = ReadBytes::new(self.bytes.as_ref());
        let rows = ObjectRows::new(raw);
        let symbols = ObjectSymbols { values: symbols.as_ptr(), count: symbols.len() };
        let status = unsafe {
            object_relocations(input, rows, symbols, row,
                std::ptr::from_mut(&mut consume).cast(), consume_relocation::<F>)
        };
        status.format.check()?;
        status.object.check()
    }
    fn is_module(&self, section: ReadBytes) -> Result<bool> {
        let metadata = Metadata::read(self.metadata(section)?, MetadataNames::default().sink(0))?;
        Ok(matches!(metadata.prefix.header.kind, ReadObjectKind::Module))
    }

    /// B archives preserve member bytes and borrow names into the physical directory.
    pub(crate) fn archive_symbols<F: FnMut(&'a str)>(&self, emit: F) -> Result<bool> {
        let section_count = self.headers.len() / 40;
        let mut loadable = Vec::with_capacity(section_count);
        let mut table = ReadSymbols::default();
        let parts = self.read_sections(std::ptr::null_mut(), loadable.as_mut_ptr(), &mut table)?;
        if !self.is_module(parts.metadata)? { return Ok(false); }
        unsafe { loadable.set_len(section_count) };

        let input = ReadBytes::new(self.bytes.as_ref());
        let table = self.symbol_table(table)?;
        let mut exports = ArchiveExports { loadable: &loadable, emit, invalid_name: None, invalid_section: false };
        let error = unsafe {
            object_symbols(input, table, std::ptr::null_mut(), std::ptr::null_mut(), 0,
                std::ptr::from_mut(&mut exports).cast(), collect_archive_export::<F>)
        };
        error.error.check()?;
        if let Some(error) = exports.invalid_name { return Err(error.into()); }
        if exports.invalid_section { return Err("导出符号引用不可装载的节".into()); }
        Ok(true)
    }
    /// Native archives additionally consume the actual DLL import relocations.
    pub(super) fn append_host_members(
        &self, name: &'a str, imports: &mut Vec<ImportKey<'a>>, members: &mut Vec<HostMember<'a>>,
    ) -> Result<bool> {
        let (parts, raw, table) = self.sections(true)?;
        if !self.is_module(parts.metadata)? { return Ok(false); }
        let mut definitions = vec![SectionDefinition::default(); self.headers.len() / 40];
        let (symbols, offset) = self.symbol_rows(&mut definitions, table)?;
        let needs_body = raw.iter().any(|row| {
            row.mapped != 0 && unsafe { coff_needs_body(row.flags, row.relocation_count) != 0 }
        });
        let mut groups = if needs_body || host::needs_groups(&symbols) {
            host::Groups::new(&raw, &definitions, needs_body)?
        } else {
            None
        };
        let names = if groups.is_none() { export_names(&raw, &symbols)? } else { Vec::new() };
        for row in raw.iter().filter(|row| row.mapped != 0) {
            self.relocations(&raw, &symbols, row, |section, offset, kind, target| {
                if matches!(target.kind, ObjectTargetKind::Import) {
                    imports.push(unsafe { target.import_key() });
                }
                if let Some(groups) = &mut groups {
                    groups.relocation(section, offset, kind, target);
                }
            })?;
        }
        let build = parts.build_section.checked_sub(1)
            .map(|index| self.host_build_section(index, &raw[index], offset)).transpose()?;
        match groups {
            Some(groups) => groups.encode_into(self.bytes.as_ref(), &raw, &symbols, build.as_ref(), name, members)?,
            None => members.push(HostMember {
                name: Cow::Borrowed(name),
                bytes: Cow::Borrowed(self.bytes.as_ref()),
                build,
                names,
            }),
        }
        Ok(true)
    }

    /// 从库成员借用公开接口与合同。完整对象负责代码、重定位和诊断材料。
    pub(crate) fn interface(&self) -> Result<(String, Kind, BTreeMap<String, Vec<u8>>)> {
        let (parts, _, table) = self.sections(false)?;
        let mut names = MetadataNames::default();
        let metadata = Metadata::read(self.metadata(parts.metadata)?, names.sink(REQUIREMENTS))?;
        let mut output = MetadataOutput { names, ..Default::default() };
        let context = std::ptr::from_mut(&mut output).cast();
        unsafe { metadata_restore_kind(metadata.prefix.header, MetadataOutput::receiver(context)) };
        output.check()?;
        let (name, kind) = output.identity.take().unwrap();
        let recovery = unsafe {
            metadata_restore_bindings(
                &mut metadata.bindings_cursor(), metadata.prefix.binding_count, 0, context, MetadataOutput::binding,
            )
        };
        recovery.error.check()?;
        output.check()?;
        let bindings = output.bindings;
        let mut exports = BTreeSet::new();
        self.for_each_definition(table, |symbol| {
            if !bindings.contains_key(symbol) || !exports.insert(symbol) {
                return Err("导出符号缺少模块接口合同".into());
            }
            Ok(())
        })?;
        Ok((name, kind, bindings))
    }

    fn for_each_definition(&self, table: ReadSymbols, mut consume: impl FnMut(&'a str) -> Result<()>) -> Result<()> {
        let input = ReadBytes::new(self.bytes.as_ref());
        let table = self.symbol_table(table)?;
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
        let (parts, _, table) = self.sections(false)?;
        let metadata = Metadata::read(self.metadata(parts.metadata)?, MetadataNames::default().sink(0))?;
        let mut cursor = metadata.bindings_cursor();
        let mut names = BTreeSet::new();
        for _ in 0..metadata.prefix.binding_count {
            let value = unsafe { metadata_read_binding(&mut cursor) };
            value.error.check()?;
            names.insert(std::str::from_utf8(unsafe { value.name.borrowed() })?);
        }
        let input = ReadBytes::new(self.bytes.as_ref());
        let table = self.symbol_table(table)?;
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
            table,
            has_archive: !parts.runtime.bytes.is_null(),
        })
    }
    /// 非根资源侧库来自原编译产物；其C符号沿真实ABI恢复，不消费B公开合同。
    pub(crate) fn native<Bytes: super::ReadSection>(&self, name: &str) -> Result<Object<Bytes>> {
        self.restore(Some(name), None, self.symbol_table(ReadSymbols::default())?, false)
    }
}
#[repr(C)]
struct MetadataRecovery { error: MetadataReadError, complete: u32 }

#[repr(C)]
struct MetadataReceiver {
    context: *mut c_void,
    kind: unsafe extern "C" fn(*mut c_void, MetadataReadHeader) -> u32,
    valid_text: ValidateObjectName,
    binding: unsafe extern "C" fn(*mut c_void, MetadataReadBinding, u32) -> u32,
    resource: unsafe extern "C" fn(*mut c_void, ReadBytes, MetadataReadResourceBody),
    code: unsafe extern "C" fn(*mut c_void, ReadBytes),
    frame: unsafe extern "C" fn(*mut c_void, MetadataReadFrame),
    call: unsafe extern "C" fn(*mut c_void, MetadataReadCall),
}

#[derive(Default)]
struct MetadataOutput {
    names: MetadataNames,
    identity: Option<(String, Kind)>,
    bindings: BTreeMap<String, Vec<u8>>,
    resources: Option<ResourceBinding>,
    stack: StackMetadata,
    error: Option<Box<dyn std::error::Error>>,
}

impl MetadataOutput {
    fn receiver(context: *mut c_void) -> MetadataReceiver {
        MetadataReceiver {
            context, kind: Self::kind, valid_text: Self::validate_text, binding: Self::binding,
            resource: Self::resource, code: Self::code, frame: Self::frame, call: Self::call,
        }
    }

    fn name_sink(context: *mut c_void) -> MetadataNameSink {
        MetadataNameSink { context, emit: Self::name, groups: 0 }
    }

    unsafe extern "C" fn name(
        context: *mut c_void, group: MetadataNameGroup, text: ReadBytes, requirement: ReadRequirementKind,
    ) {
        let output = unsafe { &mut *context.cast::<Self>() };
        unsafe { consume_metadata_name(std::ptr::from_mut(&mut output.names).cast(), group, text, requirement) };
    }

    fn check(&mut self) -> Result<()> {
        self.error.take().map_or(Ok(()), Err)
    }

    unsafe extern "C" fn kind(context: *mut c_void, header: MetadataReadHeader) -> u32 {
        let output = unsafe { &mut *context.cast::<Self>() };
        let text = |value| unsafe { validated_text(value) };
        let kind = match header.kind {
            ReadObjectKind::Application => Kind::Application { stack_bytes: header.stack as u32 },
            ReadObjectKind::Library => Kind::Library {
                prefix: text(header.prefix).to_owned(),
                header: text(header.header).to_owned(),
                binding: text(header.binding).to_owned(),
            },
            ReadObjectKind::Module => Kind::Module {
                interface: text(header.interface).into(),
                requirements: Arc::new(std::mem::take(&mut output.names.requirements)),
                stack_bytes: header.stack as u32,
            },
        };
        match output.names.check() {
            Ok(()) => {
                output.identity = Some((text(header.name).to_owned(), kind));
                1
            }
            Err(error) => { output.error = Some(error); 0 }
        }
    }

    unsafe extern "C" fn validate_text(context: *mut c_void, text: ReadBytes) -> u32 {
        let output = unsafe { &mut *context.cast::<Self>() };
        match std::str::from_utf8(unsafe { text.borrowed() }) {
            Ok(_) => 1,
            Err(error) => { output.error = Some(error.into()); 0 }
        }
    }

    unsafe extern "C" fn binding(context: *mut c_void, item: MetadataReadBinding, same_producer: u32) -> u32 {
        let output = unsafe { &mut *context.cast::<Self>() };
        match read_text(item.name).and_then(|name| {
            let bytes = crate::check::read_function_contract(unsafe { item.value.borrowed() }, same_producer != 0)?;
            Ok((name, bytes))
        }) {
            Ok((name, bytes)) => { output.bindings.insert(name, bytes); 1 }
            Err(error) => { output.error = Some(error); 0 }
        }
    }

    unsafe extern "C" fn resource(context: *mut c_void, prefix: ReadBytes, body: MetadataReadResourceBody) {
        let output = unsafe { &mut *context.cast::<Self>() };
        output.resources = Some(ResourceBinding {
            prefix: unsafe { validated_text(prefix) }.to_owned(),
            types: body.types as u32,
            programs: body.programs as u32,
            abi: unsafe { body.abi.borrowed() }.to_vec(),
            code: Vec::new(),
        });
    }

    unsafe extern "C" fn code(context: *mut c_void, text: ReadBytes) {
        let output = unsafe { &mut *context.cast::<Self>() };
        output.resources.as_mut().unwrap().code.push(unsafe { validated_text(text) }.to_owned());
    }

    unsafe extern "C" fn frame(context: *mut c_void, frame: MetadataReadFrame) {
        unsafe { &mut *context.cast::<Self>() }.stack.frames.push(frame);
    }

    unsafe extern "C" fn call(context: *mut c_void, call: MetadataReadCall) {
        unsafe { &mut *context.cast::<Self>() }.stack.calls.push(call);
    }
}

// All callers borrow the reader input for the synchronous restore and only
// use this after the same name passed its UTF-8 consumer.
unsafe fn validated_text<'a>(bytes: ReadBytes) -> &'a str {
    unsafe { std::str::from_utf8_unchecked(bytes.borrowed()) }
}

#[repr(C)]
struct RestoreSymbols<'a> { rows: ObjectSymbols<'a>, definitions: *mut SectionDefinition }
#[repr(C)]
struct RestoreRelocations { values: *const Relocation, count: usize }
#[repr(C)]
struct RestoreStack {
    frames: *mut MetadataReadFrame, count: usize, calls: *mut MetadataReadCall, nodes: *mut [u64; 9],
}
#[repr(C)]
struct RestoreSource {
    input: ReadBytes,
    headers: ReadBytes,
    table: ReadSymbols,
    native_name: ReadBytes,
    reused_prefix: *const MetadataReadPrefix,
    reused_cursor: MetadataReadCursor,
    format: u64,
    relocation_stride: usize,
    stack_output: u32,
}
#[repr(C)]
struct RestoreObjectSink<'a> {
    context: *mut c_void,
    name_context: *mut c_void,
    symbol_name: ValidateSymbolName,
    section_name: ValidateObjectName,
    symbols: unsafe extern "C" fn(*mut c_void, usize, usize) -> RestoreSymbols<'a>,
    sections: unsafe extern "C" fn(*mut c_void, usize) -> ObjectRows,
    archive: unsafe extern "C" fn(*mut c_void, ReadBytes),
    section: unsafe extern "C" fn(*mut c_void, ReadBytes, ReadBytes, u64, usize),
    associate: unsafe extern "C" fn(*mut c_void, usize, usize),
    relocation: unsafe extern "C" fn(*mut c_void, usize, usize, u32, ObjectTarget<'a>),
    relocated: unsafe extern "C" fn(*mut c_void, usize) -> RestoreRelocations,
    bind_import: unsafe extern "C" fn(*mut c_void, ReadBytes, u32) -> u32,
    take_contract: unsafe extern "C" fn(*mut c_void, ReadBytes) -> u32,
    definition: unsafe extern "C" fn(*mut c_void, ObjectSymbol<'a>, usize, u32),
    stack: unsafe extern "C" fn(*mut c_void) -> RestoreStack,
    frame: unsafe extern "C" fn(*mut c_void, MetadataReadFrame),
    call: unsafe extern "C" fn(*mut c_void, MetadataReadCall),
}
#[repr(C)]
struct RestoreObjectStatus { format: ReadError, object: ObjectReadError, metadata: MetadataReadError }
unsafe extern "C" {
    #[link_name = "bootstrap.object_read::restore_object"]
    fn restore_object(source: RestoreSource, sink: RestoreObjectSink<'_>,
        names: MetadataNameSink, metadata: MetadataReceiver,
    ) -> RestoreObjectStatus;
}

struct ObjectOwner<'reader, 'input, Bytes> {
    reader: &'reader Reader<'input, super::ObjectBytes>,
    metadata: *mut MetadataOutput,
    raw: Vec<ObjectSection>,
    primary: Vec<ObjectSymbol<'input>>,
    definitions: Vec<SectionDefinition>,
    sections: Vec<Section<SectionData<Bytes>>>,
    symbols: Vec<Symbol>,
    archive: Option<super::ObjectBytes>,
    contract: Option<(String, Vec<u8>)>,
    stack: Vec<stack::Frame>,
    positions: Vec<[u64; 9]>,
}

impl<'input, Bytes: super::ReadSection> ObjectOwner<'_, 'input, Bytes> {
    fn sink(&mut self, invalid: &mut Option<std::str::Utf8Error>) -> RestoreObjectSink<'input> {
        RestoreObjectSink {
            context: std::ptr::from_mut(self).cast(),
            name_context: std::ptr::from_mut(invalid).cast(),
            symbol_name: validate_symbol_name, section_name: validate_object_name,
            symbols: Self::symbols, sections: Self::sections,
            archive: Self::archive, section: Self::section, associate: Self::associate,
            relocation: Self::relocation, relocated: Self::relocated,
            bind_import: Self::bind_import, take_contract: Self::take_contract,
            definition: Self::definition, stack: Self::stack, frame: Self::frame, call: Self::call,
        }
    }

    // B initializes and borrows these Copy rows within the same call. The
    // owner keeps the allocations and does not read elements or drop them.
    unsafe extern "C" fn symbols(context: *mut c_void, count: usize, sections: usize) -> RestoreSymbols<'input> {
        let owner = unsafe { &mut *context.cast::<Self>() };
        owner.primary = Vec::with_capacity(count);
        owner.definitions = vec![SectionDefinition::default(); sections];
        RestoreSymbols {
            rows: ObjectSymbols { values: owner.primary.as_mut_ptr(), count },
            definitions: owner.definitions.as_mut_ptr(),
        }
    }

    unsafe extern "C" fn sections(context: *mut c_void, count: usize) -> ObjectRows {
        let owner = unsafe { &mut *context.cast::<Self>() };
        owner.raw = Vec::with_capacity(count);
        ObjectRows { values: owner.raw.as_mut_ptr(), count }
    }

    unsafe extern "C" fn archive(context: *mut c_void, range: ReadBytes) {
        let owner = unsafe { &mut *context.cast::<Self>() };
        let bytes = owner.reader.bytes;
        let start = range.bytes.addr() - bytes.as_ptr().addr();
        owner.archive = Some(bytes.slice(start..start + range.length));
        unsafe { &mut *owner.metadata }.names.archive = true;
    }

    unsafe extern "C" fn section(context: *mut c_void, name: ReadBytes, body: ReadBytes, flags: u64, zero_fill: usize) {
        let owner = unsafe { &mut *context.cast::<Self>() };
        let bytes = if zero_fill == 0 {
            SectionData::Initialized(Bytes::from_coff(owner.reader.bytes, unsafe { body.borrowed() }))
        } else {
            SectionData::ZeroFill(zero_fill)
        };
        owner.sections.push(Section {
            associated: Vec::new(), layout: None, name: unsafe { validated_text(name) }.to_owned(),
            bytes, flags: flags as u32, relocations: Vec::new(),
        });
    }

    unsafe extern "C" fn associate(context: *mut c_void, parent: usize, child: usize) {
        let owner = unsafe { &mut *context.cast::<Self>() };
        owner.sections[parent].associated.push(child);
    }

    unsafe extern "C" fn relocation(
        context: *mut c_void, section: usize, offset: usize, kind: u32, value: ObjectTarget<'input>,
    ) {
        let owner = unsafe { &mut *context.cast::<Self>() };
        let target = match value.kind {
            ObjectTargetKind::Section => Target::Section(value.section, value.value as u32),
            ObjectTargetKind::Import => {
                let key = unsafe { value.import_key() };
                Target::Import(Import { dll: key.dll().to_owned(), name: key.symbol().to_owned() })
            }
            ObjectTargetKind::Symbol => Target::Symbol(unsafe { validated_text(value.name) }.to_owned()),
        };
        owner.sections[section].relocations.push(Relocation { offset, kind: kind as u16, target });
    }

    unsafe extern "C" fn relocated(context: *mut c_void, section: usize) -> RestoreRelocations {
        let owner = unsafe { &*context.cast::<Self>() };
        let records = &owner.sections[section].relocations;
        RestoreRelocations { values: records.as_ptr(), count: records.len() }
    }

    unsafe extern "C" fn bind_import(context: *mut c_void, name: ReadBytes, native: u32) -> u32 {
        let owner = unsafe { &*context.cast::<Self>() };
        let bindings = &mut unsafe { &mut *owner.metadata }.bindings;
        let name = unsafe { validated_text(name) };
        if native != 0 {
            bindings.entry(name.to_owned()).or_default();
            1
        } else {
            u32::from(bindings.contains_key(name))
        }
    }

    unsafe extern "C" fn take_contract(context: *mut c_void, name: ReadBytes) -> u32 {
        let owner = unsafe { &mut *context.cast::<Self>() };
        let metadata = unsafe { &mut *owner.metadata };
        owner.contract = metadata.bindings.remove_entry(unsafe { validated_text(name) });
        u32::from(owner.contract.is_some())
    }

    unsafe extern "C" fn definition(context: *mut c_void, symbol: ObjectSymbol<'input>, section: usize, native: u32) {
        let owner = unsafe { &mut *context.cast::<Self>() };
        let (name, contract) = if native != 0 {
            (unsafe { symbol.validated_name() }.to_owned(), Vec::new())
        } else {
            owner.contract.take().unwrap()
        };
        owner.symbols.push(Symbol {
            definition: Definition { name, section, value: symbol.value, function: symbol.is_function() },
            contract,
        });
    }

    unsafe extern "C" fn stack(context: *mut c_void) -> RestoreStack {
        let owner = unsafe { &mut *context.cast::<Self>() };
        let stack = &mut unsafe { &mut *owner.metadata }.stack;
        owner.positions = vec![[0; 9]; stack.frames.len()];
        RestoreStack {
            frames: stack.frames.as_mut_ptr(), count: stack.frames.len(),
            calls: stack.calls.as_mut_ptr(), nodes: owner.positions.as_mut_ptr(),
        }
    }

    unsafe extern "C" fn frame(context: *mut c_void, frame: MetadataReadFrame) {
        let owner = unsafe { &mut *context.cast::<Self>() };
        owner.stack.push(stack::Frame {
            name: unsafe { validated_text(frame.name) }.to_owned(),
            coff: (frame.has_coff != 0).then_some((frame.section as usize, frame.start as u32)),
            local: frame.local as u32,
            calls: Vec::with_capacity(frame.calls as usize),
        });
    }

    unsafe extern "C" fn call(context: *mut c_void, call: MetadataReadCall) {
        let owner = unsafe { &mut *context.cast::<Self>() };
        let call = match call.kind {
            ReadCallKind::Reference => stack::Call::Reference { relocation: call.offset as usize, tail: false },
            ReadCallKind::Tail => stack::Call::Reference { relocation: call.offset as usize, tail: true },
            ReadCallKind::Indirect => stack::Call::Indirect,
            ReadCallKind::Unreported => stack::Call::Unreported(unsafe { validated_text(call.reason) }.to_owned()),
        };
        owner.stack.last_mut().unwrap().calls.push(call);
    }
}

impl Reader<'_, super::ObjectBytes> {
    fn restore<Bytes: super::ReadSection>(&self, native: Option<&str>,
        reuse: Option<(Metadata<'_>, Vec<String>)>, table: ReadSymbols, stack_output: bool,
    ) -> Result<Object<Bytes>> {
        let (reused, libraries) = match reuse {
            Some((reused, libraries)) => (Some(reused), libraries),
            None => (None, Vec::new()),
        };
        let mut output = MetadataOutput { names: MetadataNames { libraries, ..Default::default() }, ..Default::default() };
        let metadata: *mut c_void = std::ptr::from_mut(&mut output).cast();
        let mut owner = ObjectOwner {
            reader: self, metadata: metadata.cast(), raw: Vec::new(), primary: Vec::new(),
            definitions: Vec::new(), sections: Vec::new(), symbols: Vec::new(), archive: None,
            contract: None, stack: Vec::new(), positions: Vec::new(),
        };
        let source = RestoreSource {
            input: ReadBytes::new(self.bytes.as_ref()),
            headers: ReadBytes::new(self.headers),
            table,
            native_name: native.map_or_else(ReadBytes::default, |name| ReadBytes::new(name.as_bytes())),
            reused_prefix: reused.as_ref().map_or(std::ptr::null(), |meta| &raw const meta.prefix),
            reused_cursor: reused.as_ref().map_or_else(|| MetadataReadCursor::new(&[]), Metadata::bindings_cursor),
            format: u64::from(FORMAT), relocation_stride: size_of::<Relocation>(), stack_output: u32::from(stack_output),
        };
        let mut invalid = None;
        let status = unsafe {
            restore_object(source, owner.sink(&mut invalid),
                MetadataOutput::name_sink(metadata), MetadataOutput::receiver(metadata))
        };
        status.format.check()?;
        status.object.check()?;
        status.metadata.check()?;
        if let Some(error) = invalid { return Err(error.into()); }
        output.check()?;

        let (name, kind) = output.identity.unwrap();
        let archive = owner.archive.map(|bytes| Archive {
            bytes, libraries: output.names.libraries, exports: output.names.exports, resources: output.resources,
        });
        Ok(Object {
            name, kind, archive, symbols: owner.symbols, imports: output.bindings,
            sections: owner.sections, stack: owner.stack,
        })
    }
}

pub(super) mod facts;
mod host;
mod native_import;
pub(crate) use native_import::member as native_import_member;
