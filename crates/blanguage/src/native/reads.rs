//! The B read scheduler borrows the original instructions and owns their ordering.
use super::*;
use crate::ir::dataflow::{self, Node, Span};
use std::{collections::BTreeMap, ffi::c_void, ops::Range, ptr, vec::IntoIter};

#[allow(dead_code)]
#[repr(u32)]
enum Queue { Immediate, Value, Shared, Local }

#[allow(dead_code)]
#[repr(u32)]
enum WorkKind { Done, Expand, Emit }

#[allow(dead_code)]
#[repr(u32)]
enum Clear { All, Shared, Local }

#[repr(C)]
struct Work { id: usize, kind: WorkKind }

#[repr(C)]
struct PrivateRead { id: usize, first: usize, end: usize, pending: bool }

#[derive(Default)]
struct Pending {
    producers: BTreeMap<Reg, usize>,
    code: Vec<Option<Inst>>,
    reads: Vec<usize>,
    private_reads: Vec<(Range<usize>, usize)>,
    work: Vec<Work>,
    out: Vec<Inst>,
}

struct Input<'a> {
    remaining: IntoIter<Inst>,
    current: Option<Inst>,
    writes: &'a [usize],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Host {
    ir: dataflow::Host,
    math_outgoing: unsafe extern "C" fn(u32) -> u64,
    next: unsafe extern "C" fn(*mut Input<'_>) -> Node,
    child: unsafe extern "C" fn(*mut Input<'_>, u32),
    store: unsafe extern "C" fn(*mut Input<'_>, *mut Pending, Queue, usize, usize),
    queued: unsafe extern "C" fn(*const Pending, usize) -> Node,
    enqueue_operand: unsafe extern "C" fn(*mut Pending, Reg) -> u32,
    demand_operand: unsafe extern "C" fn(*mut Pending, Reg) -> u32,
    work_len: unsafe extern "C" fn(*const Pending) -> usize,
    push: unsafe extern "C" fn(*mut Pending, Work),
    reverse: unsafe extern "C" fn(*mut Pending, usize),
    pop: unsafe extern "C" fn(*mut Pending) -> Work,
    emit: unsafe extern "C" fn(*mut Pending, usize),
    count: unsafe extern "C" fn(*const Pending, Clear) -> usize,
    read_at: unsafe extern "C" fn(*const Pending, usize) -> usize,
    private_at: unsafe extern "C" fn(*const Pending, usize) -> PrivateRead,
    retain: unsafe extern "C" fn(*mut Pending, usize, usize),
    clear: unsafe extern "C" fn(*mut Pending, Clear, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::place_reads"]
    fn place_reads(input: *mut c_void, pending: *mut c_void, writes: Span, host: Host);
    #[link_name = "bootstrap.ir_dataflow::read_demand"]
    fn read_demand(pending: *mut c_void, id: usize, host: Host);
}

unsafe extern "C" fn next(input: *mut Input<'_>) -> Node {
    let input = unsafe { &mut *input };
    input.current = input.remaining.next();
    input.current.as_ref().map_or(Node::EMPTY, Node::instruction)
}

unsafe extern "C" fn child(input: *mut Input<'_>, index: u32) {
    let input = unsafe { &mut *input };
    let inst = unsafe { input.current.as_mut().unwrap_unchecked() };
    let body = match (&mut inst.kind, index) {
        (InstKind::If { yes, .. }, 0) => yes,
        (InstKind::If { no, .. }, 1) => no,
        (InstKind::Loop { condition, .. }, 0) => condition,
        (InstKind::Loop { body, .. }, 1) | (InstKind::Scope(body), 0) => body,
        _ => unsafe { std::hint::unreachable_unchecked() },
    };
    place(body, input.writes);
}

unsafe extern "C" fn store(input: *mut Input<'_>, pending: *mut Pending, queue: Queue, first: usize, end: usize) {
    let inst = unsafe { (&mut *input).current.take().unwrap_unchecked() };
    let pending = unsafe { &mut *pending };
    if matches!(queue, Queue::Immediate) {
        pending.out.push(inst);
        return;
    }
    let id = pending.code.len();
    for result in inst.results() {
        pending.producers.insert(result, id);
    }
    pending.code.push(Some(inst));
    match queue {
        Queue::Shared => pending.reads.push(id),
        Queue::Local => pending.private_reads.push((first..end, id)),
        Queue::Value | Queue::Immediate => {}
    }
}

unsafe extern "C" fn queued(pending: *const Pending, id: usize) -> Node {
    let code = unsafe { &(*pending).code };
    code[id].as_ref().map_or(Node::EMPTY, Node::instruction)
}

unsafe extern "C" fn enqueue_operand(pending: *mut Pending, reg: Reg) -> u32 {
    if let Some(&id) = unsafe { &(*pending).producers }.get(&reg) {
        unsafe { &mut (*pending).work }.push(Work { id, kind: WorkKind::Expand });
    }
    1
}

unsafe extern "C" fn demand_operand(pending: *mut Pending, reg: Reg) -> u32 {
    let id = unsafe { &(*pending).producers }.get(&reg).copied();
    if let Some(id) = id {
        unsafe { read_demand(pending.cast(), id, HOST) };
    }
    1
}

unsafe extern "C" fn work_len(pending: *const Pending) -> usize {
    unsafe { &(*pending).work }.len()
}

unsafe extern "C" fn push(pending: *mut Pending, work: Work) {
    unsafe { &mut (*pending).work }.push(work);
}

unsafe extern "C" fn reverse(pending: *mut Pending, start: usize) {
    let work = unsafe { &mut (*pending).work };
    work[start..].reverse();
}

unsafe extern "C" fn pop(pending: *mut Pending) -> Work {
    unsafe { &mut (*pending).work }.pop().unwrap_or(Work { id: 0, kind: WorkKind::Done })
}

unsafe extern "C" fn emit(pending: *mut Pending, id: usize) {
    let pending = unsafe { &mut *pending };
    if let Some(inst) = pending.code[id].take() {
        for reg in inst.results() {
            pending.producers.remove(&reg);
        }
        pending.out.push(inst);
    }
}

unsafe extern "C" fn count(pending: *const Pending, kind: Clear) -> usize {
    let pending = unsafe { &*pending };
    match kind {
        Clear::All => pending.code.len(),
        Clear::Shared => pending.reads.len(),
        Clear::Local => pending.private_reads.len(),
    }
}

unsafe extern "C" fn read_at(pending: *const Pending, index: usize) -> usize {
    unsafe { &*pending }.reads[index]
}

unsafe extern "C" fn private_at(pending: *const Pending, index: usize) -> PrivateRead {
    let pending = unsafe { &*pending };
    let (range, id) = &pending.private_reads[index];
    PrivateRead { id: *id, first: range.start, end: range.end, pending: pending.code[*id].is_some() }
}

unsafe extern "C" fn retain(pending: *mut Pending, source: usize, target: usize) {
    let rows = &mut unsafe { &mut *pending }.private_reads;
    let (range, id) = &rows[source];
    rows[target] = (range.start..range.end, *id);
}

unsafe extern "C" fn clear(pending: *mut Pending, kind: Clear, count: usize) {
    let pending = unsafe { &mut *pending };
    match kind {
        Clear::All => {
            pending.producers.clear();
            pending.code.clear();
            pending.reads.clear();
            pending.private_reads.clear();
        }
        Clear::Shared => pending.reads.clear(),
        Clear::Local => pending.private_reads.truncate(count),
    }
}

const HOST: Host = Host {
    ir: dataflow::HOST,
    math_outgoing: operands::scalar_math_outgoing,
    next, child, store, queued, enqueue_operand, demand_operand, work_len, push, reverse, pop, emit,
    count, read_at, private_at, retain, clear,
};

pub(super) fn place(code: &mut Vec<Inst>, writes: &[usize]) {
    let mut input = Input { remaining: std::mem::take(code).into_iter(), current: None, writes };
    let mut pending = Pending::default();
    unsafe {
        place_reads(ptr::from_mut(&mut input).cast(), ptr::from_mut(&mut pending).cast(), Span::slice(writes), HOST)
    };
    *code = pending.out;
}
