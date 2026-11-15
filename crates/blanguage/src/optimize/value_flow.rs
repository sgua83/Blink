//! B owns local value-flow decisions; Rust retains the original IR and origins.
use super::*;
mod host;

#[repr(C)]
struct LocalValue {
    slot: usize,
    value: Reg,
}

type LocalValues = Vec<LocalValue>;

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::intersect_local_values"]
    fn intersect_local_values(left: *mut LocalValue, left_count: usize,
        right: *const LocalValue, right_count: usize) -> usize;
}
// Only pending origin facts for this invocation; SSA keys never enter retained metadata.
type Reuses = BTreeMap<Reg, Vec<RetainedUse>>;
enum RetainedUse {
    Expression(ExpressionReuse),
    LocalRead(OperationOrigin),
    IndexedRead(OperationOrigin),
    IndexedStoreRead { store: OperationOrigin, read: OperationOrigin },
}

fn retain_reuses(code: &mut [Inst], reuses: &mut Reuses) {
    for inst in code {
        let mut reused = Vec::new();
        let mut reads = Vec::new();
        let mut indexed = Vec::new();
        let mut indexed_reads = Vec::new();
        let result_count = inst.results().count();
        for (result_index, reg) in inst.results().enumerate() {
            let result = match &inst.kind {
                InstKind::IntegerDivide { result, .. } => match result {
                    IntegerDivisionResult::Quotient(_) => ReuseResult::Quotient,
                    IntegerDivisionResult::Remainder(_) => ReuseResult::Remainder,
                    IntegerDivisionResult::Both { quotient, .. } if *quotient == reg => ReuseResult::Quotient,
                    IntegerDivisionResult::Both { .. } => ReuseResult::Remainder,
                },
                _ => ReuseResult::Value,
            };
            for source in reuses.remove(&reg).into_iter().flatten() {
                match source {
                    RetainedUse::Expression(source) => reused.push(source),
                    RetainedUse::LocalRead(origin) => reads.push(LocalReadForward { result_index, result_count, result, origin }),
                    RetainedUse::IndexedRead(origin) => indexed_reads.push(origin),
                    RetainedUse::IndexedStoreRead { store, read } => indexed.push(IndexedStoreReadForward { result_index, result_count, result, store, read }),
                }
            }
        }
        if !reused.is_empty() {
            let retained = OperationOrigin::from_instruction(inst);
            inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::ExpressionReuse,
                trigger: Some(GenerationAnchor::ExpressionReuse { retained, reused }),
            }));
        }
        if !reads.is_empty() {
            let retained = OperationOrigin::from_instruction(inst);
            inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::LocalReadForward,
                trigger: Some(GenerationAnchor::LocalReadForward { retained, reads }),
            }));
        }
        if !indexed.is_empty() {
            let retained = OperationOrigin::from_instruction(inst);
            inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::IndexedStoreReadForward,
                trigger: Some(GenerationAnchor::IndexedStoreReadForward { retained, reads: indexed }),
            }));
        }
        if !indexed_reads.is_empty() {
            let retained = OperationOrigin::from_instruction(inst);
            inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::IndexedReadReuse,
                trigger: Some(GenerationAnchor::IndexedReadReuse { retained, reused: indexed_reads }),
            }));
        }
        match &mut inst.kind {
            InstKind::Scope(body) => retain_reuses(body, reuses),
            InstKind::If { yes, no, .. } => {
                retain_reuses(yes, reuses);
                retain_reuses(no, reuses);
            }
            InstKind::Loop { condition, body, .. } => {
                retain_reuses(condition, reuses);
                retain_reuses(body, reuses);
            }
            _ => {}
        }
    }
}
pub(super) fn run(code: &mut Vec<Inst>, registers: &[Scalar]) {
    run_code(code, registers, &mut []);
}
// Only the original entry snapshot, merged SSA aliases and pending origins for
// this one reduction. B selects the independent bodies and the final transfer.
#[derive(Default)]
pub(super) struct ReductionFlow {
    entry: host::Values,
    aliases: host::Values,
    reuses: Reuses,
    forwarded: Vec<ReductionInputReadForward>,
}
impl ReductionFlow {
    pub(super) fn entry(&mut self, code: &mut Vec<Inst>, registers: &[Scalar]) {
        host::run(code, &mut self.entry, registers, &mut self.reuses);
    }
    pub(super) fn seed(&mut self) { self.aliases = host::aliases_only(&self.entry); }
    pub(super) fn body(&mut self, code: &mut Vec<Inst>, registers: &[Scalar], take: bool) {
        let mut values = if take { std::mem::take(&mut self.entry) } else { self.entry.clone() };
        host::run(code, &mut values, registers, &mut self.reuses);
        host::extend_aliases(&mut self.aliases, values);
    }
    pub(super) fn retain(&mut self, code: &mut [Inst]) { retain_reuses(code, &mut self.reuses); }
    pub(super) fn forward(&mut self, role: ReductionInputRole, component_index: usize, reg: Reg) {
        let reads = input_reads(reg, &mut self.reuses);
        if !reads.is_empty() { self.forwarded.push(ReductionInputReadForward { role, component_index, reads }); }
    }
    pub(super) fn origin(&mut self, target: &mut Option<std::sync::Arc<GeneratedOrigin>>) {
        if !self.forwarded.is_empty() {
            let retained = target.take();
            *target = Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::LocalReadForward,
                trigger: Some(GenerationAnchor::ReductionInputForward {
                    retained, inputs: std::mem::take(&mut self.forwarded),
                }),
            }));
        }
    }
    pub(super) fn root(&self, reg: Reg) -> Reg { host::root(&self.aliases, reg) }
}

fn input_reads(reg: Reg, reuses: &mut Reuses) -> Vec<OperationOrigin> {
    reuses.remove(&reg).into_iter().flatten().filter_map(|source| match source {
        RetainedUse::LocalRead(origin) => Some(origin),
        RetainedUse::Expression(_) | RetainedUse::IndexedRead(_) | RetainedUse::IndexedStoreRead { .. } => None,
    }).collect()
}
fn run_code(code: &mut Vec<Inst>, registers: &[Scalar], outputs: &mut [Reg]) -> Reuses {
    let mut values = host::Values::default();
    let mut reuses = Reuses::new();
    host::run(code, &mut values, registers, &mut reuses);
    retain_reuses(code, &mut reuses);
    for r in outputs {
        *r = host::root(&values, *r);
    }
    reuses
}
pub(super) fn run_value(value: &mut ValueCode) {
    let mut reuses = run_code(&mut value.code, &value.registers, &mut value.outputs);
    let mut inputs = Vec::new();
    for (input_index, reg) in value.inputs.iter().enumerate() {
        let reads = input_reads(*reg, &mut reuses);
        if !reads.is_empty() { inputs.push(InputReadForward { input_index, reads }); }
    }
    if !inputs.is_empty() {
        let retained = value.generated_origin.take();
        value.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
            reason: GenerationReason::LocalReadForward,
            trigger: Some(GenerationAnchor::LocalInputForward { retained, inputs }),
        }));
    }
}
