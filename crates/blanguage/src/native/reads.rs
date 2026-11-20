//! CPU producers reach their first consumer; actual memory and control effects keep their order.
use super::*;
use std::{collections::BTreeMap, ops::Range};

#[repr(u32)]
enum Input {
    Value, Load, ViewLoad, LocalRead, LocalWrite, Memory, Uniform, Barrier,
}

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

impl Pending {
    fn defer(&mut self, inst: Inst, queue: Queue, first: usize, end: usize) {
        let id = self.code.len();
        for reg in inst.results() {
            self.producers.insert(reg, id);
        }
        self.code.push(Some(inst));
        match queue {
            Queue::Shared => self.reads.push(id),
            Queue::Local => self.private_reads.push((first..end, id)),
            Queue::Value | Queue::Immediate => (),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Host {
    expand: unsafe extern "C" fn(*mut Pending, usize),
    pop: unsafe extern "C" fn(*mut Pending) -> Work,
    emit: unsafe extern "C" fn(*mut Pending, usize),
    count: unsafe extern "C" fn(*const Pending, Clear) -> usize,
    read_at: unsafe extern "C" fn(*const Pending, usize) -> usize,
    private_at: unsafe extern "C" fn(*const Pending, usize) -> PrivateRead,
    retain: unsafe extern "C" fn(*mut Pending, usize, usize),
    clear: unsafe extern "C" fn(*mut Pending, Clear, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::read_prepare"]
    fn read_prepare(pending: *mut Pending, input: Input, first: usize, end: usize, host: Host) -> Queue;
    #[link_name = "bootstrap.ir_dataflow::read_demand"]
    fn read_demand(pending: *mut Pending, id: usize, host: Host);
    #[link_name = "bootstrap.ir_dataflow::read_finish"]
    fn read_finish(pending: *mut Pending, host: Host);
}

unsafe extern "C" fn pop(pending: *mut Pending) -> Work {
    unsafe { &mut *pending }.work.pop().unwrap_or(Work { id: 0, kind: WorkKind::Done })
}

unsafe extern "C" fn expand(pending: *mut Pending, id: usize) {
    let p = unsafe { &mut *pending };
    if let Some(inst) = &p.code[id] {
        p.work.push(Work { id, kind: WorkKind::Emit });
        let first_dependency = p.work.len();
        inst.all_operands(&mut |reg| {
            if let Some(&producer) = p.producers.get(&reg) {
                p.work.push(Work { id: producer, kind: WorkKind::Expand });
            }
            true
        });
        p.work[first_dependency..].reverse();
    }
}

unsafe extern "C" fn emit(pending: *mut Pending, id: usize) {
    let p = unsafe { &mut *pending };
    if let Some(inst) = p.code[id].take() {
        for reg in inst.results() {
            p.producers.remove(&reg);
        }
        p.out.push(inst);
    }
}

unsafe extern "C" fn count(pending: *const Pending, kind: Clear) -> usize {
    let p = unsafe { &*pending };
    match kind {
        Clear::All => p.code.len(),
        Clear::Shared => p.reads.len(),
        Clear::Local => p.private_reads.len(),
    }
}

unsafe extern "C" fn read_at(pending: *const Pending, index: usize) -> usize {
    unsafe { &*pending }.reads[index]
}

unsafe extern "C" fn private_at(pending: *const Pending, index: usize) -> PrivateRead {
    let p = unsafe { &*pending };
    let (range, id) = &p.private_reads[index];
    PrivateRead { id: *id, first: range.start, end: range.end, pending: p.code[*id].is_some() }
}

unsafe extern "C" fn retain(pending: *mut Pending, source: usize, target: usize) {
    let rows = &mut unsafe { &mut *pending }.private_reads;
    let (range, id) = &rows[source];
    rows[target] = (range.start..range.end, *id);
}

unsafe extern "C" fn clear(pending: *mut Pending, kind: Clear, count: usize) {
    let p = unsafe { &mut *pending };
    match kind {
        Clear::All => {
            p.producers.clear();
            p.code.clear();
            p.reads.clear();
            p.private_reads.clear();
        }
        Clear::Shared => p.reads.clear(),
        Clear::Local => p.private_reads.truncate(count),
    }
}

const HOST: Host = Host { expand, pop, emit, count, read_at, private_at, retain, clear };

fn input(inst: &Inst, writes: &[usize]) -> (Input, usize, usize) {
    match &inst.kind {
        InstKind::Constant(..) | InstKind::Index(_) | InstKind::Unary { .. }
        | InstKind::Binary { .. } | InstKind::IntegerDivide { .. } | InstKind::Cast { .. }
        | InstKind::Select { .. } => (Input::Value, 0, 0),
        InstKind::Math { op, .. } if operands::math_outgoing(*op) == 0 => (Input::Value, 0, 0),
        InstKind::Load { binding, .. } => (Input::Load, usize::from(writes.contains(binding)), 0),
        InstKind::ViewLoad { .. } => (Input::ViewLoad, 0, 0),
        InstKind::LoadLocal(_, slot) => (Input::LocalRead, *slot, *slot + 1),
        InstKind::LoadLocalIndexed { base, len, .. } => (Input::LocalRead, *base, *base + *len),
        InstKind::StoreLocal(slot, _) => (Input::LocalWrite, *slot, *slot + 1),
        InstKind::StoreLocalIndexed { base, len, .. } => (Input::LocalWrite, *base, *base + *len),
        InstKind::Store { .. } | InstKind::StoreIndexed { .. } | InstKind::LoadIndexed { .. } =>
            (Input::Memory, 0, 0),
        InstKind::Uniform(..) => (Input::Uniform, 0, 0),
        _ => (Input::Barrier, 0, 0),
    }
}

pub(super) fn place(code: &mut Vec<Inst>, writes: &[usize]) {
    let mut pending = Pending::default();
    for mut inst in std::mem::take(code) {
        let (input, first, end) = input(&inst, writes);
        let queue = unsafe { read_prepare(&mut pending, input, first, end, HOST) };
        if !matches!(queue, Queue::Immediate) {
            pending.defer(inst, queue, first, end);
            continue;
        }
        match &mut inst.kind {
            InstKind::If { yes, no, .. } => {
                place(yes, writes);
                place(no, writes);
            }
            InstKind::Loop { condition, body, .. } => {
                place(condition, writes);
                place(body, writes);
            }
            InstKind::Scope(body) => place(body, writes),
            _ => {}
        }
        inst.all_operands(&mut |reg| {
            if let Some(&id) = pending.producers.get(&reg) {
                unsafe { read_demand(&mut pending, id, HOST); }
            }
            true
        });
        pending.out.push(inst);
    }
    unsafe { read_finish(&mut pending, HOST); }
    *code = pending.out;
}
