//! 两次原生范围遍历：先交付范围摘要，再用范围前缀直接写唯一的段尾结果。
use super::*;
use super::reduce::ReductionRange;

#[repr(C)]
struct SegmentedRange {
    values: ReductionRange,
    target: u32,
    target_word: u32,
    parameters: u32,
    head_word: u32,
    value_words: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::segmented_reduce_range"]
    fn segmented_reduce_range(input: SegmentedRange, finish: u32, output: *mut u8) -> u64;
}

pub fn emit_segmented_reduce_finish(
    program: &Program,
    reduction: &SegmentedReduction,
) -> NativeCode {
    emit(program, reduction, true)
}

pub(super) fn emit(program: &Program, reduction: &SegmentedReduction, finish: bool) -> NativeCode {
    let values = &reduction.values;
    let input = SegmentedRange {
        values: ReductionRange::new(&values.fields, &values.ty, values.operator),
        target: values.fields.target as u32 * BUFFER_BYTES,
        target_word: values.fields.target_word,
        parameters: program.buffers.len() as u32 * BUFFER_BYTES,
        head_word: reduction.head_word,
        value_words: values.ty.size.div_ceil(4),
    };
    let mut e = Emitter {
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
    };
    e.push_registers(&[12, 13]);
    e.emit_bytes(512, |output| unsafe { segmented_reduce_range(input, u32::from(finish), output) });
    e.finish(16)
}
