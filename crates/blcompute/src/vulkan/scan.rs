//! 前缀扫描的全部层级留在设备上，提交拥有其临时数据直到最后一个消费者完成。
use super::*;
use blanguage::ir::Scalar;
pub(super) fn allocate(c: &Compiled, mut count: u32, words: usize) -> Result<Scratch> {
    let ty = Ty::vector(Scalar::U32, words);
    let stride = ty.size;
    let mut buffers = Vec::new();
    loop {
        count = count.div_ceil(64).max(1);
        buffers.push(GpuDataset::new(c.inner.clone(), ty.clone(), stride, count)?);
        if count == 1 {
            break;
        }
        buffers.push(GpuDataset::new(c.inner.clone(), ty.clone(), stride, count)?);
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
    input: &GpuDataset,
    root_target: &Allocation,
    temporary: &[GpuDataset],
    arguments: Option<&GpuDataset>,
    infos: &[VkDescriptorBufferInfo],
    accelerations: &[Acceleration],
    uniforms: &mut [u32],
    partition: NonZeroU32,
    argument_base: usize,
) -> Result<()> {
    let mut lengths = Vec::new();
    let mut length = input.count;
    loop {
        lengths.push(length);
        if length <= 64 {
            break;
        }
        length = length.div_ceil(64);
    }
    for (level, &length) in lengths.iter().enumerate() {
        if level > 0 {
            commands.barrier(0x800, 0x800, 0x40, 0x60);
        }
        let source = if level == 0 {
            &input.allocation
        } else {
            &temporary[(level - 1) * 2].allocation
        };
        let target = if level == 0 {
            root_target
        } else {
            &temporary[(level - 1) * 2 + 1].allocation
        };
        let auxiliary = &temporary[level * 2].allocation;
        let mut pass_infos = infos.to_vec();
        pass_infos[r.source] = info(source);
        pass_infos[r.target] = info(target);
        let set = descriptor_set(
            commands,
            c,
            &pass_infos,
            accelerations,
            Some(auxiliary),
            arguments.map(|a| &a.allocation),
        )?;
        let pipeline = kernels[usize::from(level != 0)];
        dispatch(
            commands,
            c,
            pipeline,
            set,
            length,
            uniforms,
            partition,
            arguments,
            argument_base + level,
        );
    }
    for level in (0..lengths.len() - 1).rev() {
        commands.barrier(0x800, 0x800, 0x40, 0x60);
        let source = &temporary[level * 2 + 1].allocation;
        let target = if level == 0 {
            root_target
        } else {
            &temporary[(level - 1) * 2 + 1].allocation
        };
        let mut pass_infos = infos.to_vec();
        pass_infos[r.source] = info(source);
        pass_infos[r.target] = info(target);
        let set = descriptor_set(
            commands,
            c,
            &pass_infos,
            accelerations,
            if level == 0 && segmented {
                Some(&temporary[0].allocation)
            } else {
                None
            },
            arguments.map(|a| &a.allocation),
        )?;
        dispatch(
            commands,
            c,
            kernels[if level == 0 { 3 } else { 2 }],
            set,
            lengths[level],
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
    input: &GpuDataset,
    temporary: &[GpuDataset],
    arguments: Option<&GpuDataset>,
    infos: &[VkDescriptorBufferInfo],
    accelerations: &[Acceleration],
    uniforms: &mut [u32],
    partition: NonZeroU32,
) -> Result<()> {
    let mut lengths = Vec::new();
    let mut length = input.count;
    loop {
        lengths.push(length);
        if length <= 64 {
            break;
        }
        length = length.div_ceil(64);
    }
    for (level, &length) in lengths.iter().enumerate() {
        if level > 0 {
            commands.barrier(0x800, 0x800, 0x40, 0x60);
        }
        let mut pass_infos = infos.to_vec();
        if level > 0 {
            pass_infos[r.source] = info(&temporary[(level - 1) * 2].allocation);
            pass_infos[r.target] = info(&temporary[(level - 1) * 2 + 1].allocation);
        }
        let set = descriptor_set(
            commands,
            c,
            &pass_infos,
            accelerations,
            Some(&temporary[level * 2].allocation),
            arguments.map(|a| &a.allocation),
        )?;
        dispatch(
            commands,
            c,
            kernels[usize::from(level != 0)],
            set,
            length,
            uniforms,
            partition,
            arguments,
            level,
        );
    }
    // 只回传摘要前缀；根域留给最后一次源数据遍历直接写段尾。
    for level in (1..lengths.len() - 1).rev() {
        commands.barrier(0x800, 0x800, 0x40, 0x60);
        let mut pass_infos = infos.to_vec();
        pass_infos[r.source] = info(&temporary[level * 2 + 1].allocation);
        pass_infos[r.target] = info(&temporary[(level - 1) * 2 + 1].allocation);
        let set = descriptor_set(
            commands,
            c,
            &pass_infos,
            accelerations,
            None,
            arguments.map(|a| &a.allocation),
        )?;
        dispatch(
            commands,
            c,
            kernels[2],
            set,
            lengths[level],
            uniforms,
            partition,
            arguments,
            level,
        );
    }
    commands.barrier(0x800, 0x800, 0x40, 0x60);
    let prefix = &temporary[usize::from(input.count > 64)].allocation;
    let set = descriptor_set(
        commands,
        c,
        infos,
        accelerations,
        Some(prefix),
        arguments.map(|a| &a.allocation),
    )?;
    dispatch(
        commands,
        c,
        kernels[3],
        set,
        input.count,
        uniforms,
        partition,
        arguments,
        0,
    );
    Ok(())
}
fn info(a: &Allocation) -> VkDescriptorBufferInfo {
    VkDescriptorBufferInfo {
        buffer: a.buffer,
        offset: 0,
        range: a.size,
    }
}
pub(super) fn dispatch(
    commands: &Commands,
    c: &Compiled,
    pipeline: VkPipeline,
    set: u64,
    count: u32,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    arguments: Option<&GpuDataset>,
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
        let groups = count.div_ceil(64).max(1);
        let batch =
            c.inner.properties.limits.maxComputeWorkGroupCount[0].min(partition.get().div_ceil(64));
        let mut begin = 0;
        while begin < groups {
            let n = (groups - begin).min(batch);
            uniforms[0] = count;
            uniforms[1] = begin * 64;
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
