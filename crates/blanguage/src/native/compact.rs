//! 范围计数写出u32结果；稳定散写使用执行框架提供的范围前缀，保持全部记录字节。
use super::*;
fn emitter<'a>(program: &'a Program) -> Emitter<'a> {
    Emitter {
        unwind: Vec::new(),
        prolog: 0,
        probe: None,
        code: Vec::new(),
        relocations: Vec::new(),
        calls: Vec::new(),
        operation_ranges: None,
        function_abi: None,
        literals: std::collections::BTreeMap::new(),
        stage: &program.stages[0],
        buffers: &program.buffers,
        textures: &program.textures,
        breaks: Vec::new(),
        exits: Vec::new(),
        continues: Vec::new(),
        allocation: Allocation::default(),
        frame: Frame::default(),
    }
}

#[repr(C)]
struct CompactScatter {
    source: u32,
    target: u32,
    flags: u32,
    prefix: u32,
    stride_offset: u32,
    flag_word: u32,
    record_bytes: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::compact_count"]
    fn compact_count(flags: u32, stride_offset: u32, flag_word: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::compact_scatter"]
    fn compact_scatter(input: CompactScatter, output: *mut u8) -> u64;
}

pub(super) fn emit_count(program: &Program, c: &Compaction) -> NativeCode {
    let mut emitter = emitter(program);
    emitter.push_registers(&[12]);
    let flags = c.flags as u32 * BUFFER_BYTES;
    emitter.emit_bytes(96, |output| unsafe {
        compact_count(flags, BUFFER_STRIDE, c.flag_word, output)
    });
    emitter.finish(8)
}

pub fn emit_compact_scatter(program: &Program, c: &Compaction) -> NativeCode {
    let mut emitter = emitter(program);
    emitter.push_registers(&[3, 6, 7, 12, 13, 14, 15]);
    let input = CompactScatter {
        source: c.source as u32 * BUFFER_BYTES,
        target: c.target as u32 * BUFFER_BYTES,
        flags: c.flags as u32 * BUFFER_BYTES,
        prefix: program.buffers.len() as u32 * BUFFER_BYTES,
        stride_offset: BUFFER_STRIDE,
        flag_word: c.flag_word,
        record_bytes: program.buffers[c.source].stride,
    };
    emitter.emit_bytes(224, |output| unsafe { compact_scatter(input, output) });
    emitter.finish(56)
}
