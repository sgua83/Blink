//! COFF 节、符号与 AMD64 重定位；Blink 元数据不替代标准重定位表。
use super::*;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
const META: &str = ".blmeta";
const ARCHIVE: &str = ".blrt";
const BUILD: &str = ".blbuild";
const FORMAT: u32 = 36;
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
        if get16(bytes, 0)? != 0x8664 || get16(bytes, 16)? != 0 {
            return Err("需要 Windows x64 COFF 目标文件".into());
        }
        let headers = slice(bytes, 20, get16(bytes, 2)? as usize * 40)?;
        Ok(Self { bytes, headers })
    }
    pub(crate) fn build_inputs(&self) -> Result<Option<&'a [u8]>> {
        for header in self.headers.chunks_exact(40) {
            if &header[..8] == BUILD.as_bytes() {
                return Ok(Some(slice(self.bytes, get32(header, 20)? as usize, get32(header, 16)? as usize)?));
            }
        }
        Ok(None)
    }
    pub(crate) fn object(&self) -> Result<Object> {
        decode_object(self)
    }
}
fn decode_object(reader: &Reader<'_>) -> Result<Object> {
    let b = reader.bytes;
    let sym = get32(b, 8)? as usize;
    let count = get32(b, 12)? as usize;
    let strings_at = sym + count * 18;
    let strings = slice(b, strings_at, get32(b, strings_at)? as usize)?;
    let mut symbols = vec![None; count];
    let mut definitions = BTreeMap::new();
    let mut i = 0;
    while i < count {
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
        if r[16] == 3 && get16(r, 12)? as i16 > 0 && r[17] > 0 {
            let auxiliary = slice(b, sym + (i + 1) * 18, 18)?;
            definitions.insert(get16(r, 12)? as usize - 1, (auxiliary[14], get16(auxiliary, 12)?));
        }
        symbols[i] = Some((
            name,
            get16(r, 12)? as i16,
            get32(r, 8)?,
            r[16],
            get16(r, 14)? == 0x20,
        ));
        i += 1 + r[17] as usize;
    }
    let mut raw = Vec::new();
    for h in reader.headers.chunks_exact(40) {
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
    if tag != *b"BLCO" || read_word(&mut c)? != FORMAT {
        return Err("不支持的 Blink 目标文件版本".into());
    }
    let name = read_string(&mut c)?;
    let kind = match read_word(&mut c)? {
        2 => Kind::Module {
            interface: read_string(&mut c)?,
            requirements: (0..read_word(&mut c)?)
                .map(|_| {
                    let name = read_string(&mut c)?;
                    let requirement = match read_word(&mut c)? {
                        0 => crate::execution::Requirement::Invocation,
                        1 => crate::execution::Requirement::Domain,
                        2 => crate::execution::Requirement::Host,
                        _ => return Err("未知的模块参与要求".into()),
                    };
                    Ok((name, requirement))
                })
                .collect::<Result<_>>()?,
            stack_bytes: read_word(&mut c)?,
        },
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
    use crate::module::codec::{Decode, Reader};
    let size = read_word(&mut c)? as usize;
    let at = c.position() as usize;
    let mut resource = Reader::new(slice(meta, at, size)?);
    c.set_position((at + size) as u64);
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
    for _ in 0..read_word(&mut c)? {
        let name = read_string(&mut c)?;
        let size = read_word(&mut c)? as usize;
        let at = c.position() as usize;
        bindings.insert(name, crate::check::canonical_function_contract(slice(meta, at, size)?)?);
        c.set_position((at + size) as u64);
    }
    let mut stack = Vec::new();
    for _ in 0..read_word(&mut c)? {
        let name = read_string(&mut c)?;
        let coff = match read_word(&mut c)? {
            0 => None,
            1 => Some((read_word(&mut c)? as usize, read_word(&mut c)?)),
            _ => return Err("未知的栈代码位置种类".into()),
        };
        let local = read_word(&mut c)?;
        let mut calls = Vec::new();
        for _ in 0..read_word(&mut c)? {
            calls.push(match read_word(&mut c)? {
                tag @ (0 | 1) => stack::Call::Reference { offset: read_word(&mut c)? as usize, tail: tag == 1 },
                2 => stack::Call::Indirect,
                3 => stack::Call::Unreported(read_string(&mut c)?),
                _ => return Err("未知的栈调用引用种类".into()),
            });
        }
        stack.push(stack::Frame { name, coff, local, calls });
    }
    let archive = raw.iter().find(|s| s.0 == ARCHIVE).map(|s| Archive {
        bytes: s.1.clone(),
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
                bytes: bytes.clone(),
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
                .and_then(Option::as_ref)
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
                offset: get32(r, 0)? as usize,
                kind: get16(r, 8)?,
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
