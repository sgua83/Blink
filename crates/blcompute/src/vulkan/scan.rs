//! 前缀扫描的全部层级留在设备上，提交拥有其临时数据直到最后一个消费者完成。
use super::*;
const GROUP_SHIFT: u32 = 6;
const GROUP_WIDTH: u32 = 1 << GROUP_SHIFT;

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

pub(super) fn allocate(c: &Compiled, mut count: u32, words: usize) -> Result<Scratch> {
    let mut buffers = Vec::new();
    let level_count = levels(count);
    for level in 0..level_count {
        count = groups(count);
        buffers.push(PagedBuffer::words(c.inner.clone(), count, words as u32)?);
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
pub(super) fn record(
    commands: &mut Commands,
    c: &Compiled,
    kernels: [VkPipeline; 4],
    r: &blanguage::ir::Reduction,
    segmented: bool,
    source: (u32, VkDescriptorBufferInfo),
    root_target: VkDescriptorBufferInfo,
    temporary: &[PagedBuffer],
    arguments: Option<&PagedBuffer>,
    infos: &[VkDescriptorBufferInfo],
    accelerations: &[Acceleration],
    uniforms: &mut [u32],
    partition: NonZeroU32,
    argument_base: usize,
) -> Result<()> {
    let count = source.0;
    let level_count = levels(count);
    for level in 0..level_count {
        if level > 0 {
            commands.barrier(0x800, 0x800, 0x40, 0x60);
        }
        let source = if level == 0 {
            source.1
        } else {
            temporary[(level - 1) * 2].descriptor()
        };
        let target = if level == 0 {
            root_target
        } else {
            temporary[(level - 1) * 2 + 1].descriptor()
        };
        let auxiliary = &temporary[level * 2];
        let bindings = [
            (r.source != r.target && (level > 0 || r.target != c.program.selection_buffer()))
                .then_some((r.source, source)),
            Some((r.target, target)),
        ];
        let set = descriptor_set(
            commands,
            c,
            infos,
            &bindings,
            accelerations,
            Some((auxiliary).descriptor()),
            arguments.map(PagedBuffer::descriptor),
            (r.target == c.program.selection_buffer()).then_some(target),
            (r.source == r.target && level > 0).then_some(source),
        )?;
        let pipeline = kernels[usize::from(level != 0)];
        dispatch(
            commands,
            c,
            pipeline,
            set,
            length_at(count, level),
            uniforms,
            partition,
            arguments,
            argument_base + level,
        );
    }
    for level in (0..level_count - 1).rev() {
        commands.barrier(0x800, 0x800, 0x40, 0x60);
        let source = temporary[level * 2 + 1].descriptor();
        let target = if level == 0 {
            root_target
        } else {
            temporary[(level - 1) * 2 + 1].descriptor()
        };
        let bindings = [
            (r.source != r.target).then_some((r.source, source)),
            Some((r.target, target)),
        ];
        let set = descriptor_set(
            commands,
            c,
            infos,
            &bindings,
            accelerations,
            if level == 0 && segmented {
                Some(temporary[0].descriptor())
            } else {
                None
            },
            arguments.map(PagedBuffer::descriptor),
            (r.target == c.program.selection_buffer()).then_some(target),
            (r.source == r.target).then_some(source),
        )?;
        dispatch(
            commands,
            c,
            kernels[if level == 0 { 3 } else { 2 }],
            set,
            length_at(count, level),
            uniforms,
            partition,
            arguments,
            argument_base + level,
        );
    }
    Ok(())
}
pub(super) fn record_segmented_reduce(
    commands: &mut Commands,
    c: &Compiled,
    kernels: [VkPipeline; 4],
    r: &blanguage::ir::Reduction,
    count: u32,
    temporary: &[PagedBuffer],
    arguments: Option<&PagedBuffer>,
    infos: &[VkDescriptorBufferInfo],
    accelerations: &[Acceleration],
    uniforms: &mut [u32],
    partition: NonZeroU32,
) -> Result<()> {
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
        let set = descriptor_set(
            commands,
            c,
            infos,
            &bindings,
            accelerations,
            Some(temporary[level * 2].descriptor()),
            arguments.map(PagedBuffer::descriptor),
            None,
            (r.source == r.target && level > 0).then(|| temporary[(level - 1) * 2].descriptor()),
        )?;
        dispatch(
            commands,
            c,
            kernels[usize::from(level != 0)],
            set,
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
        let set = descriptor_set(
            commands,
            c,
            infos,
            &bindings,
            accelerations,
            None,
            arguments.map(PagedBuffer::descriptor),
            None,
            (r.source == r.target).then_some(temporary[level * 2 + 1].descriptor()),
        )?;
        dispatch(
            commands,
            c,
            kernels[2],
            set,
            length_at(count, level),
            uniforms,
            partition,
            arguments,
            level,
        );
    }
    commands.barrier(0x800, 0x800, 0x40, 0x60);
    let prefix = &temporary[usize::from(level_count > 1)];
    let set = descriptor_set(
        commands,
        c,
        infos,
        &[],
        accelerations,
        Some((prefix).descriptor()),
        arguments.map(PagedBuffer::descriptor),
        None,
        None,
    )?;
    dispatch(
        commands,
        c,
        kernels[3],
        set,
        count,
        uniforms,
        partition,
        arguments,
        0,
    );
    Ok(())
}
pub(super) fn dispatch(
    commands: &Commands,
    c: &Compiled,
    pipeline: VkPipeline,
    set: u64,
    count: u32,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    arguments: Option<&PagedBuffer>,
    level: usize,
) {
    if let Some(args) = arguments {
        prefix::dispatch(commands, c, pipeline, set, args, uniforms, level);
        return;
    }
    unsafe {
        let f = &c.inner.functions;
        (f.vkCmdBindPipeline)(commands.command, 1, pipeline);
        (f.vkCmdBindDescriptorSets)(commands.command, 1, c.layout, 0, 1, &set, 0, null());
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
