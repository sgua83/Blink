//! CPU producers reach their first consumer; actual memory and control effects keep their order.
use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct Pending {
    producers: BTreeMap<Reg, usize>,
    code: Vec<Option<Inst>>,
    reads: Vec<usize>,
    private_reads: Vec<(std::ops::Range<usize>, usize)>,
}
impl Pending {
    fn push(&mut self, inst: Inst, read: bool, private: Option<std::ops::Range<usize>>) {
        let id = self.code.len();
        for result in inst.results() {
            self.producers.insert(result, id);
        }
        self.code.push(Some(inst));
        if read {
            self.reads.push(id);
        }
        if let Some(range) = private {
            self.private_reads.push((range, id));
        }
    }
    fn emit(&mut self, id: usize, out: &mut Vec<Inst>) {
        let mut work = Vec::new();
        if self.code[id].is_some() {
            work.push((id, false));
        }
        while let Some((id, ready)) = work.pop() {
            if ready {
                if let Some(inst) = self.code[id].take() {
                    for result in inst.results() {
                        self.producers.remove(&result);
                    }
                    out.push(inst);
                }
            } else if let Some(inst) = &self.code[id] {
                work.push((id, true));
                for source in inst.operands().into_iter().rev() {
                    if let Some(&producer) = self.producers.get(&source) {
                        work.push((producer, false));
                    }
                }
            }
        }
    }
    fn consume(&mut self, reg: Reg, out: &mut Vec<Inst>) {
        if let Some(&id) = self.producers.get(&reg) {
            self.emit(id, out);
        }
    }
    fn flush_reads(&mut self, out: &mut Vec<Inst>) {
        for id in std::mem::take(&mut self.reads) {
            self.emit(id, out);
        }
    }
    fn flush_private(&mut self, write: std::ops::Range<usize>, out: &mut Vec<Inst>) {
        for (read, id) in std::mem::take(&mut self.private_reads) {
            if self.code[id].is_some() {
                if read.start < write.end && write.start < read.end {
                    self.emit(id, out);
                } else {
                    self.private_reads.push((read, id));
                }
            }
        }
    }
    fn flush(&mut self, out: &mut Vec<Inst>) {
        for id in 0..self.code.len() {
            self.emit(id, out);
        }
        self.producers.clear();
        self.code.clear();
        self.reads.clear();
        self.private_reads.clear();
    }
}

pub(super) fn place(code: &mut Vec<Inst>, writes: &[usize]) {
    let mut out = Vec::new();
    let mut pending = Pending::default();
    for mut inst in std::mem::take(code) {
        let read = matches!(&inst, Inst::Load { binding, .. } if !writes.contains(binding))
            || matches!(inst, Inst::ViewLoad { .. });
        let value = matches!(
            inst,
            Inst::Constant(..)
                | Inst::Index(_)
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
        let private = match &inst {
            Inst::LoadLocal(_, slot) => Some(*slot..*slot + 1),
            Inst::LoadLocalIndexed { base, len, .. } => Some(*base..*base + *len),
            _ => None,
        };
        if read || value || private.is_some() {
            pending.push(inst, read, private);
            continue;
        }
        match &mut inst {
            Inst::Uniform(..) => {}
            // Private slots have their own storage owner. Preserve every earlier
            // snapshot of the actual written slot or indexed span before its write.
            Inst::StoreLocal(slot, _) => {
                pending.flush_reads(&mut out);
                pending.flush_private(*slot..*slot + 1, &mut out);
            }
            Inst::StoreLocalIndexed { base, len, .. } => {
                pending.flush_reads(&mut out);
                pending.flush_private(*base..*base + *len, &mut out);
            }
            // A different dataset binding name is not an alias proof.
            Inst::Store { .. }
            | Inst::StoreIndexed { .. }
            | Inst::LoadIndexed { .. }
            | Inst::Load { .. } => pending.flush_reads(&mut out),
            // Calls, control and synchronization still close the whole region.
            _ => pending.flush(&mut out),
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
            pending.consume(reg, &mut out);
        }
        out.push(inst);
    }
    pending.flush(&mut out);
    *code = out;
}
