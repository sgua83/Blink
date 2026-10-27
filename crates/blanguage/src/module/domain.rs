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
impl Interface {
    pub(crate) fn new(program: &Program, target: Target) -> Self {
        let mut outputs = Vec::new();
        let mut add = |binding, domain, first, ty: &Ty| {
            outputs.push(Output {
                binding,
                domain,
                words: ty
                    .components
                    .iter()
                    .map(|(_, offset)| first + offset / 4)
                    .collect(),
            });
        };
        for stage in &program.stages {
            match &stage.operation {
                Operation::Map if stage.prefix.is_none() && stage.selection.is_none() => {
                    for &binding in &stage.writes {
                        if program.buffers[binding].access == crate::ast::Access::Write {
                            add(binding, Some(stage.domain), 0, &program.buffers[binding].ty);
                        }
                    }
                }
                Operation::Reduce(v) => add(v.fields.target, None, v.fields.target_word, &v.ty),
                Operation::CustomReduce(v) => {
                    add(v.fields.target, None, v.fields.target_word, &v.ty)
                }
                Operation::ScanExclusive(v) if stage.prefix.is_none() => {
                    add(
                        v.values.target,
                        Some(v.values.source),
                        v.values.target_word,
                        &v.ty,
                    );
                }
                Operation::CustomScan(v) if stage.prefix.is_none() => {
                    add(
                        v.values.fields.target,
                        Some(v.values.fields.source),
                        v.values.fields.target_word,
                        &v.values.ty,
                    );
                }
                Operation::StableSort(v) if stage.prefix.is_none() => {
                    add(v.target, Some(v.source), 0, &program.buffers[v.target].ty);
                }
                Operation::Compact(v) => add(v.count, None, v.count_word, &Ty::scalar(Scalar::U32)),
                Operation::SegmentedReduce(v) => add(
                    v.count.binding,
                    None,
                    v.count.word,
                    &Ty::scalar(Scalar::U32),
                ),
                Operation::CustomSegmentedReduce(v) => add(
                    v.count.binding,
                    None,
                    v.count.word,
                    &Ty::scalar(Scalar::U32),
                ),
                _ => {}
            }
        }
        Self {
            instance: program.name.clone(),
            target,
            buffers: program.buffers.clone(),
            uniforms: program.uniforms.clone(),
            outputs,
        }
    }
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
