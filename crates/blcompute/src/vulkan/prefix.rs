//! 当前有效域的全部层级参数一次在设备产生，同一提交持有到最后一个消费者完成。
use super::*;
use blanguage::ir::{ExecutionStage as Stage, Operation};
pub(super) fn allocate(c: &Compiled, stage: &Stage) -> Result<PagedBuffer> {
    let levels = if stage.selection.is_some() {
        blanguage::spirv::PREFIX_LEVELS + 1
    } else {
        match stage.operation {
            Operation::Map => 1,
            Operation::StableSort(_) => blanguage::spirv::PREFIX_LEVELS * 2,
            _ => blanguage::spirv::PREFIX_LEVELS,
        }
    };
    PagedBuffer::new(c.inner.clone(), 4, (levels * 4) as u32, 0x100, 4)
}
pub(super) fn prepare(
    commands: &Commands,
    c: &Compiled,
    stage: usize,
    set: u64,
    uniforms: &mut [u32],
    partition: NonZeroU32,
    reused: bool,
) {
    if reused {
        commands.barrier(0x802, 0x800, 0x61, 0x40);
    }
    unsafe {
        let f = &c.inner.functions;
        (f.vkCmdBindDescriptorSets)(commands.command, 1, c.layout, 0, 1, &set, 0, null());
        (f.vkCmdBindPipeline)(commands.command, 1, c.dispatches[stage].unwrap());
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
    set: u64,
    args: &PagedBuffer,
    uniforms: &mut [u32],
    level: usize,
) {
    unsafe {
        let f = &c.inner.functions;
        (f.vkCmdBindPipeline)(commands.command, 1, pipeline);
        (f.vkCmdBindDescriptorSets)(commands.command, 1, c.layout, 0, 1, &set, 0, null());
        uniforms[0] = 0;
        uniforms[1] = (level * 4) as u32;
        (f.vkCmdPushConstants)(
            commands.command,
            c.layout,
            0x20,
            0,
            c.program.uniform_bytes(),
            uniforms.as_ptr().cast(),
        );
        (f.vkCmdDispatchIndirect)(commands.command, args.pages[0].buffer, (level * 16) as u64);
    }
}
