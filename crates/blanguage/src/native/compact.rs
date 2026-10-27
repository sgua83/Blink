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
    e.bytes(&[0x4c, 0x8b, 0x91]);
    e.u32(c.flags as u32 * 8);
    e.bytes(&[0x44, 0x89, 0xc8]);
    e.scale_index(0, program.buffers[c.flags].stride);
    e.bytes(&[0x49, 0x01, 0xc2, 0x44, 0x89, 0xc9, 0x31, 0xc0]);
    let begin = e.code.len();
    e.bytes(&[0x44, 0x39, 0xc1]);
    let end = e.jump(Some(0x83));
    e.bytes(&[0x41, 0x83, 0xba]);
    e.u32(c.flag_word * 4);
    e.bytes(&[0, 0x0f, 0x95, 0xc2, 0x0f, 0xb6, 0xd2, 0x01, 0xd0]);
    e.bytes(&[0x49, 0x81, 0xc2]);
    e.u32(program.buffers[c.flags].stride);
    e.bytes(&[0xff, 0xc1]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.bytes(&[0xc3]);
    e.finish(0)
}
pub fn emit_compact_scatter(program: &Program, c: &Compaction) -> NativeCode {
    let mut e = emitter(program);
    let stride = program.buffers[c.source].stride;
    // Windows x64 的 RBX、RSI、RDI、R12 均由本函数保存；无调用，不使用栈临时数据。
    e.bytes(&[0x53, 0x56, 0x57, 0x41, 0x54]);
    e.bytes(&[0x48, 0x8b, 0xb1]);
    e.u32(c.source as u32 * 8);
    e.bytes(&[0x48, 0x8b, 0xb9]);
    e.u32(c.target as u32 * 8);
    e.bytes(&[0x4c, 0x8b, 0x91]);
    e.u32(c.flags as u32 * 8);
    e.bytes(&[0x48, 0x8b, 0x91]);
    e.u32(program.buffers.len() as u32 * 8);
    e.bytes(&[0x8b, 0x02]);
    e.scale_index(0, stride);
    e.bytes(&[0x48, 0x01, 0xc7]);
    e.bytes(&[0x44, 0x89, 0xc8]);
    e.scale_index(0, stride);
    e.bytes(&[0x48, 0x01, 0xc6]);
    e.bytes(&[0x44, 0x89, 0xc8]);
    e.scale_index(0, program.buffers[c.flags].stride);
    e.bytes(&[0x49, 0x01, 0xc2]);
    e.bytes(&[0x44, 0x89, 0xcb, 0x45, 0x89, 0xc4]);
    let begin = e.code.len();
    e.bytes(&[0x44, 0x39, 0xe3]);
    let end = e.jump(Some(0x83));
    e.bytes(&[0x41, 0x83, 0xba]);
    e.u32(c.flag_word * 4);
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
    if stride % 16 >= 8 {
        e.bytes(&[0x48, 0xa5]);
    }
    if stride % 8 != 0 {
        e.bytes(&[0xa5]);
    }
    let next = e.jump(None);
    e.patch(skip, e.code.len());
    e.bytes(&[0x48, 0x81, 0xc6]);
    e.u32(stride);
    e.patch(next, e.code.len());
    e.bytes(&[0x49, 0x81, 0xc2]);
    e.u32(program.buffers[c.flags].stride);
    e.bytes(&[0xff, 0xc3]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.bytes(&[0x41, 0x5c, 0x5f, 0x5e, 0x5b, 0xc3]);
    e.finish(32)
}
