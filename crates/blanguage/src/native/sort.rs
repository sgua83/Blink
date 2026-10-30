//! 私有排序指针表：[原记录、目标记录、键索引输入、键索引输出、范围参数]。
use super::*;
fn emitter(program: &Program) -> Emitter<'_> {
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
pub fn emit_sort_key(program: &Program, s: &Sorting, key: usize) -> NativeCode {
    initialize(program, s, key, false)
}
pub(super) fn initialize(program: &Program, s: &Sorting, key: usize, first: bool) -> NativeCode {
    let mut e = emitter(program);
    let stride = program.buffers[s.source].stride;
    e.bytes(&[0x4c, 0x8b, 0x11, 0x4c, 0x8b, 0x59, 0x18]);
    if first {
        e.bytes(&[0x44, 0x89, 0xc8]);
        e.scale_index(0, stride);
        e.bytes(&[0x49, 0x01, 0xc2]);
    }
    let begin = e.code.len();
    e.bytes(&[0x45, 0x39, 0xc1]);
    let end = e.jump(Some(0x83));
    let word = s.keys[key].0;
    let scalar = s.keys[key].1;
    let memory = if first {
        e.bytes(&[0x47, 0x89, 0x4c, 0xcb, 0x04]);
        operands::Memory::new(10, word)
    } else {
        e.bytes(&[0x43, 0x8b, 0x44, 0xcb, 0x04]);
        e.scaled_memory(10, 0, stride, word)
    };
    if scalar == Scalar::F64 {
        e.instruction(None, &[0x8b], false, 1, memory.offset(4).into());
    }
    let load: &[u8] = match scalar.size() {
        1 => &[0x0f, 0xb6],
        2 => &[0x0f, 0xb7],
        _ => &[0x8b],
    };
    e.instruction(None, load, false, 0, memory.into());
    // 全序编码按32位子键装入；后续八轮共用键／原始序号。
    let bias = scalar.order_bias() as u32;
    if bias != 0 {
        e.bytes(&[0x35]);
        e.u32(bias);
    } else if scalar == Scalar::F32 {
        e.bytes(&[0x89, 0xc2, 0xc1, 0xfa, 0x1f, 0x81, 0xca]);
        e.u32(0x8000_0000);
        e.bytes(&[0x31, 0xd0]);
    } else if scalar == Scalar::F64 {
        e.bytes(&[0xc1, 0xf9, 0x1f, 0x31, 0xc8]);
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
    e.finish(0)
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
    e.finish(0)
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
    e.finish(8)
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
    e.bytes(&[0x42, 0x8b, 0x44, 0xca, 0x04]);
    e.scale_index(0, stride);
    e.bytes(&[0x49, 0x8d, 0x34, 0x02, 0x44, 0x89, 0xc8]);
    e.scale_index(0, stride);
    e.bytes(&[0x49, 0x8d, 0x3c, 0x03]);
    let vectors = stride / 16;
    if vectors != 0 {
        if vectors != 1 {
            e.bytes(&[0xb9]);
            e.u32(vectors);
        }
        let copy = e.code.len();
        // XMM0 属于原范围 ABI 的易失寄存器；只读取当前记录内的完整 16 字节块。
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
    e.bytes(&[0x41, 0xff, 0xc1]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    e.bytes(&[0x5f, 0x5e, 0x5b, 0xc3]);
    e.finish(24)
}
