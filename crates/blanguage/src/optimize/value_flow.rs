//! B owns local value-flow decisions; Rust retains the original IR and origins.
use super::*;
use crate::ir::{dataflow::ResultRole, rewrite::{self, ResultOriginKind}};
use std::{ffi::c_void, ptr, sync::Arc};

mod host;

// Only pending origin facts for this invocation; SSA keys never enter retained metadata.
type Reuses = BTreeMap<Reg, Vec<RetainedUse>>;
enum RetainedUse {
    Expression(ExpressionReuse),
    LocalRead(OperationOrigin),
    IndexedRead(OperationOrigin),
    IndexedStoreRead { store: OperationOrigin, read: OperationOrigin },
}

#[derive(Default)]
struct Origins {
    expressions: Vec<ExpressionReuse>,
    local_reads: Vec<LocalReadForward>,
    indexed_stores: Vec<IndexedStoreReadForward>,
    indexed_reads: Vec<OperationOrigin>,
}
struct Retention<'a> {
    pending: &'a mut Reuses,
    origins: Origins,
}

unsafe extern "C" fn collect_origin(
    context: *mut c_void, reg: Reg, result_index: usize, result_count: usize, role: ResultRole,
) -> u32 {
    let state = unsafe { &mut *context.cast::<Retention<'_>>() };
    let Some(sources) = state.pending.remove(&reg) else { return 0; };
    let mut kinds = 0;
    for source in sources {
        let kind = match source {
            RetainedUse::Expression(source) => {
                state.origins.expressions.push(source);
                ResultOriginKind::Expression
            }
            RetainedUse::LocalRead(origin) => {
                state.origins.local_reads.push(LocalReadForward {
                    result_index, result_count, result: role.reuse(), origin,
                });
                ResultOriginKind::LocalRead
            }
            RetainedUse::IndexedStoreRead { store, read } => {
                state.origins.indexed_stores.push(IndexedStoreReadForward {
                    result_index, result_count, result: role.reuse(), store, read,
                });
                ResultOriginKind::IndexedStoreRead
            }
            RetainedUse::IndexedRead(origin) => {
                state.origins.indexed_reads.push(origin);
                ResultOriginKind::IndexedRead
            }
        };
        kinds |= 1 << kind as u32;
    }
    kinds
}

unsafe extern "C" fn attach_origin(context: *mut c_void, inst: *mut Inst, kind: ResultOriginKind) {
    let state = unsafe { &mut *context.cast::<Retention<'_>>() };
    let inst = unsafe { &mut *inst };
    let retained = OperationOrigin {
        source_site: inst.source_site.clone(),
        generated_origin: inst.generated_origin.take(),
    };
    let (reason, anchor) = match kind {
        ResultOriginKind::Expression => (
            GenerationReason::ExpressionReuse,
            GenerationAnchor::ExpressionReuse { retained, reused: std::mem::take(&mut state.origins.expressions) },
        ),
        ResultOriginKind::LocalRead => (
            GenerationReason::LocalReadForward,
            GenerationAnchor::LocalReadForward { retained, reads: std::mem::take(&mut state.origins.local_reads) },
        ),
        ResultOriginKind::IndexedStoreRead => (
            GenerationReason::IndexedStoreReadForward,
            GenerationAnchor::IndexedStoreReadForward { retained, reads: std::mem::take(&mut state.origins.indexed_stores) },
        ),
        ResultOriginKind::IndexedRead => (
            GenerationReason::IndexedReadReuse,
            GenerationAnchor::IndexedReadReuse { retained, reused: std::mem::take(&mut state.origins.indexed_reads) },
        ),
    };
    inst.generated_origin = Some(Arc::new(GeneratedOrigin { reason, trigger: Some(anchor) }));
}

unsafe extern "C" fn pending_origins(context: *mut c_void) -> u32 {
    u32::from(!unsafe { &*context.cast::<Retention<'_>>() }.pending.is_empty())
}

fn retain_reuses(code: &mut [Inst], reuses: &mut Reuses) {
    if reuses.is_empty() { return; }
    let mut state = Retention { pending: reuses, origins: Origins::default() };
    unsafe { rewrite::retain_result_origins(code, ptr::from_mut(&mut state).cast(),
        collect_origin, attach_origin, pending_origins); }
}
pub(super) fn run(code: &mut Vec<Inst>, registers: &[Scalar]) {
    run_code(code, registers, &mut []);
}
// Only the original entry snapshot, merged SSA aliases and pending origins for
// this one reduction. B selects the independent bodies and the final transfer.
#[derive(Default)]
pub(super) struct ReductionFlow {
    entry: host::Values,
    expressions: host::Expressions,
    aliases: host::Values,
    reuses: Reuses,
    forwarded: Vec<ReductionInputReadForward>,
}
impl ReductionFlow {
    pub(super) fn entry(&mut self, code: &mut Vec<Inst>, registers: &[Scalar]) {
        host::run(code, &mut self.entry, &mut self.expressions, registers, &mut self.reuses, true);
    }
    pub(super) fn seed(&mut self) { self.aliases = host::aliases_only(&self.entry); }
    pub(super) fn body(&mut self, code: &mut Vec<Inst>, registers: &[Scalar], take: bool) {
        let mut values = if take { std::mem::take(&mut self.entry) } else { self.entry.clone() };
        host::run(code, &mut values, &mut self.expressions, registers, &mut self.reuses, take);
        if take { self.expressions = host::Expressions::default(); }
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
    let mut expressions = host::Expressions::default();
    host::run(code, &mut values, &mut expressions, registers, &mut reuses, true);
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
