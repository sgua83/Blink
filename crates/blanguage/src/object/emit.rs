//! 应用编译产物：入口、标准输出、导入跳板和展开信息都在编译阶段生成。
use super::{Import, Relocation, Result, Section, Target, stack::{Frame, Call}};
use crate::native::{NativeCode, NativeSymbol, unwind_info};
use super::coff::facts::observations::Layout;
use std::collections::{BTreeMap, BTreeSet};
fn put32(v: &mut [u8], at: usize, n: u32) {
    v[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn put64(v: &mut [u8], at: usize, n: u64) {
    v[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn relative(v: &mut [u8], at: usize, target: usize) -> Result<()> {
    put32(v, at, i32::try_from(target as i64 - at as i64 - 4)? as u32);
    Ok(())
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
        let mut bytes = vec![0; 12];
        put32(&mut bytes, 4, end);
        sections.push(Section {
            name: ".pdata".into(), bytes, flags: 0x40301040,
            relocations: [(0, text), (4, text), (8, xdata)].into_iter()
                .map(|(offset, section)| Relocation { offset, kind: 3, target: Target::Section(section, 0) }).collect(),
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
fn jmp(v: &mut Vec<u8>, condition: Option<u8>) -> usize {
    if let Some(c) = condition {
        v.extend([0x0f, c]);
    } else {
        v.push(0xe9);
    }
    let at = v.len();
    v.extend([0; 4]);
    at
}
fn call(v: &mut Vec<u8>, patches: &mut Vec<(usize, Import)>, import: Import) {
    v.extend([0xff, 0x15]);
    let at = v.len();
    v.extend([0; 4]);
    patches.push((at, import));
}
fn print(scalar: crate::ir::Scalar) -> Result<Helper> {
    let signed = scalar.is_signed();
    let wide = scalar.size() == 8;
    let boolean = scalar == crate::ir::Scalar::Bool;
    let float = scalar.is_float();
    let saved = if signed { vec![3, 6, 7] } else { vec![3, 6] };
    let frame = if float {
        120u8
    } else if signed {
        80u8
    } else {
        88u8
    };
    let mut v = Vec::new();
    let mut operations = Vec::new();
    let mut patches = Vec::new();
    let mut failures = Vec::new();
    for &reg in &saved {
        v.push(0x50 + reg);
        let at = v.len() as u8;
        operations.push((at, vec![at, reg << 4]));
    }
    v.extend([0x48, 0x83, 0xec, frame]);
    let at = v.len() as u8;
    operations.push((at, vec![at, ((frame / 8 - 1) << 4) | 2]));
    let unwind = unwind_info(at, operations);
    if float {
        // Intrinsics receive original bits; only f32 needs promotion for C's %g.
        v.extend([0x66]);
        if scalar == crate::ir::Scalar::F64 { v.push(0x48); }
        v.extend([0x0f, 0x6e, 0xc1]);
        if scalar == crate::ir::Scalar::F32 { v.extend([0xf3, 0x0f, 0x5a, 0xc0]); }
        v.extend([0xf2, 0x0f, 0x11, 0x44, 0x24, 0x38,
            0x66, 0xc7, 0x44, 0x24, 0x40, b'C', 0,
            0xc7, 0x44, 0x24, 0x48, b'%', b'.', if scalar == crate::ir::Scalar::F64 { b'1' } else { b'9' }, if scalar == crate::ir::Scalar::F64 { b'7' } else { b'g' },
            0xc7, 0x44, 0x24, 0x4c]);
        v.extend(if scalar == crate::ir::Scalar::F64 { [b'g',10,0,0] } else { [10,0,0,0] });
        v.extend([0xb9, 4, 0, 0, 0, 0x48, 0x8d, 0x54, 0x24, 0x40]);
        call(&mut v, &mut patches, ucrt("_create_locale"));
        v.extend([0x48, 0x85, 0xc0]);
        failures.push(jmp(&mut v, Some(0x84)));
        v.extend([
            0x48, 0x89, 0xc6, 0xb9, 2, 0, 0, 0, 0x48, 0x8d, 0x54, 0x24, 0x50, 0x41, 0xb8, 32, 0, 0,
            0, 0x4c, 0x8d, 0x4c, 0x24, 0x48, 0x48, 0x89, 0x74, 0x24, 0x20, 0x48, 0x8d, 0x44, 0x24,
            0x38, 0x48, 0x89, 0x44, 0x24, 0x28,
        ]);
        call(&mut v, &mut patches, ucrt("__stdio_common_vsprintf"));
        v.extend([0x89, 0xc3, 0x48, 0x89, 0xf1]);
        call(&mut v, &mut patches, ucrt("_free_locale"));
        v.extend([0x85, 0xdb]);
        failures.push(jmp(&mut v, Some(0x88)));
        v.extend([0x48, 0x8d, 0x74, 0x24, 0x50]);
    } else if boolean {
        v.extend([0x85, 0xc9]);
        let no = jmp(&mut v, Some(0x84));
        v.extend([0xc7, 0x44, 0x24, 0x30]);
        v.extend(u32::from_le_bytes(*b"true").to_le_bytes());
        v.extend([0xc6, 0x44, 0x24, 0x34, 10, 0xbb, 5, 0, 0, 0]);
        let end = jmp(&mut v, None);
        let target = v.len();
        relative(&mut v, no, target)?;
        v.extend([0xc7, 0x44, 0x24, 0x30]);
        v.extend(u32::from_le_bytes(*b"fals").to_le_bytes());
        v.extend([0x66, 0xc7, 0x44, 0x24, 0x34, b'e', 10, 0xbb, 6, 0, 0, 0]);
        let target = v.len();
        relative(&mut v, end, target)?;
        v.extend([0x48, 0x8d, 0x74, 0x24, 0x30]);
    } else {
        if wide { v.push(0x48); }
        v.extend([0x89, 0xc8]);
        if signed {
            v.extend([0x31, 0xff]);
            if wide { v.push(0x48); }
            v.extend([0x85, 0xc0]);
            let positive = jmp(&mut v, Some(0x89));
            if wide { v.push(0x48); }
            v.extend([0xf7, 0xd8, 0xbf, 1, 0, 0, 0]);
            let target = v.len();
            relative(&mut v, positive, target)?;
        }
        v.extend([
            0x48, 0x8d, 0x74, 0x24, if wide { 0x48 } else { 0x40 }, 0xc6, 0x06, 10, 0xbb, 1, 0, 0, 0, 0xb9, 10, 0, 0, 0,
        ]);
        let begin = v.len();
        v.extend([0x31, 0xd2]);
        if wide { v.push(0x48); }
        v.extend([0xf7, 0xf1, 0x80, 0xc2, b'0', 0x48, 0xff, 0xce, 0x88, 0x16, 0xff, 0xc3]);
        if wide { v.push(0x48); }
        v.extend([0x85, 0xc0]);
        let back = jmp(&mut v, Some(0x85));
        relative(&mut v, back, begin)?;
        if signed {
            v.extend([0x85, 0xff]);
            let positive = jmp(&mut v, Some(0x84));
            v.extend([0x48, 0xff, 0xce, 0xc6, 0x06, b'-', 0xff, 0xc3]);
            let target = v.len();
            relative(&mut v, positive, target)?;
        }
    }
    v.extend([0xb9]);
    v.extend((-11i32).to_le_bytes());
    call(&mut v, &mut patches, kernel("GetStdHandle"));
    v.extend([
        0x48, 0x89, 0xc1, 0x48, 0x89, 0xf2, 0x41, 0x89, 0xd8, 0x4c, 0x8d, 0x4c, 0x24, 0x2c, 0x48,
        0xc7, 0x44, 0x24, 0x20, 0, 0, 0, 0,
    ]);
    call(&mut v, &mut patches, kernel("WriteFile"));
    v.extend([0x85, 0xc0]);
    failures.push(jmp(&mut v, Some(0x84)));
    v.extend([0x39, 0x5c, 0x24, 0x2c, 0x0f, 0x94, 0xc0, 0x0f, 0xb6, 0xc0]);
    let finish = jmp(&mut v, None);
    let target = v.len();
    for failed in failures {
        relative(&mut v, failed, target)?;
    }
    v.extend([0x31, 0xc0]);
    let target = v.len();
    relative(&mut v, finish, target)?;
    v.extend([0x48, 0x83, 0xc4, frame]);
    for &reg in saved.iter().rev() {
        v.push(0x58 + reg);
    }
    v.push(0xc3);
    Ok(Helper {
        local: u32::from(frame) + saved.len() as u32 * 8,
        bytes: v,
        patches,
        unwind,
    })
}
fn thunk(text: &mut Vec<u8>, patches: &mut Vec<(usize, Import)>, import: Import) {
    text.extend([0xff, 0x25]);
    let at = text.len();
    text.extend([0; 4]);
    patches.push((at, import));
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
        let mut bytes = vec![0x48, 0x83, 0xec, 0x28, 0xe8, 0, 0, 0, 0, 0x89, 0xc1];
        let mut patches = Vec::new();
        call(&mut bytes, &mut patches, kernel("ExitProcess"));
        let end = u32::try_from(bytes.len())?;
        let calls = std::iter::once(Call::Reference { offset: 5, tail: false }).chain(patches.iter().map(|(offset, _)| Call::Reference { offset: *offset, tail: false })).collect();
        let entry = function(&mut sections, &mut frames, "mainCRTStartup".into(), 40, calls, bytes, Some((end, unwind_info(4, vec![(4, vec![4, 0x42])]))), Some(Layout::Generated));
        let calls = native.calls.into_iter().map(|reference| reference.map_or(Call::Indirect, |offset| Call::Reference { offset, tail: false })).collect();
        let main = function(&mut sections, &mut frames, format!("entry {name}"), native.stack_bytes, calls, native.bytes, Some((native.function_end.try_into()?, native.unwind)), native.tail_regions.map(Layout::Function));
        symbols.insert("main".into(), (main, 0));
        sections[entry].relocations.push(Relocation { offset: 5, kind: 4, target: Target::Section(main, 0) });
        sections[entry].relocations.extend(patches.into_iter()
            .map(|(offset, import)| Relocation { offset, kind: 4, target: Target::Import(import) }));
        images.push((main, native.relocations));
    }
    let mut targets = BTreeMap::new();
    for (name, code) in defined {
        let calls = code.calls.into_iter().map(|reference| reference.map_or(Call::Indirect, |offset| Call::Reference { offset, tail: false })).collect();
        let text = function(&mut sections, &mut frames, name.clone(), code.stack_bytes, calls, code.bytes, Some((code.function_end.try_into()?, code.unwind)), code.tail_regions.map(Layout::Function));
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
                thunk(&mut bytes, &mut patches, Import {
                    dll: name.library.clone(),
                    name: if name.library == crate::application::RUNTIME_LIBRARY
                        && (name.symbol.starts_with("blink_allocate_") || name.symbol.starts_with("blink_run_")
                            || name.symbol.starts_with("blink_complete_")) {
                        format!("{prefix}_{}", name.symbol.strip_prefix("blink_").unwrap())
                    } else { name.symbol.clone() },
                });
                let calls = patches.iter().map(|(offset, _)| Call::Reference { offset: *offset, tail: true }).collect();
                function(&mut sections, &mut frames, format!("{op:?}"), 0, calls, bytes, None, Some(Layout::Generated))
            }
            NativeSymbol::Print(scalar) => {
                let helper = print(*scalar)?;
                patches = helper.patches;
                let end = u32::try_from(helper.bytes.len())?;
                let calls = patches.iter().map(|(offset, _)| Call::Reference { offset: *offset, tail: false }).collect();
                function(&mut sections, &mut frames, format!("{op:?}"), helper.local, calls, helper.bytes, Some((end, helper.unwind)), Some(Layout::Generated))
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
