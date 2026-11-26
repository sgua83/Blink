//! The B read scheduler borrows the original instructions and owns their ordering.
use super::*;
use crate::ir::dataflow::{self, Node, Span};
use std::{ffi::c_void, ptr, vec::IntoIter};

#[derive(Default)]
struct Pending {
    code: Vec<Option<Inst>>,
    out: Vec<Inst>,
}

struct Input<'a> {
    remaining: IntoIter<Inst>,
    current: Option<Inst>,
    expanded: IntoIter<Inst>,
    registers: &'a mut Vec<Scalar>,
    writes: &'a [usize],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Host {
    ir: dataflow::Host,
    math_outgoing: unsafe extern "C" fn(u32) -> u64,
    next: unsafe extern "C" fn(*mut Input<'_>) -> Node,
    sample: unsafe extern "C" fn(*mut Input<'_>) -> usize,
    child: unsafe extern "C" fn(*mut Input<'_>, u32, *mut c_void),
    store: unsafe extern "C" fn(*mut Input<'_>, *mut Pending, bool),
    queued: unsafe extern "C" fn(*const Pending, usize) -> Node,
    emit: unsafe extern "C" fn(*mut Pending, usize),
    clear: unsafe extern "C" fn(*mut Pending),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::place_reads"]
    fn place_reads(input: *mut c_void, pending: *mut c_void, writes: Span, registers: usize, host: Host);
    #[link_name = "bootstrap.ir_dataflow::place_read_block"]
    fn place_read_block(input: *mut c_void, pending: *mut c_void,
        scratch: *mut c_void, writes: Span, host: Host);
}

unsafe extern "C" fn next(input: *mut Input<'_>) -> Node {
    let input = unsafe { &mut *input };
    input.current = input.expanded.next().or_else(|| input.remaining.next());
    input.current.as_ref().map_or(Node::EMPTY, Node::instruction)
}

unsafe extern "C" fn sample(input: *mut Input<'_>) -> usize {
    let input = unsafe { &mut *input };
    let source = unsafe { input.current.take().unwrap_unchecked() };
    let InstKind::TextureSample { result, texture, coordinate, lod } = source.kind else {
        // B requests expansion only after inspecting OP_TEXTURE_SAMPLE.
        unsafe { std::hint::unreachable_unchecked() }
    };
    let origin = OperationOrigin {
        source_site: source.source_site,
        generated_origin: source.generated_origin,
    };
    input.expanded = texture::sample(input.registers, texture, &coordinate, lod, &result, origin).into_iter();
    input.registers.len()
}

unsafe extern "C" fn child(input: *mut Input<'_>, index: u32, scratch: *mut c_void) {
    let input = unsafe { &mut *input };
    let inst = unsafe { input.current.as_mut().unwrap_unchecked() };
    let body = match (&mut inst.kind, index) {
        (InstKind::If { yes, .. }, 0) => yes,
        (InstKind::If { no, .. }, 1) => no,
        (InstKind::Loop { condition, .. }, 0) => condition,
        (InstKind::Loop { body, .. }, 1) | (InstKind::Scope(body), 0) => body,
        _ => unsafe { std::hint::unreachable_unchecked() },
    };
    place_block(body, input.registers, input.writes, |input, pending, writes| unsafe {
        place_read_block(input.cast(), pending.cast(), scratch, writes, HOST)
    });
}

unsafe extern "C" fn store(input: *mut Input<'_>, pending: *mut Pending, defer: bool) {
    let inst = unsafe { (&mut *input).current.take().unwrap_unchecked() };
    let pending = unsafe { &mut *pending };
    if defer {
        pending.code.push(Some(inst));
    } else {
        pending.out.push(inst);
    }
}

unsafe extern "C" fn queued(pending: *const Pending, id: usize) -> Node {
    let code = unsafe { &(*pending).code };
    code[id].as_ref().map_or(Node::EMPTY, Node::instruction)
}

unsafe extern "C" fn emit(pending: *mut Pending, id: usize) {
    let pending = unsafe { &mut *pending };
    let inst = unsafe { pending.code[id].take().unwrap_unchecked() };
    pending.out.push(inst);
}

unsafe extern "C" fn clear(pending: *mut Pending) {
    unsafe { &mut (*pending).code }.clear();
}

const HOST: Host = Host {
    ir: dataflow::HOST,
    math_outgoing: operands::scalar_math_outgoing,
    next, sample, child, store, queued, emit, clear,
};

pub(super) fn place(code: &mut Vec<Inst>, registers: &mut Vec<Scalar>, writes: &[usize]) {
    let register_count = registers.len();
    place_block(code, registers, writes, |input, pending, writes| unsafe {
        place_reads(input.cast(), pending.cast(), writes, register_count, HOST)
    });
}

fn place_block(code: &mut Vec<Inst>, registers: &mut Vec<Scalar>, writes: &[usize],
    schedule: impl FnOnce(*mut Input<'_>, *mut Pending, Span),
) {
    let mut input = Input {
        remaining: std::mem::take(code).into_iter(), current: None, expanded: Vec::new().into_iter(),
        registers, writes,
    };
    let mut pending = Pending::default();
    schedule(ptr::from_mut(&mut input), ptr::from_mut(&mut pending), Span::slice(writes));
    *code = pending.out;
}
