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
pub(super) fn with_prelude(
    prelude: &mut Vec<Inst>, bodies: &mut [&mut Vec<Inst>], registers: &[Scalar],
    inputs: &[(ReductionInputRole, &[Reg])],
) -> (impl Fn(Reg) -> Reg + use<>, Vec<ReductionInputReadForward>) {
    let mut entry = host::Values::default();
    let mut reuses = Reuses::new();
    host::run(prelude, &mut entry, registers, &mut reuses);
    let mut aliases = host::aliases_only(&entry);
    for body in bodies.iter_mut() {
        let mut values = entry.clone();
        host::run(body, &mut values, registers, &mut reuses);
        host::extend_aliases(&mut aliases, values);
    }
    retain_reuses(prelude, &mut reuses);
    for body in bodies { retain_reuses(body, &mut reuses); }
    let mut forwarded = Vec::new();
    for (role, registers) in inputs {
        for (component_index, reg) in registers.iter().enumerate() {
            let reads = input_reads(*reg, &mut reuses);
            if !reads.is_empty() { forwarded.push(ReductionInputReadForward { role: *role, component_index, reads }); }
        }
    }
    (move |reg| host::root(&aliases, reg), forwarded)
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
