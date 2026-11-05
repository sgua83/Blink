//! 封闭集合入口只计算一次统一纯值；不跨越用户分支、循环或资源访问。
use super::*;

fn extract(code: &mut Vec<Inst>, entry: &mut Vec<Inst>, uniform: &mut BTreeSet<Reg>, partition: ReductionPartition) {
    code.retain(|inst| {
        let operation = match &inst.kind {
            InstKind::Constant(..) => Some(ReductionUniformOperation::Constant),
            InstKind::Uniform(..) => Some(ReductionUniformOperation::Uniform),
            InstKind::BufferStride(..) => Some(ReductionUniformOperation::BufferStride),
            InstKind::Unary { .. } => Some(ReductionUniformOperation::Unary),
            InstKind::Binary { .. } => Some(ReductionUniformOperation::Binary),
            InstKind::Cast { .. } => Some(ReductionUniformOperation::Cast),
            InstKind::Math { .. } => Some(ReductionUniformOperation::Math),
            InstKind::Select { .. } => Some(ReductionUniformOperation::Select),
            InstKind::IntegerDivide { divisor: IntegerDivisor::Constant(_), .. } => Some(ReductionUniformOperation::ConstantIntegerDivide),
            _ => None,
        };
        if let Some(operation) = operation {
            let mut uniform_operand_count = 0;
            if inst.all_operands(&mut |reg| { uniform_operand_count += 1; uniform.contains(&reg) }) {
                uniform.extend(inst.results());
                let mut extracted = inst.clone();
                extracted.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                    reason: GenerationReason::ReductionUniformExtract,
                    trigger: Some(GenerationAnchor::ReductionUniformExtract {
                        retained: OperationOrigin::from_instruction(inst), partition, operation, uniform_operand_count,
                    }),
                }));
                entry.push(extracted);
                return false;
            }
        }
        true
    });
}

pub(crate) fn prepare_reduction(kernel: &mut ReductionKernel) {
    let mut combine = kernel.stage.code.split_off(kernel.project_end);
    let mut project = kernel.stage.code.split_off(kernel.initialize_end);
    let mut identity = std::mem::take(&mut kernel.stage.code);
    let mut entry = Vec::new();
    let mut uniform = BTreeSet::new();
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
