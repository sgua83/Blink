//! 叶范围函数直接累计各数值分量，经第五个参数写出本范围的部分结果。
use super::*;
pub(super) fn emit(program: &Program, reduction: &ReductionSpec) -> NativeCode {
    let r = &reduction.fields;
    let scalar = reduction.ty.components[0].0;
    let float = scalar.is_float();
    let wide = scalar.size() == 8;
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
    // rcx=buffer pointers, r8d=end, r9d=begin；r10=当前源地址，ecx=当前元素。
    e.bytes(&[0x4c, 0x8b, 0x91]);
    e.u32(r.source as u32 * 8);
    e.bytes(&[0x44, 0x89, 0xc8]);
    e.scale_index(0, program.buffers[r.source].stride);
    e.bytes(&[0x49, 0x01, 0xc2, 0x44, 0x89, 0xc9]);
    let registers = [0u8, 2, 9, 11];
    let identity = reduction.operator.identity(scalar);
    for (component, _) in reduction.ty.components.iter().enumerate() {
        if float {
            e.literal_width(allocation::Location::Xmm(component as u8), identity, wide);
        } else {
            let register = registers[component];
            let rex = 0x40 | if wide { 8 } else { 0 } | if register >= 8 { 1 } else { 0 };
            if rex != 0x40 {
                e.bytes(&[rex]);
            }
            e.bytes(&[0xb8 + (register & 7)]);
            if wide { e.bytes(&identity.to_le_bytes()); } else { e.u32(identity as u32); }
        }
    }
    let begin = e.code.len();
    e.bytes(&[0x44, 0x39, 0xc1]);
    let end = e.jump(Some(0x83));
    for (component, (_, offset)) in reduction.ty.components.iter().enumerate() {
        if float {
            e.bytes(&[
                if wide { 0xf2 } else { 0xf3 },
                0x41,
                0x0f,
                match reduction.operator {
                    CollectiveOperator::Sum => Some(0x58),
                    CollectiveOperator::Min => Some(0x5d),
                    CollectiveOperator::Max => Some(0x5f),
                    CollectiveOperator::BitAnd
                    | CollectiveOperator::BitOr
                    | CollectiveOperator::BitXor => None,
                }
                .unwrap(),
                0x82 + component as u8 * 8,
            ]);
            e.u32(r.source_word + offset);
        } else {
            let register = registers[component];
            let prefix = (scalar.size() == 2).then_some(0x66);
            let memory = Memory::new(10, r.source_word + offset);
            let opcode = match reduction.operator {
                CollectiveOperator::Sum => Some(0x03),
                CollectiveOperator::BitAnd => Some(0x23),
                CollectiveOperator::BitOr => Some(0x0b),
                CollectiveOperator::BitXor => Some(0x33),
                CollectiveOperator::Min | CollectiveOperator::Max => None,
            };
            if let Some(opcode) = opcode {
                e.instruction(prefix, &[opcode - u8::from(scalar.size() == 1)], wide, register, memory.into());
            } else {
                let condition = match (scalar.is_signed(), reduction.operator == CollectiveOperator::Min) {
                    (false, true) => 0x47,
                    (true, true) => 0x4f,
                    (false, false) => 0x42,
                    (true, false) => 0x4c,
                };
                e.instruction(prefix, &[0x3b - u8::from(scalar.size() == 1)], wide, register, memory.into());
                if scalar.size() == 1 {
                    // CMOV没有字节形式；原字节比较之后只在实际取值时读取一个字节。
                    let skip = e.jump(Some((condition + 0x40) ^ 1));
                    e.instruction(None, &[0x8a], false, register, memory.into());
                    e.patch(skip, e.code.len());
                } else {
                    e.instruction(prefix, &[0x0f, condition], wide, register, memory.into());
                }
            }
        }
    }
    e.bytes(&[0x49, 0x81, 0xc2]);
    e.u32(program.buffers[r.source].stride);
    e.bytes(&[0xff, 0xc1]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    // Windows x64：返回地址与四个 shadow slots 之后是第五个参数。
    e.bytes(&[0x4c, 0x8b, 0x54, 0x24, 0x28]);
    for (component, (_, offset)) in reduction.ty.components.iter().enumerate() {
        let value = if float { allocation::Location::Xmm(component as u8) } else { allocation::Location::Gpr(registers[component]) };
        e.write_value_address(value, Memory::new(10, *offset), scalar.size());
    }
    e.bytes(&[0xc3]);
    e.finish(0)
}
