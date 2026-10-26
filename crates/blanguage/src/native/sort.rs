//! 私有排序指针表：[原记录、目标记录、键索引输入、键索引输出、范围参数]。
use super::*;
fn emitter(program: &Program) -> Emitter<'_> {
    Emitter {
        code: Vec::new(),
        relocations: Vec::new(),
        stage: &program.stages[0],
        program,
        breaks: Vec::new(),
        exits: Vec::new(),
        continues: Vec::new(),
        allocation: Allocation::default(),
        frame: Frame::default(),
    }
}
fn finish(e: Emitter<'_>, stack_bytes: u32) -> NativeCode {
    NativeCode {
        bytes: e.code,
        relocations: Vec::new(),
        stack_bytes,
    }
}
pub(super) fn emit_initialize(program: &Program, s: &Sorting) -> NativeCode {
    initialize(program, s, s.keys.len() - 1, true)
}
pub fn emit_sort_key(program: &Program, s: &Sorting, key: usize) -> NativeCode {
    initialize(program, s, key, false)
}
fn initialize(program: &Program, s: &Sorting, key: usize, first: bool) -> NativeCode {
    let mut e = emitter(program);
    let stride = program.buffers[s.source].stride;
    e.bytes(&[0x4c, 0x8b, 0x11, 0x4c, 0x8b, 0x59, 0x18]);
    if first {
        e.bytes(&[0x44, 0x89, 0xc8, 0x48, 0x69, 0xc0]);
        e.u32(stride);
        e.bytes(&[0x49, 0x01, 0xc2]);
    }
    let begin = e.code.len();
    e.bytes(&[0x45, 0x39, 0xc1]);
    let end = e.jump(Some(0x83));
    if first {
        e.bytes(&[0x47, 0x89, 0x4c, 0xcb, 0x04, 0x41, 0x8b, 0x82]);
    } else {
        e.bytes(&[0x43, 0x8b, 0x44, 0xcb, 0x04, 0x48, 0x69, 0xc0]);
        e.u32(stride);
        e.bytes(&[0x41, 0x8b, 0x84, 0x02]);
    }
    e.u32(s.keys[key].0 * 4);
    // 每个键仅装入一次全序编码；后续八轮共用键／原始序号。
    match s.keys[key].1 {
        SortKey::U32 => {}
        SortKey::I32 => {
            e.bytes(&[0x35]);
            e.u32(0x8000_0000);
        }
        SortKey::F32 => {
            // 负数逐位取反，非负数翻转符号位；不执行浮点算术或改变源位值。
            e.bytes(&[0x89, 0xc2, 0xc1, 0xfa, 0x1f, 0x81, 0xca]);
            e.u32(0x8000_0000);
            e.bytes(&[0x31, 0xd0]);
        }
    }
    if s.keys[key].2 == SortOrder::Descending {
        e.bytes(&[0xf7, 0xd0]); // not eax：反向全序编码，不改写原记录。
    }
    e.bytes(&[0x43, 0x89, 0x04, 0xcb]);
    if first {
        e.bytes(&[0x49, 0x81, 0xc2]);
        e.u32(stride);
    }
    e.bytes(&[0x41, 0xff, 0xc1]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.bytes(&[0xc3]);
    finish(e, 0)
}
pub fn emit_sort_histogram(program: &Program) -> NativeCode {
    let mut e = emitter(program);
    // 参数行：[shift, 16 个本范围私有计数]。
    e.bytes(&[0x4c, 0x8b, 0x51, 0x10, 0x48, 0x8b, 0x51, 0x20, 0x8b, 0x0a]);
    let begin = e.code.len();
    e.bytes(&[0x45, 0x39, 0xc1]);
    let end = e.jump(Some(0x83));
    e.bytes(&[
        0x43, 0x8b, 0x04, 0xca, 0xd3, 0xe8, 0x83, 0xe0, 0x0f, 0xff, 0x44, 0x82, 0x04, 0x41, 0xff,
        0xc1,
    ]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.bytes(&[0xc3]);
    finish(e, 0)
}
pub fn emit_sort_scatter(program: &Program) -> NativeCode {
    let mut e = emitter(program);
    e.bytes(&[
        0x53, 0x4c, 0x8b, 0x51, 0x10, 0x4c, 0x8b, 0x59, 0x18, 0x48, 0x8b, 0x51, 0x20, 0x8b, 0x0a,
    ]);
    let begin = e.code.len();
    e.bytes(&[0x45, 0x39, 0xc1]);
    let end = e.jump(Some(0x83));
    e.bytes(&[
        0x43, 0x8b, 0x04, 0xca, 0xd3, 0xe8, 0x83, 0xe0, 0x0f, 0x48, 0x8d, 0x44, 0x82, 0x04, 0x8b,
        0x18, 0xff, 0x00, 0x4b, 0x8b, 0x04, 0xca, 0x49, 0x89, 0x04, 0xdb, 0x41, 0xff, 0xc1,
    ]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.bytes(&[0x5b, 0xc3]);
    finish(e, 8)
}
pub fn emit_sort_gather(program: &Program, s: &Sorting) -> NativeCode {
    let mut e = emitter(program);
    let stride = program.buffers[s.source].stride;
    e.bytes(&[
        0x53, 0x56, 0x57, 0x4c, 0x8b, 0x11, 0x4c, 0x8b, 0x59, 0x08, 0x48, 0x8b, 0x51, 0x10,
    ]);
    let begin = e.code.len();
    e.bytes(&[0x45, 0x39, 0xc1]);
    let end = e.jump(Some(0x83));
    e.bytes(&[0x42, 0x8b, 0x44, 0xca, 0x04, 0x48, 0x69, 0xc0]);
    e.u32(stride);
    e.bytes(&[0x49, 0x8d, 0x34, 0x02, 0x44, 0x89, 0xc8, 0x48, 0x69, 0xc0]);
    e.u32(stride);
    e.bytes(&[0x49, 0x8d, 0x3c, 0x03, 0xb9]);
    e.u32(stride / 4);
    e.bytes(&[0xf3, 0xa5, 0x41, 0xff, 0xc1]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.bytes(&[0x5f, 0x5e, 0x5b, 0xc3]);
    finish(e, 24)
}
