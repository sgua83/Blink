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
    let scalar = scan.ty.components[0].0;
    let float = scalar.is_float();
    let wide = scalar.size() == 8;
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
    for (component, (_, offset)) in scan.ty.components.iter().enumerate() {
        let reg = registers[component];
        if float {
            if add {
                e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x0f, 0x10, 0x82 + component as u8 * 8]);
                e.u32(*offset);
            } else if scan.operator == CollectiveOperator::Sum {
                e.bytes(&[0x0f, 0x57, 0xc0 + component as u8 * 9]);
            } else {
                e.literal_width(allocation::Location::Xmm(component as u8), scan.operator.identity(scalar), wide);
            }
        } else if add {
            e.instruction((scalar.size() == 2).then_some(0x66), &[if scalar.size() == 1 { 0x8a } else { 0x8b }], wide, reg, Memory::new(2, *offset).into());
        } else {
            let rex = 0x40 | if wide { 8 } else { 0 } | if reg >= 8 { 1 } else { 0 };
            if rex != 0x40 {
                e.bytes(&[rex]);
            }
            e.bytes(&[0xb8 + (reg & 7)]);
            let identity = scan.operator.identity(scalar);
            if wide { e.bytes(&identity.to_le_bytes()); } else { e.u32(identity as u32); }
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
                e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, float_opcode, 0xa2]);
                e.u32(r.target_word + offset);
                if let Some(first) = first {
                    e.patch(first, e.code.len());
                }
                e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, 0x11, 0xa2]);
                e.u32(r.target_word + offset);
            } else {
                e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, 0x11, 0x83 + component as u8 * 8]);
                e.u32(r.target_word + offset);
                e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, float_opcode, 0x82 + component as u8 * 8]);
                e.u32(r.source_word + offset);
            }
            continue;
        }
        let reg = registers[component];
        let prefix = (scalar.size() == 2).then_some(0x66);
        let byte = u8::from(scalar.size() == 1);
        let memory = Memory::new(10, if add { r.target_word } else { r.source_word } + offset);
        let opcode = match scan.operator {
            CollectiveOperator::Sum => Some(if add { 0x01 } else { 0x03 }),
            CollectiveOperator::BitAnd => Some(if add { 0x21 } else { 0x23 }),
            CollectiveOperator::BitOr => Some(if add { 0x09 } else { 0x0b }),
            CollectiveOperator::BitXor => Some(if add { 0x31 } else { 0x33 }),
            CollectiveOperator::Min | CollectiveOperator::Max => None,
        };
        if !add {
            e.write_value_address(allocation::Location::Gpr(reg), Memory::new(11, r.target_word + offset), scalar.size());
        }
        if let Some(opcode) = opcode {
            e.instruction(prefix, &[opcode - byte], wide, reg, memory.into());
        } else {
            let condition = match (scalar.is_signed(), scan.operator == CollectiveOperator::Min) {
                (false, true) => 0x47,
                (true, true) => 0x4f,
                (false, false) => 0x42,
                (true, false) => 0x4c,
            };
            let (target, source) = if add {
                e.instruction(prefix, &[0x8b - byte], wide, 2, memory.into());
                (2, super::operands::Operand::Register(reg))
            } else { (reg, memory.into()) };
            e.instruction(prefix, &[0x3b - byte], wide, target, source);
            if scalar.size() == 1 {
                let skip = e.jump(Some((condition + 0x40) ^ 1));
                e.instruction(None, &[0x8a], false, target, source);
                e.patch(skip, e.code.len());
            } else {
                e.instruction(prefix, &[0x0f, condition], wide, target, source);
            }
            if add { e.write_value_address(allocation::Location::Gpr(target), memory, scalar.size()); }
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
        for (component, (_, offset)) in scan.ty.components.iter().enumerate() {
            let value = if float { allocation::Location::Xmm(component as u8) } else { allocation::Location::Gpr(registers[component]) };
            e.write_value_address(value, Memory::new(10, *offset), scalar.size());
        }
    }
    e.bytes(&[0xc3]);
    e.finish(0)
}
fn emit_segmented(program: &Program, scan: &Scan, head: u32, add: bool) -> NativeCode {
    let r = &scan.values;
    let scalar = scan.ty.components[0].0;
    let wide = scalar.size() == 8;
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
    for (component, (_, offset)) in scan.ty.components.iter().enumerate() {
        if add {
            e.bytes(&[0xf3, 0x0f, if wide {0x7e} else {0x10}, 0x82 + component as u8 * 8]);
            e.u32(*offset);
        } else {
            if wide {e.bytes(&[0x48]);}
            e.bytes(&[0xb8]);
            let identity = scan.operator.identity(scalar);
            if wide {e.bytes(&identity.to_le_bytes());} else {e.u32(identity as u32);}
            e.bytes(&[0x66]);
            if wide {e.bytes(&[0x48]);}
            e.bytes(&[0x0f, 0x6e, 0xc0 + component as u8 * 8]);
        }
    }
    if add && scalar.is_float() && scan.operator != CollectiveOperator::Sum {
        e.bytes(&[0x45, 0x89, 0xc1]);
    } else {
        e.bytes(&[0x45, 0x31, 0xc9]); // r9d = 范围是否含段起点
    }
    let begin = e.code.len();
    e.bytes(&[0x45, 0x85, 0xc0]);
    let end = e.jump(Some(0x84));
    e.bytes(&[0x41, 0x83, 0xba]);
    e.u32(head);
    e.bytes(&[0]);
    let no_head = e.jump(Some(0x84));
    let head_end = if add {
        // 段头与其后的局部扫描已经完整，只修正范围内第一个段头之前的值。
        Some(e.jump(None))
    } else {
        for (component, _) in scan.ty.components.iter().enumerate() {
            if wide {e.bytes(&[0x48]);}
            e.bytes(&[0xb8]);
            let identity = scan.operator.identity(scalar);
            if wide {e.bytes(&identity.to_le_bytes());} else {e.u32(identity as u32);}
            e.bytes(&[0x66]);
            if wide {e.bytes(&[0x48]);}
            e.bytes(&[0x0f, 0x6e, 0xc0 + component as u8 * 8]);
        }
        e.bytes(&[0x41, 0xb9]);
        e.u32(1);
        None
    };
    e.patch(no_head, e.code.len());
    // xmm0–3 持有各分量；eax 是整数合并临时值，xmm4 是浮点传播临时值。
    for (component, (_, offset)) in scan.ty.components.iter().enumerate() {
        let c = component as u8;
        if scalar.is_float() {
            if add {
                e.bytes(&[0x0f, 0x28, 0xe0 + c]);
                let first = if scan.operator != CollectiveOperator::Sum {
                    e.bytes(&[0x45, 0x39, 0xc8]);
                    Some(e.jump(Some(0x84)))
                } else {
                    None
                };
                e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, float_opcode, 0xa3]);
                e.u32(r.target_word + offset);
                if let Some(first) = first {
                    e.patch(first, e.code.len());
                }
                e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, 0x11, 0xa3]);
                e.u32(r.target_word + offset);
            } else {
                e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, 0x11, 0x83 + c * 8]);
                e.u32(r.target_word + offset);
                e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, float_opcode, 0x82 + c * 8]);
                e.u32(r.source_word + offset);
            }
            continue;
        }
        e.bytes(&[0x66]);
        if wide {e.bytes(&[0x48]);}
        e.bytes(&[0x0f, 0x7e, 0xc0 + c * 8]);
        if !add {
            e.write_value_address(allocation::Location::Gpr(0), Memory::new(11, r.target_word + offset), scalar.size());
        }
        let prefix = (scalar.size() == 2).then_some(0x66);
        let byte = u8::from(scalar.size() == 1);
        let memory = Memory::new(if add { 11 } else { 10 }, if add { r.target_word } else { r.source_word } + offset);
        let opcode = match scan.operator {
            CollectiveOperator::Sum => Some(0x03),
            CollectiveOperator::BitAnd => Some(0x23),
            CollectiveOperator::BitOr => Some(0x0b),
            CollectiveOperator::BitXor => Some(0x33),
            CollectiveOperator::Min | CollectiveOperator::Max => None,
        };
        if let Some(opcode) = opcode {
            e.instruction(prefix, &[opcode - byte], wide, 0, memory.into());
        } else {
            let condition = match (scalar.is_signed(), scan.operator == CollectiveOperator::Min) {
                (false, true) => 0x47,
                (true, true) => 0x4f,
                (false, false) => 0x42,
                (true, false) => 0x4c,
            };
            e.instruction(prefix, &[0x3b - byte], wide, 0, memory.into());
            if scalar.size() == 1 {
                let skip = e.jump(Some((condition + 0x40) ^ 1));
                e.instruction(None, &[0x8a], false, 0, memory.into());
                e.patch(skip, e.code.len());
            } else {
                e.instruction(prefix, &[0x0f, condition], wide, 0, memory.into());
            }
        }
        if add {
            e.write_value_address(allocation::Location::Gpr(0), Memory::new(11, r.target_word + offset), scalar.size());
        } else {
            e.bytes(&[0x66]);
            if wide {e.bytes(&[0x48]);}
            e.bytes(&[0x0f, 0x6e, 0xc0 + c * 8]);
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
        for (component, (_, offset)) in scan.ty.components.iter().enumerate() {
            e.write_value_address(allocation::Location::Xmm(component as u8), Memory::new(10, *offset), scalar.size());
        }
        e.bytes(&[0x45, 0x89, 0x8a]);
        e.u32(scan.ty.size.div_ceil(4) * 4);
    }
    e.bytes(&[0xc3]);
    e.finish(0)
}
