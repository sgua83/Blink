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

pub(super) struct Pipelines {
    inner: Rc<Inner>,
    handles: Vec<VkPipeline>,
}
impl Drop for Pipelines {
    fn drop(&mut self) {
        unsafe {
            for p in &self.handles {
                (self.inner.functions.vkDestroyPipeline)(self.inner.owner.device, *p, null());
            }
        }
    }
}
impl Pipelines {
    pub fn load<P>(c: &Compiled<P>, shaders: &[blanguage::spirv::ShaderCode]) -> Result<Self> {
        let mut out = Self {
            inner: c.inner.clone(),
            handles: Vec::new(),
        };
        for shader in shaders {
            out.handles
                .push(pipeline(&c.inner, c.layout, &shader.words, shader.ranks)?);
        }
        Ok(out)
    }
    fn scan(&self) -> [VkPipeline; 4] {
        [
            self.handles[16],
            self.handles[17],
            self.handles[18],
            self.handles[18],
        ]
    }
}
pub(super) struct Storage {
    pairs: [PagedBuffer; 2],
    histogram: PagedBuffer,
    prefixes: PagedBuffer,
    scan: Vec<PagedBuffer>,
    histogram_count: u32,
}
pub(super) fn allocate(c: &Compiled, count: u32) -> Result<Storage> {
    let histogram_count = scan::groups(count) * 16;
    Ok(Storage {
        pairs: [
            PagedBuffer::words(c.inner.clone(), count, 2)?,
            PagedBuffer::words(c.inner.clone(), count, 2)?,
        ],
        histogram: PagedBuffer::words(c.inner.clone(), histogram_count, 1)?,
        prefixes: PagedBuffer::words(c.inner.clone(), histogram_count, 1)?,
        scan: scan::allocate(c, histogram_count, 1)?.buffers,
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
    prefix: Option<u64>,
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
    let prefix = arguments.map(|args| prefix::descriptor(writer, args)).transpose()?;
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
    let r = blanguage::ir::Reduction {
        source: sorting.source,
        source_word: 0,
        target: sorting.target,
        target_word: 0,
    };
    let scan = scan::descriptors(
        writer,
        &r,
        false,
        (storage.histogram_count, storage.histogram.descriptor()),
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
    Ok(Descriptors { prefix, initialize, histogram, scan, scatter, finish })
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
    if let Some(set) = sets.prefix {
        prefix::prepare(commands, c, stage, set, uniforms, partition, reused);
    }
    let kernels = c.sorts[stage].as_ref().unwrap();
    for key in (0..key_count).rev() {
        scan::dispatch(
            commands,
            c,
            if key + 1 == key_count {
                c.pipelines[stage]
            } else {
                kernels.handles[20 + key]
            },
            sets.initialize,
            count,
            uniforms,
            partition,
            arguments,
            0,
        );
        for pass in 0..8 {
            commands.barrier(0x800, 0x800, 0x40, 0x60);
            scan::dispatch(
                commands,
                c,
                kernels.handles[pass],
                sets.histogram[pass % 2],
                count,
                uniforms,
                partition,
                arguments,
                0,
            );
            commands.barrier(0x800, 0x800, 0x40, 0x60);
            scan::record_dispatches(
                commands,
                c,
                kernels.scan(),
                storage.histogram_count,
                &sets.scan,
                arguments,
                uniforms,
                partition,
                blanguage::spirv::PREFIX_LEVELS,
            );
            commands.barrier(0x800, 0x800, 0x40, 0x60);
            scan::dispatch(
                commands,
                c,
                kernels.handles[8 + pass],
                sets.scatter[pass % 2],
                count,
                uniforms,
                partition,
                arguments,
                0,
            );
        }
        commands.barrier(0x800, 0x800, 0x40, 0x60);
    }
    let pipeline = kernels.handles[19];
    scan::dispatch(
        commands, c, pipeline, sets.finish, count, uniforms, partition, arguments, 0,
    );
}
