//! The B pass owns Select traversal, eligibility and emission decisions.
//! These callbacks only project or move the original typed IR containers.
use super::*;
use std::{ffi::c_void, mem::{self, size_of, MaybeUninit}, ptr};

#[derive(Clone, Copy)]
#[repr(C)]
struct Span { data: *const c_void, count: usize, stride: usize }
impl Span {
    fn of<T>(values: &[T]) -> Self {
        Self { data: values.as_ptr().cast(), count: values.len(), stride: size_of::<T>() }
    }
}
#[repr(C)]
struct Slot { yes: usize, no: usize }
#[repr(C)]
struct Value { slot: usize, destination: Reg, yes: Reg, no: Reg }
#[derive(Clone, Copy)]
#[repr(C)]
struct Input { registers: Span, indices: *mut u8, slots: *mut MaybeUninit<Slot>, local_count: usize, domain: usize }
#[repr(C)]
struct Children { first: *mut c_void, second: *mut c_void }
#[repr(C)]
struct Next { instruction: *mut c_void, following: Span }
#[repr(C)]
struct Plan { values: *const Value, count: usize }
#[derive(Clone, Copy)]
#[repr(C)]
struct Host {
    ir: crate::ir::dataflow::Host, context: *mut c_void,
    block: unsafe extern "C" fn(*mut c_void) -> Span,
    children: unsafe extern "C" fn(*mut c_void) -> Children,
    scratch: unsafe extern "C" fn(*mut c_void, usize) -> *mut Value,
    rebuild: unsafe extern "C" fn(*mut c_void, *mut c_void, usize, Plan),
    next: unsafe extern "C" fn(*mut c_void) -> Next,
    keep: unsafe extern "C" fn(*mut c_void),
    arm: unsafe extern "C" fn(*mut c_void, *mut c_void),
    emit: unsafe extern "C" fn(*mut c_void, Reg, Value),
}
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::selection_run"] fn b_run(code: *mut c_void, input: Input, host: Host) -> u32;
    #[link_name = "bootstrap.ir_dataflow::selection_continue"] fn b_continue(cursor: *mut c_void, first: Next, plan: Plan, input: Input, host: Host);
    #[link_name = "bootstrap.ir_dataflow::selection_move_arm"] fn b_arm(cursor: *mut c_void, host: Host);
}
struct State { input: Input, values: Vec<Value> }
impl State {
    fn host(&mut self) -> Host {
        Host { ir: crate::ir::dataflow::HOST, context: ptr::from_mut(self).cast(),
            block, children, scratch, rebuild, next, keep, arm, emit }
    }
}
// Each cursor is scoped to the Rust callback that owns its original IntoIter.
// The output is reborrowed only while an actual move or construction occurs.
struct Cursor { output: *mut Vec<Inst>, source: std::vec::IntoIter<Inst>, current: Option<Inst>, host: Host }

pub(super) fn run(code: &mut Vec<Inst>, registers: &[Scalar], domain: usize, local_count: usize) -> bool {
    let mut indices = vec![0; registers.len()];
    let mut slots = Vec::<MaybeUninit<Slot>>::with_capacity(local_count);
    let input = Input { registers: Span::of(registers), indices: indices.as_mut_ptr(),
        slots: slots.as_mut_ptr(), local_count, domain };
    let mut state = State { input, values: Vec::new() };
    let host = state.host();
    unsafe { b_run(ptr::from_mut(code).cast(), input, host) != 0 }
}
unsafe extern "C" fn block(owner: *mut c_void) -> Span {
    let code = unsafe { &mut *owner.cast::<Vec<Inst>>() };
    Span { data: code.as_mut_ptr().cast(), count: code.len(), stride: size_of::<Inst>() }
}
unsafe extern "C" fn children(instruction: *mut c_void) -> Children {
    let kind = unsafe { &mut (*instruction.cast::<Inst>()).kind };
    let (first, second) = match kind {
        InstKind::Scope(body) => (ptr::from_mut(body), ptr::null_mut()),
        InstKind::If { yes, no, .. } => (ptr::from_mut(yes), ptr::from_mut(no)),
        InstKind::Loop { condition, body, .. } => (ptr::from_mut(condition), ptr::from_mut(body)),
        _ => (ptr::null_mut(), ptr::null_mut()),
    };
    Children { first: first.cast(), second: second.cast() }
}
unsafe extern "C" fn scratch(context: *mut c_void, capacity: usize) -> *mut Value {
    let state = unsafe { &mut *context.cast::<State>() };
    state.values.reserve(capacity);
    state.values.as_mut_ptr()
}
unsafe extern "C" fn rebuild(context: *mut c_void, owner: *mut c_void, prefix: usize, plan: Plan) {
    let state = unsafe { &mut *context.cast::<State>() };
    let input = state.input;
    let host = state.host();
    let output = unsafe { &mut *owner.cast::<Vec<Inst>>() };
    let mut source = mem::take(output).into_iter();
    output.extend(source.by_ref().take(prefix));
    let mut cursor = Cursor { output, source, current: None, host };
    let pointer = ptr::from_mut(&mut cursor).cast();
    let first = unsafe { next(pointer) };
    unsafe { b_continue(pointer, first, plan, input, host); }
}
unsafe extern "C" fn next(cursor: *mut c_void) -> Next {
    let cursor = unsafe { &mut *cursor.cast::<Cursor>() };
    cursor.current = cursor.source.next();
    Next { instruction: cursor.current.as_mut().map_or(ptr::null_mut(), |inst| ptr::from_mut(inst).cast()),
        following: Span::of(cursor.source.as_slice()) }
}
unsafe extern "C" fn keep(cursor: *mut c_void) {
    let cursor = unsafe { &mut *cursor.cast::<Cursor>() };
    unsafe { (*cursor.output).extend(cursor.current.take()); }
}
unsafe extern "C" fn arm(cursor: *mut c_void, owner: *mut c_void) {
    let cursor = unsafe { &mut *cursor.cast::<Cursor>() };
    let source = mem::take(unsafe { &mut *owner.cast::<Vec<Inst>>() }).into_iter();
    let mut child = Cursor { output: cursor.output, source, current: None, host: cursor.host };
    unsafe { b_arm(ptr::from_mut(&mut child).cast(), child.host); }
}
unsafe extern "C" fn emit(cursor: *mut c_void, condition: Reg, value: Value) {
    let cursor = unsafe { &mut *cursor.cast::<Cursor>() };
    let output = unsafe { &mut *cursor.output };
    output.push(Inst::new(InstKind::Select { dst: value.destination, condition, a: value.yes, b: value.no }));
    // The original mutable slot remains observable by a later loop.
    output.push(Inst::new(InstKind::StoreLocal(value.slot, value.destination)));
}
