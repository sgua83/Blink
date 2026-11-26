//! B owns local value-flow decisions; Rust retains the original IR and origins.
use super::*;
use crate::ir::dataflow::ResultRole;
use std::{ffi::c_void, mem::MaybeUninit, ptr, sync::Arc};

mod host;

#[repr(u32)]
#[derive(Clone, Copy)]
enum ReuseKind { Expression, LocalRead, IndexedStoreRead, IndexedRead }

#[repr(C)]
struct OriginRow {
    result: Reg,
    kind: ReuseKind,
    pending: u32,
    role: ResultRole,
    paired: u32,
    read: MaybeUninit<OperationOrigin>,
    store: MaybeUninit<OperationOrigin>,
}

#[repr(C)]
struct ResultLocation { index: usize, count: usize, role: ResultRole }

#[derive(Default)]
struct Origins {
    expressions: Vec<ExpressionReuse>,
    local_reads: Vec<LocalReadForward>,
    indexed_stores: Vec<IndexedStoreReadForward>,
    indexed_reads: Vec<OperationOrigin>,
}
unsafe extern "C" fn collect_origin(context: *mut c_void, row: *mut OriginRow, result: ResultLocation) {
    let state = unsafe { &mut *context.cast::<Origins>() };
    let row = unsafe { &*row };
    let origin = unsafe { row.read.assume_init_read() };
    match row.kind {
        ReuseKind::Expression => state.expressions.push(ExpressionReuse {
            result: row.role.reuse(), paired_input: row.paired != 0, origin,
        }),
        ReuseKind::LocalRead => state.local_reads.push(LocalReadForward {
            result_index: result.index, result_count: result.count, result: result.role.reuse(), origin,
        }),
        ReuseKind::IndexedStoreRead => state.indexed_stores.push(IndexedStoreReadForward {
            result_index: result.index, result_count: result.count, result: result.role.reuse(),
            store: unsafe { row.store.assume_init_read() }, read: origin,
        }),
        ReuseKind::IndexedRead => state.indexed_reads.push(origin),
    }
}

unsafe extern "C" fn attach_origin(context: *mut c_void, inst: *mut Inst, kind: ReuseKind) {
    let origins = unsafe { &mut *context.cast::<Origins>() };
    let inst = unsafe { &mut *inst };
    let retained = OperationOrigin {
        source_site: inst.source_site.clone(),
        generated_origin: inst.generated_origin.take(),
    };
    let (reason, anchor) = match kind {
        ReuseKind::Expression => (
            GenerationReason::ExpressionReuse,
            GenerationAnchor::ExpressionReuse { retained, reused: std::mem::take(&mut origins.expressions) },
        ),
        ReuseKind::LocalRead => (
            GenerationReason::LocalReadForward,
            GenerationAnchor::LocalReadForward { retained, reads: std::mem::take(&mut origins.local_reads) },
        ),
        ReuseKind::IndexedStoreRead => (
            GenerationReason::IndexedStoreReadForward,
            GenerationAnchor::IndexedStoreReadForward { retained, reads: std::mem::take(&mut origins.indexed_stores) },
        ),
        ReuseKind::IndexedRead => (
            GenerationReason::IndexedReadReuse,
            GenerationAnchor::IndexedReadReuse { retained, reused: std::mem::take(&mut origins.indexed_reads) },
        ),
    };
    inst.generated_origin = Some(Arc::new(GeneratedOrigin { reason, trigger: Some(anchor) }));
}

fn retain_reuses(code: &mut [Inst], reuses: &mut host::Reuses) {
    let mut origins = Origins::default();
    reuses.retain(code, ptr::from_mut(&mut origins).cast(), collect_origin, attach_origin);
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
    reuses: host::Reuses,
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
        if take {
            self.expressions = host::Expressions::default();
            self.reuses.order();
        }
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

unsafe extern "C" fn accept_input(context: *mut c_void, row: *mut OriginRow) {
    let origin = unsafe { (*row).read.assume_init_read() };
    unsafe { &mut *context.cast::<Vec<OperationOrigin>>() }.push(origin);
}

fn input_reads(reg: Reg, reuses: &mut host::Reuses) -> Vec<OperationOrigin> {
    let mut reads = Vec::new();
    reuses.input(reg, ptr::from_mut(&mut reads).cast(), accept_input);
    reads
}
fn run_code(code: &mut Vec<Inst>, registers: &[Scalar], outputs: &mut [Reg]) -> host::Reuses {
    let mut values = host::Values::default();
    let mut reuses = host::Reuses::default();
    host::run(code, &mut values, &mut host::Expressions::default(), registers, &mut reuses, true);
    reuses.order();
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
