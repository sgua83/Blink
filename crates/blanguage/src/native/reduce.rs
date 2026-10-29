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
            e.u32(r.source_word * 4 + offset);
        } else {
            let register = registers[component];
            let rex = 0x41 | if wide { 8 } else { 0 } | if register >= 8 { 4 } else { 0 };
            let memory = 0x82 + (register & 7) * 8;
            let opcode = match reduction.operator {
                CollectiveOperator::Sum => Some(0x03),
                CollectiveOperator::BitAnd => Some(0x23),
                CollectiveOperator::BitOr => Some(0x0b),
                CollectiveOperator::BitXor => Some(0x33),
                CollectiveOperator::Min | CollectiveOperator::Max => None,
            };
            if let Some(opcode) = opcode {
                e.bytes(&[rex, opcode, memory]);
                e.u32(r.source_word * 4 + offset);
            } else {
                e.bytes(&[rex, 0x3b, memory]);
                e.u32(r.source_word * 4 + offset);
                let min = reduction.operator == CollectiveOperator::Min;
                e.bytes(&[
                    rex,
                    0x0f,
                    match (scalar.is_signed(), min) {
                        (false, true) => 0x47,
                        (true, true) => 0x4f,
                        (false, false) => 0x42,
                        (true, false) => 0x4c,
                    },
                    memory,
                ]);
                e.u32(r.source_word * 4 + offset);
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
        if float {
            e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, 0x11, 0x82 + component as u8 * 8]);
        } else {
            let register = registers[component];
            e.bytes(&[
                0x41 | if wide { 8 } else { 0 } | if register >= 8 { 4 } else { 0 },
                0x89,
                0x82 + (register & 7) * 8,
            ]);
        }
        e.u32(*offset);
    }
    e.bytes(&[0xc3]);
    e.finish(0)
}
