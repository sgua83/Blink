//! CPU 范围私有直方图、桶／范围前缀与稳定散写；大记录只在最后 gather 一次。
use super::*;
pub(super) struct Code {
    source: usize,
    target: usize,
    initialize: Vec<Executable>,
    histogram: Executable,
    scatter: Executable,
    gather: Executable,
}
impl Code {
    pub(super) fn executables(&self) -> impl Iterator<Item = &Executable> {
        self.initialize.iter().chain([&self.histogram, &self.scatter, &self.gather])
    }
    pub fn load<C>(code: &blanguage::native::SortCode<C>, load: &impl Fn(&C) -> Executable) -> Self {
        Self {
            source: code.source,
            target: code.target,
            initialize: code
                .initialize
                .iter()
                .map(load)
                .collect(),
            histogram: load(&code.histogram),
            scatter: load(&code.scatter),
            gather: load(&code.gather),
        }
    }
}
pub(super) fn run(
    host: &CpuCode,
    code: &Code,
    pointers: &[BufferAddress],
    count: u32,
    stage: usize,
    timings: &mut Option<&mut CpuRangeTimings>,
) -> Result<()> {
    // 原生初始化／scatter在首次读取前完整写入；这里只持有容量，不形成未初始化值切片。
    let mut a = Vec::<u32>::new();
    a.try_reserve_exact(count as usize * 2)?;
    let mut b = Vec::<u32>::new();
    b.try_reserve_exact(count as usize * 2)?;
    let grain = (count as usize).div_ceil(host.workers * 4).max(1);
    let chunks = (count as usize).div_ceil(grain);
    let mut rows = words(chunks * 17)?;
    let mut buffers = [
        pointers[code.source],
        pointers[code.target],
        BufferAddress { address: a.as_mut_ptr(), stride: 8 },
        BufferAddress { address: a.as_mut_ptr(), stride: 8 },
    ];
    let mut work = RangeWork {
        frame: None,
        entry: code.initialize[0].address,
        buffers: buffers.as_ptr(),
        range_parameter_slot: buffers.len(),
        uniforms: std::ptr::null(),
        count: count as usize,
        grain,
        next: AtomicUsize::new(0),
        result: RangeResult::None,
        partials: std::ptr::null_mut(),
        errors: std::ptr::null_mut(),
        range_parameters: std::ptr::null(),
        range_parameter_words: 17,
        stage,
        timing: None,
    };
    for key in &code.initialize {
        buffers[2].address = a.as_mut_ptr();
        buffers[3].address = a.as_mut_ptr();
        work.buffers = buffers.as_ptr();
        work.entry = key.address;
        work.range_parameters = std::ptr::null();
        work.next = AtomicUsize::new(0);
        host.execute(&mut work, timings)?;
        for shift in (0..32).step_by(4) {
            rows.fill(0);
            for row in rows.chunks_exact_mut(17) {
                row[0] = shift;
            }
            buffers[2].address = a.as_mut_ptr();
            buffers[3].address = b.as_mut_ptr();
            work.buffers = buffers.as_ptr();
            work.entry = code.histogram.address;
            work.range_parameters = rows.as_mut_ptr();
            work.next = AtomicUsize::new(0);
            host.execute(&mut work, timings)?;
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
            host.execute(&mut work, timings)?;
            std::mem::swap(&mut a, &mut b);
        }
    }
    buffers[2].address = a.as_mut_ptr();
    work.buffers = buffers.as_ptr();
    work.entry = code.gather.address;
    work.range_parameters = std::ptr::null();
    work.next = AtomicUsize::new(0);
    host.execute(&mut work, timings)
}

fn words(count: usize) -> Result<Vec<u32>> {
    let mut words = Vec::new();
    words.try_reserve_exact(count)?;
    words.resize(count, 0);
    Ok(words)
}
