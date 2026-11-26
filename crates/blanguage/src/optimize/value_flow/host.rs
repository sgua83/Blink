//! The original owning IR, primitive containers and source construction.
use super::*;
use super::super::{BlockSpan, InstructionStorage, instruction_span, instruction_storage, swap_instructions};
use crate::ir::dataflow::{REGISTER_HOST, RegisterHost, Span};
use std::collections::btree_map::Entry;

#[repr(C)]
struct LocalValue { slot: usize, value: Reg }

#[repr(C)]
#[derive(Default)]
struct RegisterRows { data: usize, count: usize, capacity: usize }

#[repr(C)]
#[derive(Default)]
struct Registers { aliases: RegisterRows, locals: RegisterRows }

impl Clone for Registers {
    fn clone(&self) -> Self {
        let mut copy = Self::default();
        unsafe { value_copy_registers(&mut copy, self, 1, resize_rows); }
        copy
    }
}

impl Drop for Registers {
    fn drop(&mut self) {
        unsafe { value_release_registers(self, resize_rows); }
    }
}

#[repr(C)]
#[derive(Clone, Default)]
pub(super) struct Values {
    registers: Registers,
    expressions: BTreeMap<ExpressionKey, Reg>,
    memory: BTreeMap<(usize, Reg, u32), (Reg, Option<OperationOrigin>)>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ExpressionKey { kind: u32, operation: u32, words: [u64; 3], arguments: Vec<Reg> }
#[repr(C)]
#[derive(Clone, Copy)]
struct Expression { kind: u32, operation: u32, words: [u64; 3], arguments: *const Reg, count: usize }
#[repr(C)]
#[derive(Clone, Copy)]
struct Optional { value: usize, present: u32 }
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Division { instruction: *mut Inst, quotient: Reg, remainder: Reg, mask: u32, present: u32 }
#[repr(C)]
struct Memory { value: Reg, origin: *const OperationOrigin, present: u32 }
#[repr(u32)]
enum ReuseKind { Expression, LocalRead, IndexedRead, IndexedStoreRead }

#[repr(C)]
struct Reuse { origin: *const OperationOrigin, kind: ReuseKind, role: ResultRole, paired: u32 }
#[repr(u32)]
#[derive(Clone, Copy)]
enum Map { Expressions, Memory }
#[repr(u32)]
enum StateOp { New, Clone, Drop, Move, Copy, Clear }
#[repr(u32)]
enum DivisionOp { Clear, Take, Put }
#[repr(u32)]
enum DivisionChoice { Quotient = 1, Remainder, Both }
#[repr(C)]
struct Path { rows: RegisterRows, reached: u32 }
#[repr(C)]
struct Flow { next: Path, exits: Path }
type ResizeRows = unsafe extern "C" fn(*mut LocalValue, usize, usize) -> *mut LocalValue;
struct Context<'a> {
    registers: &'a [Scalar], reuses: &'a mut Reuses,
    divisions: BTreeMap<(Reg, u64, u64), Division>,
}
#[repr(C)]
struct Host {
    values: RegisterHost,
    storage: unsafe extern "C" fn(*mut Inst) -> InstructionStorage,
    remap: unsafe extern "C" fn(*mut Inst, *const Values),
    block: unsafe extern "C" fn(*mut Vec<Inst>, *mut Values, *mut Context<'_>) -> Flow,
    instruction_span: unsafe extern "C" fn(*mut Vec<Inst>) -> BlockSpan,
    swap_instructions: unsafe extern "C" fn(*mut Inst, *mut Inst),
    truncate: unsafe extern "C" fn(*mut Vec<Inst>, usize),
    state: unsafe extern "C" fn(StateOp, *mut Values, *mut Values, Map) -> *mut Values,
    resize_rows: ResizeRows,
    expression: unsafe extern "C" fn(*mut Values, Expression, Reg) -> Optional,
    division: unsafe extern "C" fn(*mut Context<'_>, DivisionOp, Reg, u64, u64, Division) -> Division,
    rewrite_division: unsafe extern "C" fn(*mut IntegerDivisionResult, DivisionChoice, Reg, Reg),
    pair: unsafe extern "C" fn(*mut Inst, *const Inst, *const Inst, *const IntegerDivisor, Reg, Reg, Reg),
    note: unsafe extern "C" fn(*mut Context<'_>, Reg, *const Inst, Reuse),
    memory_get: unsafe extern "C" fn(*const Values, usize, Reg, u32) -> Memory,
    memory_put: unsafe extern "C" fn(*mut Values, usize, Reg, u32, Reg, *const Inst),
    memory_filter: unsafe extern "C" fn(*mut Values, u32, usize, Reg, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::value_block"]
    fn value_block(code: *mut Vec<Inst>, values: *mut Values, context: *mut Context<'_>, registers: Span, host: Host) -> Flow;
    #[link_name = "bootstrap.stage_fusion::value_discard_flow"]
    fn value_discard_flow(flow: Flow, resize: ResizeRows);
    #[link_name = "bootstrap.stage_fusion::value_root"]
    fn value_root(values: *const Registers, reg: Reg) -> Reg;
    #[link_name = "bootstrap.stage_fusion::value_copy_registers"]
    fn value_copy_registers(target: *mut Registers, source: *const Registers, locals: u32, resize: ResizeRows);
    #[link_name = "bootstrap.stage_fusion::value_release_registers"]
    fn value_release_registers(values: *mut Registers, resize: ResizeRows);
    #[link_name = "bootstrap.stage_fusion::value_merge_aliases"]
    fn value_merge_aliases(target: *mut Registers, source: *const Registers, resize: ResizeRows);
    #[link_name = "bootstrap.stage_fusion::value_memory_keeps"]
    fn value_memory_keeps(mode: u32, binding: usize, index: Reg, word: u32, target: usize, at: Reg, field: u32) -> u32;
}
pub(super) extern "C" fn root(values: &Values, reg: Reg) -> Reg { unsafe { value_root(&values.registers, reg) } }
pub(super) fn run(code: &mut Vec<Inst>, values: &mut Values, registers: &[Scalar], reuses: &mut Reuses) {
    let flow = run_block(code, values, registers, reuses);
    unsafe { value_discard_flow(flow, resize_rows) }
}
pub(super) fn extend_aliases(target: &mut Values, source: Values) {
    unsafe { value_merge_aliases(&mut target.registers, &source.registers, resize_rows); }
}

pub(super) fn aliases_only(values: &Values) -> Values {
    let mut target = Values::default();
    unsafe { value_copy_registers(&mut target.registers, &values.registers, 0, resize_rows); }
    target
}

unsafe extern "C" fn resize_rows(rows: *mut LocalValue, old: usize, new: usize) -> *mut LocalValue {
    use std::alloc::{alloc, dealloc, handle_alloc_error, realloc, Layout};
    if new == 0 {
        unsafe { dealloc(rows.cast(), Layout::array::<LocalValue>(old).unwrap()); }
        return ptr::null_mut();
    }

    let layout = Layout::array::<LocalValue>(new).unwrap();
    let allocated = if old == 0 {
        unsafe { alloc(layout) }
    } else {
        let previous = Layout::array::<LocalValue>(old).unwrap();
        unsafe { realloc(rows.cast(), previous, layout.size()) }
    };
    if allocated.is_null() {
        handle_alloc_error(layout);
    }
    allocated.cast()
}

fn run_block(code: &mut Vec<Inst>, values: &mut Values, registers: &[Scalar], reuses: &mut Reuses) -> Flow {
    let mut context = Context { registers, reuses, divisions: BTreeMap::new() };
    unsafe { value_block(code, values, &mut context, Span::slice(registers), HOST) }
}
unsafe extern "C" fn truncate(code: *mut Vec<Inst>, kept: usize) {
    unsafe { &mut *code }.truncate(kept);
}
unsafe extern "C" fn remap(instruction: *mut Inst, values: *const Values) {
    unsafe { crate::ir::rewrite::operands(&mut *instruction, &*values, root); }
}
unsafe extern "C" fn block(code: *mut Vec<Inst>, values: *mut Values, context: *mut Context<'_>) -> Flow {
    let context = unsafe { &mut *context }; run_block(unsafe { &mut *code }, unsafe { &mut *values }, context.registers, context.reuses)
}
unsafe extern "C" fn state(op: StateOp, target: *mut Values, source: *mut Values, map: Map) -> *mut Values {
    macro_rules! fields { ($action:ident) => { match map {
        Map::Expressions => fields!(@$action expressions),
        Map::Memory => fields!(@$action memory),
    } }; (@move $field:ident) => { unsafe { &mut *target }.$field = std::mem::take(&mut unsafe { &mut *source }.$field) };
        (@copy $field:ident) => { unsafe { &mut *target }.$field = unsafe { &*source }.$field.clone() };
        (@clear $field:ident) => { unsafe { &mut *target }.$field.clear() };
    }
    match op {
        StateOp::New => return Box::into_raw(Box::new(Values::default())),
        StateOp::Clone => return Box::into_raw(Box::new(unsafe { &*source }.clone())),
        StateOp::Drop => drop(unsafe { Box::from_raw(target) }),
        StateOp::Move => fields!(move), StateOp::Copy => fields!(copy), StateOp::Clear => fields!(clear),
    }
    std::ptr::null_mut()
}
unsafe extern "C" fn expression(values: *mut Values, key: Expression, dst: Reg) -> Optional {
    let arguments = if key.count == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(key.arguments, key.count) }.to_vec()
    };
    let key = ExpressionKey { kind: key.kind, operation: key.operation, words: key.words, arguments };
    match unsafe { &mut *values }.expressions.entry(key) {
        Entry::Occupied(old) => Optional { value: *old.get(), present: 1 },
        Entry::Vacant(slot) => { slot.insert(dst); Optional { value: 0, present: 0 } }
    }
}
unsafe extern "C" fn division(context: *mut Context<'_>, op: DivisionOp, src: Reg, tag: u64, divisor: u64, value: Division) -> Division {
    let context = unsafe { &mut *context }; match op {
        DivisionOp::Clear => context.divisions.clear(),
        DivisionOp::Take => return context.divisions.remove(&(src, tag, divisor)).unwrap_or_default(),
        DivisionOp::Put => { context.divisions.insert((src, tag, divisor), value); }
    }
    Division::default()
}
unsafe extern "C" fn rewrite_division(result: *mut IntegerDivisionResult, choice: DivisionChoice, quotient: Reg, remainder: Reg) {
    unsafe { *result = match choice { DivisionChoice::Quotient => IntegerDivisionResult::Quotient(quotient),
        DivisionChoice::Remainder => IntegerDivisionResult::Remainder(remainder), DivisionChoice::Both => IntegerDivisionResult::Both { quotient, remainder } }; }
}
unsafe extern "C" fn pair(retained: *mut Inst, quotient_from: *const Inst, remainder_from: *const Inst,
    divisor: *const IntegerDivisor, src: Reg, quotient: Reg, remainder: Reg) {
    let quotient_origin = OperationOrigin::from_instruction(unsafe { &*quotient_from });
    let remainder_origin = OperationOrigin::from_instruction(unsafe { &*remainder_from });
    unsafe { *retained = Inst::generated(
        InstKind::IntegerDivide { result: IntegerDivisionResult::Both { quotient, remainder }, src, divisor: *divisor },
        std::sync::Arc::new(GeneratedOrigin { reason: GenerationReason::IntegerDivisionPair,
            trigger: Some(GenerationAnchor::IntegerDivisionPair { quotient: quotient_origin, remainder: remainder_origin }) }),
    ); }
}
unsafe extern "C" fn note(context: *mut Context<'_>, retained: Reg, instruction: *const Inst, reuse: Reuse) {
    let origin = OperationOrigin::from_instruction(unsafe { &*instruction }); let fact = match reuse.kind {
        ReuseKind::Expression => RetainedUse::Expression(ExpressionReuse { result: reuse.role.reuse(), paired_input: reuse.paired != 0, origin }),
        ReuseKind::LocalRead => RetainedUse::LocalRead(origin), ReuseKind::IndexedRead => RetainedUse::IndexedRead(origin),
        ReuseKind::IndexedStoreRead => RetainedUse::IndexedStoreRead { store: unsafe { &*reuse.origin }.clone(), read: origin },
    }; unsafe { &mut *context }.reuses.entry(retained).or_default().push(fact);
}
unsafe extern "C" fn memory_get(values: *const Values, binding: usize, index: Reg, word: u32) -> Memory {
    match unsafe { &*values }.memory.get(&(binding, index, word)) {
        Some((value, origin)) => Memory { value: *value, origin: origin.as_ref().map_or(std::ptr::null(), |origin| origin), present: 1 },
        None => Memory { value: 0, origin: std::ptr::null(), present: 0 },
    }
}
unsafe extern "C" fn memory_put(values: *mut Values, binding: usize, index: Reg, word: u32, value: Reg, instruction: *const Inst) {
    let origin = unsafe { instruction.as_ref() }.map(OperationOrigin::from_instruction); unsafe { &mut *values }.memory.insert((binding, index, word), (value, origin));
}
unsafe extern "C" fn memory_filter(values: *mut Values, mode: u32, binding: usize, index: Reg, word: u32) {
    unsafe { &mut *values }.memory.retain(|&(b, i, w), _| unsafe { value_memory_keeps(mode, b, i, w, binding, index, word) != 0 });
}
const HOST: Host = Host {
    values: REGISTER_HOST, storage: instruction_storage, remap, block, instruction_span, swap_instructions, truncate,
    state, resize_rows, expression, division, rewrite_division, pair, note, memory_get, memory_put, memory_filter,
};
