//! where 的私有索引队列与间接参数沿原命令／完成点持有。
use super::*;
use blanguage::ir::Scalar;
pub(super) fn allocate(c: &Compiled, count: u32) -> Result<Scratch> {
    let mut out = compact::allocate(c, count)?;
    out.buffers.push(GpuDataset::new(
        c.inner.clone(),
        Ty::scalar(Scalar::U32),
        4,
        count,
    )?);
    Ok(out)
}
pub(super) fn record(
    commands: &mut Commands,
    c: &Compiled,
    stage: usize,
    data: &[GpuDataset],
    scratch: &Scratch,
    infos: &[VkDescriptorBufferInfo],
    accelerations: &[Acceleration],
    uniforms: &mut [u32],
    partition: NonZeroU32,
) -> Result<()> {
    let selected = c.program.stages()[stage].selection.unwrap();
    let root = &scratch.buffers[scratch.buffers.len() - 2].allocation;
    let indices = &scratch.buffers.last().unwrap().allocation;
    let args = scratch.arguments.as_ref().unwrap();
    scan::record(
        commands,
        c,
        [
            c.selectors[stage].unwrap(),
            c.tails[stage].unwrap(),
            c.scan_adds[stage].unwrap(),
            c.scan_adds[stage].unwrap(),
        ],
        &selected.scan(c.program.selection_buffer()),
        false,
        &data[c.program.stages()[stage].domain],
        root,
        &scratch.buffers,
        Some(args),
        infos,
        accelerations,
        uniforms,
        partition,
        0,
    )?;
    commands.barrier(0x800, 0x800, 0x40, 0x60);
    let set = descriptor_set(
        commands,
        c,
        infos,
        accelerations,
        Some(indices),
        Some(&args.allocation),
        Some(root),
    )?;
    prefix::dispatch(
        commands,
        c,
        c.finishes[stage].unwrap(),
        set,
        args,
        uniforms,
        0,
    );
    commands.barrier(0x800, 0x802, 0x40, 0x21);
    prefix::dispatch(
        commands,
        c,
        c.pipelines[stage],
        set,
        args,
        uniforms,
        blanguage::spirv::PREFIX_LEVELS,
    );
    Ok(())
}
