//! 范围计数返回 u32；稳定散写使用执行框架提供的范围前缀，保持全部记录字节。
use super::*;
fn emitter<'a>(program: &'a Program) -> Emitter<'a> {
    Emitter {
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
    }
}
pub(super) fn emit_count(program: &Program, c: &Compaction) -> NativeCode {
    let mut e = emitter(program);
    e.bytes(&[0x41, 0x54]);
    e.instruction(None, &[0x8b], true, 12, operands::Operand::memory(1,
        c.flags as u32 * BUFFER_BYTES + BUFFER_STRIDE));
    e.bytes(&[0x4c, 0x8b, 0x91]);
    e.u32(c.flags as u32 * BUFFER_BYTES);
    e.bytes(&[0x44, 0x89, 0xc8]);
    e.instruction(None, &[0x0f, 0xaf], true, 0, operands::Operand::Register(12));
    e.bytes(&[0x49, 0x01, 0xc2, 0x44, 0x89, 0xc9, 0x31, 0xc0]);
    let begin = e.code.len();
    e.bytes(&[0x44, 0x39, 0xc1]);
    let end = e.jump(Some(0x83));
    e.bytes(&[0x41, 0x83, 0xba]);
    e.u32(c.flag_word);
    e.bytes(&[0, 0x0f, 0x95, 0xc2, 0x0f, 0xb6, 0xd2, 0x01, 0xd0]);
    e.instruction(None, &[0x03], true, 10, operands::Operand::Register(12));
    e.bytes(&[0xff, 0xc1]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.bytes(&[0x41, 0x5c, 0xc3]);
    e.finish(8)
}
pub fn emit_compact_scatter(program: &Program, c: &Compaction) -> NativeCode {
    let mut e = emitter(program);
    let stride = program.buffers[c.source].stride;
    // 实际源／目标／谓词stride分别归R13–R15；所有非易失寄存器由本范围保存和恢复。
    e.bytes(&[0x53, 0x56, 0x57, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57]);
    for (binding, register) in [(c.source, 13), (c.target, 14), (c.flags, 15)] {
        e.instruction(None, &[0x8b], true, register,
            operands::Operand::memory(1, binding as u32 * BUFFER_BYTES + BUFFER_STRIDE));
    }
    e.bytes(&[0x48, 0x8b, 0xb1]);
    e.u32(c.source as u32 * BUFFER_BYTES);
    e.bytes(&[0x48, 0x8b, 0xb9]);
    e.u32(c.target as u32 * BUFFER_BYTES);
    e.bytes(&[0x4c, 0x8b, 0x91]);
    e.u32(c.flags as u32 * BUFFER_BYTES);
    e.bytes(&[0x48, 0x8b, 0x91]);
    e.u32(program.buffers.len() as u32 * BUFFER_BYTES);
    e.bytes(&[0x8b, 0x02]);
    e.instruction(None, &[0x0f, 0xaf], true, 0, operands::Operand::Register(14));
    e.bytes(&[0x48, 0x01, 0xc7]);
    e.bytes(&[0x44, 0x89, 0xc8]);
    e.instruction(None, &[0x0f, 0xaf], true, 0, operands::Operand::Register(13));
    e.bytes(&[0x48, 0x01, 0xc6]);
    e.bytes(&[0x44, 0x89, 0xc8]);
    e.instruction(None, &[0x0f, 0xaf], true, 0, operands::Operand::Register(15));
    e.bytes(&[0x49, 0x01, 0xc2]);
    e.bytes(&[0x44, 0x89, 0xcb, 0x45, 0x89, 0xc4]);
    let begin = e.code.len();
    e.bytes(&[0x44, 0x39, 0xe3]);
    let end = e.jump(Some(0x83));
    e.bytes(&[0x41, 0x83, 0xba]);
    e.u32(c.flag_word);
    e.bytes(&[0]);
    let skip = e.jump(Some(0x84));
    let vectors = stride / 16;
    if vectors != 0 {
        if vectors != 1 {
            e.bytes(&[0xb9]);
            e.u32(vectors);
        }
        let copy = e.code.len();
        // XMM0 是易失寄存器；读取当前记录内的完整块，并推进两个实际游标。
        e.bytes(&[
            0x0f, 0x10, 0x06, 0x0f, 0x11, 0x07, 0x48, 0x83, 0xc6, 0x10, 0x48, 0x83, 0xc7, 0x10,
        ]);
        if vectors != 1 {
            e.bytes(&[0xff, 0xc9]);
            let again = e.jump(Some(0x85));
            e.patch(again, copy);
        }
    }
    for (size, copy) in [(8, &[0x48, 0xa5][..]), (4, &[0xa5][..]), (2, &[0x66, 0xa5][..]), (1, &[0xa4][..])] {
        if stride % (size * 2) >= size {
            e.bytes(copy);
        }
    }
    for (pointer, stride_register) in [(6, 13), (7, 14)] {
        e.instruction(None, &[0x03], true, pointer, operands::Operand::Register(stride_register));
        e.instruction(None, &[0x81], true, 5, operands::Operand::Register(pointer));
        e.u32(stride);
    }
    let next = e.jump(None);
    e.patch(skip, e.code.len());
    e.instruction(None, &[0x03], true, 6, operands::Operand::Register(13));
    e.patch(next, e.code.len());
    e.instruction(None, &[0x03], true, 10, operands::Operand::Register(15));
    e.bytes(&[0xff, 0xc3]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.bytes(&[0x41, 0x5f, 0x41, 0x5e, 0x41, 0x5d, 0x41, 0x5c, 0x5f, 0x5e, 0x5b, 0xc3]);
    e.finish(56)
}
