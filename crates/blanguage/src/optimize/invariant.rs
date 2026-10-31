//! 封闭集合入口只计算一次统一纯值；不跨越用户分支、循环或资源访问。
use super::*;

fn extract(code: &mut Vec<Inst>, entry: &mut Vec<Inst>, uniform: &mut BTreeSet<Reg>) {
    code.retain(|inst| {
        let pure = matches!(
            inst,
            Inst::Constant(..)
                | Inst::Uniform(..)
                | Inst::Unary { .. }
                | Inst::Binary { .. }
                | Inst::Cast { .. }
                | Inst::Math { .. }
                | Inst::Select { .. }
                | Inst::IntegerDivide {
                    divisor: IntegerDivisor::Constant(_),
                    ..
                }
        );
        if pure && inst.all_operands(&mut |reg| uniform.contains(&reg)) {
            uniform.extend(inst.results());
            entry.push(inst.clone());
            false
        } else {
            true
        }
    });
}

pub(crate) fn prepare_reduction(kernel: &mut ReductionKernel) {
    let mut combine = kernel.stage.code.split_off(kernel.project_end);
    let mut project = kernel.stage.code.split_off(kernel.initialize_end);
    let mut identity = std::mem::take(&mut kernel.stage.code);
    let mut entry = Vec::new();
    let mut uniform = BTreeSet::new();
    for code in [&mut identity, &mut project, &mut combine] {
        extract(code, &mut entry, &mut uniform);
    }
    // 每个函数体只继承入口表达式，不能从未执行的另一个函数体取得局部值。
    let binding = value_flow::with_prelude(
        &mut entry,
        &mut [&mut identity, &mut project, &mut combine],
        &kernel.stage.registers,
    );
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
