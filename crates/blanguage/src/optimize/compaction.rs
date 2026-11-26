//! B owns complete slot compaction; Rust retains the original typed owners.
use crate::ir::*;
use std::{collections::{btree_map, BTreeMap}, ffi::c_void, mem::size_of};

#[derive(Clone, Copy)]
#[repr(C)]
struct Span { data: *const c_void, count: usize, stride: usize }
impl Span {
    fn write<T>(values: &mut [T]) -> Self {
        Self { data: values.as_mut_ptr().cast(), count: values.len(), stride: size_of::<T>() }
    }
}
#[repr(C)]
struct Input {
    code: Span, registers: Span, inputs: Span, outputs: Span,
    register_map: *mut usize, local_map: *mut usize, local_count: usize,
}
#[repr(C)]
struct Declaration { present: u32, end: usize, count: usize }
#[repr(C)]
struct Host {
    ir: crate::ir::dataflow::Host, context: *mut State,
    next: unsafe extern "C" fn(*mut State) -> Declaration,
    whole: unsafe extern "C" fn(*mut State, usize),
    scalar: unsafe extern "C" fn(*mut State, usize, usize),
    rewrite: crate::ir::rewrite::Host,
}
struct State {
    declarations: btree_map::IntoIter<usize, Ty>, current: Option<(usize, Ty)>,
    locals: *mut BTreeMap<usize, Ty>,
}
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::compact_slots"]
    fn compact(input: Input, host: Host) -> usize;
}
unsafe extern "C" fn next(context: *mut State) -> Declaration {
    let state = unsafe { &mut *context };
    state.current = state.declarations.next();
    match &state.current {
        Some((end, ty)) => Declaration { present: 1, end: *end, count: ty.components.len() },
        None => Declaration { present: 0, end: 0, count: 0 },
    }
}
unsafe extern "C" fn whole(context: *mut State, end: usize) {
    let state = unsafe { &mut *context };
    let (_, ty) = state.current.take().unwrap();
    unsafe { (&mut *state.locals).insert(end, ty); }
}
unsafe extern "C" fn scalar(context: *mut State, index: usize, end: usize) {
    let state = unsafe { &mut *context };
    let ty = &state.current.as_ref().unwrap().1;
    let scalar = ty.components[index].0;
    unsafe { (&mut *state.locals).insert(end, Ty::scalar(scalar)); }
}
pub(super) fn run(code: &mut [Inst], registers: &mut Vec<Scalar>,
    locals: &mut BTreeMap<usize, Ty>, inputs: &mut [Reg], outputs: &mut [Reg]) {
    let mut register_map = vec![0; registers.len()];
    let mut local_map = vec![0; locals.last_key_value().map_or(0, |(&end, _)| end)];
    let code_count = code.len();
    let code = code.as_mut_ptr();
    let mut state = State {
        declarations: std::mem::take(locals).into_iter(), current: None,
        locals,
    };
    let input = Input {
        code: Span { data: code.cast(), count: code_count, stride: size_of::<Inst>() },
        registers: Span::write(registers),
        inputs: Span::write(inputs), outputs: Span::write(outputs),
        register_map: register_map.as_mut_ptr(), local_map: local_map.as_mut_ptr(),
        local_count: local_map.len(),
    };
    // Scalar records are Copy. B reads each original record before its stable
    // in-place move; the original Vec is resized after the last B consumer.
    let kept = unsafe { compact(input, Host {
        ir: crate::ir::dataflow::HOST, context: &mut state,
        next, whole, scalar, rewrite: crate::ir::rewrite::HOST,
    }) };
    unsafe { registers.set_len(kept); }
}
