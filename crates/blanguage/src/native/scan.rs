//! 原生数值独占扫描：一次遍历累计各分量，范围前缀由同一执行框架传播。
use super::*;
use super::reduce::{ReductionCode, ReductionRange};
pub fn emit_scan_add(program: &Program, scan: &Scan) -> NativeCode {
    emit_scan(program, scan, true)
}
pub(super) fn emit_scan(program: &Program, scan: &Scan, add: bool) -> NativeCode {
    match scan.head_word {
        Some(head) => emit_segmented(program, scan, head, add),
        None => emit(program, scan, add),
    }
}
#[repr(C)]
struct ScanRange {
    values: ReductionRange,
    target: u32,
    target_word: u32,
    parameters: u32,
}

#[repr(C)]
struct SegmentedScanRange {
    scan: ScanRange,
    head_word: u32,
    value_words: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::segmented_scan_range"]
    fn segmented_scan_range(input: SegmentedScanRange, add: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::scan_range"]
    fn scan_range(input: ScanRange, add: u32, output: *mut u8) -> ReductionCode;
}

impl ScanRange {
    fn new(program: &Program, scan: &Scan) -> Self {
        Self {
            values: ReductionRange::new(&scan.values, &scan.ty, scan.operator),
            target: scan.values.target as u32 * BUFFER_BYTES,
            target_word: scan.values.target_word,
            parameters: program.buffers.len() as u32 * BUFFER_BYTES,
        }
    }
}

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

pub(super) fn emit(program: &Program, scan: &Scan, add: bool) -> NativeCode {
    let input = ScanRange::new(program, scan);
    let identity = input.values.identity;
    let mut e = emitter(program);
    let saved: &[u8] = if add { &[12] } else { &[12, 13] };
    e.push_registers(saved);
    e.emit_collective(512, identity, |output| unsafe { scan_range(input, u32::from(add), output) });
    e.finish(saved.len() as u32 * 8)
}

fn emit_segmented(program: &Program, scan: &Scan, head: u32, add: bool) -> NativeCode {
    let input = SegmentedScanRange {
        scan: ScanRange::new(program, scan),
        head_word: head,
        value_words: scan.ty.size.div_ceil(4),
    };
    let mut e = emitter(program);
    e.push_registers(&[12, 13]);
    e.emit_bytes(512, |output| unsafe { segmented_scan_range(input, u32::from(add), output) });
    e.finish(16)
}
