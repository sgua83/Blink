//! CPU 范围私有直方图、桶／范围前缀与稳定散写；大记录只在最后 gather 一次。
use super::*;
use blanguage::ir::{Program, Sorting};
pub(super) struct Code {
    source: usize,
    target: usize,
    initialize: Executable,
    histogram: Executable,
    scatter: Executable,
    gather: Executable,
}
impl Code {
    pub fn new(program: &Program, s: &Sorting, initialize: Executable) -> Result<Self> {
        Ok(Self {
            source: s.source,
            target: s.target,
            initialize,
            histogram: Executable::new(blanguage::native::emit_sort_histogram(program))?,
            scatter: Executable::new(blanguage::native::emit_sort_scatter(program))?,
            gather: Executable::new(blanguage::native::emit_sort_gather(program, s))?,
        })
    }
}
pub(super) fn run(host: &CpuCode, code: &Code, pointers: &[*mut u32], count: u32) -> Result<()> {
    let mut a = words(count as usize * 2)?;
    let mut b = words(count as usize * 2)?;
    let grain = (count as usize).div_ceil(host.workers * 4).max(1);
    let chunks = (count as usize).div_ceil(grain);
    let mut rows = words(chunks * 17)?;
    let mut buffers = [
        pointers[code.source],
        pointers[code.target],
        a.as_mut_ptr(),
        a.as_mut_ptr(),
    ];
    let mut work = RangeWork {
        entry: code.initialize.address,
        buffers: buffers.as_ptr(),
        range_parameter_slot: buffers.len(),
        uniforms: std::ptr::null(),
        count: count as usize,
        grain,
        next: AtomicUsize::new(0),
        result: RangeResult::None,
        partials: std::ptr::null_mut(),
        range_parameters: std::ptr::null(),
        range_parameter_words: 17,
    };
    host.execute(&mut work)?;
    for shift in (0..32).step_by(4) {
        rows.fill(0);
        for row in rows.chunks_exact_mut(17) {
            row[0] = shift;
        }
        buffers[2] = a.as_mut_ptr();
        buffers[3] = b.as_mut_ptr();
        work.buffers = buffers.as_ptr();
        work.entry = code.histogram.address;
        work.range_parameters = rows.as_mut_ptr();
        work.next = AtomicUsize::new(0);
        host.execute(&mut work)?;
        let mut offset = 0u32;
        for bin in 0..16 {
            for row in rows.chunks_exact_mut(17) {
                let n = row[bin + 1];
                row[bin + 1] = offset;
                offset += n;
            }
        }
        work.range_parameters = rows.as_mut_ptr();
        work.entry = code.scatter.address;
        work.next = AtomicUsize::new(0);
        host.execute(&mut work)?;
        std::mem::swap(&mut a, &mut b);
    }
    buffers[2] = a.as_mut_ptr();
    work.buffers = buffers.as_ptr();
    work.entry = code.gather.address;
    work.range_parameters = std::ptr::null();
    work.next = AtomicUsize::new(0);
    host.execute(&mut work)
}

fn words(count: usize) -> Result<Vec<u32>> {
    let mut words = Vec::new();
    words.try_reserve_exact(count)?;
    words.resize(count, 0);
    Ok(words)
}
