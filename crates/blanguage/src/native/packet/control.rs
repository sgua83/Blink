use super::*;

pub(super) fn visit(code: &[Inst], f: &mut impl FnMut(&Inst)) {
    for inst in code {
        f(inst);
        match inst {
            Inst::If { yes, no, .. } => {
                visit(yes, f);
                visit(no, f);
            }
            Inst::Loop {
                condition, body, ..
            } => {
                visit(condition, f);
                visit(body, f);
            }
            Inst::Scope(body) => visit(body, f),
            _ => {}
        }
    }
}

pub(super) fn uniform_registers(stage: &Stage) -> Vec<bool> {
    // A local is uniform only when every assignment is uniform, including loop backedges.
    // Start at the greatest fixed point so an induction cycle can retain its uniform seed.
    let mut registers = vec![true; stage.registers.len()];
    let mut locals = vec![true; stage.locals.len()];
    loop {
        let mut changed = false;
        visit(&stage.code, &mut |inst| {
            let uniform = match inst {
                Inst::Index(_) => false,
                Inst::Load { binding, index, .. } => {
                    !stage.writes.contains(binding) && registers[*index]
                }
                Inst::LoadLocal(_, slot) => locals[*slot],
                _ => inst.operands().iter().all(|r| registers[*r]),
            };
            for dst in inst.results() {
                if registers[dst] && !uniform {
                    registers[dst] = false;
                    changed = true;
                }
            }
            if let Inst::StoreLocal(slot, src) = inst
                && locals[*slot]
                && !registers[*src]
            {
                locals[*slot] = false;
                changed = true;
            }
        });
        if !changed {
            return registers;
        }
    }
}

pub(super) fn remove_address_indices(code: &mut Vec<Step>, indices: &BTreeSet<Reg>) {
    code.retain(|step| !matches!(step, Step::Index(r) if indices.contains(r)));
    for step in code {
        match step {
            Step::If { yes, no, .. } => {
                remove_address_indices(yes, indices);
                remove_address_indices(no, indices);
            }
            Step::Loop {
                condition, body, ..
            } => {
                remove_address_indices(condition, indices);
                remove_address_indices(body, indices);
            }
            Step::Scope(body) => remove_address_indices(body, indices),
            _ => {}
        }
    }
}
