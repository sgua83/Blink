//! The original owning IR, primitive containers and source construction.
use super::*;
use super::super::{BlockSpan, InstructionStorage, instruction_span, instruction_storage, swap_instructions};
use crate::ir::{dataflow::{REGISTER_HOST, RegisterHost, Span}, rewrite};
use std::mem::{offset_of, size_of, MaybeUninit};

#[repr(C)]
struct LocalValue { slot: usize, value: Reg }

#[repr(C)]
#[derive(Default)]
struct Rows { data: usize, count: usize, capacity: usize }

#[repr(C)]
#[derive(Default)]
struct Registers { aliases: Rows, locals: Rows }

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
struct MemoryRows(Rows);

impl Drop for MemoryRows {
    fn drop(&mut self) {
        unsafe { value_release_memory(&mut self.0, MEMORY_HOST); }
    }
}

#[repr(C)]
#[derive(Default)]
pub(super) struct Values {
    registers: Registers,
    memory: MemoryRows,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct Expressions {
    entries: Rows,
    arguments: Rows,
    buckets: Rows,
}

impl Drop for Expressions {
    fn drop(&mut self) {
        unsafe { value_release_expressions(self, resize_rows::<u64>); }
    }
}

#[repr(C)]
struct Reuse { origin: *const OperationOrigin, kind: ReuseKind, role: ResultRole, paired: u32 }

#[repr(C)]
#[derive(Default)]
pub(super) struct Reuses { order: Rows, facts: Rows }

type CollectOrigin = unsafe extern "C" fn(*mut c_void, *mut OriginRow, ResultLocation);
type AttachOrigin = unsafe extern "C" fn(*mut c_void, *mut Inst, ReuseKind);
type AcceptOrigin = unsafe extern "C" fn(*mut c_void, *mut OriginRow);
type FinishReductionInput = unsafe extern "C" fn(*mut c_void, *mut c_void, u32, usize);

#[repr(C)]
struct Reduction {
    parts: *const *mut Vec<Inst>,
    registers: Span,
    roots: *const rewrite::Span,
}

#[repr(C)]
struct ReductionOrigins {
    reads: *mut c_void,
    inputs: *mut c_void,
    accept: AcceptOrigin,
    finish_input: FinishReductionInput,
}

#[repr(C)]
struct OriginWriter {
    sink: *mut c_void,
    collect: CollectOrigin,
    attach: AttachOrigin,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OriginHost {
    stride: usize,
    resize: ResizeRows,
    capture: unsafe extern "C" fn(*mut OriginRow, Reg, *const Inst, Reuse),
    discard: unsafe extern "C" fn(*mut OriginRow),
}

impl Reuses {
    pub(super) fn order(&mut self) {
        unsafe { value_order_origins(self, size_of::<OriginRow>()); }
    }

    pub(super) fn retain(&mut self, code: &mut [Inst], sink: *mut c_void, collect: CollectOrigin, attach: AttachOrigin) {
        let writer = OriginWriter { sink, collect, attach };
        unsafe { value_retain_origins(rewrite::Span::slice(code), self, crate::ir::dataflow::HOST,
            rewrite::HOST, size_of::<OriginRow>(), writer); }
    }

    pub(super) fn input(&mut self, reg: Reg, sink: *mut c_void, accept: AcceptOrigin) {
        unsafe { value_take_input_origins(self, reg, sink, ORIGIN_HOST, accept); }
    }
}

impl Drop for Reuses {
    fn drop(&mut self) {
        unsafe { value_release_origins(self, resize_rows::<u64>, ORIGIN_HOST); }
    }
}
#[repr(u32)]
enum DivisionChoice { Quotient = 1, Remainder, Both }
#[repr(C)]
struct Path { rows: Rows, reached: u32 }
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
#[repr(C)]
struct Host {
    values: RegisterHost,
    storage: unsafe extern "C" fn(*mut Inst) -> InstructionStorage,
    rewrite: rewrite::Host,
    instruction_span: unsafe extern "C" fn(*mut Vec<Inst>) -> BlockSpan,
    swap_instructions: unsafe extern "C" fn(*mut Inst, *mut Inst),
    truncate: unsafe extern "C" fn(*mut Vec<Inst>, usize),
    resize_rows: ResizeRows,
    scratch: ResizeRows,
    rewrite_division: unsafe extern "C" fn(*mut IntegerDivisionResult, DivisionChoice, Reg, Reg),
    pair: unsafe extern "C" fn(*mut Inst, *const Inst, *const Inst, *const IntegerDivisor, Reg, Reg, Reg),
    origin: OriginHost,
    memory: MemoryHost,
}

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::value_reduction"]
    fn value_reduction(input: Reduction, host: Host, writer: OriginWriter, origins: ReductionOrigins);
    #[link_name = "bootstrap.stage_fusion::value_order_origins"]
    fn value_order_origins(state: *mut Reuses, stride: usize);
    #[link_name = "bootstrap.stage_fusion::value_retain_origins"]
    fn value_retain_origins(code: rewrite::Span, state: *mut Reuses, read: crate::ir::dataflow::Host,
        write: rewrite::Host, stride: usize, writer: OriginWriter);
    #[link_name = "bootstrap.stage_fusion::value_take_input_origins"]
    fn value_take_input_origins(state: *mut Reuses, reg: Reg, sink: *mut c_void, host: OriginHost, accept: AcceptOrigin) -> bool;
    #[link_name = "bootstrap.stage_fusion::value_release_origins"]
    fn value_release_origins(state: *mut Reuses, resize: ResizeRows, host: OriginHost);
    #[link_name = "bootstrap.stage_fusion::value_block"]
    fn value_block(code: *mut Vec<Inst>, values: *mut Values, expressions: *mut Expressions,
        reuses: *mut Reuses, registers: Span, host: Host, keep_expressions: bool) -> Flow;
    #[link_name = "bootstrap.stage_fusion::value_release_expressions"]
    fn value_release_expressions(expressions: *mut Expressions, resize: ResizeRows);
    #[link_name = "bootstrap.stage_fusion::value_discard_flow"]
    fn value_discard_flow(flow: Flow, resize: ResizeRows);
    #[link_name = "bootstrap.stage_fusion::value_rebind_roots"]
    fn value_rebind_roots(registers: *const c_void, roots: rewrite::Span);
    #[link_name = "bootstrap.stage_fusion::value_release_registers"]
    fn value_release_registers(values: *mut Registers, resize: ResizeRows);
    #[link_name = "bootstrap.stage_fusion::value_release_memory"]
    fn value_release_memory(values: *mut Rows, memory: MemoryHost);
}
pub(super) fn rebind_roots(values: &Values, outputs: &mut [Reg]) {
    let registers = ptr::from_ref(&values.registers).cast();
    unsafe { value_rebind_roots(registers, rewrite::Span::slice(outputs)); }
}

pub(super) fn run_reduction(
    parts: &mut [Vec<Inst>; 3], kernel: &mut PreparedReductionKernel, origins: &mut Origins,
    reads: &mut Vec<OperationOrigin>, inputs: Option<&mut Vec<ReductionInputReadForward>>,
) {
    let [identity, project, combine] = parts;
    let blocks = [
        ptr::from_mut(&mut kernel.stage.code), ptr::from_mut(identity),
        ptr::from_mut(project), ptr::from_mut(combine),
    ];
    let roots = [
        rewrite::Span::slice(&mut kernel.source), rewrite::Span::slice(&mut kernel.projected),
        rewrite::Span::slice(&mut kernel.initial), rewrite::Span::slice(&mut kernel.left),
        rewrite::Span::slice(&mut kernel.right), rewrite::Span::slice(&mut kernel.result),
    ];
    let input = Reduction { parts: blocks.as_ptr(), registers: Span::slice(&kernel.stage.registers), roots: roots.as_ptr() };
    let writer = OriginWriter { sink: ptr::from_mut(origins).cast(), collect: collect_origin, attach: attach_origin };
    let input_origins = ReductionOrigins {
        reads: ptr::from_mut(reads).cast(), inputs: inputs.map_or(ptr::null_mut(), |output| ptr::from_mut(output).cast()),
        accept: accept_input, finish_input: finish_reduction_input,
    };
    unsafe { value_reduction(input, HOST, writer, input_origins); }
}

pub(super) fn run(code: &mut Vec<Inst>, values: &mut Values, expressions: &mut Expressions,
    registers: &[Scalar], reuses: &mut Reuses, keep_expressions: bool,
) {
    unsafe {
        let flow = value_block(code, values, expressions, reuses, Span::slice(registers), HOST, keep_expressions);
        value_discard_flow(flow, resize_rows::<LocalValue>);
    }
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

unsafe extern "C" fn truncate(code: *mut Vec<Inst>, kept: usize) {
    unsafe { &mut *code }.truncate(kept);
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
unsafe extern "C" fn capture_origin(row: *mut OriginRow, result: Reg, instruction: *const Inst, reuse: Reuse) {
    let read = OperationOrigin::from_instruction(unsafe { &*instruction });
    let store = if matches!(reuse.kind, ReuseKind::IndexedStoreRead) {
        MaybeUninit::new(unsafe { &*reuse.origin }.clone())
    } else {
        MaybeUninit::uninit()
    };
    unsafe { row.write(OriginRow { result, kind: reuse.kind, pending: 1, role: reuse.role, paired: reuse.paired,
        read: MaybeUninit::new(read), store }); }
}

unsafe extern "C" fn discard_origin(row: *mut OriginRow) {
    let row = unsafe { &mut *row };
    unsafe {
        row.read.assume_init_drop();
        if matches!(row.kind, ReuseKind::IndexedStoreRead) { row.store.assume_init_drop(); }
    }
}

const ORIGIN_HOST: OriginHost = OriginHost {
    stride: size_of::<OriginRow>(),
    resize: resize_rows::<OriginRow>,
    capture: capture_origin,
    discard: discard_origin,
};

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
    values: REGISTER_HOST, storage: instruction_storage, rewrite: rewrite::HOST, instruction_span, swap_instructions, truncate,
    resize_rows: resize_rows::<LocalValue>, scratch: resize_rows::<u64>, rewrite_division, pair, origin: ORIGIN_HOST, memory: MEMORY_HOST,
};
