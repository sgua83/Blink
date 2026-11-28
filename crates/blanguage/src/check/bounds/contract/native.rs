//! B owns the numbered immutable decisions and Clause memo. This bridge
//! supplies typed variables and the temporary values used by the normalizer.
use super::super::facts;
use super::super::fact_fields;
use super::*;
use std::{ffi::c_void, mem::MaybeUninit, ptr};
type Slot = *mut c_void;
type Borrowed = *const c_void;

#[repr(C)]
#[derive(Default)]
pub(super) struct IdentityTable {
    entries: *mut c_void,
    count: usize,
    capacity: usize,
}

impl IdentityTable {
    pub(super) fn storage(&self) -> crate::compiler::AnalysisStorage {
        crate::compiler::AnalysisStorage::entries::<usize, usize>(self.count, Some(self.capacity))
    }
}

#[repr(C)]
#[derive(Default)]
pub(super) struct ClauseMemo {
    pub(super) imported: IdentityTable,
    pub(super) emitted: IdentityTable,
    pub(super) negatives: IdentityTable,
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_index::start_contract_clause_memo"]
    fn start_clause_memo(memo: &mut ClauseMemo);
    #[link_name = "bootstrap.condition_index::drop_contract_clause_memo"]
    fn drop_clause_memo(memo: &mut ClauseMemo);
    #[link_name = "bootstrap.condition_index::contract_memo_imported"]
    fn memo_imported(memo: &ClauseMemo, clause: Borrowed) -> usize;
    #[link_name = "bootstrap.condition_index::remember_contract_import"]
    fn remember_import(memo: &mut ClauseMemo, clause: Borrowed, node: usize);
    #[link_name = "bootstrap.condition_index::contract_memo_emitted"]
    fn memo_emitted(memo: &ClauseMemo, node: usize, retain: u32) -> *mut c_void;
    #[link_name = "bootstrap.condition_index::remember_contract_emission"]
    fn remember_emission(memo: &mut ClauseMemo, node: usize, owned: *mut c_void);
    #[link_name = "bootstrap.condition_index::contract_memo_negative"]
    fn memo_negative(memo: &ClauseMemo, node: usize) -> usize;
    #[link_name = "bootstrap.condition_index::remember_contract_negative"]
    fn remember_negative(memo: &mut ClauseMemo, node: usize, negative: usize);
}

// The memo has unique storage. A stored Clause is an immutable, retained handle.
unsafe impl Send for ClauseMemo {}
unsafe impl Sync for ClauseMemo {}

impl ClauseMemo {
    pub(super) fn new() -> Self {
        let mut memo = Self::default();
        unsafe { start_clause_memo(&mut memo) };
        memo
    }

    fn imported(&self, clause: &Clause) -> Option<usize> {
        let value = unsafe { memo_imported(self, clause.raw()) };
        (value != usize::MAX).then_some(value)
    }

    fn remember_import(&mut self, clause: &Clause, node: usize) {
        unsafe { remember_import(self, clause.raw(), node) };
    }

    fn emitted(&self, node: usize) -> Option<Clause> {
        std::ptr::NonNull::new(unsafe { memo_emitted(self, node, 1) })
            .map(|value| Clause(value.cast()))
    }

    fn has_emitted(&self, node: usize) -> bool {
        !unsafe { memo_emitted(self, node, 0) }.is_null()
    }

    fn remember_emitted(&mut self, node: usize, clause: Clause) {
        unsafe { remember_emission(self, node, clause.into_raw()) };
    }

    fn negative(&self, node: usize) -> Option<usize> {
        let value = unsafe { memo_negative(self, node) };
        (value != usize::MAX).then_some(value)
    }

    fn remember_negative(&mut self, node: usize, negative: usize) {
        unsafe { remember_negative(self, node, negative) };
    }
}

impl Drop for ClauseMemo {
    fn drop(&mut self) {
        unsafe { drop_clause_memo(self) };
    }
}

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


#[repr(C)]
#[derive(Clone, Copy)]
struct DecisionCell {
    end: End,
    value: usize,
}

impl From<DecisionCell> for Cell {
    fn from(value: DecisionCell) -> Self {
        Self { present: 1, end: value.end, value: value.value }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Decision {
    variable: Borrowed,
    cells: *const DecisionCell,
    count: usize,
}

impl Decision {
    fn variable(&self) -> &Variable {
        unsafe { &*self.variable.cast::<Variable>() }
    }

    fn cell(&self, index: usize) -> Cell {
        let cells = unsafe { std::slice::from_raw_parts(self.cells, self.count) };
        cells[index].into()
    }
}

#[repr(C)]
#[derive(Default)]
pub(super) struct DecisionTree {
    records: Slot,
    root: usize,
    count: usize,
    capacity: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_index::intern_contract_decision"]
    fn intern_decision(tree: &mut DecisionTree, key: Decision, variable_bytes: usize,
        compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32) -> usize;
    #[link_name = "bootstrap.condition_index::contract_decision_at"]
    fn decision_at(tree: &DecisionTree, node: usize) -> Decision;
    #[link_name = "bootstrap.condition_index::drop_contract_decisions"]
    fn drop_decisions(tree: &mut DecisionTree);
}

// This is an exclusively owned tree of immutable, Copy variables and cells.
unsafe impl Send for DecisionTree {}
unsafe impl Sync for DecisionTree {}

unsafe extern "C" fn compare_decision_variable(left: Borrowed, right: Borrowed) -> i32 {
    unsafe { (&*left.cast::<Variable>()).cmp(&*right.cast::<Variable>()) as i32 }
}

impl DecisionTree {
    fn intern(&mut self, variable: &Variable, cells: &[DecisionCell]) -> usize {
        // The B owner copies only Copy payloads and reserves 16-byte alignment.
        const { assert!(std::mem::align_of::<Variable>() <= 16); }
        let key = Decision { variable: borrow(variable), cells: cells.as_ptr(), count: cells.len() };
        unsafe { intern_decision(self, key, std::mem::size_of::<Variable>(), compare_decision_variable) }
    }

    fn get(&self, node: usize) -> Decision {
        // Returned payloads stay at their original addresses across inserts.
        // The synchronous normalizer callbacks borrow them until this tree drops.
        unsafe { decision_at(self, node) }
    }

    pub(super) fn storage(&self) -> crate::compiler::AnalysisStorage {
        crate::compiler::AnalysisStorage {
            len: self.count,
            capacity: Some(self.capacity),
            element_bytes: std::mem::size_of::<[usize; 5]>(),
        }
    }
}

impl Drop for DecisionTree {
    fn drop(&mut self) {
        unsafe { drop_decisions(self) };
    }
}

struct Node {
    normal: *mut Normalizer,
    variable: Variable,
    input: std::vec::IntoIter<(u128, usize)>,
    merged: Vec<DecisionCell>,
}
unsafe extern "C" fn node_next(s: Slot) -> Cell {
    cell(unsafe { frame::<Node>(s) }.input.next())
}
unsafe extern "C" fn node_last(s: Slot) -> Cell {
    unsafe { frame::<Node>(s) }.merged.last().copied()
        .map(Cell::from).unwrap_or_else(|| cell(None))
}
unsafe extern "C" fn node_extend(s: Slot, end: End) {
    let n = unsafe { frame::<Node>(s) };
    let last = n.merged.len() - 1;
    n.merged[last].end = end;
}
unsafe extern "C" fn node_push(s: Slot, value: Cell) {
    unsafe { frame::<Node>(s) }
        .merged
        .push(DecisionCell { end: value.end, value: value.value });
}
unsafe extern "C" fn node_count(s: Slot) -> usize {
    unsafe { frame::<Node>(s) }.merged.len()
}
unsafe extern "C" fn node_intern(s: Slot) -> usize {
    let c = unsafe { frame::<Node>(s) };
    unsafe { &mut *c.normal }.decisions.intern(&c.variable, &c.merged)
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
struct Pairs {
    left: Decision,
    right: Decision,
    output: Vec<(u128, usize, usize)>,
}
unsafe extern "C" fn pairs_compare(s: Slot) -> i32 {
    let c = unsafe { frame::<Pairs>(s) };
    c.left.variable().cmp(c.right.variable()) as i32
}
unsafe extern "C" fn pairs_count(s: Slot, side: u32) -> usize {
    let c = unsafe { frame::<Pairs>(s) };
    [c.left, c.right][side as usize].count
}
unsafe extern "C" fn pairs_cell(s: Slot, side: u32, i: usize) -> Cell {
    let c = unsafe { frame::<Pairs>(s) };
    [c.left, c.right][side as usize].cell(i)
}
unsafe extern "C" fn pairs_push(s: Slot, end: End, a: usize, b: usize) {
    unsafe { frame::<Pairs>(s) }
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
        left: normal.decisions.get(a),
        right: normal.decisions.get(b),
        output: Vec::new(),
    };
    let side = unsafe {
        b_pairs(
            (&mut c as *mut Pairs).cast(),
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
    (*[c.left, c.right][side as usize].variable(), c.output)
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
    snapshot: Option<Decision>,
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
        unsafe { &*frame::<Negate>(s).normal }.memo.negative(index_),
    )
}
unsafe extern "C" fn not_remember(s: Slot, index: usize, value: usize) {
    unsafe { &mut *frame::<Negate>(s).normal }
        .memo.remember_negative(index, value);
}
unsafe extern "C" fn not_load(s: Slot, index: usize) {
    let c = unsafe { frame::<Negate>(s) };
    c.snapshot = Some(unsafe { &*c.normal }.decisions.get(index));
}
unsafe extern "C" fn not_count(s: Slot) -> usize {
    unsafe { frame::<Negate>(s) }.snapshot.unwrap().count
}
unsafe extern "C" fn not_cell(s: Slot, i: usize) -> Cell {
    unsafe { frame::<Negate>(s) }.snapshot.unwrap().cell(i)
}
unsafe extern "C" fn not_emit(s: Slot, end: End, value: usize) {
    unsafe { frame::<Negate>(s) }
        .cells
        .push((end.into(), value));
}
unsafe extern "C" fn not_make(s: Slot) -> usize {
    let c = unsafe { frame::<Negate>(s) };
    let v = *c.snapshot.unwrap().variable();
    let cells = std::mem::take(&mut c.cells);
    unsafe { &mut *c.normal }.node(v, cells)
}
unsafe extern "C" fn not_release(s: Slot) {
    unsafe { frame::<Negate>(s) }.snapshot = None;
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
    let mut c = Negate {
        normal,
        pending: Vec::new(),
        snapshot: None,
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

#[derive(Default)]
struct OrderedOperands<'a> {
    priority: Vec<&'a Clause>,
    remaining: Vec<&'a Clause>,
}
impl<'a> OrderedOperands<'a> {
    fn len(&self) -> usize {
        self.priority.len() + self.remaining.len()
    }

    fn at(&self, index: usize) -> &'a Clause {
        let priority_count = self.priority.len();
        if index < priority_count {
            self.priority[index]
        } else {
            self.remaining[index - priority_count]
        }
    }
}

struct Operands<'a, 'b> {
    normal: &'a Normalizer,
    pending: Vec<&'b Clause>,
    seen: HashSet<usize>,
    output: OrderedOperands<'b>,
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
            .memo.imported(unsafe { &*clause.cast::<Clause>() }).is_some(),
    )
}
unsafe extern "C" fn operand_emit(s: Slot, clause: Borrowed, priority: u32) {
    let output = &mut unsafe { frame::<Operands<'_, '_>>(s) }.output;
    let span = if priority != 0 {
        &mut output.priority
    } else {
        &mut output.remaining
    };
    span.push(unsafe { &*clause.cast::<Clause>() });
}
#[repr(C)]
struct OperandsHost {
    pop: unsafe extern "C" fn(Slot) -> Borrowed,
    push: unsafe extern "C" fn(Slot, Borrowed),
    seen: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    imported: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    all_unary: unsafe extern "C" fn(Borrowed) -> u32,
    emit: unsafe extern "C" fn(Slot, Borrowed, u32),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::contract_operands"]
    fn b_operands(context: Slot, either: u32, host: OperandsHost);
}
fn operands<'a>(normal: &Normalizer, root: &'a Clause, or: bool) -> OrderedOperands<'a> {
    let mut c = Operands {
        normal,
        pending: vec![root],
        seen: HashSet::new(),
        output: OrderedOperands::default(),
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
                all_unary,
                emit: operand_emit,
            },
        )
    };
    c.output
}

struct Fold<'a> {
    operands: OrderedOperands<'a>,
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
    operands: OrderedOperands<'a>,
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
    let operands = operands(unsafe { &*c.normal }, clause, or != 0);
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
            .memo.imported(unsafe { &*clause.cast::<Clause>() }),
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
    borrow(unsafe { frame::<Import<'_>>(s) }.operands.at(i))
}
unsafe extern "C" fn import_remember(s: Slot, clause: Borrowed, value: usize) {
    let clause = unsafe { &*clause.cast::<Clause>() };
    let c = unsafe { frame::<Import<'_>>(s) };
    unsafe { &mut *c.normal }
        .memo.remember_import(clause, value);
    c.operands = OrderedOperands::default();
}
#[repr(C)]
struct ImportHost {
    pop: unsafe extern "C" fn(Slot) -> ImportWork,
    visit: unsafe extern "C" fn(Slot, Borrowed),
    start: unsafe extern "C" fn(Slot, Borrowed, u32, usize),
    resume: unsafe extern "C" fn(Slot, Borrowed, u32, usize, usize),
    lookup: unsafe extern "C" fn(Slot, Borrowed) -> Index,
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
        operands: OrderedOperands::default(),
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
    snapshot: Option<Decision>,
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
            .memo.has_emitted(index),
    )
}
unsafe extern "C" fn emit_load(s: Slot, index: usize) {
    let c = unsafe { frame::<Emit>(s) };
    c.snapshot = Some(unsafe { &*c.normal }.decisions.get(index));
}
unsafe extern "C" fn emit_variable(s: Slot) -> VariableView {
    let decision = unsafe { frame::<Emit>(s) }.snapshot.unwrap();
    let variable = decision.variable();
    let (kind, fact) = match variable {
        Variable::Range(id, scalar) => (0, fact_fields::bound(id, scalar, 0, true)),
        Variable::Atom(fact) => (1, fact_fields::Fields::from(fact)),
    };
    VariableView {
        kind,
        fact,
        end: variable.end().into(),
    }
}
unsafe extern "C" fn emit_count(s: Slot) -> usize {
    unsafe { frame::<Emit>(s) }.snapshot.unwrap().count
}
unsafe extern "C" fn emit_cell(s: Slot, i: usize) -> Cell {
    unsafe { frame::<Emit>(s) }.snapshot.unwrap().cell(i)
}
unsafe extern "C" fn emit_bound(s: Slot, fact: fact_fields::Fields) {
    let fact = unsafe { fact_fields::key(fact) };
    unsafe { frame::<Emit>(s) }.facts.push(fact);
}
unsafe extern "C" fn emit_guard(s: Slot, begin: End, end: End) {
    let c = unsafe { frame::<Emit>(s) };
    let variable = *c.snapshot.unwrap().variable();
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
    let guard = normal.memo.emitted(interval).unwrap_or_else(|| {
        let guard = Clause::facts(c.facts.iter().copied().collect());
        normal.memo.remember_emitted(interval, guard.clone());
        guard
    });
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
unsafe extern "C" fn emit_guard_clause(s: Slot) -> Borrowed {
    unsafe { frame::<Emit>(s) }.guard.cast()
}
unsafe extern "C" fn emit_child(s: Slot, index: usize) {
    let c = unsafe { frame::<Emit>(s) };
    let child = unsafe { &*c.normal }.memo.emitted(index).unwrap();
    unsafe {
        c.guard.drop_in_place();
        c.guard.write(child)
    };
}
unsafe extern "C" fn emit_both(s: Slot, index: usize) {
    let c = unsafe { frame::<Emit>(s) };
    let child = unsafe { &*c.normal }.memo.emitted(index).unwrap();
    let guard = unsafe { c.guard.read() };
    unsafe { c.guard.write(Clause::both_node(guard, child)) };
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
            .write(Clause::either_node(previous, term))
    };
}
unsafe extern "C" fn emit_remember(s: Slot, index: usize, found: u32) {
    let c = unsafe { frame::<Emit>(s) };
    let result = if found != 0 {
        unsafe { c.result.read() }
    } else {
        Clause::impossible()
    };
    unsafe { &mut *c.normal }.memo.remember_emitted(index, result);
}
unsafe extern "C" fn emit_release(s: Slot) {
    unsafe { frame::<Emit>(s) }.snapshot = None;
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
    guard_clause: unsafe extern "C" fn(Slot) -> Borrowed,
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
    let mut guard = MaybeUninit::<Clause>::uninit();
    let mut result = MaybeUninit::<Clause>::uninit();
    let mut c = Emit {
        normal,
        pending: Vec::new(),
        snapshot: None,
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
                guard_clause: emit_guard_clause,
                child: emit_child,
                both: emit_both,
                first: emit_first,
                either: emit_either,
                remember: emit_remember,
                release: emit_release,
            },
        )
    };
    normal.memo.emitted(root).unwrap()
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
