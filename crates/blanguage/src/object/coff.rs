//! COFF 节、符号与 AMD64 重定位；Blink 元数据不替代标准重定位表。
use super::*;
use std::collections::BTreeMap;
const META: &str = ".blmeta";
const ARCHIVE: &str = ".blrt";
const BUILD: &str = ".blbuild";
const FORMAT: u32 = 91;
// Caller-owned views; original names and bodies stay in Object until encoding ends.
#[repr(C)]
struct CoffSectionView {
    name: *const u8, name_length: usize,
    bytes: *const u8, byte_length: usize, flags: u64,
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
#[repr(C)]
#[derive(Clone, Copy)]
struct BindingWork { symbols: *mut CoffSymbolView, nodes: *mut [u64; 9], relocations: *mut CoffRelocationView }
#[repr(u32)]
#[derive(Clone, Copy)]
enum BindingError { None = 0, SectionCount = 2, RelocationOffset = 5 }
#[repr(C)]
#[derive(Clone, Copy)]
struct BindingPlan { symbols: usize, names: usize, relocations: usize, error: BindingError }
unsafe extern "C" {
    #[link_name = "bootstrap.bindings::prepare"]
    fn binding_prepare(input: BindingInput, work: BindingWork) -> BindingPlan;
    #[link_name = "bootstrap.bindings::materialize"]
    fn binding_materialize(input: BindingInput, work: BindingWork, plan: BindingPlan, names: *mut u8);
}

#[repr(C)]
#[derive(Clone, Copy)]
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
    #[link_name = "bootstrap.coff_read::headers"]
    fn read_headers(input: ReadBytes) -> ReadRange;
    #[link_name = "bootstrap.coff_read::build_inputs"]
    fn read_build_inputs(input: ReadBytes, headers: ReadBytes) -> ReadBuild;
    #[link_name = "bootstrap.coff_read::symbols"]
    fn read_symbols(input: ReadBytes) -> ReadSymbols;
    #[link_name = "bootstrap.coff_read::symbol"]
    fn read_symbol(input: ReadBytes, table: ReadSymbols, index: usize) -> ReadSymbol;
    #[link_name = "bootstrap.coff_read::relocations"]
    fn read_relocations(input: ReadBytes, offset: usize, count: usize, flags: u64) -> ReadRelocations;
    #[link_name = "bootstrap.coff_read::relocation"]
    fn read_relocation(input: ReadBytes, offset: usize, index: usize) -> ReadRelocation;
}
// B owns logical section identity and COFF target interpretation; rows borrow the original input.
#[repr(C)]
#[derive(Clone, Copy)]
struct ObjectSection {
    name: ReadBytes, bytes: ReadBytes, flags: u64, relocations: usize, relocation_count: usize, mapped: usize, zero_fill: usize,
}
#[repr(C)]
struct ObjectSectionResult { value: ObjectSection, error: ReadError }
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
    StackSection, StackPosition, StackNoLocation, StackRelocation,
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
        }
    }
}
#[repr(C)]
struct ObjectAssociation { present: u64, owner: usize, error: ObjectReadError }
#[repr(u64)]
enum ObjectTargetKind { Section, Import, Symbol }
#[repr(C)]
struct ObjectTarget {
    kind: ObjectTargetKind, section: usize, value: u64, dll: ReadBytes, name: ReadBytes, error: ObjectReadError,
}
unsafe extern "C" {
    #[link_name = "bootstrap.object_read::logical_section"]
    fn object_logical_section(input: ReadBytes, headers: ReadBytes, index: usize, mapped: *mut u64) -> ObjectSectionResult;
    #[link_name = "bootstrap.object_read::association"]
    fn object_association(rows: ObjectRows, selection: u64, owner: u64) -> ObjectAssociation;
    #[link_name = "bootstrap.object_read::resolve_target"]
    fn object_resolve_target(rows: ObjectRows, name: ReadBytes, section: i64, value: u64) -> ObjectTarget;
}
fn object_index(rows: &[ObjectSection], index: usize) -> Option<usize> {
    rows.get(index)?.mapped.checked_sub(1)
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
struct MetadataReadHeader {
    name: ReadBytes, kind: ReadObjectKind, interface: ReadBytes, prefix: ReadBytes, header: ReadBytes, binding: ReadBytes,
    requirements: u64, stack: u64, error: MetadataReadError,
}
#[repr(C)]
struct MetadataReadRequirement { name: ReadBytes, kind: ReadRequirementKind, error: MetadataReadError }
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
    #[link_name = "bootstrap.metadata_read::bytes"]
    fn metadata_read_bytes(cursor: *mut MetadataReadCursor) -> MetadataReadField;
    #[link_name = "bootstrap.metadata_read::header"]
    fn metadata_read_header(cursor: *mut MetadataReadCursor, format: u64) -> MetadataReadHeader;
    #[link_name = "bootstrap.metadata_read::requirement"]
    fn metadata_read_requirement(cursor: *mut MetadataReadCursor) -> MetadataReadRequirement;
    #[link_name = "bootstrap.metadata_read::binding"]
    fn metadata_read_binding(cursor: *mut MetadataReadCursor) -> MetadataReadBinding;
    #[link_name = "bootstrap.metadata_read::frame"]
    fn metadata_read_frame(cursor: *mut MetadataReadCursor) -> MetadataReadFrame;
    #[link_name = "bootstrap.metadata_read::call"]
    fn metadata_read_call(cursor: *mut MetadataReadCursor) -> MetadataReadCall;
    #[link_name = "bootstrap.object_read::recover_stack"]
    fn object_recover_stack(
        input: ReadBytes, rows: ObjectRows, frames: *mut MetadataReadFrame,
        count: usize, calls: *const MetadataReadCall, nodes: *mut [u64; 9],
    ) -> ObjectReadError;
}

#[derive(Default)]
struct StackMetadata {
    frames: Vec<MetadataReadFrame>,
    calls: Vec<MetadataReadCall>,
}

impl StackMetadata {
    fn restore(mut self, input: ReadBytes, rows: ObjectRows) -> Result<Vec<stack::Frame>> {
        let mut nodes = vec![[0; 9]; self.frames.len()];
        unsafe {
            object_recover_stack(input, rows, self.frames.as_mut_ptr(), self.frames.len(),
                self.calls.as_ptr(), nodes.as_mut_ptr())
        }.check()?;
        // read_metadata already checked UTF-8 in these borrowed fields.
        let text = |field: ReadBytes| unsafe { std::str::from_utf8_unchecked(field.borrowed()) }.to_owned();
        let mut calls = self.calls.into_iter();
        Ok(self.frames.into_iter().map(|frame| {
            let calls = calls.by_ref().take(frame.calls as usize).map(|call| match call.kind {
                ReadCallKind::Reference => stack::Call::Reference { offset: call.offset as usize, tail: false },
                ReadCallKind::Tail => stack::Call::Reference { offset: call.offset as usize, tail: true },
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
    fn bytes(&mut self) -> Result<ReadBytes> {
        let value = unsafe { metadata_read_bytes(self) };
        value.error.check()?;
        Ok(value.value)
    }
    fn strings(&mut self, keep: bool) -> Result<Vec<String>> {
        let count = self.word()? as usize;
        read_strings(count, keep, || Ok(std::str::from_utf8(unsafe { self.bytes()?.borrowed() })?))
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
struct MetadataCall { kind: u64, offset: u64, reason: MetadataBytes }
#[repr(C)]
struct MetadataFrame {
    name: MetadataBytes, has_coff: u64, section: u64, start: u64, local: u64,
    calls: *const MetadataCall, call_count: usize,
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
    bindings: *const MetadataBinding, binding_count: usize,
    frames: *const MetadataFrame, frame_count: usize,
}
#[repr(u32)]
#[derive(Clone, Copy)]
enum MetadataError { None, Count, Field, SectionSize }
#[repr(C)]
#[derive(Clone, Copy)]
struct MetadataLayout { length: usize, error: MetadataError }
unsafe extern "C" {
    #[link_name = "bootstrap.metadata::layout"]
    fn metadata_layout(input: MetadataInput) -> MetadataLayout;
    #[link_name = "bootstrap.metadata::encode"]
    fn metadata_encode(input: MetadataInput, output: *mut u8);
}

fn metadata(o: &Object) -> Result<Vec<u8>> {
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
    let mut bindings = BTreeMap::new();
    for symbol in &o.symbols {
        bindings.insert(symbol.name.as_str(), symbol.contract.as_slice());
    }
    for (name, contract) in &o.imports {
        if let Some(expected) = bindings.insert(name.as_str(), contract.as_slice())
            && expected != contract.as_slice()
        {
            return Err(format!("对象内模块接口冲突 {name}").into());
        }
    }
    let bindings = bindings.iter().map(|(name, value)| MetadataBinding {
        name: MetadataBytes::new(name.as_bytes()), value: MetadataBytes::new(value),
    }).collect::<Vec<_>>();
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
    let calls = o.stack.iter().map(|frame| frame.calls.iter().map(|call| {
        let (kind, offset, reason) = match call {
            stack::Call::Reference { offset, tail } => (u64::from(*tail), *offset as u64, empty),
            stack::Call::Indirect => (2, 0, empty),
            stack::Call::Unreported(reason) => (3, 0, MetadataBytes::new(reason.as_bytes())),
        };
        MetadataCall { kind, offset, reason }
    }).collect::<Vec<_>>()).collect::<Vec<_>>();
    let frames = o.stack.iter().zip(&calls).map(|(frame, calls)| {
        let (has_coff, section, start) = match frame.coff {
            Some((section, start)) => (1, section as u64, u64::from(start)),
            None => (0, 0, 0),
        };
        MetadataFrame {
            name: MetadataBytes::new(frame.name.as_bytes()), has_coff, section, start,
            local: u64::from(frame.local), calls: calls.as_ptr(), call_count: calls.len(),
        }
    }).collect::<Vec<_>>();
    let mut input = MetadataInput {
        format_version: u64::from(FORMAT), name: MetadataBytes::new(o.name.as_bytes()), kind: 0,
        interface: empty, stack_bytes: 0, prefix: empty, header: empty, binding: empty,
        requirements: requirements.as_ptr(), requirement_count: requirements.len(),
        libraries: libraries.as_ptr(), library_count: libraries.len(),
        exports: exports.as_ptr(), export_count: exports.len(), resource,
        bindings: bindings.as_ptr(), binding_count: bindings.len(),
        frames: frames.as_ptr(), frame_count: frames.len(),
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
    let mut output = Vec::with_capacity(layout.length);
    unsafe {
        metadata_encode(input, output.as_mut_ptr());
        output.set_len(layout.length);
    }
    Ok(output)
}

pub(super) fn encode(o: &Object) -> Result<Vec<u8>> {
    encode_build(o, None)
}
pub(super) fn encode_build(o: &Object, build: Option<&[u8]>) -> Result<Vec<u8>> {
    let meta = metadata(o)?;
    let mut extra = vec![(META, meta.as_slice(), 0x840, &[][..])];
    if let Some(build) = build { extra.push((BUILD, build, 0x840, &[])); }
    if let Some(archive) = &o.archive { extra.push((ARCHIVE, &archive.bytes, 0x840, &[])); }
    encode_sections(&o.name, &o.sections, &o.symbols, &extra)
}

pub(crate) fn encode_native(name: &str, source: &[Section], declared: &[Symbol]) -> Result<Vec<u8>> {
    encode_sections(name, source, declared, &[])
}

fn encode_sections(
    name: &str, source: &[Section], declared: &[Symbol],
    extra: &[(&str, &[u8], u32, &[Relocation])],
) -> Result<Vec<u8>> {
    let sections = source.iter().map(|section|
        (section.name.as_str(), section.bytes.as_slice(), section.flags, section.relocations.as_slice())
    ).chain(extra.iter().copied()).collect::<Vec<_>>();
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
    let mut binding_sections = source.iter().zip(&input_relocations).map(|(section, relocations)| BindingSection {
        name: BindingBytes::new(section.name.as_bytes()), flags: u64::from(section.flags),
        relocations: relocations.as_ptr(), relocation_count: relocations.len(),
        associated: section.associated.as_ptr(), associated_count: section.associated.len(), owner: 0, symbol: 0,
    }).collect::<Vec<_>>();
    let definitions = declared.iter().map(|symbol| BindingDefinition {
        name: BindingBytes::new(symbol.name.as_bytes()), section: symbol.section as u64,
        value: u64::from(symbol.value), function: u64::from(symbol.function),
    }).collect::<Vec<_>>();
    let relocation_count: usize = input_relocations.iter().map(Vec::len).sum();
    // At most two symbols per section and one per definition/reference.
    // Each definition/reference contributes at most one shared tree node.
    let mut symbols = Vec::<CoffSymbolView>::with_capacity(2 * source.len() + declared.len() + relocation_count);
    let mut nodes = Vec::<[u64; 9]>::with_capacity(declared.len() + relocation_count);
    let mut relocations = Vec::<CoffRelocationView>::with_capacity(relocation_count);
    let binding_input = BindingInput {
        name: BindingBytes::new(name.as_bytes()),
        sections: binding_sections.as_mut_ptr(), section_count: binding_sections.len(),
        definitions: definitions.as_ptr(), definition_count: definitions.len(),
    };
    let work = BindingWork { symbols: symbols.as_mut_ptr(), nodes: nodes.as_mut_ptr(), relocations: relocations.as_mut_ptr() };
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
    let mut sections = sections.iter().map(|(name, bytes, flags, source)| {
        let bound = &relocations[at..at + source.len()];
        at += source.len();
        CoffSectionView {
            name: name.as_ptr(), name_length: name.len(),
            bytes: bytes.as_ptr(), byte_length: bytes.len(), flags: u64::from(*flags),
            relocations: bound.as_ptr(), relocation_count: bound.len(), offset: 0, relocation_offset: 0,
        }
    }).collect::<Vec<_>>();
    let input = CoffInput {
        sections: sections.as_mut_ptr(), section_count: sections.len(),
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
        output.set_len(layout.length);
    }
    Ok(output)

}

pub(super) fn decode(b: &super::ObjectBytes) -> Result<Object> {
    Reader::new(b)?.object()
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
    fn section_rows(&self) -> impl Iterator<Item = Result<ObjectSection>> + '_ {
        let input = ReadBytes::new(self.bytes.as_ref());
        let headers = ReadBytes::new(self.headers);
        let mut mapped = 0u64;
        (0..self.headers.len() / 40).map(move |index| {
            let row = unsafe { object_logical_section(input, headers, index, &mut mapped) };
            row.error.check()?;
            std::str::from_utf8(unsafe { row.value.name.borrowed() })?;
            Ok(row.value)
        })
    }
    /// 从库成员借用公开接口与合同。完整对象负责代码、重定位和诊断材料。
    pub(crate) fn interface(&self) -> Result<(String, Kind, BTreeMap<String, Vec<u8>>)> {
        let mut metadata = None;
        for row in self.section_rows() {
            let row = row?;
            if metadata.is_none() && unsafe { row.name.borrowed() } == META.as_bytes() {
                metadata = Some(unsafe { row.bytes.borrowed() });
            }
        }
        let (name, kind, mut bindings, _, _) = read_metadata(
            metadata.ok_or("缺少 Blink 目标文件元数据")?, None, false,
        )?;
        let input = ReadBytes::new(self.bytes.as_ref());
        let table = unsafe { read_symbols(input) };
        table.error.check()?;
        let mut definitions = BTreeMap::new();
        let mut i = 0;
        while i < table.count {
            let row = unsafe { read_symbol(input, table, i) };
            row.error.check()?;
            if row.storage == 2 && row.section > 0 {
                let symbol = std::str::from_utf8(unsafe { row.name.borrowed() })?;
                let (name, contract) = bindings.remove_entry(symbol).ok_or("导出符号缺少模块接口合同")?;
                definitions.insert(name, contract);
            }
            i = row.next;
        }
        definitions.extend(bindings);
        Ok((name, kind, definitions))
    }
}
impl Reader<'_, super::ObjectBytes> {
    /// 非根资源侧库来自原编译产物；其C符号沿真实ABI恢复，不消费B公开合同。
    pub(crate) fn native(&self, name: &str) -> Result<Object> { decode_object(self, Some(name)) }
    pub(crate) fn object(&self) -> Result<Object> {
        decode_object(self, None)
    }
}
fn read_metadata(
    meta: &[u8], archive: Option<super::ObjectBytes>, keep_stack: bool,
) -> Result<(String, Kind, BTreeMap<String, Vec<u8>>, StackMetadata, Option<Archive>)> {
    let mut c = MetadataReadCursor::new(meta);
    let header = unsafe { metadata_read_header(&mut c, u64::from(FORMAT)) };
    header.error.check()?;
    let name = read_text(header.name)?;
    let kind = match header.kind {
        ReadObjectKind::Module => {
            let interface = read_text(header.interface)?;
            let requirements = (0..header.requirements).map(|_| {
                let value = unsafe { metadata_read_requirement(&mut c) };
                value.error.check()?;
                let requirement = match value.kind {
                    ReadRequirementKind::Invocation => crate::execution::Requirement::Invocation,
                    ReadRequirementKind::Domain => crate::execution::Requirement::Domain,
                    ReadRequirementKind::Host => crate::execution::Requirement::Host,
                };
                Ok((read_text(value.name)?, requirement))
            }).collect::<Result<_>>()?;
            Kind::Module { interface, requirements, stack_bytes: c.word()? }
        }
        ReadObjectKind::Application => Kind::Application { stack_bytes: header.stack as u32 },
        ReadObjectKind::Library => Kind::Library {
            prefix: read_text(header.prefix)?, header: read_text(header.header)?, binding: read_text(header.binding)?,
        },
    };
    let keep_archive = archive.is_some();
    let libraries = c.strings(keep_archive)?;
    let exports = c.strings(keep_archive)?;
    let mut resource = MetadataReadCursor::new(unsafe { c.bytes()?.borrowed() });
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
    let mut bindings = BTreeMap::new();
    for _ in 0..c.word()? {
        let value = unsafe { metadata_read_binding(&mut c) };
        value.error.check()?;
        bindings.insert(read_text(value.name)?, crate::check::canonical_function_contract(unsafe { value.value.borrowed() })?);
    }
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
            if keep_stack { stack.calls.push(call); }
        }
        if keep_stack { stack.frames.push(frame); }
    }
    let archive = archive.map(|bytes| Archive {
        bytes,
        libraries,
        exports,
        resources,
    });
    Ok((name, kind, bindings, stack, archive))
}
fn decode_object(reader: &Reader<'_, super::ObjectBytes>, native: Option<&str>) -> Result<Object> {
    let b = reader.bytes;
    let input = ReadBytes::new(b);
    let table = unsafe { read_symbols(input) };
    table.error.check()?;
    let mut symbols = vec![None; table.count];
    let mut definitions = BTreeMap::new();
    let mut i = 0;
    while i < table.count {
        let row = unsafe { read_symbol(input, table, i) };
        row.error.check()?;
        let name = std::str::from_utf8(unsafe { row.name.borrowed() })?;
        if row.storage == 3 && row.section > 0 && row.auxiliary > 0 {
            definitions.insert(row.section as usize - 1, (row.selection as u8, row.owner as u16));
        }
        symbols[i] = Some((name, row.section as i16, row.value as u32, row.storage == 2, row.function != 0));
        i = row.next;
    }
    let raw = reader.section_rows().collect::<Result<Vec<_>>>()?;
    let (name, kind, mut bindings, stack, archive) = match native {
        Some(name) => (name.to_owned(), Kind::Module { interface: String::new(), requirements: BTreeMap::new(), stack_bytes: 0 }, BTreeMap::new(), StackMetadata::default(), None),
        None => {
            let section = |name: &str| raw.iter().find(|row| unsafe { row.name.borrowed() } == name.as_bytes())
                .map(|row| unsafe { row.bytes.borrowed() });
            let archive = section(ARCHIVE).map(|bytes| {
                let start = bytes.as_ptr().addr() - b.as_ptr().addr();
                b.slice(start..start + bytes.len())
            });
            read_metadata(section(META).ok_or("缺少 Blink 目标文件元数据")?, archive, true)?
        }
    };
    let mut sections = Vec::new();
    for row in &raw {
        if row.mapped != 0 {
            sections.push(Section {
                associated: Vec::new(),
                layout: None,
                name: read_text(row.name)?,
                bytes: if row.zero_fill == 0 {
                    unsafe { row.bytes.borrowed() }.to_vec()
                } else {
                    vec![0; row.zero_fill]
                },
                flags: row.flags as u32,
                relocations: Vec::new(),
            });
        }
    }
    let rows = ObjectRows::new(&raw);
    for (old, row) in raw.iter().enumerate() {
        if row.mapped == 0 || row.flags & 0x1000 == 0 { continue; }
        let &(selection, owner) = definitions.get(&old).ok_or("COFF COMDAT节缺少定义辅助记录")?;
        let value = unsafe { object_association(rows, u64::from(selection), u64::from(owner)) };
        value.error.check()?;
        if value.present != 0 { sections[value.owner].associated.push(row.mapped - 1); }
    }
    for row in &raw {
        if row.mapped == 0 { continue; }
        let table = unsafe { read_relocations(input, row.relocations, row.relocation_count, row.flags) };
        table.error.check()?;
        for i in 0..table.count {
            let r = unsafe { read_relocation(input, table.offset, i) };
            r.error.check()?;
            if let Some(kind)=crate::native::CoffRelocationKind::of(r.kind as u16){
                kind.range(sections[row.mapped-1].bytes.len(),r.offset)?;
            }
            let &(name, section, offset, _, _) = symbols.get(r.symbol).and_then(Option::as_ref)
                .ok_or("COFF 重定位符号越界")?;
            let value = unsafe { object_resolve_target(rows, ReadBytes::new(name.as_bytes()), i64::from(section), u64::from(offset)) };
            value.error.check()?;
            let target = match value.kind {
                ObjectTargetKind::Section => Target::Section(value.section, value.value as u32),
                ObjectTargetKind::Import => Target::Import(Import { dll: read_text(value.dll)?, name: read_text(value.name)? }),
                ObjectTargetKind::Symbol => {
                    if native.is_some() { bindings.entry(name.to_owned()).or_default(); }
                    else { bindings.get(name).ok_or("未定义符号缺少模块接口合同")?; }
                    Target::Symbol(name.to_owned())
                }
            };
            sections[row.mapped - 1].relocations.push(Relocation { offset: r.offset, kind: r.kind as u16, target });
        }
    }
    let symbols = symbols
            .into_iter()
            .flatten()
            .filter(|(_, section, _, external, _)| *external && *section > 0)
            .map(|(name, section, value, _, function)| {
                let (name, contract) = if native.is_some() {
                    (name.to_owned(), Vec::new())
                } else {
                    bindings.remove_entry(name).ok_or("导出符号缺少模块接口合同")?
                };
                Ok(super::Symbol {
                    function,
                    name,
                    contract,
                    section: object_index(&raw, section as usize - 1)
                        .ok_or("导出符号引用不可装载的节")?,
                    value,
                })
            })
            .collect::<Result<_>>()?;
    Ok(Object {
        stack: stack.restore(input, rows)?,
        name,
        symbols,
        imports: bindings,
        sections,
        archive,
        kind,
    })
}

pub(super) mod facts;
