//! 原生数值独占扫描：一次遍历累计各分量，范围前缀由同一执行框架传播。
use super::*;
use super::reduce::ReductionRange;
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
    fn segmented_scan_range(input: SegmentedScanRange, add: u32, output: LeafOutput) -> LeafRange;
    #[link_name = "bootstrap.x64_instruction::scan_range"]
    fn scan_range(input: ScanRange, add: u32, output: CollectiveLeafOutput) -> CollectiveLeaf;
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

pub(super) fn emit(program: &Program, scan: &Scan, add: bool) -> NativeCode {
    let input = ScanRange::new(program, scan);
    emit_collective_leaf(512, |output| unsafe { scan_range(input, u32::from(add), output) })
}

fn emit_segmented(program: &Program, scan: &Scan, head: u32, add: bool) -> NativeCode {
    let input = SegmentedScanRange {
        scan: ScanRange::new(program, scan),
        head_word: head,
        value_words: scan.ty.size.div_ceil(4),
    };
    emit_leaf(512, |output| unsafe { segmented_scan_range(input, u32::from(add), output) })
}
