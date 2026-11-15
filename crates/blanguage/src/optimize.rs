//! 仅在读写与跨元素依赖允许时融合同域阶段；phase 保留全域阶段边界。
use crate::ir::*;
use std::collections::{BTreeMap, BTreeSet};
mod invariant;
mod selection;
mod value_flow;
mod eliminate;
mod compaction;
mod pipeline;
pub(crate) use invariant::prepare_reduction;

#[repr(C)]
struct BindingSpan {
    data: *const usize,
    count: usize,
}

impl From<&[usize]> for BindingSpan {
    fn from(bindings: &[usize]) -> Self {
        Self { data: bindings.as_ptr(), count: bindings.len() }
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct FusionBlock {
    instructions: *const std::ffi::c_void,
    count: usize,
}

impl From<&[Inst]> for FusionBlock {
    fn from(code: &[Inst]) -> Self {
        Self { instructions: code.as_ptr().cast(), count: code.len() }
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::merge_bindings"]
    fn merge_bindings(output: *mut usize, left_count: usize, right: BindingSpan) -> usize;
}

fn union_bindings(left: &mut Vec<usize>, right: Vec<usize>) {
    if right.is_empty() {
        return;
    }
    if left.is_empty() {
        *left = right;
        return;
    }
    let left_count = left.len();
    let total = left_count + right.len();
    left.reserve(right.len());
    let output = left.as_mut_ptr();
    // B initializes the suffix of this allocation; move only the produced union.
    unsafe {
        let start = merge_bindings(output, left_count, right.as_slice().into());
        let count = total - start;
        output.add(start).copy_to(output, count);
        left.set_len(count);
    }
}

#[repr(C)]
struct StageWrite {
    binding: usize,
    start: u32,
    end: u32,
}

#[repr(C)]
struct StageGuarantee {
    binding: usize,
    start: u64,
    end: u64,
    retained: u64,
}

impl StageGuarantee {
    fn new(binding: usize, offset: u32, bytes: u32) -> Self {
        let start = u64::from(offset);
        Self { binding, start, end: start + u64::from(bytes), retained: 0 }
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::retain_stage_guarantees"]
    fn mark_stage_guarantees(writes: *mut StageWrite, write_count: usize,
        guarantees: *mut StageGuarantee, guarantee_count: usize);
}

fn retain_stage_guarantees(previous: &mut Stage, write_regions: &BTreeMap<usize, BTreeSet<(u32, u32)>>) {
    if write_regions.is_empty()
        || (previous.prefix_guarantees.is_empty() && previous.element_guarantees.is_empty()) {
        return;
    }
    let mut writes = write_regions.iter().flat_map(|(&binding, ranges)| {
        ranges.iter().map(move |&(start, end)| StageWrite { binding, start, end })
    }).collect::<Vec<_>>();
    let mut guarantees = previous.prefix_guarantees.iter()
        .map(|relation| StageGuarantee::new(relation.count.binding, relation.count.word, 4))
        .chain(previous.element_guarantees.iter().map(|fact| match fact {
            ElementFact::Range(range) => StageGuarantee::new(range.binding, range.offset, range.scalar.size()),
            ElementFact::Index { binding, offset, .. } => StageGuarantee::new(*binding, *offset, 4),
        })).collect::<Vec<_>>();
    // Both slices are borrowed only for this call. B preserves the order of the
    // guarantee rows, so the original Stage can retain its own payloads.
    unsafe {
        mark_stage_guarantees(writes.as_mut_ptr(), writes.len(), guarantees.as_mut_ptr(), guarantees.len());
    }
    let mut retained = guarantees.into_iter().map(|row| row.retained != 0);
    previous.prefix_guarantees.retain(|_| retained.next().unwrap());
    previous.element_guarantees.retain(|_| retained.next().unwrap());
}

pub fn optimize(program: &mut Program, worker: &crate::compiler::Worker) -> std::io::Result<()> {
    pipeline::run(program, worker)
}

pub(crate) fn value_code(value: &mut ValueCode) {
    pipeline::run_value(value);
}

fn compact_slots(
    code: &mut [Inst], registers: &mut Vec<Scalar>, locals: &mut BTreeMap<usize, Ty>,
    inputs: &mut [Reg], outputs: &mut [Reg],
) {
    compaction::run(code, registers, locals, inputs, outputs);
}

pub(crate) fn visit(code: &[Inst], f: &mut impl FnMut(&Inst)) {
    for i in code {
        f(i);
        match &(i).kind {
            InstKind::Scope(body) | InstKind::Cleanup{body,..} => visit(body, f),
            InstKind::If { yes, no, .. } => {
                visit(yes, f);
                visit(no, f);
            }
            InstKind::Loop {
                condition, body, ..
            } => {
                visit(condition, f);
                visit(body, f);
            }
            _ => {}
        }
    }
}
pub(crate) fn map(code: &mut [Inst], mapping: crate::ir::rewrite::Remap<'_>) {
    crate::ir::rewrite::map(code, mapping);
}

