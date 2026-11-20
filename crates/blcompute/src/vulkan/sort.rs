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
    pub(super) histogram_count: u32,
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
pub(super) fn record(
    commands: &mut Commands,
    c: &Compiled,
    stage: usize,
    s: &blanguage::ir::Sorting,
    data: &impl std::ops::Index<usize, Output = GpuDataset>,
    storage: &Storage,
    arguments: Option<&PagedBuffer>,
    infos: &[VkDescriptorBufferInfo],
    accelerations: &[Acceleration],
    uniforms: &mut [u32],
    partition: NonZeroU32,
) -> Result<()> {
    let kernels = c.sorts[stage].as_ref().unwrap();
    let n = data[s.source].count;
    let args = arguments.map(PagedBuffer::descriptor);
    for key in (0..s.keys.len()).rev() {
        let bindings = [Some((s.target, storage.pairs[0].descriptor()))];
        let set = descriptor_set(
            commands,
            c,
            infos,
            &bindings,
            accelerations,
            None,
            args,
            None,
            None,
        )?;
        scan::dispatch(
            commands,
            c,
            if key + 1 == s.keys.len() {
                c.pipelines[stage]
            } else {
                kernels.handles[20 + key]
            },
            set,
            n,
            uniforms,
            partition,
            arguments,
            0,
        );
        for pass in 0..8 {
            commands.barrier(0x800, 0x800, 0x40, 0x60);
            let input = &storage.pairs[pass % 2];
            let target = &storage.pairs[(pass + 1) % 2];
            let bindings = [
                Some((s.source, input.descriptor())),
                Some((s.target, target.descriptor())),
            ];
            let set = descriptor_set(
                commands,
                c,
                infos,
                &bindings,
                accelerations,
                Some(storage.histogram.descriptor()),
                args,
                None,
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
                (storage.histogram_count, storage.histogram.descriptor()),
                storage.prefixes.descriptor(),
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
                infos,
                &bindings,
                accelerations,
                Some(storage.prefixes.descriptor()),
                args,
                None,
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
    }
    let set = descriptor_set(
        commands,
        c,
        infos,
        &[],
        accelerations,
        Some(storage.pairs[0].descriptor()),
        args,
        None,
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
