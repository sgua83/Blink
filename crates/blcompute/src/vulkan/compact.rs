//! 压缩筛选重用扫描的设备层级；完整前缀与原始记录分开驻留。
use super::*;
use blanguage::ir::Compaction;
pub(super) fn allocate(c: &Compiled, count: u32) -> Result<Scratch> {
    let mut scratch = scan::allocate(c, count, 1)?;
    scratch.buffers.push(PagedBuffer::words(c.inner.clone(), count, 1)?);
    Ok(scratch)
}
pub(super) fn record(
    commands: &Commands,
    writer: &mut DescriptorWriter<'_>,
    c: &Compiled,
    stage: usize,
    selection: &Compaction,
    data: &impl std::ops::Index<usize, Output = GpuDataset>,
    scratch: &Scratch,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    reused: bool,
) -> Result<()> {
    let prefix = scratch.buffers.last().unwrap().descriptor();
    let source = writer.binding(selection.flags);
    scan::record(
        commands,
        writer,
        c,
        stage,
        [
            c.pipelines[stage],
            c.tails[stage].unwrap(),
            c.scan_adds[stage].unwrap(),
            c.scan_adds[stage].unwrap(),
        ],
        &selection.scan(),
        false,
        (data[selection.flags].count, source),
        prefix,
        &scratch.buffers,
        scratch.arguments.as_ref(),
        uniforms,
        partition,
        reused,
    )?;
    commands.barrier(0x800, 0x800, 0x40, 0x60);
    let set = writer.set(
        &[],
        Some(prefix),
        scratch.arguments.as_ref().map(PagedBuffer::descriptor),
        None,
        None,
    )?;
    commands.bind_compute_set(c.layout, set);
    scan::dispatch(
        commands,
        c,
        c.finishes[stage].unwrap(),
        data[selection.source].count,
        uniforms,
        partition,
        scratch.arguments.as_ref(),
        0,
    );
    Ok(())
}
