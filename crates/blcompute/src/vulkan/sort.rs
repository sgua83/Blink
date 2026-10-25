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
    pub fn new(c: &Compiled, stage: usize) -> Result<Self> {
        use blanguage::spirv::{Pass, emit_pass};
        let mut out = Self {
            inner: c.inner.clone(),
            handles: Vec::new(),
        };
        let passes = (0..8)
            .map(|i| Pass::SortHistogram(i * 4))
            .chain((0..8).map(|i| Pass::SortScatter(i * 4)))
            .chain([
                Pass::SortScan,
                Pass::ScanTail,
                Pass::ScanAdd,
                Pass::SortGather,
            ]);
        for pass in passes {
            let ranks = matches!(pass, Pass::SortHistogram(_) | Pass::SortScatter(_));
            let words = emit_pass(&c.program, stage, pass).map_err(error)?;
            out.handles
                .push(pipeline(&c.inner, c.layout, &words, ranks)?);
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
    pairs: [GpuDataset; 2],
    histogram: GpuDataset,
    prefixes: GpuDataset,
    scan: Vec<GpuDataset>,
}
pub(super) fn allocate(c: &Compiled, count: u32) -> Result<Scratch> {
    use blanguage::ir::Scalar;
    let pair = Ty {
        name: "sort-key-index".into(),
        arguments: Vec::new(),
        values: Vec::new(),
        size: 8,
        align: 8,
        components: vec![(Scalar::U32, 0), (Scalar::U32, 4)],
        fields: Vec::new(),
        view: None,
        array: None,
    };
    let scalar = Ty::scalar(Scalar::U32);
    let n = count.div_ceil(64).max(1) * 16;
    Ok(Scratch {
        buffers: Vec::new(),
        arguments: None,
        sorting: Some(Storage {
            pairs: [
                GpuDataset::new(c.inner.clone(), pair.clone(), 8, count)?,
                GpuDataset::new(c.inner.clone(), pair, 8, count)?,
            ],
            histogram: GpuDataset::new(c.inner.clone(), scalar.clone(), 4, n)?,
            prefixes: GpuDataset::new(c.inner.clone(), scalar, 4, n)?,
            scan: scan::allocate(c, n, 1)?.buffers,
        }),
    })
}
fn info(a: &Allocation) -> VkDescriptorBufferInfo {
    VkDescriptorBufferInfo {
        buffer: a.buffer,
        offset: 0,
        range: a.size,
    }
}
pub(super) fn record(
    commands: &mut Commands,
    c: &Compiled,
    stage: usize,
    s: &blanguage::ir::Sorting,
    data: &[GpuDataset],
    storage: &Storage,
    arguments: Option<&GpuDataset>,
    infos: &[VkDescriptorBufferInfo],
    accelerations: &[Acceleration],
    uniforms: &mut [u32],
    partition: NonZeroU32,
) -> Result<()> {
    let kernels = c.sorts[stage].as_ref().unwrap();
    let n = data[s.source].count;
    let args = arguments.map(|a| &a.allocation);
    let mut pass_infos = infos.to_vec();
    pass_infos[s.target] = info(&storage.pairs[0].allocation);
    let set = descriptor_set(commands, c, &pass_infos, accelerations, None, args, None)?;
    scan::dispatch(
        commands,
        c,
        c.pipelines[stage],
        set,
        n,
        uniforms,
        partition,
        arguments,
        0,
    );
    for pass in 0..8 {
        commands.barrier(0x800, 0x800, 0x40, 0x60);
        let input = &storage.pairs[pass % 2].allocation;
        let target = &storage.pairs[(pass + 1) % 2].allocation;
        pass_infos = infos.to_vec();
        pass_infos[s.source] = info(input);
        pass_infos[s.target] = info(target);
        let set = descriptor_set(
            commands,
            c,
            &pass_infos,
            accelerations,
            Some(&storage.histogram.allocation),
            args,
            None,
        )?;
        scan::dispatch(
            commands,
            c,
            kernels.handles[pass],
            set,
            n,
            uniforms,
            partition,
            arguments,
            0,
        );
        commands.barrier(0x800, 0x800, 0x40, 0x60);
        let r = blanguage::ir::Reduction {
            source: s.source,
            source_word: 0,
            target: s.target,
            target_word: 0,
        };
        scan::record(
            commands,
            c,
            kernels.scan(),
            &r,
            false,
            &storage.histogram,
            &storage.prefixes.allocation,
            &storage.scan,
            arguments,
            infos,
            accelerations,
            uniforms,
            partition,
            blanguage::spirv::PREFIX_LEVELS,
        )?;
        commands.barrier(0x800, 0x800, 0x40, 0x60);
        let set = descriptor_set(
            commands,
            c,
            &pass_infos,
            accelerations,
            Some(&storage.prefixes.allocation),
            args,
            None,
        )?;
        scan::dispatch(
            commands,
            c,
            kernels.handles[8 + pass],
            set,
            n,
            uniforms,
            partition,
            arguments,
            0,
        );
    }
    commands.barrier(0x800, 0x800, 0x40, 0x60);
    let set = descriptor_set(
        commands,
        c,
        infos,
        accelerations,
        Some(&storage.pairs[0].allocation),
        args,
        None,
    )?;
    scan::dispatch(
        commands,
        c,
        kernels.handles[19],
        set,
        n,
        uniforms,
        partition,
        arguments,
        0,
    );
    Ok(())
}
