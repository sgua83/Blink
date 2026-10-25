//! 压缩筛选重用扫描的设备层级；完整前缀与原始记录分开驻留。
use super::*;
use blanguage::ir::{Compaction, Scalar};
pub(super) fn allocate(c: &Compiled, count: u32) -> Result<Scratch> {
    let mut scratch = scan::allocate(c, count, 1)?;
    scratch.buffers.push(GpuDataset::new(
        c.inner.clone(),
        Ty::scalar(Scalar::U32),
        4,
        count,
    )?);
    Ok(scratch)
}
pub(super) fn record(
    commands: &mut Commands,
    c: &Compiled,
    stage: usize,
    selection: &Compaction,
    data: &[GpuDataset],
    scratch: &Scratch,
    infos: &[VkDescriptorBufferInfo],
    accelerations: &[Acceleration],
    uniforms: &mut [u32],
    partition: NonZeroU32,
) -> Result<()> {
    let prefix = &scratch.buffers.last().unwrap().allocation;
    scan::record(
        commands,
        c,
        [
            c.pipelines[stage],
            c.tails[stage].unwrap(),
            c.scan_adds[stage].unwrap(),
            c.scan_adds[stage].unwrap(),
        ],
        &selection.scan(),
        false,
        &data[selection.flags],
        prefix,
        &scratch.buffers,
        scratch.arguments.as_ref(),
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
        Some(prefix),
        scratch.arguments.as_ref().map(|a| &a.allocation),
        None,
    )?;
    scan::dispatch(
        commands,
        c,
        c.finishes[stage].unwrap(),
        set,
        data[selection.source].count,
        uniforms,
        partition,
        scratch.arguments.as_ref(),
        0,
    );
    Ok(())
}
