//! The original normalizer owns every typed map, work item and result.
//! B receives only synchronous field values and primitive container operations.
use super::super::facts;
use super::super::{condition_graph, fact_fields};
use super::*;
use std::{ffi::c_void, mem::MaybeUninit, ptr};
type Slot = *mut c_void;
type Borrowed = *const c_void;
fn borrow<T>(value: &T) -> Borrowed {
    ptr::from_ref(value).cast()
}
unsafe fn frame<'a, T>(slot: Slot) -> &'a mut T {
    unsafe { &mut *slot.cast() }
}
#[derive(Clone, Copy)]
#[repr(C)]
struct End {
    low: u64,
    high: u64,
}
impl From<u128> for End {
    fn from(value: u128) -> Self {
        Self {
            low: value as u64,
            high: (value >> 64) as u64,
        }
    }
}
impl From<End> for u128 {
    fn from(value: End) -> Self {
        u128::from(value.low) | (u128::from(value.high) << 64)
    }
}
#[repr(C)]
struct Cell {
    present: u32,
    end: End,
    value: usize,
}
fn cell(value: Option<(u128, usize)>) -> Cell {
    match value {
        Some((end, value)) => Cell {
            present: 1,
            end: end.into(),
            value,
        },
        None => Cell {
            present: 0,
            end: 0.into(),
            value: 0,
        },
    }
}
#[repr(C)]
struct Index {
    present: u32,
    value: usize,
}
fn index(value: Option<usize>) -> Index {
    Index {
        present: u32::from(value.is_some()),
        value: value.unwrap_or(0),
    }
}
#[repr(C)]
struct Pair {
    end: End,
    left: usize,
    right: usize,
}
#[repr(C)]
struct Pending {
    present: u32,
    ready: u32,
    index: usize,
}
fn pending(value: Option<(usize, bool)>) -> Pending {
    match value {
        Some((index, ready)) => Pending {
            present: 1,
            ready: u32::from(ready),
            index,
        },
        None => Pending {
            present: 0,
            ready: 0,
            index: 0,
        },
    }
}
#[repr(C)]
struct Operation {
    present: u32,
    ready: u32,
    either: u32,
    left: usize,
    right: usize,
}
type Key = (bool, usize, usize);

struct Node {
    normal: *mut Normalizer,
    variable: Variable,
    input: std::vec::IntoIter<(u128, usize)>,
    merged: Vec<(u128, usize)>,
}
unsafe extern "C" fn node_next(s: Slot) -> Cell {
    cell(unsafe { frame::<Node>(s) }.input.next())
}
unsafe extern "C" fn node_last(s: Slot) -> Cell {
    cell(unsafe { frame::<Node>(s) }.merged.last().copied())
}
unsafe extern "C" fn node_extend(s: Slot, end: End) {
    let n = unsafe { frame::<Node>(s) };
    let last = n.merged.len() - 1;
    n.merged[last].0 = end.into();
}
unsafe extern "C" fn node_push(s: Slot, value: Cell) {
    unsafe { frame::<Node>(s) }
        .merged
        .push((value.end.into(), value.value));
}
unsafe extern "C" fn node_count(s: Slot) -> usize {
    unsafe { frame::<Node>(s) }.merged.len()
}
unsafe extern "C" fn node_intern(s: Slot) -> usize {
    let c = unsafe { frame::<Node>(s) };
    let decision = Decision {
        variable: c.variable,
        cells: std::mem::take(&mut c.merged),
    };
    let n = unsafe { &mut *c.normal };
    if let Some(&i) = n.unique.get(&decision) {
        return i;
    }
    let i = n.nodes.len();
    let decision = Arc::new(decision);
    n.nodes.push(Some(Arc::clone(&decision)));
    n.unique.insert(decision, i);
    i
}
#[repr(C)]
struct NodeHost {
    next: unsafe extern "C" fn(Slot) -> Cell,
    last: unsafe extern "C" fn(Slot) -> Cell,
    extend: unsafe extern "C" fn(Slot, End),
    push: unsafe extern "C" fn(Slot, Cell),
    count: unsafe extern "C" fn(Slot) -> usize,
    intern: unsafe extern "C" fn(Slot) -> usize,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_node"]
    fn b_node(context: Slot, host: NodeHost) -> usize;
}
pub(super) fn node(
    normal: &mut Normalizer,
    variable: Variable,
    cells: Vec<(u128, usize)>,
) -> usize {
    let mut c = Node {
        normal,
        variable,
        input: cells.into_iter(),
        merged: Vec::new(),
    };
    unsafe {
        b_node(
            (&mut c as *mut Node).cast(),
            NodeHost {
                next: node_next,
                last: node_last,
                extend: node_extend,
                push: node_push,
                count: node_count,
                intern: node_intern,
            },
        )
    }
}
struct Pairs<'a> {
    left: &'a Decision,
    right: &'a Decision,
    output: Vec<(u128, usize, usize)>,
}
unsafe extern "C" fn pairs_compare(s: Slot) -> i32 {
    let c = unsafe { frame::<Pairs<'_>>(s) };
    c.left.variable.cmp(&c.right.variable) as i32
}
unsafe extern "C" fn pairs_count(s: Slot, side: u32) -> usize {
    let c = unsafe { frame::<Pairs<'_>>(s) };
    [c.left, c.right][side as usize].cells.len()
}
unsafe extern "C" fn pairs_cell(s: Slot, side: u32, i: usize) -> Cell {
    let c = unsafe { frame::<Pairs<'_>>(s) };
    cell(Some([c.left, c.right][side as usize].cells[i]))
}
unsafe extern "C" fn pairs_push(s: Slot, end: End, a: usize, b: usize) {
    unsafe { frame::<Pairs<'_>>(s) }
        .output
        .push((end.into(), a, b));
}
#[repr(C)]
struct PairHost {
    compare: unsafe extern "C" fn(Slot) -> i32,
    count: unsafe extern "C" fn(Slot, u32) -> usize,
    cell: unsafe extern "C" fn(Slot, u32, usize) -> Cell,
    push: unsafe extern "C" fn(Slot, End, usize, usize),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_pairs"]
    fn b_pairs(context: Slot, left: usize, right: usize, host: PairHost) -> u32;
}
pub(super) fn pairs(
    normal: &Normalizer,
    a: usize,
    b: usize,
) -> (Variable, Vec<(u128, usize, usize)>) {
    let mut c = Pairs {
        left: normal.nodes[a].as_ref().unwrap(),
        right: normal.nodes[b].as_ref().unwrap(),
        output: Vec::new(),
    };
    let side = unsafe {
        b_pairs(
            (&mut c as *mut Pairs<'_>).cast(),
            a,
            b,
            PairHost {
                compare: pairs_compare,
                count: pairs_count,
                cell: pairs_cell,
                push: pairs_push,
            },
        )
    };
    ([c.left, c.right][side as usize].variable, c.output)
}

struct Operations {
    normal: *mut Normalizer,
    pending: Vec<(Key, bool)>,
    variable: MaybeUninit<Variable>,
    pairs: Vec<(u128, usize, usize)>,
    cells: Vec<(u128, usize)>,
}
unsafe extern "C" fn op_pop(s: Slot) -> Operation {
    match unsafe { frame::<Operations>(s) }.pending.pop() {
        Some(((either, left, right), ready)) => Operation {
            present: 1,
            ready: u32::from(ready),
            either: u32::from(either),
            left,
            right,
        },
        None => Operation {
            present: 0,
            ready: 0,
            either: 0,
            left: 0,
            right: 0,
        },
    }
}
unsafe extern "C" fn op_push(s: Slot, v: Operation) {
    unsafe { frame::<Operations>(s) }
        .pending
        .push(((v.either != 0, v.left, v.right), v.ready != 0));
}
unsafe extern "C" fn op_lookup(s: Slot, or: u32, a: usize, b: usize) -> Index {
    let n = unsafe { &*frame::<Operations>(s).normal };
    index(n.operations.get(&(or != 0, a, b)).copied())
}
unsafe extern "C" fn op_remember(s: Slot, or: u32, a: usize, b: usize, value: usize) {
    let n = unsafe { &mut *frame::<Operations>(s).normal };
    n.operations.insert((or != 0, a, b), value);
}
unsafe extern "C" fn op_prepare(s: Slot, a: usize, b: usize) {
    let c = unsafe { frame::<Operations>(s) };
    let (v, p) = unsafe { &*c.normal }.pairs(a, b);
    c.variable.write(v);
    c.pairs = p;
}
unsafe extern "C" fn op_count(s: Slot) -> usize {
    unsafe { frame::<Operations>(s) }.pairs.len()
}
unsafe extern "C" fn op_pair(s: Slot, i: usize) -> Pair {
    let (end, left, right) = unsafe { frame::<Operations>(s) }.pairs[i];
    Pair {
        end: end.into(),
        left,
        right,
    }
}
unsafe extern "C" fn op_cell(s: Slot, end: End, value: usize) {
    unsafe { frame::<Operations>(s) }
        .cells
        .push((end.into(), value));
}
unsafe extern "C" fn op_make(s: Slot) -> usize {
    let c = unsafe { frame::<Operations>(s) };
    let cells = std::mem::take(&mut c.cells);
    unsafe { &mut *c.normal }.node(unsafe { c.variable.assume_init() }, cells)
}
unsafe extern "C" fn op_release(s: Slot) {
    unsafe { frame::<Operations>(s) }.pairs.clear();
}
#[repr(C)]
struct OperationHost {
    pop: unsafe extern "C" fn(Slot) -> Operation,
    push: unsafe extern "C" fn(Slot, Operation),
    lookup: unsafe extern "C" fn(Slot, u32, usize, usize) -> Index,
    remember: unsafe extern "C" fn(Slot, u32, usize, usize, usize),
    prepare: unsafe extern "C" fn(Slot, usize, usize),
    count: unsafe extern "C" fn(Slot) -> usize,
    pair: unsafe extern "C" fn(Slot, usize) -> Pair,
    cell: unsafe extern "C" fn(Slot, End, usize),
    make: unsafe extern "C" fn(Slot) -> usize,
    release: unsafe extern "C" fn(Slot),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_operation"]
    fn b_operation(
        context: Slot,
        left: usize,
        right: usize,
        either: u32,
        host: OperationHost,
    ) -> usize;
}
pub(super) fn operation(normal: &mut Normalizer, a: usize, b: usize, or: bool) -> usize {
    let mut c = Operations {
        normal,
        pending: Vec::new(),
        variable: MaybeUninit::uninit(),
        pairs: Vec::new(),
        cells: Vec::new(),
    };
    unsafe {
        b_operation(
            (&mut c as *mut Operations).cast(),
            a,
            b,
            u32::from(or),
            OperationHost {
                pop: op_pop,
                push: op_push,
                lookup: op_lookup,
                remember: op_remember,
                prepare: op_prepare,
                count: op_count,
                pair: op_pair,
                cell: op_cell,
                make: op_make,
                release: op_release,
            },
        )
    }
}

struct Negate {
    normal: *mut Normalizer,
    pending: Vec<(usize, bool)>,
    snapshot: *mut Arc<Decision>,
    cells: Vec<(u128, usize)>,
}
unsafe extern "C" fn not_pop(s: Slot) -> Pending {
    pending(unsafe { frame::<Negate>(s) }.pending.pop())
}
unsafe extern "C" fn not_push(s: Slot, index: usize, ready: u32) {
    unsafe { frame::<Negate>(s) }
        .pending
        .push((index, ready != 0));
}
unsafe extern "C" fn not_lookup(s: Slot, index_: usize) -> Index {
    index(
        unsafe { &*frame::<Negate>(s).normal }
            .negatives
            .get(&index_)
            .copied(),
    )
}
unsafe extern "C" fn not_remember(s: Slot, index: usize, value: usize) {
    unsafe { &mut *frame::<Negate>(s).normal }
        .negatives
        .insert(index, value);
}
unsafe extern "C" fn not_load(s: Slot, index: usize) {
    let c = unsafe { frame::<Negate>(s) };
    let value = unsafe { &*c.normal }.nodes[index].as_ref().unwrap().clone();
    unsafe { c.snapshot.write(value) };
}
unsafe extern "C" fn not_count(s: Slot) -> usize {
    let p = unsafe { frame::<Negate>(s) }.snapshot;
    unsafe { &*p }.cells.len()
}
unsafe extern "C" fn not_cell(s: Slot, i: usize) -> Cell {
    let p = unsafe { frame::<Negate>(s) }.snapshot;
    cell(Some(unsafe { &*p }.cells[i]))
}
unsafe extern "C" fn not_emit(s: Slot, end: End, value: usize) {
    unsafe { frame::<Negate>(s) }
        .cells
        .push((end.into(), value));
}
unsafe extern "C" fn not_make(s: Slot) -> usize {
    let c = unsafe { frame::<Negate>(s) };
    let v = unsafe { &*c.snapshot }.variable;
    let cells = std::mem::take(&mut c.cells);
    unsafe { &mut *c.normal }.node(v, cells)
}
unsafe extern "C" fn not_release(s: Slot) {
    unsafe { frame::<Negate>(s).snapshot.drop_in_place() };
}
#[repr(C)]
struct NotHost {
    pop: unsafe extern "C" fn(Slot) -> Pending,
    push: unsafe extern "C" fn(Slot, usize, u32),
    lookup: unsafe extern "C" fn(Slot, usize) -> Index,
    remember: unsafe extern "C" fn(Slot, usize, usize),
    load: unsafe extern "C" fn(Slot, usize),
    count: unsafe extern "C" fn(Slot) -> usize,
    cell: unsafe extern "C" fn(Slot, usize) -> Cell,
    emit: unsafe extern "C" fn(Slot, End, usize),
    make: unsafe extern "C" fn(Slot) -> usize,
    release: unsafe extern "C" fn(Slot),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_not"]
    fn b_not(context: Slot, root: usize, host: NotHost) -> usize;
}
pub(super) fn not(normal: &mut Normalizer, root: usize) -> usize {
    let mut snapshot = MaybeUninit::<Arc<Decision>>::uninit();
    let mut c = Negate {
        normal,
        pending: Vec::new(),
        snapshot: snapshot.as_mut_ptr(),
        cells: Vec::new(),
    };
    unsafe {
        b_not(
            (&mut c as *mut Negate).cast(),
            root,
            NotHost {
                pop: not_pop,
                push: not_push,
                lookup: not_lookup,
                remember: not_remember,
                load: not_load,
                count: not_count,
                cell: not_cell,
                emit: not_emit,
                make: not_make,
                release: not_release,
            },
        )
    }
}

struct Semantic {
    normal: *mut Normalizer,
    scalars: [*mut Scalar; 2],
    length: *mut ValueIdentity,
}
unsafe extern "C" fn sem_atom(s: Slot, fact: fact_fields::Fields) -> usize {
    let fact = unsafe { fact_fields::key(fact) };
    unsafe { &mut *frame::<Semantic>(s).normal }.node(Variable::Atom(fact), vec![(1, 0), (2, 1)])
}
unsafe extern "C" fn sem_range(s: Slot, id: Borrowed, scalar: Borrowed, upper: u64) -> usize {
    let (id, scalar) = unsafe { (*id.cast::<ValueIdentity>(), *scalar.cast::<Scalar>()) };
    unsafe { &mut *frame::<Semantic>(s).normal }.node(
        Variable::Range(id, scalar),
        vec![
            (u128::from(upper) + 1, 1),
            (u128::from(scalar.bit_mask()) + 1, 0),
        ],
    )
}
unsafe extern "C" fn sem_operation(s: Slot, a: usize, b: usize, or: u32) -> usize {
    unsafe { &mut *frame::<Semantic>(s).normal }.operation(a, b, or != 0)
}
unsafe extern "C" fn sem_not(s: Slot, index: usize) -> usize {
    unsafe { &mut *frame::<Semantic>(s).normal }.not(index)
}
unsafe extern "C" fn sem_integer(s: Slot, id: Borrowed, pos: u32) -> Borrowed {
    let c = unsafe { frame::<Semantic>(s) };
    match unsafe { &*c.normal }.integer_type(unsafe { *id.cast::<ValueIdentity>() }) {
        Some(v) => {
            let p = c.scalars[pos as usize];
            unsafe { p.write(v) };
            p.cast()
        }
        None => ptr::null(),
    }
}
unsafe extern "C" fn sem_boolean(s: Slot, id: Borrowed) -> u32 {
    u32::from(
        unsafe { &*frame::<Semantic>(s).normal }
            .types
            .get(unsafe { &*id.cast::<ValueIdentity>() })
            == Some(&Some(Scalar::Bool)),
    )
}
unsafe extern "C" fn sem_length(s: Slot, id: Borrowed) -> Borrowed {
    let c = unsafe { frame::<Semantic>(s) };
    match unsafe { &*c.normal }
        .lengths
        .get(unsafe { &*id.cast::<ValueIdentity>() })
        .copied()
    {
        Some(v) => {
            unsafe { c.length.write(v) };
            c.length.cast()
        }
        None => ptr::null(),
    }
}
#[repr(C)]
struct SemanticHost {
    atom: unsafe extern "C" fn(Slot, fact_fields::Fields) -> usize,
    range: unsafe extern "C" fn(Slot, Borrowed, Borrowed, u64) -> usize,
    operation: unsafe extern "C" fn(Slot, usize, usize, u32) -> usize,
    invert: unsafe extern "C" fn(Slot, usize) -> usize,
    integer: unsafe extern "C" fn(Slot, Borrowed, u32) -> Borrowed,
    boolean: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    length: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    same_scalar: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    scalar: unsafe extern "C" fn(Borrowed) -> fact_fields::ScalarView,
    predicate: unsafe extern "C" fn(Borrowed) -> fact_fields::PredicateView,
    domain: unsafe extern "C" fn(Borrowed) -> fact_fields::DomainView,
    unsigned32: Borrowed,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_fact"]
    fn b_fact(context: Slot, fact: fact_fields::Fields, host: SemanticHost) -> usize;
}
pub(super) fn fact(normal: &mut Normalizer, fact: Fact) -> usize {
    let mut a = MaybeUninit::<Scalar>::uninit();
    let mut b = MaybeUninit::<Scalar>::uninit();
    let mut length = MaybeUninit::<ValueIdentity>::uninit();
    let unsigned32 = Scalar::U32;
    let mut c = Semantic {
        normal,
        scalars: [a.as_mut_ptr(), b.as_mut_ptr()],
        length: length.as_mut_ptr(),
    };
    unsafe {
        b_fact(
            (&mut c as *mut Semantic).cast(),
            fact_fields::Fields::from(&fact),
            SemanticHost {
                atom: sem_atom,
                range: sem_range,
                operation: sem_operation,
                invert: sem_not,
                integer: sem_integer,
                boolean: sem_boolean,
                length: sem_length,
                compare: fact_fields::compare,
                same_scalar: fact_fields::same_scalar,
                scalar: fact_fields::scalar,
                predicate: fact_fields::predicate,
                domain: fact_fields::domain,
                unsigned32: borrow(&unsigned32),
            },
        )
    }
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_unary"]
    fn b_unary(
        fact: fact_fields::Fields,
        predicate: unsafe extern "C" fn(Borrowed) -> fact_fields::PredicateView,
        domain: unsafe extern "C" fn(Borrowed) -> fact_fields::DomainView,
    ) -> u32;
    #[link_name = "bootstrap.condition_graph::contract_all_unary"]
    fn b_all_unary(
        context: Slot,
        next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
        predicate: unsafe extern "C" fn(Borrowed) -> fact_fields::PredicateView,
        domain: unsafe extern "C" fn(Borrowed) -> fact_fields::DomainView,
    ) -> u32;
}
struct Unary<'a> {
    input: facts::Iter<'a>,
}
unsafe extern "C" fn unary_next(s: Slot) -> fact_fields::Fields {
    unsafe { frame::<Unary<'_>>(s) }
        .input
        .next()
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn all_unary(facts: Borrowed) -> u32 {
    let mut c = Unary {
        input: unsafe { &*facts.cast::<Facts>() }.iter(),
    };
    unsafe {
        b_all_unary(
            (&mut c as *mut Unary<'_>).cast(),
            unary_next,
            fact_fields::predicate,
            fact_fields::domain,
        )
    }
}
#[repr(C)]
struct PriorityHost {
    inspect: unsafe extern "C" fn(Borrowed) -> condition_graph::SourceNode,
    all_unary: unsafe extern "C" fn(Borrowed) -> u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_priority"]
    fn b_priority(source: Borrowed, host: PriorityHost) -> u32;
}
unsafe extern "C" fn source(source: Borrowed) -> condition_graph::SourceNode {
    condition_graph::source_node(unsafe { &*source.cast::<Clause>() })
}
fn priority(clause: &Clause) -> bool {
    (unsafe {
        b_priority(
            borrow(clause),
            PriorityHost {
                inspect: source,
                all_unary,
            },
        )
    }) != 0
}

struct Operands<'a, 'b> {
    normal: &'a Normalizer,
    pending: Vec<&'b Clause>,
    seen: HashSet<usize>,
    output: Vec<&'b Clause>,
}
unsafe extern "C" fn operand_pop(s: Slot) -> Borrowed {
    unsafe { frame::<Operands<'_, '_>>(s) }
        .pending
        .pop()
        .map(borrow)
        .unwrap_or(ptr::null())
}
unsafe extern "C" fn operand_push(s: Slot, clause: Borrowed) {
    unsafe { frame::<Operands<'_, '_>>(s) }
        .pending
        .push(unsafe { &*clause.cast::<Clause>() });
}
unsafe extern "C" fn operand_seen(s: Slot, clause: Borrowed) -> u32 {
    u32::from(
        unsafe { frame::<Operands<'_, '_>>(s) }
            .seen
            .insert(unsafe { &*clause.cast::<Clause>() }.key()),
    )
}
unsafe extern "C" fn operand_imported(s: Slot, clause: Borrowed) -> u32 {
    u32::from(
        unsafe { frame::<Operands<'_, '_>>(s) }
            .normal
            .clauses
            .contains_key(&unsafe { &*clause.cast::<Clause>() }.key()),
    )
}
unsafe extern "C" fn operand_emit(s: Slot, clause: Borrowed) {
    unsafe { frame::<Operands<'_, '_>>(s) }
        .output
        .push(unsafe { &*clause.cast::<Clause>() });
}
#[repr(C)]
struct OperandsHost {
    pop: unsafe extern "C" fn(Slot) -> Borrowed,
    push: unsafe extern "C" fn(Slot, Borrowed),
    seen: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    imported: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    inspect: unsafe extern "C" fn(Borrowed) -> condition_graph::SourceNode,
    emit: unsafe extern "C" fn(Slot, Borrowed),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_operands"]
    fn b_operands(context: Slot, either: u32, host: OperandsHost);
}
pub(super) fn operands<'a>(normal: &Normalizer, root: &'a Clause, or: bool) -> Vec<&'a Clause> {
    let mut c = Operands {
        normal,
        pending: vec![root],
        seen: HashSet::new(),
        output: Vec::new(),
    };
    unsafe {
        b_operands(
            (&mut c as *mut Operands<'_, '_>).cast(),
            u32::from(or),
            OperandsHost {
                pop: operand_pop,
                push: operand_push,
                seen: operand_seen,
                imported: operand_imported,
                inspect: source,
                emit: operand_emit,
            },
        )
    };
    c.output.sort_by_key(|c| !priority(c));
    c.output
}

struct Fold<'a> {
    operands: Vec<&'a Clause>,
    or: bool,
    next: usize,
    result: usize,
}
struct Work<'a> {
    clause: &'a Clause,
    fold: Option<Fold<'a>>,
}
struct Import<'a> {
    normal: *mut Normalizer,
    pending: Vec<Work<'a>>,
    operands: Vec<&'a Clause>,
    facts: Option<facts::Iter<'a>>,
}
#[repr(C)]
struct ImportWork {
    present: u32,
    fold: u32,
    clause: Borrowed,
    either: u32,
    next: usize,
    result: usize,
    count: usize,
}
unsafe extern "C" fn import_pop(s: Slot) -> ImportWork {
    let c = unsafe { frame::<Import<'_>>(s) };
    match c.pending.pop() {
        None => ImportWork {
            present: 0,
            fold: 0,
            clause: ptr::null(),
            either: 0,
            next: 0,
            result: 0,
            count: 0,
        },
        Some(Work { clause, fold }) => match fold {
            None => ImportWork {
                present: 1,
                fold: 0,
                clause: borrow(clause),
                either: 0,
                next: 0,
                result: 0,
                count: 0,
            },
            Some(Fold {
                operands,
                or,
                next,
                result,
            }) => {
                let count = operands.len();
                c.operands = operands;
                ImportWork {
                    present: 1,
                    fold: 1,
                    clause: borrow(clause),
                    either: u32::from(or),
                    next,
                    result,
                    count,
                }
            }
        },
    }
}
unsafe extern "C" fn import_visit(s: Slot, clause: Borrowed) {
    unsafe { frame::<Import<'_>>(s) }.pending.push(Work {
        clause: unsafe { &*clause.cast::<Clause>() },
        fold: None,
    });
}
unsafe extern "C" fn import_start(s: Slot, clause: Borrowed, or: u32, result: usize) {
    let c = unsafe { frame::<Import<'_>>(s) };
    let clause = unsafe { &*clause.cast::<Clause>() };
    let operands = unsafe { &*c.normal }.operands(clause, or != 0);
    c.pending.push(Work {
        clause,
        fold: Some(Fold {
            operands,
            or: or != 0,
            next: 0,
            result,
        }),
    });
}
unsafe extern "C" fn import_resume(s: Slot, clause: Borrowed, or: u32, next: usize, result: usize) {
    let c = unsafe { frame::<Import<'_>>(s) };
    let operands = std::mem::take(&mut c.operands);
    c.pending.push(Work {
        clause: unsafe { &*clause.cast::<Clause>() },
        fold: Some(Fold {
            operands,
            or: or != 0,
            next,
            result,
        }),
    });
}
unsafe extern "C" fn import_lookup(s: Slot, clause: Borrowed) -> Index {
    index(
        unsafe { &*frame::<Import<'_>>(s).normal }
            .clauses
            .get(&unsafe { &*clause.cast::<Clause>() }.key())
            .map(|(_, i)| *i),
    )
}
unsafe extern "C" fn import_begin(s: Slot, facts: Borrowed) {
    unsafe { frame::<Import<'_>>(s) }.facts = Some(unsafe { &*facts.cast::<Facts>() }.iter());
}
unsafe extern "C" fn import_next(s: Slot) -> fact_fields::Fields {
    unsafe { frame::<Import<'_>>(s) }
        .facts
        .as_mut()
        .and_then(Iterator::next)
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn import_fact(s: Slot, fact: fact_fields::Fields) -> usize {
    let fact = unsafe { fact_fields::key(fact) };
    unsafe { &mut *frame::<Import<'_>>(s).normal }.fact(fact)
}
unsafe extern "C" fn import_operation(s: Slot, a: usize, b: usize, or: u32) -> usize {
    unsafe { &mut *frame::<Import<'_>>(s).normal }.operation(a, b, or != 0)
}
unsafe extern "C" fn import_operand(s: Slot, i: usize) -> Borrowed {
    borrow(unsafe { frame::<Import<'_>>(s) }.operands[i])
}
unsafe extern "C" fn import_remember(s: Slot, clause: Borrowed, value: usize) {
    let clause = unsafe { &*clause.cast::<Clause>() };
    let c = unsafe { frame::<Import<'_>>(s) };
    unsafe { &mut *c.normal }
        .clauses
        .insert(clause.key(), (clause.clone(), value));
    c.operands.clear();
}
#[repr(C)]
struct ImportHost {
    pop: unsafe extern "C" fn(Slot) -> ImportWork,
    visit: unsafe extern "C" fn(Slot, Borrowed),
    start: unsafe extern "C" fn(Slot, Borrowed, u32, usize),
    resume: unsafe extern "C" fn(Slot, Borrowed, u32, usize, usize),
    lookup: unsafe extern "C" fn(Slot, Borrowed) -> Index,
    inspect: unsafe extern "C" fn(Borrowed) -> condition_graph::SourceNode,
    begin: unsafe extern "C" fn(Slot, Borrowed),
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    predicate: unsafe extern "C" fn(Borrowed) -> fact_fields::PredicateView,
    domain: unsafe extern "C" fn(Borrowed) -> fact_fields::DomainView,
    fact: unsafe extern "C" fn(Slot, fact_fields::Fields) -> usize,
    operation: unsafe extern "C" fn(Slot, usize, usize, u32) -> usize,
    operand: unsafe extern "C" fn(Slot, usize) -> Borrowed,
    remember: unsafe extern "C" fn(Slot, Borrowed, usize),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_import"]
    fn b_import(context: Slot, root: Borrowed, host: ImportHost) -> usize;
}
pub(super) fn import(normal: &mut Normalizer, root: &Clause) -> usize {
    let mut c = Import {
        normal,
        pending: Vec::new(),
        operands: Vec::new(),
        facts: None,
    };
    unsafe {
        b_import(
            (&mut c as *mut Import<'_>).cast(),
            borrow(root),
            ImportHost {
                pop: import_pop,
                visit: import_visit,
                start: import_start,
                resume: import_resume,
                lookup: import_lookup,
                inspect: source,
                begin: import_begin,
                next: import_next,
                predicate: fact_fields::predicate,
                domain: fact_fields::domain,
                fact: import_fact,
                operation: import_operation,
                operand: import_operand,
                remember: import_remember,
            },
        )
    }
}

struct Emit {
    normal: *mut Normalizer,
    pending: Vec<(usize, bool)>,
    snapshot: *mut Arc<Decision>,
    guard: *mut Clause,
    result: *mut Clause,
    facts: Vec<Fact>,
}
#[repr(C)]
struct VariableView {
    kind: u32,
    fact: fact_fields::Fields,
    end: End,
}
unsafe extern "C" fn emit_pop(s: Slot) -> Pending {
    pending(unsafe { frame::<Emit>(s) }.pending.pop())
}
unsafe extern "C" fn emit_push(s: Slot, index: usize, ready: u32) {
    unsafe { frame::<Emit>(s) }
        .pending
        .push((index, ready != 0));
}
unsafe extern "C" fn emit_known(s: Slot, index: usize) -> u32 {
    u32::from(
        unsafe { &*frame::<Emit>(s).normal }
            .emitted
            .contains_key(&index),
    )
}
unsafe extern "C" fn emit_load(s: Slot, index: usize) {
    let c = unsafe { frame::<Emit>(s) };
    let d = unsafe { &*c.normal }.nodes[index].as_ref().unwrap().clone();
    unsafe { c.snapshot.write(d) };
}
unsafe extern "C" fn emit_variable(s: Slot) -> VariableView {
    let p = unsafe { frame::<Emit>(s) }.snapshot;
    let d = unsafe { &*p };
    let (kind, fact) = match &d.variable {
        Variable::Range(id, scalar) => (0, fact_fields::bound(id, scalar, 0, true)),
        Variable::Atom(fact) => (1, fact_fields::Fields::from(fact)),
    };
    VariableView {
        kind,
        fact,
        end: d.variable.end().into(),
    }
}
unsafe extern "C" fn emit_count(s: Slot) -> usize {
    let p = unsafe { frame::<Emit>(s) }.snapshot;
    unsafe { &*p }.cells.len()
}
unsafe extern "C" fn emit_cell(s: Slot, i: usize) -> Cell {
    let p = unsafe { frame::<Emit>(s) }.snapshot;
    cell(Some(unsafe { &*p }.cells[i]))
}
unsafe extern "C" fn emit_bound(s: Slot, fact: fact_fields::Fields) {
    let fact = unsafe { fact_fields::key(fact) };
    unsafe { frame::<Emit>(s) }.facts.push(fact);
}
unsafe extern "C" fn emit_guard(s: Slot, begin: End, end: End) {
    let c = unsafe { frame::<Emit>(s) };
    let variable = unsafe { &*c.snapshot }.variable;
    let (begin, end) = (u128::from(begin), u128::from(end));
    let limit = variable.end();
    let normal = unsafe { &mut *c.normal };
    let mut cells = Vec::with_capacity(3);
    if begin != 0 {
        cells.push((begin, 0));
    }
    cells.push((end, 1));
    if end != limit {
        cells.push((limit, 0));
    }
    let interval = normal.node(variable, cells);
    let guard = normal.emitted.entry(interval)
        .or_insert_with(|| Clause::facts(c.facts.iter().copied().collect()))
        .clone();
    c.facts.clear();
    unsafe { c.guard.write(guard) };
}
unsafe extern "C" fn emit_atom(s: Slot, fact: fact_fields::Fields) {
    let fact = unsafe { fact_fields::key(fact) };
    let p = unsafe { frame::<Emit>(s) }.guard;
    unsafe { p.write(Clause::facts([fact].into())) };
}
unsafe extern "C" fn emit_negate(s: Slot) {
    let p = unsafe { frame::<Emit>(s) }.guard;
    let value = unsafe { &*p }.negate();
    unsafe {
        p.drop_in_place();
        p.write(value)
    };
}
unsafe extern "C" fn emit_guard_node(s: Slot) -> condition_graph::SourceNode {
    let p = unsafe { frame::<Emit>(s) }.guard;
    condition_graph::source_node(unsafe { &*p })
}
unsafe extern "C" fn emit_child(s: Slot, index: usize) {
    let c = unsafe { frame::<Emit>(s) };
    let child = unsafe { &*c.normal }.emitted[&index].clone();
    unsafe {
        c.guard.drop_in_place();
        c.guard.write(child)
    };
}
unsafe extern "C" fn emit_both(s: Slot, index: usize) {
    let c = unsafe { frame::<Emit>(s) };
    let child = unsafe { &*c.normal }.emitted[&index].clone();
    let guard = unsafe { c.guard.read() };
    unsafe { c.guard.write(Clause::node(ClauseNode::Both(guard, child))) };
}
unsafe extern "C" fn emit_first(s: Slot) {
    let c = unsafe { frame::<Emit>(s) };
    unsafe { c.result.write(c.guard.read()) };
}
unsafe extern "C" fn emit_either(s: Slot) {
    let c = unsafe { frame::<Emit>(s) };
    let (previous, term) = unsafe { (c.result.read(), c.guard.read()) };
    unsafe {
        c.result
            .write(Clause::node(ClauseNode::Either(previous, term)))
    };
}
unsafe extern "C" fn emit_remember(s: Slot, index: usize, found: u32) {
    let c = unsafe { frame::<Emit>(s) };
    let result = if found != 0 {
        unsafe { c.result.read() }
    } else {
        Clause::impossible()
    };
    unsafe { &mut *c.normal }.emitted.insert(index, result);
}
unsafe extern "C" fn emit_release(s: Slot) {
    unsafe { frame::<Emit>(s).snapshot.drop_in_place() };
}
#[repr(C)]
struct EmitHost {
    pop: unsafe extern "C" fn(Slot) -> Pending,
    push: unsafe extern "C" fn(Slot, usize, u32),
    known: unsafe extern "C" fn(Slot, usize) -> u32,
    load: unsafe extern "C" fn(Slot, usize),
    variable: unsafe extern "C" fn(Slot) -> VariableView,
    count: unsafe extern "C" fn(Slot) -> usize,
    cell: unsafe extern "C" fn(Slot, usize) -> Cell,
    bound: unsafe extern "C" fn(Slot, fact_fields::Fields),
    guard: unsafe extern "C" fn(Slot, End, End),
    atom: unsafe extern "C" fn(Slot, fact_fields::Fields),
    negate: unsafe extern "C" fn(Slot),
    guard_node: unsafe extern "C" fn(Slot) -> condition_graph::SourceNode,
    child: unsafe extern "C" fn(Slot, usize),
    both: unsafe extern "C" fn(Slot, usize),
    first: unsafe extern "C" fn(Slot),
    either: unsafe extern "C" fn(Slot),
    remember: unsafe extern "C" fn(Slot, usize, u32),
    release: unsafe extern "C" fn(Slot),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_emit"]
    fn b_emit(context: Slot, root: usize, host: EmitHost);
}
pub(super) fn emit(normal: &mut Normalizer, root: usize) -> Clause {
    let mut snapshot = MaybeUninit::<Arc<Decision>>::uninit();
    let mut guard = MaybeUninit::<Clause>::uninit();
    let mut result = MaybeUninit::<Clause>::uninit();
    let mut c = Emit {
        normal,
        pending: Vec::new(),
        snapshot: snapshot.as_mut_ptr(),
        guard: guard.as_mut_ptr(),
        result: result.as_mut_ptr(),
        facts: Vec::with_capacity(2),
    };
    unsafe {
        b_emit(
            (&mut c as *mut Emit).cast(),
            root,
            EmitHost {
                pop: emit_pop,
                push: emit_push,
                known: emit_known,
                load: emit_load,
                variable: emit_variable,
                count: emit_count,
                cell: emit_cell,
                bound: emit_bound,
                guard: emit_guard,
                atom: emit_atom,
                negate: emit_negate,
                guard_node: emit_guard_node,
                child: emit_child,
                both: emit_both,
                first: emit_first,
                either: emit_either,
                remember: emit_remember,
                release: emit_release,
            },
        )
    };
    normal.emitted[&root].clone()
}

struct Union<'a> {
    normal: *mut Normalizer,
    left: &'a Condition,
    right: &'a Condition,
    output: *mut Clause,
}
unsafe extern "C" fn union_case(s: Slot, side: u32, truth: u32) -> Borrowed {
    let c = unsafe { frame::<Union<'_>>(s) };
    borrow(&[c.left, c.right][side as usize].cases[truth as usize])
}
unsafe extern "C" fn union_import(s: Slot, clause: Borrowed) -> usize {
    unsafe { &mut *frame::<Union<'_>>(s).normal }.import(unsafe { &*clause.cast::<Clause>() })
}
unsafe extern "C" fn union_operation(s: Slot, a: usize, b: usize, or: u32) -> usize {
    unsafe { &mut *frame::<Union<'_>>(s).normal }.operation(a, b, or != 0)
}
unsafe extern "C" fn union_emit(s: Slot, truth: u32, index: usize) {
    let c = unsafe { frame::<Union<'_>>(s) };
    let value = unsafe { &mut *c.normal }.emit(index);
    unsafe { c.output.add(truth as usize).write(value) };
}
#[repr(C)]
struct UnionHost {
    case: unsafe extern "C" fn(Slot, u32, u32) -> Borrowed,
    import: unsafe extern "C" fn(Slot, Borrowed) -> usize,
    operation: unsafe extern "C" fn(Slot, usize, usize, u32) -> usize,
    emit: unsafe extern "C" fn(Slot, u32, usize),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_union"]
    fn b_union(context: Slot, host: UnionHost);
}
pub(super) fn union(normal: &mut Normalizer, left: &Condition, right: &Condition) -> Condition {
    let mut output = MaybeUninit::<[Clause; 2]>::uninit();
    let mut c = Union {
        normal,
        left,
        right,
        output: output.as_mut_ptr().cast(),
    };
    unsafe {
        b_union(
            (&mut c as *mut Union<'_>).cast(),
            UnionHost {
                case: union_case,
                import: union_import,
                operation: union_operation,
                emit: union_emit,
            },
        )
    };
    Condition {
        cases: unsafe { output.assume_init() },
    }
}
