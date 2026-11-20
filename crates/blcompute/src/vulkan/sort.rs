//! 稳定键排序的 GPU 基数阶段、临时存储与必要 subgroup 能力。
use super::*;
#[repr(C)]
pub(super) struct Features {
    pub kind: u32,
    pub next: *mut c_void,
    pub size_control: u32,
    pub full: u32,
}
#[repr(C)]
pub(super) struct ControlProperties {
    pub kind: u32,
    pub next: *mut c_void,
    pub minimum: u32,
    pub maximum: u32,
    pub max_groups: u32,
    pub stages: u32,
}
#[repr(C)]
pub(super) struct RequiredSize {
    pub kind: u32,
    pub next: *const c_void,
    pub size: u32,
}

pub(super) struct Storage {
    pairs: [PagedBuffer; 2],
    histogram: PagedBuffer,
    prefixes: PagedBuffer,
    scan: Vec<PagedBuffer>,
    histogram_count: u32,
}
pub(super) fn allocate(c: &Compiled, stage: usize, count: u32) -> Result<Storage> {
    let histogram_count = blanguage::spirv::sort_histogram_count(count);
    Ok(Storage {
        pairs: [
            PagedBuffer::words(c.inner.clone(), count, 2)?,
            PagedBuffer::words(c.inner.clone(), count, 2)?,
        ],
        histogram: PagedBuffer::words(c.inner.clone(), histogram_count, 1)?,
        prefixes: PagedBuffer::words(c.inner.clone(), histogram_count, 1)?,
        scan: scan::allocate(c, stage, histogram_count, 1)?.buffers,
        histogram_count,
    })
}
impl Storage {
    pub(super) fn descriptor_count(&self) -> u64 {
        // Initialization and finish, two histogram and two scatter sets, then scan levels.
        (6 + 2 * scan::levels(self.histogram_count) - 1) as u64
    }
}

pub(super) struct Descriptors {
    initialize: u64,
    histogram: [u64; 2],
    scan: scan::Descriptors,
    scatter: [u64; 2],
    finish: u64,
}

pub(super) fn descriptors(
    writer: &mut DescriptorWriter<'_>,
    sorting: &blanguage::ir::Sorting,
    storage: &Storage,
    arguments: Option<&PagedBuffer>,
) -> Result<Descriptors> {
    let indirect = arguments.map(PagedBuffer::descriptor);
    let initialize = writer.set(
        &[Some((sorting.target, storage.pairs[0].descriptor()))],
        None,
        indirect,
        None,
        None,
    )?;
    let mut histogram = [0; 2];
    let mut scatter = [0; 2];
    for (parity, input) in storage.pairs.iter().enumerate() {
        let bindings = [
            Some((sorting.source, input.descriptor())),
            Some((sorting.target, storage.pairs[1 - parity].descriptor())),
        ];
        histogram[parity] = writer.set(
            &bindings,
            Some(storage.histogram.descriptor()),
            indirect,
            None,
            None,
        )?;
        scatter[parity] = writer.set(
            &bindings,
            Some(storage.prefixes.descriptor()),
            indirect,
            None,
            None,
        )?;
    }
    let scan = scan::descriptors(
        writer,
        (sorting.source, sorting.target),
        false,
        storage.histogram_count,
        scan::ScanRoot::Source(storage.histogram.descriptor()),
        storage.prefixes.descriptor(),
        &storage.scan,
        indirect,
    )?;
    let finish = writer.set(
        &[],
        Some(storage.pairs[0].descriptor()),
        indirect,
        None,
        None,
    )?;
    Ok(Descriptors { initialize, histogram, scan, scatter, finish })
}

pub(super) fn record(
    commands: &Commands,
    c: &Compiled,
    stage: usize,
    key_count: usize,
    count: u32,
    storage: &Storage,
    sets: &Descriptors,
    arguments: Option<&PagedBuffer>,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    reused: bool,
) {
    let kernels = &c.additional[stage].sort;
    for key in (0..key_count).rev() {
        commands.bind_compute_set(c.layout, sets.initialize);
        let initialize = if key + 1 == key_count {
            if arguments.is_some() {
                prefix::prepare(commands, c, stage, uniforms, partition, reused);
            }
            c.pipelines[stage]
        } else {
            kernels[20 + key]
        };
        scan::dispatch(
            commands,
            c,
            initialize,
            count,
            uniforms,
            partition,
            arguments,
            0,
        );
        for pass in 0..8 {
            commands.barrier(0x800, 0x800, 0x40, 0x60);
            commands.bind_compute_set(c.layout, sets.histogram[pass % 2]);
            scan::dispatch(
                commands,
                c,
                kernels[pass],
                count,
                uniforms,
                partition,
                arguments,
                0,
            );
            commands.barrier(0x800, 0x800, 0x40, 0x60);
            commands.bind_compute_set(c.layout, sets.scan.root());
            scan::record_dispatches(
                commands,
                c,
                stage,
                [kernels[16], kernels[17], kernels[18], kernels[18]],
                storage.histogram_count,
                &sets.scan,
                arguments,
                uniforms,
                partition,
                blanguage::spirv::PREFIX_LEVELS,
            );
            commands.barrier(0x800, 0x800, 0x40, 0x60);
            commands.bind_compute_set(c.layout, sets.scatter[pass % 2]);
            scan::dispatch(
                commands,
                c,
                kernels[8 + pass],
                count,
                uniforms,
                partition,
                arguments,
                0,
            );
        }
        commands.barrier(0x800, 0x800, 0x40, 0x60);
    }
    let pipeline = kernels[19];
    commands.bind_compute_set(c.layout, sets.finish);
    scan::dispatch(
        commands, c, pipeline, count, uniforms, partition, arguments, 0,
    );
}
