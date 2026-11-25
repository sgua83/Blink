//! 仅在读写与跨元素依赖允许时融合同域阶段；phase 保留全域阶段边界。
use crate::ir::*;
use std::{collections::{BTreeMap, BTreeSet}, ffi::c_void, ptr};
mod invariant;
mod selection;
mod value_flow;
mod eliminate;
mod compaction;
mod pipeline;
pub(crate) use invariant::kernel as reduction_kernel;

#[repr(C)]
struct BlockSpan { data: *mut c_void, count: usize, stride: usize }

unsafe extern "C" fn instruction_span(code: *mut Vec<Inst>) -> BlockSpan {
    let code = unsafe { &mut *code };
    BlockSpan { data: code.as_mut_ptr().cast(), count: code.len(), stride: std::mem::size_of::<Inst>() }
}

unsafe extern "C" fn swap_instructions(first: *mut Inst, second: *mut Inst) {
    unsafe { ptr::swap(first, second) };
}

#[repr(C)]
struct InstructionStorage {
    first_code: *mut Vec<Inst>,
    second_code: *mut Vec<Inst>,
    payload: *mut c_void,
    input: *const c_void,
}
// Only exclusive addresses of original owning fields. B has already consumed
// the immutable scalar/tag projection before requesting these physical owners.
unsafe extern "C" fn instruction_storage(instruction: *mut Inst) -> InstructionStorage {
    let kind = unsafe { &mut (*instruction).kind };
    let mut fields = InstructionStorage {
        first_code: ptr::null_mut(), second_code: ptr::null_mut(),
        payload: ptr::null_mut(), input: ptr::null(),
    };
    match kind {
        InstKind::Scope(body) => fields.first_code = body,
        InstKind::If { yes, no, .. } => { fields.first_code = yes; fields.second_code = no; }
        InstKind::Loop { condition, test, body } => {
            fields.first_code = condition;
            fields.second_code = body;
            fields.payload = ptr::from_mut(test).cast();
        }
        InstKind::IntegerDivide { result, divisor, .. } => {
            fields.payload = ptr::from_mut(result).cast();
            fields.input = ptr::from_ref(divisor).cast();
        }
        InstKind::TileCollective(collective) => fields.payload = (collective as *mut TileCollective).cast(),
        _ => {}
    }
    fields
}
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
struct WriteEnd {
    end: u32,
    present: u32,
}

struct WriteCursor<'a> {
    regions: &'a BTreeMap<usize, BTreeSet<(u32, u32)>>,
    current: Option<std::collections::btree_set::Range<'a, (u32, u32)>>,
}

unsafe extern "C" fn seek_writes(context: *mut c_void, binding: usize, end: u64) -> WriteEnd {
    let cursor = unsafe { &mut *context.cast::<WriteCursor<'_>>() };
    cursor.current = cursor.regions.get(&binding).map(|regions| match u32::try_from(end) {
        Ok(end) => regions.range(..(end, 0)),
        Err(_) => regions.range(..),
    });
    unsafe { next_write(context) }
}

unsafe extern "C" fn next_write(context: *mut c_void) -> WriteEnd {
    let cursor = unsafe { &mut *context.cast::<WriteCursor<'_>>() };
    match cursor.current.as_mut().and_then(|regions| regions.next_back()) {
        Some(&(_, end)) => WriteEnd { end, present: 1 },
        None => WriteEnd { end: 0, present: 0 },
    }
}

#[repr(C)]
struct StageGuarantee {
    binding: usize,
    start: u64,
    end: u64,
}

impl StageGuarantee {
    fn new(binding: usize, offset: u32, bytes: u32) -> Self {
        let start = u64::from(offset);
        Self { binding, start, end: start + u64::from(bytes) }
    }
}

#[allow(dead_code)] // Passed by bootstrap.stage_fusion.
#[repr(u32)]
enum GuaranteeKind { Prefix, Element }

#[repr(C)]
struct GuaranteeCounts { prefix: usize, element: usize }

#[repr(C)]
struct GuaranteeHost {
    context: *mut c_void,

    field: unsafe extern "C" fn(*const c_void, GuaranteeKind, usize) -> StageGuarantee,
    swap_slots: unsafe extern "C" fn(*mut c_void, GuaranteeKind, usize, usize),
    seek: unsafe extern "C" fn(*mut c_void, usize, u64) -> WriteEnd,
    next: unsafe extern "C" fn(*mut c_void) -> WriteEnd,
}

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::retain_stage_guarantees"]
    fn compact_stage_guarantees(
        writes: *mut c_void, counts: GuaranteeCounts, host: GuaranteeHost,
    ) -> GuaranteeCounts;
}

unsafe extern "C" fn guarantee_field(context: *const c_void, kind: GuaranteeKind, index: usize) -> StageGuarantee {
    let stage = unsafe { &*context.cast::<Stage>() };
    match kind {
        GuaranteeKind::Prefix => {
            let count = stage.prefix_guarantees[index].count;
            StageGuarantee::new(count.binding, count.word, 4)
        }
        GuaranteeKind::Element => match &stage.element_guarantees[index] {
            ElementFact::Range(range) => StageGuarantee::new(range.binding, range.offset, range.scalar.size()),
            ElementFact::Index { binding, offset, .. } => StageGuarantee::new(*binding, *offset, 4),
        },
    }
}

unsafe extern "C" fn swap_guarantees(context: *mut c_void, kind: GuaranteeKind, left: usize, right: usize) {
    let stage = unsafe { &mut *context.cast::<Stage>() };
    match kind {
        GuaranteeKind::Prefix => stage.prefix_guarantees.swap(left, right),
        GuaranteeKind::Element => stage.element_guarantees.swap(left, right),
    }
}

fn retain_stage_guarantees(previous: &mut Stage, write_regions: &BTreeMap<usize, BTreeSet<(u32, u32)>>) {
    if write_regions.is_empty()
        || (previous.prefix_guarantees.is_empty() && previous.element_guarantees.is_empty()) {
        return;
    }
    let mut writes = WriteCursor { regions: write_regions, current: None };
    let counts = GuaranteeCounts {
        prefix: previous.prefix_guarantees.len(), element: previous.element_guarantees.len(),
    };
    let host = GuaranteeHost {
        context: ptr::from_mut(previous).cast(), field: guarantee_field, swap_slots: swap_guarantees,
        seek: seek_writes, next: next_write,
    };
    let retained = unsafe { compact_stage_guarantees(ptr::from_mut(&mut writes).cast(), counts, host) };
    previous.prefix_guarantees.truncate(retained.prefix);
    previous.element_guarantees.truncate(retained.element);
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

