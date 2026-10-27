//! 应用编译产物：入口、标准输出、导入跳板和展开信息都在编译阶段生成。
use super::{Import, Relocation, Result, Section, Target};
use crate::native::{ApplicationCode, NativeIntrinsic, NativeSymbol, application_unwind};
use std::collections::{BTreeMap, BTreeSet};
fn align(n: usize, a: usize) -> usize {
    n.div_ceil(a) * a
}
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
fn math(op: NativeIntrinsic) -> Option<Import> {
    let name = match op {
        NativeIntrinsic::Sin => "sinf",
        NativeIntrinsic::Cos => "cosf",
        NativeIntrinsic::Floor => "floorf",
        NativeIntrinsic::Exp => "expf",
        NativeIntrinsic::Exp2 => "exp2f",
        NativeIntrinsic::Log => "logf",
        NativeIntrinsic::Log2 => "log2f",
        NativeIntrinsic::Pow => "powf",
        NativeIntrinsic::PrintU32
        | NativeIntrinsic::PrintI32
        | NativeIntrinsic::PrintBool
        | NativeIntrinsic::PrintF32 => {
            return None;
        }
    };
    Some(ucrt(name))
}
struct Function {
    begin: usize,
    end: usize,
    unwind: Vec<u8>,
}
struct Helper {
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
fn print(op: NativeIntrinsic) -> Result<Helper> {
    let signed = op == NativeIntrinsic::PrintI32;
    let boolean = op == NativeIntrinsic::PrintBool;
    let float = op == NativeIntrinsic::PrintF32;
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
    let unwind = application_unwind(at, operations);
    if float {
        // intrinsic 的参数仍是原始 32 位；只为 C 的 %g 参数提升到 double。
        v.extend([
            0x66, 0x0f, 0x6e, 0xc1, 0xf3, 0x0f, 0x5a, 0xc0, 0xf2, 0x0f, 0x11, 0x44, 0x24, 0x38,
            0x66, 0xc7, 0x44, 0x24, 0x40, b'C', 0, 0xc7, 0x44, 0x24, 0x48, b'%', b'.', b'9', b'g',
            0x66, 0xc7, 0x44, 0x24, 0x4c, 10, 0, 0xb9, 4, 0, 0, 0, 0x48, 0x8d, 0x54, 0x24, 0x40,
        ]);
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
        v.extend([0x89, 0xc8]);
        if signed {
            v.extend([0x31, 0xff, 0x85, 0xc0]);
            let positive = jmp(&mut v, Some(0x89));
            v.extend([0xf7, 0xd8, 0xbf, 1, 0, 0, 0]);
            let target = v.len();
            relative(&mut v, positive, target)?;
        }
        v.extend([
            0x48, 0x8d, 0x74, 0x24, 0x40, 0xc6, 0x06, 10, 0xbb, 1, 0, 0, 0, 0xb9, 10, 0, 0, 0,
        ]);
        let begin = v.len();
        v.extend([
            0x31, 0xd2, 0xf7, 0xf1, 0x80, 0xc2, b'0', 0x48, 0xff, 0xce, 0x88, 0x16, 0xff, 0xc3,
            0x85, 0xc0,
        ]);
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
    native: ApplicationCode,
    defined: Vec<(String, ApplicationCode)>,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
) -> Result<Vec<Section>> {
    Ok(functions(Some(native), defined, imports, prefix)?.0)
}
pub(crate) fn module(
    defined: Vec<(String, ApplicationCode)>,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
) -> Result<(Vec<Section>, BTreeMap<String, u32>)> {
    functions(None, defined, imports, prefix)
}
fn functions(
    native: Option<ApplicationCode>,
    defined: Vec<(String, ApplicationCode)>,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
) -> Result<(Vec<Section>, BTreeMap<String, u32>)> {
    let mut text = Vec::new();
    let mut patches = Vec::new();
    let mut functions = Vec::new();
    let mut images = Vec::new();
    let mut symbols = BTreeMap::new();
    if let Some(native) = native {
        text.extend([0x48, 0x83, 0xec, 0x28, 0xe8, 0, 0, 0, 0, 0x89, 0xc1]);
        call(&mut text, &mut patches, kernel("ExitProcess"));
        functions.push(Function {
            begin: 0,
            end: text.len(),
            unwind: application_unwind(4, vec![(4, vec![4, 0x42])]),
        });
        text.resize(align(text.len(), 16), 0x90);
        let main = text.len();
        relative(&mut text, 5, main)?;
        text.extend(&native.code.bytes);
        functions.push(Function {
            begin: main,
            end: main + native.function_end as usize,
            unwind: native.unwind,
        });
        images.push((main, native.code));
    }
    let mut targets = BTreeMap::new();
    for (name, function) in defined {
        text.resize(align(text.len(), 16), 0x90);
        let begin = text.len();
        symbols.insert(name.clone(), u32::try_from(begin)?);
        targets.insert(NativeSymbol::Function(name), begin);
        text.extend(&function.code.bytes);
        functions.push(Function {
            begin,
            end: begin + function.function_end as usize,
            unwind: function.unwind,
        });
        images.push((begin, function.code));
    }
    let operations: BTreeSet<_> = images
        .iter()
        .flat_map(|(_, code)| &code.relocations)
        .map(|r| r.target.clone())
        .collect();
    for op in operations {
        if let NativeSymbol::Function(name) = &op {
            if !targets.contains_key(&op) && !imports.contains_key(name) {
                return Err(format!("缺少模块函数接口 {name}").into());
            }
            continue;
        }
        text.resize(align(text.len(), 16), 0x90);
        let begin = text.len();
        targets.insert(op.clone(), begin);
        match op {
            NativeSymbol::Function(_) => unreachable!(),
            NativeSymbol::External(name) => thunk(
                &mut text,
                &mut patches,
                Import {
                    dll: name.library.clone(),
                    name: if name.library == crate::application::RUNTIME_LIBRARY
                        && (name.symbol.starts_with("blink_allocate_")
                            || name.symbol.starts_with("blink_run_"))
                    {
                        format!("{prefix}_{}", name.symbol.strip_prefix("blink_").unwrap())
                    } else {
                        name.symbol
                    },
                },
            ),
            NativeSymbol::Intrinsic(op) => {
                if let Some(import) = math(op) {
                    thunk(&mut text, &mut patches, import);
                } else {
                    let helper = print(op)?;
                    text.extend(helper.bytes);
                    for (at, import) in helper.patches {
                        patches.push((begin + at, import));
                    }
                    functions.push(Function {
                        begin,
                        end: text.len(),
                        unwind: helper.unwind,
                    });
                }
            }
        }
    }
    let mut locations = Vec::new();
    for (begin, code) in images {
        for relocation in code.relocations {
            let at = begin + relocation.offset;
            let target = if let NativeSymbol::Function(name) = &relocation.target
                && !targets.contains_key(&relocation.target)
            {
                Target::Symbol(name.clone(), imports[name].clone())
            } else {
                put64(&mut text, at, targets[&relocation.target] as u64);
                Target::Section(0, 0)
            };
            locations.push(Relocation {
                offset: at,
                kind: 1,
                target,
            });
        }
    }

    for (at, import) in patches {
        locations.push(Relocation {
            offset: at,
            kind: 4,
            target: Target::Import(import),
        });
    }
    functions.sort_by_key(|f| f.begin);
    let mut pdata = vec![0; functions.len() * 12];
    let mut xdata = Vec::new();
    let mut relocations = Vec::new();
    for (i, f) in functions.iter().enumerate() {
        xdata.resize(align(xdata.len(), 4), 0);
        put32(&mut pdata, i * 12, f.begin as u32);
        put32(&mut pdata, i * 12 + 4, f.end as u32);
        put32(&mut pdata, i * 12 + 8, xdata.len() as u32);
        xdata.extend(&f.unwind);
        for (offset, section) in [(0, 0), (4, 0), (8, 2)] {
            relocations.push(Relocation {
                offset: i * 12 + offset,
                kind: 3,
                target: Target::Section(section, 0),
            });
        }
    }
    Ok((
        vec![
            Section {
                name: ".text".into(),
                bytes: text,
                flags: 0x60500020,
                relocations: locations,
            },
            Section {
                name: ".pdata".into(),
                bytes: pdata,
                flags: 0x40300040,
                relocations,
            },
            Section {
                name: ".xdata".into(),
                bytes: xdata,
                flags: 0x40300040,
                relocations: Vec::new(),
            },
        ],
        symbols,
    ))
}
