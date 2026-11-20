//! 当前有效域的全部层级参数一次在设备产生，同一提交持有到最后一个消费者完成。
use super::*;
use blanguage::{ir::Operation, spirv::{DISPATCH_RECORD_BYTES, PREFIX_LEVELS, sort_histogram_count}};

pub(super) struct Preparation {
    pub(super) pipeline: VkPipeline,
    pub(super) packed: bool,
}

pub(super) const PREPARE_ENTRY: &CStr = c"prepare";

pub(super) fn entry(words: &[u32]) -> &'static CStr {
    let Some(mut instructions) = words.get(5..) else { return c"main" };
    while let Some(&header) = instructions.first() {
        let Some((instruction, rest)) = instructions.split_at_checked((header >> 16) as usize) else { break };
        match (header as u16, instruction) {
            // OpEntryPoint GLCompute with the NUL-terminated name "prepare".
            (15, [_, 5, _, 0x70657270, 0x00657261, ..]) => return PREPARE_ENTRY,
            (54, _) | (_, []) => break,
            _ => {}
        }
        instructions = rest;
    }
    c"main"
}

pub(super) fn continuation_slot(c: &Compiled, stage: usize) -> usize {
    if c.dispatches[stage].as_ref().unwrap().packed { 1 } else { PREFIX_LEVELS }
}

pub(super) fn allocate(c: &Compiled, stage: usize, count: u32) -> Result<PagedBuffer> {
    let packed = c.dispatches[stage].as_ref().unwrap().packed;
    let stage = &c.program.stages()[stage];
    let records = if stage.selection.is_some() {
        if packed { scan::levels(count).max(2) } else { PREFIX_LEVELS + 1 }
    } else {
        match stage.operation {
            Operation::Map => 1,
            Operation::StableSort(_) if packed => 1 + scan::levels(sort_histogram_count(count)),
            Operation::StableSort(_) => PREFIX_LEVELS * 2,
            _ if packed => scan::levels(count),
            _ => PREFIX_LEVELS,
        }
    };
    PagedBuffer::new(c.inner.clone(), 4, (records * (DISPATCH_RECORD_BYTES / 4)) as u32, 0x100, 4)
}

pub(super) fn prepare(
    commands: &Commands,
    c: &Compiled,
    stage: usize,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    reused: bool,
) {
    if reused {
        commands.barrier(0x802, 0x800, 0x61, 0x40);
    }
    unsafe {
        let f = &c.inner.functions;
        (f.vkCmdBindPipeline)(commands.command, 1, c.dispatches[stage].as_ref().unwrap().pipeline);
        uniforms[0] = c.inner.properties.limits.maxComputeWorkGroupCount[0].min(
            partition
                .get()
                .div_ceil(c.program.stages()[stage].workgroup_width()),
        );
        uniforms[1] = c.inner.properties.limits.maxComputeWorkGroupCount[1];
        (f.vkCmdPushConstants)(
            commands.command,
            c.layout,
            0x20,
            0,
            c.program.uniform_bytes(),
            uniforms.as_ptr().cast(),
        );
        (f.vkCmdDispatch)(commands.command, 1, 1, 1);
        commands.barrier(0x800, 0x802, 0x40, 0x21);
    }
}
pub(super) fn dispatch(
    commands: &Commands,
    c: &Compiled,
    pipeline: VkPipeline,
    args: &PagedBuffer,
    uniforms: &mut [u32],
    level: usize,
) {
    unsafe {
        let f = &c.inner.functions;
        (f.vkCmdBindPipeline)(commands.command, 1, pipeline);
        uniforms[0] = 0;
        uniforms[1] = (level * (DISPATCH_RECORD_BYTES / 4)) as u32;
        (f.vkCmdPushConstants)(
            commands.command,
            c.layout,
            0x20,
            0,
            c.program.uniform_bytes(),
            uniforms.as_ptr().cast(),
        );
        (f.vkCmdDispatchIndirect)(commands.command, args.pages[0].buffer, (level * DISPATCH_RECORD_BYTES) as u64);
    }
}
