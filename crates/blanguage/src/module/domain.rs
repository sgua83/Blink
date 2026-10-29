//! 整域模块的公开布局及完成后的初始化效果；不包含执行器或可变运行状态。
use crate::{ast::Target, ir::*};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Output {
    pub binding: usize,
    pub domain: Option<usize>,
    pub words: Vec<u32>,
}
#[derive(Clone)]
pub(crate) struct Interface {
    pub instance: String,
    pub target: Target,
    pub buffers: Vec<Buffer>,
    pub uniforms: Vec<Uniform>,
    pub outputs: Vec<Output>,
}
pub(crate) fn symbol(name: &str, target: Target) -> String {
    format!(
        "{name}#domain{}",
        if target == Target::Cpu { "cpu" } else { "gpu" }
    )
}
super::codec::record!(Buffer {
    name,
    access,
    ty,
    stride,
    count
});
super::codec::record!(Uniform { name, ty, offset });
super::codec::record!(Output {
    binding,
    domain,
    words
});
super::codec::record!(Interface {
    instance,
    target,
    buffers,
    uniforms,
    outputs
});
