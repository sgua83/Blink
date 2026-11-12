//! 私有排序绑定表：[原记录、目标记录、键索引输入、键索引输出、范围参数]。
use super::*;
fn emitter(program: &Program) -> Emitter<'_> {
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
struct SortKeyEmission {
    buffer_bytes: u32,
    stride_offset: u32,
    word: u32,
    bytes: u32,
    bias: u32,
    floating: u32,
    descending: u32,
    first: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::sort_key"]
    fn sort_key(input: SortKeyEmission, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::sort_histogram"]
    fn sort_histogram(buffer_bytes: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::sort_scatter"]
    fn sort_scatter(buffer_bytes: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::sort_gather"]
    fn sort_gather(buffer_bytes: u32, stride_offset: u32, record_bytes: u32, output: *mut u8) -> u64;
}

pub fn emit_sort_key(program: &Program, s: &Sorting, key: usize) -> NativeCode {
    initialize(program, s, key, false)
}

pub(super) fn initialize(program: &Program, s: &Sorting, key: usize, first: bool) -> NativeCode {
    let (word, scalar, order) = s.keys[key];
    let input = SortKeyEmission {
        buffer_bytes: BUFFER_BYTES,
        stride_offset: BUFFER_STRIDE,
        word,
        bytes: scalar.size(),
        bias: scalar.order_bias() as u32,
        floating: u32::from(matches!(scalar, Scalar::F32 | Scalar::F64)),
        descending: u32::from(order == SortOrder::Descending),
        first: u32::from(first),
    };
    let mut e = emitter(program);
    e.push_registers(&[12]);
    e.emit_bytes(160, |output| unsafe { sort_key(input, output) });
    e.finish(8)
}

pub fn emit_sort_histogram(program: &Program) -> NativeCode {
    let mut e = emitter(program);
    e.emit_bytes(96, |output| unsafe { sort_histogram(BUFFER_BYTES, output) });
    e.finish(0)
}

pub fn emit_sort_scatter(program: &Program) -> NativeCode {
    let mut e = emitter(program);
    e.push_registers(&[3]);
    e.emit_bytes(128, |output| unsafe { sort_scatter(BUFFER_BYTES, output) });
    e.finish(8)
}

pub fn emit_sort_gather(program: &Program, s: &Sorting) -> NativeCode {
    let mut e = emitter(program);
    let record_bytes = program.buffers[s.source].stride;
    e.push_registers(&[3, 6, 7, 12, 13]);
    e.emit_bytes(192, |output| unsafe {
        sort_gather(BUFFER_BYTES, BUFFER_STRIDE, record_bytes, output)
    });
    e.finish(40)
}
