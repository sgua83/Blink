//! The original owning IR, primitive containers and source construction.
use super::*;
use super::super::{BlockSpan, InstructionStorage, instruction_span, instruction_storage, swap_instructions};
use crate::ir::dataflow::{REGISTER_HOST, RegisterHost, Span};
use std::{collections::btree_map::Entry, mem::{offset_of, size_of, MaybeUninit}};

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
        unsafe { value_copy_registers(&mut copy, self, 1, resize_rows::<LocalValue>); }
        copy
    }
}

impl Drop for Registers {
    fn drop(&mut self) {
        unsafe { value_release_registers(self, resize_rows::<LocalValue>); }
    }
}

#[repr(C)]
struct MemoryEntry {
    binding: usize,
    index: Reg,
    word: u32,
    has_origin: u32,
    value: Reg,
}

#[repr(C)]
struct MemoryRow {
    entry: MemoryEntry,
    origin: MaybeUninit<OperationOrigin>,
}

#[repr(transparent)]
#[derive(Default)]
struct MemoryRows(RegisterRows);

impl Clone for MemoryRows {
    fn clone(&self) -> Self {
        let mut copy = Self::default();
        unsafe { value_copy_memory(&mut copy.0, &self.0, MEMORY_HOST); }
        copy
    }
}

impl Drop for MemoryRows {
    fn drop(&mut self) {
        unsafe { value_release_memory(&mut self.0, MEMORY_HOST); }
    }
}

#[repr(C)]
#[derive(Clone, Default)]
pub(super) struct Values {
    registers: Registers,
    memory: MemoryRows,
    expressions: BTreeMap<ExpressionKey, Reg>,
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
#[repr(u32)]
enum ReuseKind { Expression, LocalRead, IndexedRead, IndexedStoreRead }

#[repr(C)]
struct Reuse { origin: *const OperationOrigin, kind: ReuseKind, role: ResultRole, paired: u32 }
#[repr(u32)]
enum StateOp { New, Clone, Drop, CopyExpressions }
#[repr(u32)]
enum DivisionOp { Clear, Take, Put }
#[repr(u32)]
enum DivisionChoice { Quotient = 1, Remainder, Both }
#[repr(C)]
struct Path { rows: RegisterRows, reached: u32 }
#[repr(C)]
struct Flow { next: Path, exits: Path }
type ResizeRows = unsafe extern "C" fn(*mut c_void, usize, usize) -> *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct MemoryHost {
    stride: usize,
    origin_offset: usize,
    resize: ResizeRows,
    move_rows: unsafe extern "C" fn(*mut MemoryRow, *const MemoryRow, usize),
    capture_origin: unsafe extern "C" fn(*mut MemoryRow, *const Inst),
    copy_origin: unsafe extern "C" fn(*mut MemoryRow, *const MemoryRow),
    drop_origin: unsafe extern "C" fn(*mut MemoryRow),
}
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
    state: unsafe extern "C" fn(StateOp, *mut Values, *const Values) -> *mut Values,
    resize_rows: ResizeRows,
    expression: unsafe extern "C" fn(*mut Values, Expression, Reg) -> Optional,
    division: unsafe extern "C" fn(*mut Context<'_>, DivisionOp, Reg, u64, u64, Division) -> Division,
    rewrite_division: unsafe extern "C" fn(*mut IntegerDivisionResult, DivisionChoice, Reg, Reg),
    pair: unsafe extern "C" fn(*mut Inst, *const Inst, *const Inst, *const IntegerDivisor, Reg, Reg, Reg),
    note: unsafe extern "C" fn(*mut Context<'_>, Reg, *const Inst, Reuse),
    memory: MemoryHost,
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
    #[link_name = "bootstrap.stage_fusion::value_copy_memory"]
    fn value_copy_memory(target: *mut RegisterRows, source: *const RegisterRows, memory: MemoryHost);
    #[link_name = "bootstrap.stage_fusion::value_release_memory"]
    fn value_release_memory(values: *mut RegisterRows, memory: MemoryHost);
}
pub(super) extern "C" fn root(values: &Values, reg: Reg) -> Reg { unsafe { value_root(&values.registers, reg) } }
pub(super) fn run(code: &mut Vec<Inst>, values: &mut Values, registers: &[Scalar], reuses: &mut Reuses) {
    let flow = run_block(code, values, registers, reuses);
    unsafe { value_discard_flow(flow, resize_rows::<LocalValue>) }
}
pub(super) fn extend_aliases(target: &mut Values, source: Values) {
    unsafe { value_merge_aliases(&mut target.registers, &source.registers, resize_rows::<LocalValue>); }
}

pub(super) fn aliases_only(values: &Values) -> Values {
    let mut target = Values::default();
    unsafe { value_copy_registers(&mut target.registers, &values.registers, 0, resize_rows::<LocalValue>); }
    target
}

unsafe extern "C" fn resize_rows<T>(rows: *mut c_void, old: usize, new: usize) -> *mut c_void {
    use std::alloc::{alloc, dealloc, handle_alloc_error, realloc, Layout};
    if new == 0 {
        unsafe { dealloc(rows.cast(), Layout::array::<T>(old).unwrap()); }
        return ptr::null_mut();
    }

    let layout = Layout::array::<T>(new).unwrap();
    let allocated = if old == 0 {
        unsafe { alloc(layout) }
    } else {
        let previous = Layout::array::<T>(old).unwrap();
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
unsafe extern "C" fn state(op: StateOp, target: *mut Values, source: *const Values) -> *mut Values {
    match op {
        StateOp::New => return Box::into_raw(Box::new(Values::default())),
        StateOp::Clone => return Box::into_raw(Box::new(unsafe { &*source }.clone())),
        StateOp::Drop => drop(unsafe { Box::from_raw(target) }),
        StateOp::CopyExpressions => unsafe { (*target).expressions = (*source).expressions.clone(); },
    }
    ptr::null_mut()
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
unsafe extern "C" fn move_memory_rows(target: *mut MemoryRow, source: *const MemoryRow, count: usize) {
    unsafe { ptr::copy(source, target, count); }
}

unsafe extern "C" fn capture_memory_origin(target: *mut MemoryRow, source: *const Inst) {
    let origin = OperationOrigin::from_instruction(unsafe { &*source });
    unsafe { ptr::addr_of_mut!((*target).origin).write(MaybeUninit::new(origin)); }
}

unsafe extern "C" fn copy_memory_origin(target: *mut MemoryRow, source: *const MemoryRow) {
    let origin = unsafe { (*source).origin.assume_init_ref() }.clone();
    unsafe { ptr::addr_of_mut!((*target).origin).write(MaybeUninit::new(origin)); }
}

unsafe extern "C" fn drop_memory_origin(row: *mut MemoryRow) {
    unsafe { (*row).origin.assume_init_drop(); }
}

const MEMORY_HOST: MemoryHost = MemoryHost {
    stride: size_of::<MemoryRow>(),
    origin_offset: offset_of!(MemoryRow, origin),
    resize: resize_rows::<MemoryRow>,
    move_rows: move_memory_rows,
    capture_origin: capture_memory_origin,
    copy_origin: copy_memory_origin,
    drop_origin: drop_memory_origin,
};
const HOST: Host = Host {
    values: REGISTER_HOST, storage: instruction_storage, remap, block, instruction_span, swap_instructions, truncate,
    state, resize_rows: resize_rows::<LocalValue>, expression, division, rewrite_division, pair, note, memory: MEMORY_HOST,
};
