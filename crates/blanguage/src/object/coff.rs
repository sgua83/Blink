//! COFF 节、符号与 AMD64 重定位；Blink 元数据不替代标准重定位表。
use super::*;
use std::collections::BTreeMap;
const META: &str = ".blmeta";
const ARCHIVE: &str = ".blrt";
const BUILD: &str = ".blbuild";
const FORMAT: u32 = 36;
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
    name: BindingBytes, application: u64, sections: *mut BindingSection, section_count: usize,
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
enum ReadError { None = 0, Machine = 1, Truncated = 2, StringOffset = 4, Unterminated = 5, RelocationCount = 6 }
impl ReadError {
    fn check(self) -> Result<()> {
        match self {
            Self::None => Ok(()),
            Self::Machine => Err("需要 Windows x64 COFF 目标文件".into()),
            Self::Truncated => Err("目标文件被截断".into()),
            Self::StringOffset => Err("COFF 字符串偏移错误".into()),
            Self::Unterminated => Err("COFF 符号缺少结尾".into()),
            Self::RelocationCount => Err("COFF 重定位数量错误".into()),
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
struct ReadSection {
    name: ReadBytes, bytes: ReadBytes, flags: u64, relocations: usize, relocation_count: usize, error: ReadError,
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
    #[link_name = "bootstrap.coff_read::section"]
    fn read_section(input: ReadBytes, headers: ReadBytes, index: usize) -> ReadSection;
    #[link_name = "bootstrap.coff_read::relocations"]
    fn read_relocations(input: ReadBytes, offset: usize, count: usize, flags: u64) -> ReadRelocations;
    #[link_name = "bootstrap.coff_read::relocation"]
    fn read_relocation(input: ReadBytes, offset: usize, index: usize) -> ReadRelocation;
}
fn read_text(bytes: ReadBytes) -> Result<String> {
    Ok(std::str::from_utf8(unsafe { bytes.borrowed() })?.to_owned())
}

// One metadata cursor; B owns byte interpretation, Rust owns the recovered values.
#[repr(C)]
struct MetadataReadCursor { input: ReadBytes, position: usize }
#[repr(u32)]
#[derive(Clone, Copy)]
enum MetadataReadError { None, Word, Field, Version, Kind, Requirement, Coff, Call }
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
unsafe extern "C" {
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
    fn string(&mut self) -> Result<String> { read_text(self.bytes()?) }
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
struct MetadataInput {
    format_version: u64, name: MetadataBytes, kind: u64,
    interface: MetadataBytes, stack_bytes: u64,
    prefix: MetadataBytes, header: MetadataBytes, binding: MetadataBytes,
    requirements: *const MetadataRequirement, requirement_count: usize,
    libraries: *const MetadataBytes, library_count: usize,
    exports: *const MetadataBytes, export_count: usize,
    resource: MetadataBytes,
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
    use crate::module::codec::{Encode, Writer};
    let mut resource = Writer(Vec::new());
    if let Some(binding) = o.archive.as_ref().and_then(|a| a.resources.as_ref()) {
        true.encode(&mut resource);
        binding.prefix.encode(&mut resource);
        binding.types.encode(&mut resource);
        binding.programs.encode(&mut resource);
        binding.abi.encode(&mut resource);
    } else {
        false.encode(&mut resource);
    }
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
    let empty = MetadataBytes::new(&[]);
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
        exports: exports.as_ptr(), export_count: exports.len(), resource: MetadataBytes::new(&resource.0),
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
    if let Some(build) = build { sections.push((BUILD, build, 0x840, &[])); }
    if let Some(a) = &o.archive {
        sections.push((ARCHIVE, &a.bytes, 0x840, &[]));
    }
    let input_relocations = o.sections.iter().map(|section| {
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
    let mut binding_sections = o.sections.iter().zip(&input_relocations).map(|(section, relocations)| BindingSection {
        name: BindingBytes::new(section.name.as_bytes()), flags: u64::from(section.flags),
        relocations: relocations.as_ptr(), relocation_count: relocations.len(),
        associated: section.associated.as_ptr(), associated_count: section.associated.len(), owner: 0, symbol: 0,
    }).collect::<Vec<_>>();
    let definitions = o.symbols.iter().map(|symbol| BindingDefinition {
        name: BindingBytes::new(symbol.name.as_bytes()), section: symbol.section as u64,
        value: u64::from(symbol.value), function: u64::from(symbol.function),
    }).collect::<Vec<_>>();
    let application = matches!(o.kind, Kind::Application { .. });
    let relocation_count: usize = input_relocations.iter().map(Vec::len).sum();
    // At most two symbols per section, one per definition/reference, and the entry.
    // Each definition/reference contributes at most one shared tree node.
    let mut symbols = Vec::<CoffSymbolView>::with_capacity(2 * o.sections.len() + o.symbols.len() + relocation_count + usize::from(application));
    let mut nodes = Vec::<[u64; 9]>::with_capacity(o.symbols.len() + relocation_count);
    let mut relocations = Vec::<CoffRelocationView>::with_capacity(relocation_count);
    let binding_input = BindingInput {
        name: BindingBytes::new(o.name.as_bytes()), application: u64::from(application),
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
pub(super) fn decode(b: &[u8]) -> Result<Object> {
    Reader::new(b)?.object()
}
/// 同一COFF包络的借用视图；构建调用先消费输入记录，命中后才消费代码ABI。
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    headers: &'a [u8],
}
impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Result<Self> {
        let headers = unsafe { read_headers(ReadBytes::new(bytes)) };
        headers.error.check()?;
        Ok(Self { bytes, headers: unsafe { headers.bytes.borrowed() } })
    }
    pub(crate) fn build_inputs(&self) -> Result<Option<&'a [u8]>> {
        let input = unsafe { read_build_inputs(ReadBytes::new(self.bytes), ReadBytes::new(self.headers)) };
        input.error.check()?;
        Ok(if input.present == 0 { None } else { Some(unsafe { input.bytes.borrowed() }) })
    }
    pub(crate) fn object(&self) -> Result<Object> {
        decode_object(self)
    }
}
fn decode_object(reader: &Reader<'_>) -> Result<Object> {
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
        let name = read_text(row.name)?;
        if row.storage == 3 && row.section > 0 && row.auxiliary > 0 {
            definitions.insert(row.section as usize - 1, (row.selection as u8, row.owner as u16));
        }
        symbols[i] = Some((name, row.section as i16, row.value as u32, row.storage as u8, row.function != 0));
        i = row.next;
    }
    let headers = ReadBytes::new(reader.headers);
    let mut raw = Vec::new();
    for index in 0..reader.headers.len() / 40 {
        let row = unsafe { read_section(input, headers, index) };
        row.error.check()?;
        raw.push((
            read_text(row.name)?,
            unsafe { row.bytes.borrowed() },
            row.flags as u32,
            row.relocations,
            row.relocation_count,
        ));
    }
    let meta = raw.iter().find(|s| s.0 == META)
        .ok_or("缺少 Blink 目标文件元数据")?.1;
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
    let libraries = (0..c.word()?).map(|_| c.string()).collect::<Result<Vec<_>>>()?;
    let exports = (0..c.word()?).map(|_| c.string()).collect::<Result<Vec<_>>>()?;
    use crate::module::codec::{Decode, Reader};
    let bytes = c.bytes()?;
    let mut resource = Reader::new(unsafe { bytes.borrowed() });
    let resources = if bool::decode(&mut resource)? {
        Some(ResourceBinding {
            prefix: String::decode(&mut resource)?,
            types: u32::decode(&mut resource)?,
            programs: u32::decode(&mut resource)?,
            abi: Vec::<u8>::decode(&mut resource)?,
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
    let mut stack = Vec::new();
    for _ in 0..c.word()? {
        let value = unsafe { metadata_read_frame(&mut c) };
        value.error.check()?;
        let name = read_text(value.name)?;
        let coff = if value.has_coff == 0 { None } else { Some((value.section as usize, value.start as u32)) };
        let mut calls = Vec::new();
        for _ in 0..value.calls {
            let call = unsafe { metadata_read_call(&mut c) };
            call.error.check()?;
            calls.push(match call.kind {
                ReadCallKind::Reference => stack::Call::Reference { offset: call.offset as usize, tail: false },
                ReadCallKind::Tail => stack::Call::Reference { offset: call.offset as usize, tail: true },
                ReadCallKind::Indirect => stack::Call::Indirect,
                ReadCallKind::Unreported => stack::Call::Unreported(read_text(call.reason)?),
            });
        }
        stack.push(stack::Frame { name, coff, local: value.local as u32, calls });
    }
    let archive = raw.iter().find(|s| s.0 == ARCHIVE).map(|s| Archive {
        bytes: s.1.to_vec(),
        libraries,
        exports,
        resources,
    });
    let mut mapping = BTreeMap::new();
    let mut sections = Vec::new();
    for (i, (name, bytes, flags, _, _)) in raw.iter().enumerate() {
        if name != META && name != ARCHIVE && name != BUILD {
            mapping.insert(i, sections.len());
            sections.push(Section {
                associated: Vec::new(),
                name: name.clone(),
                bytes: bytes.to_vec(),
                flags: *flags,
                relocations: Vec::new(),
            });
        }
    }
    for (old, &index) in &mapping {
        if raw[*old].2 & 0x1000 == 0 { continue; }
        match definitions.get(old).ok_or("COFF COMDAT节缺少定义辅助记录")? {
            (1, _) => (),
            (5, owner) => {
                let owner = *mapping.get(&(*owner as usize).checked_sub(1).ok_or("COFF关联节编号错误")?)
                    .ok_or("COFF关联节不可装载")?;
                if sections[owner].flags & 0x1000 == 0 { return Err("COFF关联所有者不是COMDAT节".into()); }
                sections[owner].associated.push(index);
            }
            _ => return Err("不支持的Blink COFF COMDAT选择".into()),
        }
    }
    for (old, index) in &mapping {
        let (_, _, flags, p, count) = &raw[*old];
        let table = unsafe { read_relocations(input, *p, *count, u64::from(*flags)) };
        table.error.check()?;
        for i in 0..table.count {
            let r = unsafe { read_relocation(input, table.offset, i) };
            r.error.check()?;
            let s = symbols.get(r.symbol).and_then(Option::as_ref)
                .ok_or("COFF 重定位符号越界")?;
            let target = if s.1 > 0 {
                Target::Section(
                    *mapping
                        .get(&(s.1 as usize - 1))
                        .ok_or("COFF 引用了不可装载的节")?,
                    s.2,
                )
            } else if s.0.starts_with("__blink_import$") {
                Target::Import(read_import(&s.0)?)
            } else {
                bindings.get(&s.0).ok_or("未定义符号缺少模块接口合同")?;
                Target::Symbol(s.0.clone())
            };
            sections[*index].relocations.push(Relocation {
                offset: r.offset,
                kind: r.kind as u16,
                target,
            });
        }
    }
    let symbols = symbols
            .into_iter()
            .flatten()
            .filter(|(name, section, _, class, _)| {
                *class == 2
                    && *section > 0
                    && !(name == "mainCRTStartup" && matches!(kind, Kind::Application { .. }))
            })
            .map(|(name, section, value, _, function)| {
                Ok(super::Symbol {
                    function,
                    contract: bindings
                        .remove(&name)
                        .ok_or("导出符号缺少模块接口合同")?,
                    name,
                    section: *mapping
                        .get(&(section as usize - 1))
                        .ok_or("导出符号引用不可装载的节")?,
                    value,
                })
            })
            .collect::<Result<_>>()?;
    for frame in &mut stack {
        if let Some((section, _)) = &mut frame.coff { *section = *mapping.get(section).ok_or("栈信息引用不可装载的节")?; }
    }
    let object = Object {
        stack,
        name,
        symbols,
        imports: bindings,
        sections,
        archive,
        kind,
    };
    stack::validate(&object)?;
    Ok(object)
}
