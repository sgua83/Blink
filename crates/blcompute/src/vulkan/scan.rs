//! 前缀扫描的全部层级留在设备上，提交拥有其临时数据直到最后一个消费者完成。
use super::*;
const GROUP_SHIFT: u32 = 6;
const GROUP_WIDTH: u32 = blanguage::spirv::SCAN_WIDTH;

pub(super) fn groups(count: u32) -> u32 {
    count.div_ceil(GROUP_WIDTH).max(1)
}

pub(super) fn levels(count: u32) -> usize {
    // Each six bits of count - 1 need one pass. The empty root still runs once.
    let bits = u32::BITS - count.saturating_sub(1).leading_zeros();
    bits.div_ceil(GROUP_SHIFT).max(1) as usize
}

fn length_at(count: u32, level: usize) -> u32 {
    // Repeated rounded divisions by 64 equal one rounded shift from the root.
    let shift = GROUP_SHIFT * level as u32;
    let round_up = (1u64 << shift) - 1;
    ((u64::from(count) + round_up) >> shift) as u32
}

pub(super) fn allocate(c: &Compiled, stage: usize, mut count: u32, words: usize) -> Result<Scratch> {
    let mut buffers = Vec::new();
    let level_count = levels(count);
    let legacy_summary = c.stages[stage].scan_terminal(count).is_none();
    for level in 0..level_count {
        count = groups(count);
        if level + 1 < level_count || legacy_summary {
            buffers.push(PagedBuffer::words(c.inner.clone(), count, words as u32)?);
        }
        if level + 1 < level_count {
            buffers.push(PagedBuffer::words(c.inner.clone(), count, words as u32)?);
        }
    }
    Ok(Scratch {
        buffers,
        arguments: None,
        sorting: None,
    })
}
pub(super) struct Descriptors([u64; 2 * blanguage::spirv::PREFIX_LEVELS - 1]);

impl Descriptors {
    pub(super) fn with_root(root: u64) -> Self {
        let mut sets = Self([0; 2 * blanguage::spirv::PREFIX_LEVELS - 1]);
        sets.0[0] = root;
        sets
    }
    pub(super) fn root(&self) -> u64 {
        self.0[0]
    }
}

pub(super) enum ScanRoot {
    Source(VkDescriptorBufferInfo),
    Set(u64),
}

pub(super) fn descriptors(
    writer: &mut DescriptorWriter<'_>,
    (source_binding, target_binding): (usize, usize),
    segmented: bool,
    count: u32,
    first: ScanRoot,
    root_target: VkDescriptorBufferInfo,
    temporary: &[PagedBuffer],
    arguments: Option<VkDescriptorBufferInfo>,
) -> Result<Descriptors> {
    let level_count = levels(count);
    let selection_buffer = writer.compiled.program.selection_buffer();
    let mut sets = Descriptors([0; 2 * blanguage::spirv::PREFIX_LEVELS - 1]);
    for level in 0..level_count {
        let source = if level == 0 {
            match first {
                ScanRoot::Source(info) => info,
                ScanRoot::Set(set) => {
                    sets.0[0] = set;
                    continue;
                }
            }
        } else {
            temporary[(level - 1) * 2].descriptor()
        };
        let target = if level == 0 {
            root_target
        } else {
            temporary[(level - 1) * 2 + 1].descriptor()
        };
        let auxiliary = temporary.get(level * 2).map(PagedBuffer::descriptor);
        let bindings = [
            (source_binding != target_binding)
                .then_some((source_binding, source)),
            Some((target_binding, target)),
        ];
        sets.0[level] = writer.set(
            &bindings,
            auxiliary,
            arguments,
            (target_binding == selection_buffer).then_some(target),
            (source_binding == target_binding && level > 0).then_some(source),
        )?;
    }
    for level in (0..level_count - 1).rev() {
        let source = temporary[level * 2 + 1].descriptor();
        let target = if level == 0 {
            root_target
        } else {
            temporary[(level - 1) * 2 + 1].descriptor()
        };
        let bindings = [
            (source_binding != target_binding).then_some((source_binding, source)),
            Some((target_binding, target)),
        ];
        sets.0[level_count + level] = writer.set(
            &bindings,
            if level == 0 && segmented {
                Some(temporary[0].descriptor())
            } else {
                None
            },
            arguments,
            (target_binding == selection_buffer).then_some(target),
            (source_binding == target_binding).then_some(source),
        )?;
    }
    Ok(sets)
}

pub(super) fn record(
    commands: &ComputeCommands<'_>,
    writer: &mut DescriptorWriter<'_>,
    c: &Compiled,
    stage: usize,
    kernels: [VkPipeline; 4],
    r: &blanguage::ir::Reduction,
    segmented: bool,
    source: (u32, VkDescriptorBufferInfo),
    root_target: VkDescriptorBufferInfo,
    temporary: &[PagedBuffer],
    arguments: Option<&PagedBuffer>,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    reused: bool,
) -> Result<()> {
    let sets = descriptors(
        writer, (r.source, r.target), segmented, source.0, ScanRoot::Source(source.1),
        root_target, temporary, arguments.map(PagedBuffer::descriptor),
    )?;
    commands.bind_set(sets.root());
    if arguments.is_some() {
        prefix::prepare(commands, c, stage, uniforms, partition, reused);
    }
    record_dispatches(
        commands, c, stage, kernels, source.0, &sets, arguments, uniforms, partition, 0,
    );
    Ok(())
}

/// The caller has already bound the root set.
pub(super) fn record_dispatches(
    commands: &ComputeCommands<'_>,
    c: &Compiled,
    stage: usize,
    kernels: [VkPipeline; 4],
    count: u32,
    sets: &Descriptors,
    arguments: Option<&PagedBuffer>,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    argument_base: usize,
) {
    let level_count = levels(count);
    for level in 0..level_count {
        if level > 0 {
            commands.barrier(0x800, 0x800, 0x40, 0x60);
            commands.bind_set(sets.0[level]);
        }
        dispatch(
            commands,
            c,
            scan_pipeline(c, stage, count, level, level_count, kernels),
            length_at(count, level),
            uniforms,
            partition,
            arguments,
            argument_base + level,
        );
    }
    for level in (0..level_count - 1).rev() {
        commands.barrier(0x800, 0x800, 0x40, 0x60);
        commands.bind_set(sets.0[level_count + level]);
        dispatch(
            commands,
            c,
            kernels[if level == 0 { 3 } else { 2 }],
            length_at(count, level),
            uniforms,
            partition,
            arguments,
            argument_base + level,
        );
    }
}
fn scan_pipeline(c: &Compiled, stage: usize, count: u32, level: usize, levels: usize, kernels: [VkPipeline; 4]) -> VkPipeline {
    if level + 1 == levels && let Some(TerminalPipeline::Alternate(pipeline)) = c.stages[stage].scan_terminal(count) {
        return pipeline;
    }
    kernels[usize::from(level != 0)]
}

pub(super) fn record_segmented_reduce(
    commands: &ComputeCommands<'_>,
    writer: &mut DescriptorWriter<'_>,
    c: &Compiled,
    stage: usize,
    r: &blanguage::ir::Reduction,
    count: u32,
    temporary: &[PagedBuffer],
    arguments: Option<&PagedBuffer>,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    reused: bool,
) -> Result<()> {
    let pipelines = &c.stages[stage];
    let kernels = [
        pipelines.main,
        pipelines.tail.unwrap(),
        pipelines.scan_add.unwrap(),
        pipelines.finish.unwrap(),
    ];
    let level_count = levels(count);
    for level in 0..level_count {
        if level > 0 {
            commands.barrier(0x800, 0x800, 0x40, 0x60);
        }
        let bindings = [
            (level > 0 && r.source != r.target)
                .then(|| (r.source, temporary[(level - 1) * 2].descriptor())),
            (level > 0)
                .then(|| (r.target, temporary[(level - 1) * 2 + 1].descriptor())),
        ];
        let set = writer.set(
            &bindings,
            temporary.get(level * 2).map(PagedBuffer::descriptor),
            arguments.map(PagedBuffer::descriptor),
            None,
            (r.source == r.target && level > 0).then(|| temporary[(level - 1) * 2].descriptor()),
        )?;
        commands.bind_set(set);
        if level == 0 && arguments.is_some() {
            prefix::prepare(commands, c, stage, uniforms, partition, reused);
        }
        dispatch(
            commands,
            c,
            scan_pipeline(c, stage, count, level, level_count, kernels),
            length_at(count, level),
            uniforms,
            partition,
            arguments,
            level,
        );
    }
    // 只回传摘要前缀；根域留给最后一次源数据遍历直接写段尾。
    for level in (1..level_count - 1).rev() {
        commands.barrier(0x800, 0x800, 0x40, 0x60);
        let bindings = [
            (r.source != r.target)
                .then(|| (r.source, temporary[level * 2 + 1].descriptor())),
            Some((r.target, temporary[(level - 1) * 2 + 1].descriptor())),
        ];
        let set = writer.set(
            &bindings,
            None,
            arguments.map(PagedBuffer::descriptor),
            None,
            (r.source == r.target).then_some(temporary[level * 2 + 1].descriptor()),
        )?;
        commands.bind_set(set);
        dispatch(
            commands,
            c,
            kernels[2],
            length_at(count, level),
            uniforms,
            partition,
            arguments,
            level,
        );
    }
    commands.barrier(0x800, 0x800, 0x40, 0x60);
    // At one level the root is still bound and also supplies the final summary.
    if level_count > 1 {
        let set = writer.set(
            &[],
            Some(temporary[1].descriptor()),
            arguments.map(PagedBuffer::descriptor),
            None,
            None,
        )?;
        commands.bind_set(set);
    }
    dispatch(
        commands,
        c,
        kernels[3],
        count,
        uniforms,
        partition,
        arguments,
        0,
    );
    Ok(())
}
pub(super) fn dispatch(
    commands: &ComputeCommands<'_>,
    c: &Compiled,
    pipeline: VkPipeline,
    count: u32,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    arguments: Option<&PagedBuffer>,
    level: usize,
) {
    if let Some(args) = arguments {
        prefix::dispatch(commands, c, pipeline, args, uniforms, level);
        return;
    }
    unsafe {
        let f = &c.inner.functions;
        commands.bind_pipeline(pipeline);
        let groups = groups(count);
        let batch =
            c.inner.properties.limits.maxComputeWorkGroupCount[0].min(partition.get().div_ceil(GROUP_WIDTH));
        let mut begin = 0;
        while begin < groups {
            let n = (groups - begin).min(batch);
            uniforms[0] = count;
            uniforms[1] = begin * GROUP_WIDTH;
            (f.vkCmdPushConstants)(
                commands.command,
                c.layout,
                0x20,
                0,
                c.program.uniform_bytes(),
                uniforms.as_ptr().cast(),
            );
            (f.vkCmdDispatch)(commands.command, n, 1, 1);
            begin += n;
        }
    }
}
