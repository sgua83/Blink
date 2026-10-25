//! 只对可安全提前求值的计算／当前元素读取消除分支；外部访问与协作保留控制流。
use super::*;
fn branch_values(code: &[Inst]) -> BTreeMap<usize, Reg> {
    code.iter()
        .filter_map(|i| {
            if let Inst::StoreLocal(slot, value) = i {
                Some((*slot, *value))
            } else {
                None
            }
        })
        .collect()
}
fn speculatable(
    inst: &Inst,
    registers: &[Scalar],
    written: &BTreeSet<usize>,
    domain: usize,
    indices: &BTreeSet<Reg>,
) -> bool {
    match inst {
        Inst::Constant(..)
        | Inst::Index(_)
        | Inst::Uniform(..)
        | Inst::IntegerDivide {
            divisor: IntegerDivisor::Constant(_),
            ..
        }
        | Inst::Select { .. }
        | Inst::StoreLocal(..) => true,
        Inst::LoadLocal(_, slot) => !written.contains(slot),
        Inst::Load { binding, index, .. } => *binding == domain && indices.contains(index),
        Inst::Unary { op, .. } => *op != UnaryOp::FloatNegate,
        Inst::Binary { a, .. } => registers[*a] != Scalar::F32,
        Inst::Cast { dst, src } => registers[*dst] != Scalar::F32 && registers[*src] != Scalar::F32,
        _ => false,
    }
}
fn merge(
    inst: &Inst,
    following: &[Inst],
    registers: &[Scalar],
    domain: usize,
    indices: &BTreeSet<Reg>,
) -> Option<(Vec<Inst>, usize)> {
    let Inst::If { condition, yes, no } = inst else {
        return None;
    };
    let a = branch_values(yes);
    let b = branch_values(no);
    if a.is_empty() || a.keys().ne(b.keys()) {
        return None;
    }
    let written = a.keys().copied().collect();
    if !yes
        .iter()
        .chain(no)
        .all(|i| speculatable(i, registers, &written, domain, indices))
    {
        return None;
    }
    let mut loaded = BTreeSet::new();
    let mut results = Vec::new();
    for i in following.get(..a.len())? {
        let Inst::LoadLocal(dst, slot) = i else {
            return None;
        };
        if !a.contains_key(slot) || !loaded.insert(*slot) {
            return None;
        }
        results.push((*dst, *slot));
    }
    let mut code: Vec<_> = yes
        .iter()
        .chain(no)
        .filter(|i| !matches!(i, Inst::StoreLocal(..)))
        .cloned()
        .collect();
    for (dst, slot) in results {
        code.push(Inst::Select {
            dst,
            condition: *condition,
            a: a[&slot],
            b: b[&slot],
        });
        // 后续循环仍可能消费这个可变槽；值转发与 DCE 决定是否需要真实存储。
        code.push(Inst::StoreLocal(slot, dst));
    }
    Some((code, a.len()))
}
pub(super) fn run(code: &mut Vec<Inst>, registers: &[Scalar], domain: usize) -> bool {
    let mut indices = BTreeSet::new();
    visit(code, &mut |i| {
        if let Inst::Index(r) = i {
            indices.insert(*r);
        }
    });
    block(code, registers, domain, &indices)
}
fn block(
    code: &mut Vec<Inst>,
    registers: &[Scalar],
    domain: usize,
    indices: &BTreeSet<Reg>,
) -> bool {
    let mut changed = false;
    let mut source = std::mem::take(code).into_iter();
    while let Some(mut inst) = source.next() {
        match &mut inst {
            Inst::Scope(body) => changed |= block(body, registers, domain, indices),
            Inst::If { yes, no, .. } => {
                changed |= block(yes, registers, domain, indices);
                changed |= block(no, registers, domain, indices);
            }
            Inst::Loop {
                condition, body, ..
            } => {
                changed |= block(condition, registers, domain, indices);
                changed |= block(body, registers, domain, indices);
            }
            _ => {}
        }
        if let Some((converted, count)) =
            merge(&inst, source.as_slice(), registers, domain, indices)
        {
            code.extend(converted);
            for _ in 0..count {
                source.next();
            }
            changed = true;
        } else {
            code.push(inst);
        }
    }
    changed
}
