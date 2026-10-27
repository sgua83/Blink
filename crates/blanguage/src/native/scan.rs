//! 原生数值独占扫描：一次遍历累计各分量，范围前缀由同一执行框架传播。
use super::*;
pub fn emit_scan_add(program: &Program, scan: &Scan) -> NativeCode {
    emit_scan(program, scan, true)
}
pub(super) fn emit_scan(program: &Program, scan: &Scan, add: bool) -> NativeCode {
    match scan.head_word {
        Some(head) => emit_segmented(program, scan, head, add),
        None => emit(program, scan, add),
    }
}
pub(super) fn emit(program: &Program, scan: &Scan, add: bool) -> NativeCode {
    let r = &scan.values;
    let float = scan.ty.components[0].0 == Scalar::F32;
    let float_opcode = match scan.operator {
        CollectiveOperator::Min => 0x5d,
        CollectiveOperator::Max => 0x5f,
        _ => 0x58,
    };
    let mut e = Emitter {
        code: Vec::new(),
        relocations: Vec::new(),
        literals: std::collections::BTreeMap::new(),
        stage: &program.stages[0],
        program,
        breaks: Vec::new(),
        exits: Vec::new(),
        continues: Vec::new(),
        allocation: Allocation::default(),
        frame: Frame::default(),
    };
    if add {
        e.bytes(&[0x48, 0x8b, 0x91]);
        e.u32(program.buffers.len() as u32 * 8);
    }
    e.bytes(&[0x4c, 0x8b, 0x91]);
    e.u32(if add { r.target } else { r.source } as u32 * 8);
    if !add {
        e.bytes(&[0x4c, 0x8b, 0x99]);
        e.u32(r.target as u32 * 8);
    }
    e.bytes(&[0x44, 0x89, 0xc8]);
    e.scale_index(
        0,
        program.buffers[if add { r.target } else { r.source }].stride,
    );
    e.bytes(&[0x49, 0x01, 0xc2]);
    if !add {
        e.bytes(&[0x44, 0x89, 0xc8]);
        e.scale_index(0, program.buffers[r.target].stride);
        e.bytes(&[0x49, 0x01, 0xc3]);
    }
    e.bytes(&[0x45, 0x29, 0xc8]);
    // r8d 是剩余数量；浮点分量使用易失 xmm0–3，传播临时值使用 xmm4。
    // 主扫描的 eax/ecx/edx/r9d 各累计一个整数分量。
    // 传播内核从 rdx 取得范围前缀，随后保留 eax/ecx/r9d/r11d，edx 留作极值临时值。
    let registers = if add { [0u8, 1, 9, 11] } else { [0u8, 1, 2, 9] };
    for (component, _) in scan.ty.components.iter().enumerate() {
        let reg = registers[component];
        if float {
            if add {
                e.bytes(&[0xf3, 0x0f, 0x10, 0x82 + component as u8 * 8]);
                e.u32(component as u32 * 4);
            } else if scan.operator == CollectiveOperator::Sum {
                e.bytes(&[0x0f, 0x57, 0xc0 + component as u8 * 9]);
            } else {
                e.bytes(&[0xb8]);
                e.u32(scan.operator.identity(Scalar::F32));
                e.bytes(&[0x66, 0x0f, 0x6e, 0xc0 + component as u8 * 8]);
            }
        } else if add {
            e.bytes(&[
                0x40 | if reg >= 8 { 4 } else { 0 },
                0x8b,
                0x82 + (reg & 7) * 8,
            ]);
            e.u32(component as u32 * 4);
        } else {
            if reg >= 8 {
                e.bytes(&[0x41]);
            }
            e.bytes(&[0xb8 + (reg & 7)]);
            e.u32(scan.operator.identity(scan.ty.components[0].0));
        }
    }
    if add && float && scan.operator != CollectiveOperator::Sum {
        e.bytes(&[0x45, 0x89, 0xc1]); // r9d 保存范围实际长度，首项没有局部前缀。
    }
    let begin = e.code.len();
    e.bytes(&[0x45, 0x85, 0xc0]);
    let end = e.jump(Some(0x84));
    for (component, (scalar, offset)) in scan.ty.components.iter().enumerate() {
        if float {
            if add {
                // prefix + 本范围的局部前缀；prefix 本身留给该范围后续记录。
                e.bytes(&[0x0f, 0x28, 0xe0 + component as u8]);
                let first = if scan.operator != CollectiveOperator::Sum {
                    e.bytes(&[0x45, 0x39, 0xc8]);
                    Some(e.jump(Some(0x84)))
                } else {
                    None
                };
                e.bytes(&[0xf3, 0x41, 0x0f, float_opcode, 0xa2]);
                e.u32(r.target_word * 4 + offset);
                if let Some(first) = first {
                    e.patch(first, e.code.len());
                }
                e.bytes(&[0xf3, 0x41, 0x0f, 0x11, 0xa2]);
                e.u32(r.target_word * 4 + offset);
            } else {
                e.bytes(&[0xf3, 0x41, 0x0f, 0x11, 0x83 + component as u8 * 8]);
                e.u32(r.target_word * 4 + offset);
                e.bytes(&[0xf3, 0x41, 0x0f, float_opcode, 0x82 + component as u8 * 8]);
                e.u32(r.source_word * 4 + offset);
            }
            continue;
        }
        let reg = registers[component];
        let opcode = match scan.operator {
            CollectiveOperator::Sum => Some(if add { 0x01 } else { 0x03 }),
            CollectiveOperator::BitAnd => Some(if add { 0x21 } else { 0x23 }),
            CollectiveOperator::BitOr => Some(if add { 0x09 } else { 0x0b }),
            CollectiveOperator::BitXor => Some(if add { 0x31 } else { 0x33 }),
            CollectiveOperator::Min | CollectiveOperator::Max => None,
        };
        if !add {
            e.bytes(&[
                0x41 | if reg >= 8 { 4 } else { 0 },
                0x89,
                0x83 + (reg & 7) * 8,
            ]);
            e.u32(r.target_word * 4 + offset);
        }
        if let Some(opcode) = opcode {
            e.bytes(&[
                0x41 | if reg >= 8 { 4 } else { 0 },
                opcode,
                0x82 + (reg & 7) * 8,
            ]);
            e.u32(if add {
                r.target_word * 4 + offset
            } else {
                r.source_word * 4 + offset
            });
        } else {
            let min = scan.operator == CollectiveOperator::Min;
            let condition = match (*scalar == Scalar::I32, min) {
                (false, true) => 0x47,
                (true, true) => 0x4f,
                (false, false) => 0x42,
                (true, false) => 0x4c,
            };
            if add {
                e.bytes(&[0x41, 0x8b, 0x92]);
                e.u32(r.target_word * 4 + offset);
                let rex = 0x40 | if reg >= 8 { 1 } else { 0 };
                e.bytes(&[rex, 0x3b, 0xd0 + (reg & 7)]);
                e.bytes(&[rex, 0x0f, condition, 0xd0 + (reg & 7)]);
                e.bytes(&[0x41, 0x89, 0x92]);
                e.u32(r.target_word * 4 + offset);
            } else {
                let rex = 0x41 | if reg >= 8 { 4 } else { 0 };
                e.bytes(&[rex, 0x3b, 0x82 + (reg & 7) * 8]);
                e.u32(r.source_word * 4 + offset);
                e.bytes(&[rex, 0x0f, condition, 0x82 + (reg & 7) * 8]);
                e.u32(r.source_word * 4 + offset);
            }
        }
    }
    e.bytes(&[0x49, 0x81, 0xc2]);
    e.u32(program.buffers[if add { r.target } else { r.source }].stride);
    if !add {
        e.bytes(&[0x49, 0x81, 0xc3]);
        e.u32(program.buffers[r.target].stride);
    }
    e.bytes(&[0x41, 0xff, 0xc8]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    if !add {
        // 与归约同用第五个参数交付分量结果；普通扫描不再依赖单个 eax 返回值。
        e.bytes(&[0x4c, 0x8b, 0x54, 0x24, 0x28]);
        for (component, _) in scan.ty.components.iter().enumerate() {
            let reg = registers[component];
            if float {
                e.bytes(&[0xf3, 0x41, 0x0f, 0x11, 0x82 + component as u8 * 8]);
            } else {
                e.bytes(&[
                    0x41 | if reg >= 8 { 4 } else { 0 },
                    0x89,
                    0x82 + (reg & 7) * 8,
                ]);
            }
            e.u32(component as u32 * 4);
        }
    }
    e.bytes(&[0xc3]);
    e.finish(0)
}
fn emit_segmented(program: &Program, scan: &Scan, head: u32, add: bool) -> NativeCode {
    let r = &scan.values;
    let scalar = scan.ty.components[0].0;
    let float_opcode = match scan.operator {
        CollectiveOperator::Min => 0x5d,
        CollectiveOperator::Max => 0x5f,
        _ => 0x58,
    };
    let mut e = Emitter {
        code: Vec::new(),
        relocations: Vec::new(),
        literals: std::collections::BTreeMap::new(),
        stage: &program.stages[0],
        program,
        breaks: Vec::new(),
        exits: Vec::new(),
        continues: Vec::new(),
        allocation: Allocation::default(),
        frame: Frame::default(),
    };
    if add {
        e.bytes(&[0x48, 0x8b, 0x91]);
        e.u32(program.buffers.len() as u32 * 8);
    }
    // r10 = source，r11 = target；均为易失寄存器，不改变 Win64 非易失状态。
    for (binding, rex) in [(r.source, 0x91), (r.target, 0x99)] {
        e.bytes(&[0x4c, 0x8b, rex]);
        e.u32(binding as u32 * 8);
    }
    for (binding, pointer) in [(r.source, 0xc2), (r.target, 0xc3)] {
        e.bytes(&[0x44, 0x89, 0xc8]);
        e.scale_index(0, program.buffers[binding].stride);
        e.bytes(&[0x49, 0x01, pointer]);
    }
    e.bytes(&[0x45, 0x29, 0xc8]); // r8d = 剩余数量；begin 已消费。
    for (component, _) in scan.ty.components.iter().enumerate() {
        if add {
            e.bytes(&[0xf3, 0x0f, 0x10, 0x82 + component as u8 * 8]);
            e.u32(component as u32 * 4);
        } else {
            e.bytes(&[0xb8]);
            e.u32(scan.operator.identity(scalar));
            e.bytes(&[0x66, 0x0f, 0x6e, 0xc0 + component as u8 * 8]);
        }
    }
    if add && scalar == Scalar::F32 && scan.operator != CollectiveOperator::Sum {
        e.bytes(&[0x45, 0x89, 0xc1]);
    } else {
        e.bytes(&[0x45, 0x31, 0xc9]); // r9d = 范围是否含段起点
    }
    let begin = e.code.len();
    e.bytes(&[0x45, 0x85, 0xc0]);
    let end = e.jump(Some(0x84));
    e.bytes(&[0x41, 0x83, 0xba]);
    e.u32(head * 4);
    e.bytes(&[0]);
    let no_head = e.jump(Some(0x84));
    let head_end = if add {
        // 段头与其后的局部扫描已经完整，只修正范围内第一个段头之前的值。
        Some(e.jump(None))
    } else {
        for (component, _) in scan.ty.components.iter().enumerate() {
            e.bytes(&[0xb8]);
            e.u32(scan.operator.identity(scalar));
            e.bytes(&[0x66, 0x0f, 0x6e, 0xc0 + component as u8 * 8]);
        }
        e.bytes(&[0x41, 0xb9]);
        e.u32(1);
        None
    };
    e.patch(no_head, e.code.len());
    // xmm0–3 持有各分量；eax 是整数合并临时值，xmm4 是浮点传播临时值。
    for (component, (_, offset)) in scan.ty.components.iter().enumerate() {
        let c = component as u8;
        if scalar == Scalar::F32 {
            if add {
                e.bytes(&[0x0f, 0x28, 0xe0 + c]);
                let first = if scan.operator != CollectiveOperator::Sum {
                    e.bytes(&[0x45, 0x39, 0xc8]);
                    Some(e.jump(Some(0x84)))
                } else {
                    None
                };
                e.bytes(&[0xf3, 0x41, 0x0f, float_opcode, 0xa3]);
                e.u32(r.target_word * 4 + offset);
                if let Some(first) = first {
                    e.patch(first, e.code.len());
                }
                e.bytes(&[0xf3, 0x41, 0x0f, 0x11, 0xa3]);
                e.u32(r.target_word * 4 + offset);
            } else {
                e.bytes(&[0xf3, 0x41, 0x0f, 0x11, 0x83 + c * 8]);
                e.u32(r.target_word * 4 + offset);
                e.bytes(&[0xf3, 0x41, 0x0f, float_opcode, 0x82 + c * 8]);
                e.u32(r.source_word * 4 + offset);
            }
            continue;
        }
        e.bytes(&[0x66, 0x0f, 0x7e, 0xc0 + c * 8]);
        if !add {
            e.bytes(&[0x41, 0x89, 0x83]);
            e.u32(r.target_word * 4 + offset);
        }
        let memory = if add { 0x83 } else { 0x82 };
        let word = if add { r.target_word } else { r.source_word } * 4 + offset;
        let opcode = match scan.operator {
            CollectiveOperator::Sum => Some(0x03),
            CollectiveOperator::BitAnd => Some(0x23),
            CollectiveOperator::BitOr => Some(0x0b),
            CollectiveOperator::BitXor => Some(0x33),
            CollectiveOperator::Min | CollectiveOperator::Max => None,
        };
        if let Some(opcode) = opcode {
            e.bytes(&[0x41, opcode, memory]);
            e.u32(word);
        } else {
            e.bytes(&[0x41, 0x3b, memory]);
            e.u32(word);
            let condition = match (
                scalar == Scalar::I32,
                scan.operator == CollectiveOperator::Min,
            ) {
                (false, true) => 0x47,
                (true, true) => 0x4f,
                (false, false) => 0x42,
                (true, false) => 0x4c,
            };
            e.bytes(&[0x41, 0x0f, condition, memory]);
            e.u32(word);
        }
        if add {
            e.bytes(&[0x41, 0x89, 0x83]);
            e.u32(r.target_word * 4 + offset);
        } else {
            e.bytes(&[0x66, 0x0f, 0x6e, 0xc0 + c * 8]);
        }
    }
    for (binding, pointer) in [(r.source, 0xc2), (r.target, 0xc3)] {
        e.bytes(&[0x49, 0x81, pointer]);
        e.u32(program.buffers[binding].stride);
    }
    e.bytes(&[0x41, 0xff, 0xc8]);
    let back = e.jump(None);
    e.patch(back, begin);
    let finish = e.code.len();
    e.patch(end, finish);
    if let Some(end) = head_end {
        e.patch(end, finish);
    }
    if !add {
        // 与普通扫描共用第五个参数：紧密数值分量，随后是段起点位。
        e.bytes(&[0x4c, 0x8b, 0x54, 0x24, 0x28]);
        for (component, _) in scan.ty.components.iter().enumerate() {
            e.bytes(&[0xf3, 0x41, 0x0f, 0x11, 0x82 + component as u8 * 8]);
            e.u32(component as u32 * 4);
        }
        e.bytes(&[0x45, 0x89, 0x8a]);
        e.u32(scan.ty.components.len() as u32 * 4);
    }
    e.bytes(&[0xc3]);
    e.finish(0)
}
