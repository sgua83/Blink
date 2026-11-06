//! 整域模块的实际输入合同、公开布局及完成效果；不包含执行器或可变运行状态。
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
    pub write_regions: std::collections::BTreeMap<usize, std::collections::BTreeSet<(u32, u32)>>,
    pub uniforms: Vec<Uniform>,
    pub length_requirements: Vec<(usize, usize)>,
    pub element_requirements: Vec<ElementFact>,
    pub outputs: Vec<Output>,
    pub elements: std::collections::BTreeMap<usize, crate::check::Proof>,
}
pub(crate) fn symbol(name: &str, target: Target) -> String {
    format!(
        "{name}#domain{}",
        if target == Target::Cpu { "cpu" } else { "gpu" }
    )
}
super::codec::record!(Output {
    binding,
    domain,
    words
});
super::codec::record!(Interface {
    instance,
    target,
    buffers,
    write_regions,
    uniforms,
    length_requirements,
    element_requirements,
    outputs,
    elements
});
