//! 应用编译产物：入口、标准输出、导入跳板和展开信息都在编译阶段生成。
use super::{Import, Relocation, Result, Section, Target, stack::{Frame, Call}};
use crate::native::{NativeCode, NativeSymbol, unwind_info, UnwindOperation};
use crate::builtin_types::ScalarEncoding;
use super::coff::facts::observations::Layout;
use std::collections::{BTreeMap, BTreeSet};
#[repr(C)]
struct SymbolEncoding {
    length: u64,
    field: u64,
}

#[repr(C)]
struct ApplicationEntry {
    length: u32,
    frame: u32,
    prolog: u32,
    main_field: u32,
    exit_field: u32,
}

#[repr(C)]
struct PrintEncoding {
    length: u32,
    unwind_length: u32,
    local: u32,
    create_locale: u32,
    format: u32,
    free_locale: u32,
    stdout_handle: u32,
    write: u32,
}

#[repr(C)]
struct RuntimeFunctionFields {
    begin: u32,
    end: u32,
    unwind: u32,
    relocation: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.unwind::runtime_function"]
    fn runtime_function(end: u32, output: *mut u8) -> RuntimeFunctionFields;
    #[link_name = "bootstrap.x64_instruction::print_helper"]
    fn print_helper(scalar: ScalarEncoding, output: *mut u8, unwind: *mut u8) -> PrintEncoding;
    #[link_name = "bootstrap.x64_instruction::import_transfer"]
    fn import_transfer(tail: u32, output: *mut u8) -> SymbolEncoding;
    #[link_name = "bootstrap.x64_instruction::application_entry"]
    fn application_entry(output: *mut u8) -> ApplicationEntry;
}

fn put64(v: &mut [u8], at: usize, n: u64) {
    v[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn kernel(name: &'static str) -> Import {
    Import {
        dll: "KERNEL32.dll".into(),
        name: name.into(),
    }
}
fn ucrt(name: &'static str) -> Import {
    Import {
        dll: "ucrtbase.dll".into(),
        name: name.into(),
    }
}
fn function(sections: &mut Vec<Section>, frames: &mut Vec<Frame>, name: String, local: u32, calls: Vec<Call>, bytes: Vec<u8>, unwind: Option<(u32, Vec<u8>)>, layout: Option<Layout>) -> usize {
    let text = sections.len();
    frames.push(Frame { name, coff: Some((text, 0)), local, calls });
    sections.push(Section {
        name: ".text".into(), bytes, flags: 0x60501020,
        relocations: Vec::new(), associated: Vec::new(), layout,
    });
    if let Some((end, unwind)) = unwind {
        let pdata = text + 1;
        let xdata = text + 2;
        sections[text].associated = vec![pdata, xdata];
        let mut bytes = Vec::with_capacity(12);
        let fields = unsafe {
            let fields = runtime_function(end, bytes.as_mut_ptr());
            bytes.set_len(12);
            fields
        };
        sections.push(Section {
            name: ".pdata".into(), bytes, flags: 0x40301040,
            relocations: [(fields.begin, text), (fields.end, text), (fields.unwind, xdata)].into_iter()
                .map(|(offset, section)| Relocation {
                    offset: offset as usize,
                    kind: fields.relocation as u16,
                    target: Target::Section(section, 0),
                }).collect(),
            associated: Vec::new(), layout: None,
        });
        sections.push(Section {
            name: ".xdata".into(), bytes: unwind, flags: 0x40301040,
            relocations: Vec::new(), associated: Vec::new(), layout: None,
        });
    }
    text
}
struct Helper {
    local: u32,
    bytes: Vec<u8>,
    patches: Vec<(usize, Import)>,
    unwind: Vec<u8>,
}
fn transfer(v: &mut Vec<u8>, patches: &mut Vec<(usize, Import)>, import: Import, tail: bool) {
    let mut bytes = [0; 6];
    let encoded = unsafe { import_transfer(u32::from(tail), bytes.as_mut_ptr()) };
    patches.push((v.len() + encoded.field as usize, import));
    v.extend_from_slice(&bytes[..encoded.length as usize]);
}
fn print(scalar: crate::ir::Scalar) -> Helper {
    let mut bytes = Vec::with_capacity(224);
    let mut unwind = Vec::with_capacity(12);
    let encoded = unsafe {
        let encoded = print_helper(scalar.into(), bytes.as_mut_ptr(), unwind.as_mut_ptr());
        bytes.set_len(encoded.length as usize);
        unwind.set_len(encoded.unwind_length as usize);
        encoded
    };
    let mut patches = Vec::with_capacity(if encoded.create_locale == 0 { 2 } else { 5 });
    if encoded.create_locale != 0 {
        patches.extend([
            (encoded.create_locale as usize, ucrt("_create_locale")),
            (encoded.format as usize, ucrt("__stdio_common_vsprintf")),
            (encoded.free_locale as usize, ucrt("_free_locale")),
        ]);
    }
    patches.extend([
        (encoded.stdout_handle as usize, kernel("GetStdHandle")),
        (encoded.write as usize, kernel("WriteFile")),
    ]);
    Helper { local: encoded.local, bytes, patches, unwind }
}
pub(crate) fn application(
    native: NativeCode,
    defined: Vec<(String, NativeCode)>,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
    name: &str,
) -> Result<(Vec<Section>, BTreeMap<String, (usize, u32)>, Vec<Frame>)> {
    functions(Some((name.into(), native)), defined, imports, prefix)
}
pub(crate) fn module(
    defined: Vec<(String, NativeCode)>,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
) -> Result<(Vec<Section>, BTreeMap<String, (usize, u32)>, Vec<Frame>)> {
    functions(None, defined, imports, prefix)
}
fn functions(
    native: Option<(String, NativeCode)>,
    defined: Vec<(String, NativeCode)>,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
) -> Result<(Vec<Section>, BTreeMap<String, (usize, u32)>, Vec<Frame>)> {
    let mut sections = Vec::new();
    let mut frames = Vec::new();
    let mut images = Vec::new();
    let mut symbols = BTreeMap::new();
    if let Some((name, native)) = native {
        let mut bytes = vec![0; 17];
        let layout = unsafe { application_entry(bytes.as_mut_ptr()) };
        bytes.truncate(layout.length as usize);
        let calls = [layout.main_field, layout.exit_field].into_iter()
            .map(|offset| Call::Reference { offset: offset as usize, tail: false }).collect();
        let unwind = unwind_info(layout.prolog as u8, &[UnwindOperation::allocate(layout.prolog as u8, layout.frame)]);
        let entry = function(&mut sections, &mut frames, "mainCRTStartup".into(), layout.frame, calls, bytes,
            Some((layout.length, unwind)), Some(Layout::Startup));
        let calls = native.calls.into_iter().map(|reference| reference.map_or(Call::Indirect, |offset| Call::Reference { offset, tail: false })).collect();
        let main = function(&mut sections, &mut frames, format!("entry {name}"), native.stack_bytes, calls, native.bytes, Some((native.function_end.try_into()?, native.unwind)), native.tail_regions.map(|tails| Layout::Function { tails, references: native.tail_references, sources: native.sources }));
        symbols.insert("main".into(), (main, 0));
        sections[entry].relocations.extend([
            Relocation { offset: layout.main_field as usize, kind: 4, target: Target::Section(main, 0) },
            Relocation { offset: layout.exit_field as usize, kind: 4, target: Target::Import(kernel("ExitProcess")) },
        ]);
        images.push((main, native.relocations));
    }
    let mut targets = BTreeMap::new();
    for (name, code) in defined {
        let calls = code.calls.into_iter().map(|reference| reference.map_or(Call::Indirect, |offset| Call::Reference { offset, tail: false })).collect();
        let text = function(&mut sections, &mut frames, name.clone(), code.stack_bytes, calls, code.bytes, Some((code.function_end.try_into()?, code.unwind)), code.tail_regions.map(|tails| Layout::Function { tails, references: code.tail_references, sources: code.sources }));
        symbols.insert(name.clone(), (text, 0));
        targets.insert(NativeSymbol::Function(name), text);
        images.push((text, code.relocations));
    }
    let operations: BTreeSet<_> = images.iter().flat_map(|(_, relocations)| relocations)
        .map(|r| r.target.clone()).collect();
    for op in operations {
        if let NativeSymbol::Function(name) = &op {
            if !targets.contains_key(&op) && !imports.contains_key(name) {
                return Err(format!("缺少模块函数接口 {name}").into());
            }
            continue;
        }
        let mut patches = Vec::new();
        let text = match &op {
            NativeSymbol::Function(_) => unreachable!(),
            NativeSymbol::External(name) => {
                let mut bytes = Vec::new();
                transfer(&mut bytes, &mut patches, Import {
                    dll: name.library.clone(),
                    name: if name.library == crate::application::RUNTIME_LIBRARY
                        && ["blink_allocate_","blink_run_","blink_complete_"].iter().any(|prefix|
                            name.symbol.strip_prefix(prefix).is_some_and(|index|index.parse::<usize>().is_ok())) {
                        format!("{prefix}_{}", name.symbol.strip_prefix("blink_").unwrap())
                    } else { name.symbol.clone() },
                }, true);
                let calls = patches.iter().map(|(offset, _)| Call::Reference { offset: *offset, tail: true }).collect();
                function(&mut sections, &mut frames, format!("{op:?}"), 0, calls, bytes, None, Some(Layout::ImportThunk))
            }
            NativeSymbol::Print(scalar) => {
                let helper = print(*scalar);
                patches = helper.patches;
                let end = u32::try_from(helper.bytes.len())?;
                let calls = patches.iter().map(|(offset, _)| Call::Reference { offset: *offset, tail: false }).collect();
                function(&mut sections, &mut frames, format!("{op:?}"), helper.local, calls, helper.bytes, Some((end, helper.unwind)), Some(Layout::Print(*scalar)))
            }
        };
        targets.insert(op, text);
        sections[text].relocations.extend(patches.into_iter()
            .map(|(offset, import)| Relocation { offset, kind: 4, target: Target::Import(import) }));
    }
    for (section, relocations) in images {
        for relocation in relocations {
            let target = if let NativeSymbol::Function(name) = &relocation.target
                && !targets.contains_key(&relocation.target) {
                Target::Symbol(name.clone())
            } else {
                put64(&mut sections[section].bytes, relocation.offset, 0);
                Target::Section(targets[&relocation.target], 0)
            };
            sections[section].relocations.push(Relocation { offset: relocation.offset, kind: 1, target });
        }
    }
    Ok((sections, symbols, frames))
}
