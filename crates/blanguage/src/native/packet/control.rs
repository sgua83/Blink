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
    let mut analysis = Uniformity {
        stage,
        definitions: vec![None; stage.registers.len()],
    };
    analysis.block(
        &stage.code,
        State(vec![false; stage.registers.len() + stage.locals.last_key_value().map_or(0, |(end, _)| *end)]),
        true,
    );
    analysis
        .definitions
        .into_iter()
        .map(|uniform| uniform.unwrap_or(false))
        .collect()
}

#[derive(Clone, PartialEq, Eq)]
struct State(Vec<bool>);
impl State {
    fn intersect(&mut self, other: &Self) {
        for (value, other) in self.0.iter_mut().zip(&other.0) {
            *value &= *other;
        }
    }
}
fn merge(target: &mut Option<State>, other: Option<State>) {
    if let Some(other) = other {
        if let Some(target) = target {
            target.intersect(&other);
        } else {
            *target = Some(other);
        }
    }
}

#[derive(Default)]
struct Flow {
    next: Option<State>,
    breaks: Option<State>,
    continues: Option<State>,
    exits: Option<State>,
}
impl Flow {
    fn join(&mut self, other: Self) {
        merge(&mut self.next, other.next);
        merge(&mut self.breaks, other.breaks);
        merge(&mut self.continues, other.continues);
        merge(&mut self.exits, other.exits);
    }
}
struct Uniformity<'a> {
    stage: &'a Stage,
    definitions: Vec<Option<bool>>,
}
impl Uniformity<'_> {
    fn block(&mut self, code: &[Inst], state: State, record: bool) -> Flow {
        let mut flow = Flow {
            next: Some(state),
            ..Flow::default()
        };
        for inst in code {
            let Some(mut state) = flow.next.take() else {
                break;
            };
            let step = match inst {
                Inst::If { yes, no, .. } => {
                    let mut yes = self.block(yes, state.clone(), record);
                    yes.join(self.block(no, state, record));
                    yes
                }
                Inst::Loop {
                    condition, body, ..
                } => self.loop_flow(condition, body, state, record),
                Inst::Scope(body) => {
                    let mut scope = self.block(body, state, record);
                    merge(&mut scope.next, scope.exits.take());
                    scope
                }
                Inst::Break => Flow {
                    breaks: Some(state),
                    ..Flow::default()
                },
                Inst::Continue => Flow {
                    continues: Some(state),
                    ..Flow::default()
                },
                Inst::Exit => Flow {
                    exits: Some(state),
                    ..Flow::default()
                },
                _ => {
                    let uniform = match inst {
                        Inst::Index(_) => false,
                        Inst::Load { binding, index, .. } => {
                            !self.stage.writes.contains(binding) && state.0[*index]
                        }
                        Inst::LoadLocal(_, slot) => state.0[self.stage.registers.len() + slot],
                        _ => inst.all_operands(&mut |reg| state.0[reg]),
                    };
                    for dst in inst.results() {
                        state.0[dst] = uniform;
                        if record {
                            self.definitions[dst] = Some(
                                self.definitions[dst].is_none_or(|previous| previous) && uniform,
                            );
                        }
                    }
                    if let Inst::StoreLocal(slot, src) = inst {
                        state.0[self.stage.registers.len() + slot] = state.0[*src];
                    }
                    Flow {
                        next: Some(state),
                        ..Flow::default()
                    }
                }
            };
            flow.join(step);
        }
        flow
    }

    fn loop_flow(&mut self, condition: &[Inst], body: &[Inst], entry: State, record: bool) -> Flow {
        // Only paths that really reenter this loop constrain its header.
        // Break and scope exits carry their own facts to their actual consumers.
        let mut header = entry.clone();
        loop {
            let mut condition_flow = self.block(condition, header.clone(), false);
            let mut body_flow = condition_flow
                .next
                .clone()
                .map(|state| self.block(body, state, false))
                .unwrap_or_default();
            let mut back = body_flow.next.take();
            merge(&mut back, body_flow.continues.take());
            let mut next = entry.clone();
            if let Some(back) = back {
                next.intersect(&back);
            }
            if next == header {
                if !record {
                    merge(&mut condition_flow.next, body_flow.breaks.take());
                    merge(&mut condition_flow.exits, body_flow.exits.take());
                    return condition_flow;
                }
                break;
            }
            header = next;
        }
        // Record definitions only after the loop facts are stable, not during speculation.
        let mut condition_flow = self.block(condition, header, record);
        let mut body_flow = condition_flow
            .next
            .clone()
            .map(|state| self.block(body, state, record))
            .unwrap_or_default();
        merge(&mut condition_flow.next, body_flow.breaks.take());
        merge(&mut condition_flow.exits, body_flow.exits.take());
        condition_flow
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
