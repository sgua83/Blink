//! Stable reads stay in their original straight-line region and reach their first consumer.
use super::*;
use std::collections::BTreeMap;

fn consume(reg: Reg, pending: &mut BTreeMap<Reg, Inst>, out: &mut Vec<Inst>) {
    if let Some(inst) = pending.remove(&reg) {
        for source in inst.operands() {
            consume(source, pending, out);
        }
        out.push(inst);
    }
}

fn flush(order: &mut Vec<Reg>, pending: &mut BTreeMap<Reg, Inst>, out: &mut Vec<Inst>) {
    for reg in order.drain(..) {
        consume(reg, pending, out);
    }
}

pub(super) fn place(code: &mut Vec<Inst>, writes: &[usize]) {
    let mut out = Vec::new();
    let mut pending = BTreeMap::new();
    let mut order = Vec::new();
    for mut inst in std::mem::take(code) {
        let read = match &inst {
            Inst::Load { dst, binding, .. } if !writes.contains(binding) => Some(*dst),
            Inst::ViewLoad { dst, .. } => Some(*dst),
            _ => None,
        };
        if let Some(dst) = read {
            order.push(dst);
            pending.insert(dst, inst);
            continue;
        }
        // A different binding name is not an alias proof. Keep every write,
        // control, call and synchronization boundary after its original reads.
        let ordinary = matches!(
            inst,
            Inst::Constant(..)
                | Inst::Index(_)
                | Inst::Uniform(..)
                | Inst::LoadLocal(..)
                | Inst::Unary { .. }
                | Inst::Binary { .. }
                | Inst::IntegerDivide { .. }
                | Inst::Cast { .. }
                | Inst::Select { .. }
                | Inst::Math {
                    op: MathOp::Sqrt | MathOp::Abs | MathOp::Min | MathOp::Max,
                    ..
                }
        );
        if !ordinary {
            flush(&mut order, &mut pending, &mut out);
        }
        match &mut inst {
            Inst::If { yes, no, .. } => {
                place(yes, writes);
                place(no, writes);
            }
            Inst::Loop {
                condition, body, ..
            } => {
                place(condition, writes);
                place(body, writes);
            }
            Inst::Scope(body) => place(body, writes),
            _ => {}
        }
        for reg in inst.operands() {
            consume(reg, &mut pending, &mut out);
        }
        out.push(inst);
    }
    flush(&mut order, &mut pending, &mut out);
    *code = out;
}
