//! 两次原生范围遍历：先交付范围摘要，再用范围前缀直接写唯一的段尾结果。
use super::*;
pub fn emit_segmented_reduce_finish(
    program: &Program,
    reduction: &SegmentedReduction,
) -> NativeCode {
    emit(program, reduction, true)
}
fn initialize(e: &mut Emitter<'_>, values: &ReductionSpec) {
    for (component, (scalar, _)) in values.ty.components.iter().enumerate() {
        let wide = scalar.size() == 8;
        if wide {e.bytes(&[0x48]);}
        e.bytes(&[0xb8]);
        let identity = values.operator.identity(*scalar);
        if wide {e.bytes(&identity.to_le_bytes());} else {e.u32(identity as u32);}
        e.bytes(&[0x66]);
        if wide {e.bytes(&[0x48]);}
        e.bytes(&[0x0f, 0x6e, 0xc0 + component as u8 * 8]);
    }
}
fn accumulate(e: &mut Emitter<'_>, values: &ReductionSpec) {
    for (component, (scalar, offset)) in values.ty.components.iter().enumerate() {
        let c = component as u8;
        let wide = scalar.size() == 8;
        let float = match (*scalar, values.operator) {
            (Scalar::F32 | Scalar::F64, CollectiveOperator::Sum) => Some(0x58),
            (Scalar::F32 | Scalar::F64, CollectiveOperator::Min) => Some(0x5d),
            (Scalar::F32 | Scalar::F64, CollectiveOperator::Max) => Some(0x5f),
            _ => None,
        };
        if let Some(opcode) = float {
            e.bytes(&[if wide { 0xf2 } else { 0xf3 }, 0x41, 0x0f, opcode, 0x82 + c * 8]);
            e.u32(values.fields.source_word + offset);
        } else {
            e.bytes(&[0x66]);
            if wide {e.bytes(&[0x48]);}
            e.bytes(&[0x0f, 0x7e, 0xc0 + c * 8]);
            let opcode = match values.operator {
                CollectiveOperator::Sum => Some(0x03),
                CollectiveOperator::BitAnd => Some(0x23),
                CollectiveOperator::BitOr => Some(0x0b),
                CollectiveOperator::BitXor => Some(0x33),
                CollectiveOperator::Min | CollectiveOperator::Max => None,
            };
            let prefix = (scalar.size() == 2).then_some(0x66);
            let byte = u8::from(scalar.size() == 1);
            let memory = Memory::new(10, values.fields.source_word + offset);
            if let Some(opcode) = opcode {
                e.instruction(prefix, &[opcode - byte], wide, 0, memory.into());
            } else {
                let condition = match (scalar.is_signed(), values.operator == CollectiveOperator::Min) {
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
            e.bytes(&[0x66]);
            if wide {e.bytes(&[0x48]);}
            e.bytes(&[0x0f, 0x6e, 0xc0 + c * 8]);
        }
    }
}
pub(super) fn emit(program: &Program, reduction: &SegmentedReduction, finish: bool) -> NativeCode {
    let values = &reduction.values;
    let r = &values.fields;
    let k = values.ty.size.div_ceil(4) as usize;
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
    e.bytes(&[0x41, 0x54, 0x41, 0x55]);
    e.instruction(None, &[0x8b], true, 12, operands::Operand::memory(1,
        r.source as u32 * BUFFER_BYTES + BUFFER_STRIDE));
    if finish {
        e.instruction(None, &[0x8b], true, 13, operands::Operand::memory(1,
            r.target as u32 * BUFFER_BYTES + BUFFER_STRIDE));
    }
    // r10 是当前源记录，r11 是目标基址；r9d 是真实源序号，r8d 是范围剩余数量。
    e.bytes(&[0x4c, 0x8b, 0x91]);
    e.u32(r.source as u32 * BUFFER_BYTES);
    if finish {
        e.bytes(&[0x4c, 0x8b, 0x99]);
        e.u32(r.target as u32 * BUFFER_BYTES);
    }
    e.bytes(&[0x44, 0x89, 0xc8]);
    e.instruction(None, &[0x0f, 0xaf], true, 0, operands::Operand::Register(12));
    e.bytes(&[0x49, 0x01, 0xc2, 0x45, 0x29, 0xc8]);
    if finish {
        // 同一范围参数块：K 个前缀分量、实际总长度、进入范围前的段数。
        e.bytes(&[0x48, 0x8b, 0x91]);
        e.u32(program.buffers.len() as u32 * BUFFER_BYTES);
        e.bytes(&[0x8b, 0x8a]);
        e.u32((k as u32 + 1) * 4);
        e.bytes(&[0x66, 0x0f, 0x6e, 0xa2]);
        e.u32(k as u32 * 4);
        for (component, (scalar, offset)) in values.ty.components.iter().enumerate() {
            e.bytes(&[0xf3, 0x0f, if scalar.size()==8 {0x7e} else {0x10}, 0x82 + component as u8 * 8]);
            e.u32(*offset);
        }
    } else {
        e.bytes(&[0x31, 0xc9, 0x66, 0x0f, 0xef, 0xed]);
        initialize(&mut e, values);
    }
    let begin = e.code.len();
    e.bytes(&[0x45, 0x85, 0xc0]);
    let end = e.jump(Some(0x84));
    e.bytes(&[0x45, 0x85, 0xc9]);
    let first = e.jump(Some(0x84));
    e.bytes(&[0x41, 0x83, 0xba]);
    e.u32(reduction.head_word);
    e.bytes(&[0]);
    let no_head = e.jump(Some(0x84));
    e.patch(first, e.code.len());
    initialize(&mut e, values);
    e.bytes(&[0xff, 0xc1]); // 每个真实段头只计数一次，第一条隐含为段头。
    if !finish {
        e.bytes(&[0x66, 0x0f, 0x76, 0xed]);
    }
    e.patch(no_head, e.code.len());
    accumulate(&mut e, values);
    if finish {
        // 只让真实段尾写；范围末端仍可只读下一条源记录，不制造 halo 副本。
        e.bytes(&[0x41, 0x8d, 0x41, 1, 0x66, 0x0f, 0x7e, 0xe2, 0x39, 0xd0]);
        let last = e.jump(Some(0x84));
        e.instruction(None, &[0x83], false, 7,
            Memory::indexed(10, 12, 0, reduction.head_word).into());
        e.bytes(&[0]);
        let not_tail = e.jump(Some(0x84));
        e.patch(last, e.code.len());
        e.bytes(&[0x89, 0xc8, 0xff, 0xc8]);
        e.instruction(None, &[0x0f, 0xaf], true, 0, operands::Operand::Register(13));
        e.bytes(&[0x49, 0x8d, 0x14, 0x03]); // rdx = target + (当前段数-1)*stride
        for (component, (scalar, offset)) in values.ty.components.iter().enumerate() {
            e.write_value_address(allocation::Location::Xmm(component as u8), Memory::new(2, r.target_word + offset), scalar.size());
        }
        e.patch(not_tail, e.code.len());
    }
    e.instruction(None, &[0x03], true, 10, operands::Operand::Register(12));
    e.bytes(&[0x41, 0xff, 0xc1, 0x41, 0xff, 0xc8]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    if !finish {
        e.bytes(&[0x4c, 0x8b, 0x54, 0x24, 0x38]);
        for (component, (scalar, offset)) in values.ty.components.iter().enumerate() {
            e.write_value_address(allocation::Location::Xmm(component as u8), Memory::new(10, *offset), scalar.size());
        }
        e.bytes(&[0xf3, 0x41, 0x0f, 0x11, 0xaa]);
        e.u32(k as u32 * 4);
        e.bytes(&[0x41, 0x89, 0x8a]);
        e.u32((k as u32 + 1) * 4);
    }
    e.bytes(&[0x41, 0x5d, 0x41, 0x5c, 0xc3]);
    e.finish(16)
}
