//! 叶范围函数直接累计各数值分量，经第五个参数写出本范围的部分结果。
use super::*;
use crate::builtin_types::ScalarEncoding;

#[repr(C)]
pub(super) struct ReductionRange {
    identity: u64,
    scalar: ScalarEncoding,
    source: u32,
    stride_offset: u32,
    source_word: u32,
    operation: u32,
    count: u32,
    offsets: [u32; 4],
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::reduce_range"]
    fn reduce_range(input: ReductionRange, output: CollectiveLeafOutput) -> CollectiveLeaf;
}

pub(super) fn reduction_operator(operator: CollectiveOperator) -> u32 {
    match operator {
        CollectiveOperator::Sum => 0,
        CollectiveOperator::Min => 1,
        CollectiveOperator::Max => 2,
        CollectiveOperator::BitAnd => 3,
        CollectiveOperator::BitOr => 4,
        CollectiveOperator::BitXor => 5,
    }
}

impl ReductionRange {
    pub(super) fn new(fields: &Reduction, ty: &Ty, operator: CollectiveOperator) -> Self {
        let scalar = ty.components[0].0;
        let identity = operator.identity(scalar);
        let mut offsets = [0; 4];
        for (slot, (_, offset)) in offsets.iter_mut().zip(&ty.components) {
            *slot = *offset;
        }
        Self {
            identity,
            scalar: scalar.into(),
            source: fields.source as u32 * BUFFER_BYTES,
            stride_offset: BUFFER_STRIDE,
            source_word: fields.source_word,
            operation: reduction_operator(operator),
            count: ty.components.len() as u32,
            offsets,
        }
    }
}

pub(super) fn emit(_program: &Program, reduction: &ReductionSpec) -> NativeCode {
    let input = ReductionRange::new(&reduction.fields, &reduction.ty, reduction.operator);
    emit_collective_leaf(256, |output| unsafe { reduce_range(input, output) })
}
