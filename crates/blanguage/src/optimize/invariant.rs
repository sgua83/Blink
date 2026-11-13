//! 封闭集合入口只计算一次统一纯值；不跨越用户分支、循环或资源访问。
use super::*;

use super::BindingSpan;
use ReductionUniformOperation as UniformOperation;

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::extract_uniform"]
    fn extract_uniform(operands: BindingSpan, results: BindingSpan, uniform: *mut u8) -> bool;
}

fn extract(code: &mut Vec<Inst>, entry: &mut Vec<Inst>, uniform: &mut [u8], partition: ReductionPartition) {
    entry.extend(code.extract_if(.., |inst| {
        let mut operands = [0; 3];
        let (operation, inputs): (_, &[Reg]) = match &inst.kind {
            InstKind::Constant(..) => (UniformOperation::Constant, &[]),
            InstKind::Uniform(..) => (UniformOperation::Uniform, &[]),
            InstKind::BufferStride(..) => (UniformOperation::BufferStride, &[]),
            InstKind::Unary { a, .. } => (UniformOperation::Unary, std::slice::from_ref(a)),
            InstKind::Binary { a, b, .. } => {
                operands[..2].copy_from_slice(&[*a, *b]);
                (UniformOperation::Binary, &operands[..2])
            }
            InstKind::Cast { src, .. } => (UniformOperation::Cast, std::slice::from_ref(src)),
            InstKind::Math { args, .. } => (UniformOperation::Math, args),
            InstKind::Select { condition, a, b, .. } => {
                operands = [*condition, *a, *b];
                (UniformOperation::Select, &operands)
            }
            InstKind::IntegerDivide { divisor: IntegerDivisor::Constant(_), src, .. } =>
                (UniformOperation::ConstantIntegerDivide, std::slice::from_ref(src)),
            _ => return false,
        };
        // The eligible instructions have one result, or up to two for division.
        let mut results = [0; 2];
        let mut count = 0;
        for reg in inst.results() {
            results[count] = reg;
            count += 1;
        }
        if !unsafe { extract_uniform(inputs.into(), results[..count].into(), uniform.as_mut_ptr()) } {
            return false;
        }
        inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
            reason: GenerationReason::ReductionUniformExtract,
            trigger: Some(GenerationAnchor::ReductionUniformExtract {
                retained: OperationOrigin::from_instruction(inst),
                partition,
                operation,
                uniform_operand_count: inputs.len(),
            }),
        }));
        true
    }));
}

pub(crate) fn prepare_reduction(kernel: &mut ReductionKernel) {
    let mut combine = kernel.stage.code.split_off(kernel.project_end);
    let mut project = kernel.stage.code.split_off(kernel.initialize_end);
    let mut identity = std::mem::take(&mut kernel.stage.code);
    let mut entry = Vec::new();
    let mut uniform = vec![0; kernel.stage.registers.len()];
    for (partition, code) in [
        (ReductionPartition::Identity, &mut identity),
        (ReductionPartition::Project, &mut project),
        (ReductionPartition::Combine, &mut combine),
    ] {
        extract(code, &mut entry, &mut uniform, partition);
    }
    // 每个函数体只继承入口表达式，不能从未执行的另一个函数体取得局部值。
    let (binding, inputs) = value_flow::with_prelude(
        &mut entry,
        &mut [&mut identity, &mut project, &mut combine],
        &kernel.stage.registers,
        &[
            (ReductionInputRole::Left, &kernel.left),
            (ReductionInputRole::Right, &kernel.right),
            (ReductionInputRole::Source, &kernel.source),
        ],
    );
    if !inputs.is_empty() {
        let retained = kernel.stage.generated_origin.take();
        kernel.stage.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
            reason: GenerationReason::LocalReadForward,
            trigger: Some(GenerationAnchor::ReductionInputForward { retained, inputs }),
        }));
    }
    for values in [
        &mut kernel.source,
        &mut kernel.projected,
        &mut kernel.initial,
        &mut kernel.left,
        &mut kernel.right,
        &mut kernel.result,
    ] {
        for r in values {
            *r = binding(*r);
        }
    }
    kernel.invariants = entry.iter().flat_map(Inst::results).collect();
    kernel.invariant_end = entry.len();
    kernel.initialize_end = entry.len() + identity.len();
    kernel.project_end = kernel.initialize_end + project.len();
    entry.extend(identity);
    entry.extend(project);
    entry.extend(combine);
    kernel.stage.code = entry;
}
