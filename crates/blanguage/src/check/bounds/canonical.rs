//! B controls source-path canonicalization. Native frames hold the original
//! typed sets and iterators until their synchronous last consumer.
use super::facts;
use super::{
    Clause, Facts, Scalar, ValueIdentity, condition_graph, contradictory, fact_fields, numeric_bounds,
};
use std::{
    collections::{BTreeMap, BTreeSet, btree_map, btree_set},
    ffi::c_void,
    ops::Bound::{Excluded, Unbounded},
    ptr,
};
type Slot = *mut c_void;
type Borrowed = *const c_void;
type Terms = BTreeSet<Facts>;
type Bounds = BTreeMap<(ValueIdentity, Scalar), u64>;
fn borrowed<T>(value: &T) -> Borrowed {
    ptr::from_ref(value).cast()
}
#[repr(C)]
struct Term {
    present: u32,
    value: Borrowed,
}
fn term(value: Option<&Facts>) -> Term {
    match value {
        Some(value) => Term {
            present: 1,
            value: borrowed(value),
        },
        None => Term {
            present: 0,
            value: ptr::null(),
        },
    }
}
unsafe extern "C" fn subset(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe { (&*left.cast::<Facts>()).is_subset(&*right.cast::<Facts>()) })
}
unsafe extern "C" fn conflict(source: Borrowed) -> u32 {
    u32::from(contradictory(unsafe { &*source.cast::<Facts>() }))
}
struct Proof<'a> {
    left: std::borrow::Cow<'a, Facts>,
    right: &'a Facts,
    next: facts::Iter<'a>,
}
unsafe fn proof<'a, 'b>(slot: Slot) -> &'a mut Proof<'b> {
    unsafe { &mut *slot.cast::<Proof<'b>>() }
}
unsafe extern "C" fn same_term(slot: Slot) -> u32 {
    let proof = unsafe { proof(slot) };
    u32::from(proof.left.as_ref() == proof.right)
}
unsafe extern "C" fn proof_conflict(slot: Slot, side: u32) -> u32 {
    let proof = unsafe { proof(slot) };
    u32::from(contradictory(if side == 0 { proof.left.as_ref() } else { proof.right }))
}
unsafe extern "C" fn proof_next(slot: Slot) -> fact_fields::Fields {
    unsafe { proof(slot) }.next.next().map(fact_fields::Fields::from).unwrap_or_default()
}
unsafe extern "C" fn proof_insert(slot: Slot, fact: fact_fields::Fields) -> u32 {
    let fact = unsafe { fact_fields::key(fact) };
    let proof = unsafe { proof(slot) };
    if proof.left.contains(&fact) { return 0; }
    u32::from(proof.left.to_mut().insert(fact))
}
unsafe extern "C" fn proof_remove(slot: Slot, fact: fact_fields::Fields) {
    unsafe { proof(slot) }.left.to_mut().remove(&unsafe { fact_fields::key(fact) });
}
unsafe extern "C" fn proof_remaining(slot: Slot) -> u32 {
    u32::from(!unsafe { proof(slot) }.next.is_empty())
}
#[repr(C)]
struct ProofHost {
    same: unsafe extern "C" fn(Slot) -> u32,
    conflict: unsafe extern "C" fn(Slot, u32) -> u32,
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    insert: unsafe extern "C" fn(Slot, fact_fields::Fields) -> u32,
    remove: unsafe extern "C" fn(Slot, fact_fields::Fields),
    remaining: unsafe extern "C" fn(Slot) -> u32,
    facts: fact_fields::Host,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_term_overlap"]
    fn b_overlap(context: Slot, host: ProofHost) -> u32;
}
unsafe extern "C" fn overlap(left: Borrowed, right: Borrowed) -> u32 {
    let (left, right) = unsafe { (&*left.cast::<Facts>(), &*right.cast::<Facts>()) };
    let mut proof = Proof { left: std::borrow::Cow::Borrowed(left), right, next: right.iter() };
    let unsigned32 = Scalar::U32;
    unsafe {
        b_overlap(
            (&mut proof as *mut Proof<'_>).cast(),
            ProofHost {
                same: same_term, conflict: proof_conflict, next: proof_next,
                insert: proof_insert, remove: proof_remove, remaining: proof_remaining,
                facts: fact_fields::Host { unsigned32: borrowed(&unsigned32), compare: fact_fields::compare },
            },
        )
    }
}
struct Scan<'a> {
    terms: &'a Terms,
    cursor: btree_set::Iter<'a, Facts>,
}
unsafe fn scan<'a, 'b>(slot: Slot) -> &'a mut Scan<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn scan_next(slot: Slot) -> Term {
    term(unsafe { scan(slot) }.cursor.next())
}
unsafe extern "C" fn scan_reset(slot: Slot) {
    let scan = unsafe { scan(slot) };
    scan.cursor = scan.terms.iter();
}
#[repr(u32)]
enum Insertion {
    Reject,
    Insert,
    RetainInsert,
}
#[repr(C)]
struct InsertHost {
    next: unsafe extern "C" fn(Slot) -> Term,
    subset: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    reset: unsafe extern "C" fn(Slot),
    overlap: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
}
#[repr(C)]
struct KeepHost {
    subset: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    overlap: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_insert"]
    fn b_insert(context: Slot, candidate: Borrowed, empty: u32, host: InsertHost) -> Insertion;
    #[link_name = "bootstrap.condition_graph::canonical_keep"]
    fn b_keep(old: Borrowed, candidate: Borrowed, host: KeepHost) -> u32;
}
fn insert(output: &mut Terms, facts: Facts) -> bool {
    let mut scan = Scan { terms: output, cursor: output.iter() };
    let action = unsafe {
        b_insert(
            (&mut scan as *mut Scan<'_>).cast(),
            borrowed(&facts),
            u32::from(output.is_empty()),
            InsertHost { next: scan_next, subset, reset: scan_reset, overlap },
        )
    };
    match action {
        Insertion::Reject => false,
        Insertion::Insert => output.insert(facts),
        Insertion::RetainInsert => {
            output.retain(|old| unsafe {
                b_keep(borrowed(old), borrowed(&facts), KeepHost { subset, overlap })
            } != 0);
            output.insert(facts)
        }
    }
}
struct Normal<'a> {
    source: &'a Facts,
    input: facts::Iter<'a>,
    lower: &'a Bounds,
    upper: &'a Bounds,
    bounds: [btree_map::Iter<'a, (ValueIdentity, Scalar), u64>; 2],
    output: &'a mut Facts,
}
unsafe fn normal<'a, 'b>(slot: Slot) -> &'a mut Normal<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn normal_next(slot: Slot) -> fact_fields::Fields {
    unsafe { normal(slot) }
        .input
        .next()
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn bounds_reset(slot: Slot, side: u32) {
    let c = unsafe { normal(slot) };
    c.bounds[side as usize] = [c.lower, c.upper][side as usize].iter();
}
unsafe extern "C" fn bound_next(slot: Slot, side: u32) -> fact_fields::Fields {
    unsafe { normal(slot) }.bounds[side as usize]
        .next()
        .map(|((id, scalar), value)| fact_fields::bound(id, scalar, *value, side == 0))
        .unwrap_or_default()
}
unsafe extern "C" fn contains(slot: Slot, fields: fact_fields::Fields) -> u32 {
    let key = unsafe { fact_fields::key(fields) };
    u32::from(unsafe { normal(slot) }.source.contains(&key))
}
unsafe extern "C" fn emit(slot: Slot, fields: fact_fields::Fields) {
    let key = unsafe { fact_fields::key(fields) };
    unsafe { normal(slot) }.output.insert(key);
}
#[repr(C)]
struct NormalHost {
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    reset: unsafe extern "C" fn(Slot, u32),
    bound: unsafe extern "C" fn(Slot, u32) -> fact_fields::Fields,
    contains: unsafe extern "C" fn(Slot, fact_fields::Fields) -> u32,
    emit: unsafe extern "C" fn(Slot, fact_fields::Fields),
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    scalar: unsafe extern "C" fn(Borrowed) -> fact_fields::ScalarView,
    upper: unsafe extern "C" fn(Borrowed) -> fact_fields::UpperView,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_normalize"]
    fn b_normal(context: Slot, host: NormalHost);
}
unsafe extern "C" fn normalize(source: Borrowed, target: Slot) {
    let source = unsafe { &*source.cast::<Facts>() };
    let (lower,upper)=numeric_bounds::collect(source,None);
    let mut output = Facts::new();
    {
        let mut c = Normal {
            source,
            input: source.iter(),
            lower: &lower,
            upper: &upper,
            bounds: [lower.iter(), upper.iter()],
            output: &mut output,
        };
        unsafe {
            b_normal(
                (&mut c as *mut Normal<'_>).cast(),
                NormalHost {
                    next: normal_next,
                    reset: bounds_reset,
                    bound: bound_next,
                    contains,
                    emit,
                    compare: fact_fields::compare,
                    scalar: fact_fields::scalar,
                    upper: fact_fields::upper,
                },
            )
        };
    }
    insert(unsafe { &mut *target.cast::<Terms>() }, output);
}
#[repr(C)]
struct AcceptHost {
    conflict: unsafe extern "C" fn(Borrowed) -> u32,
    normalize: unsafe extern "C" fn(Borrowed, Slot),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_accept"]
    fn b_accept(source: Borrowed, target: Slot, host: AcceptHost);
}
fn accept(source: &Facts, target: &mut Terms) {
    unsafe {
        b_accept(
            borrowed(source),
            (target as *mut Terms).cast(),
            AcceptHost {
                conflict,
                normalize,
            },
        )
    };
}
struct EitherTerms<'a> {
    input: btree_set::Iter<'a, Facts>,
    output: &'a mut Terms,
}
unsafe fn either_terms<'a, 'b>(slot: Slot) -> &'a mut EitherTerms<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn either_next(slot: Slot) -> Term {
    term(unsafe { either_terms(slot) }.input.next())
}
unsafe extern "C" fn either_insert(slot: Slot, value: Borrowed) {
    let value = unsafe { (&*value.cast::<Facts>()).clone() };
    insert(unsafe { either_terms(slot) }.output, value);
}
#[repr(C)]
struct EitherHost {
    next: unsafe extern "C" fn(Slot) -> Term,
    insert: unsafe extern "C" fn(Slot, Borrowed),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_either_terms"]
    fn b_either(context: Slot, host: EitherHost);
}
struct Product<'a> {
    left: btree_set::Iter<'a, Facts>,
    right_source: &'a Terms,
    right: btree_set::Iter<'a, Facts>,
    output: &'a mut Terms,
}
unsafe fn product<'a, 'b>(slot: Slot) -> &'a mut Product<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn product_left(slot: Slot) -> Term {
    term(unsafe { product(slot) }.left.next())
}
unsafe extern "C" fn product_reset(slot: Slot) {
    let c = unsafe { product(slot) };
    c.right = c.right_source.iter();
}
unsafe extern "C" fn product_right(slot: Slot) -> Term {
    term(unsafe { product(slot) }.right.next())
}
unsafe extern "C" fn product_union(slot: Slot, left: Borrowed, right: Borrowed) {
    let (left, right) = unsafe { (&*left.cast::<Facts>(), &*right.cast::<Facts>()) };
    let facts = left.union(right).into_facts();
    accept(&facts, unsafe { product(slot) }.output);
}
#[repr(C)]
struct ProductHost {
    left: unsafe extern "C" fn(Slot) -> Term,
    reset: unsafe extern "C" fn(Slot),
    right: unsafe extern "C" fn(Slot) -> Term,
    union: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_product"]
    fn b_product(context: Slot, host: ProductHost);
}
struct Graph<'a> {
    nodes: Vec<&'a Clause>,
    values: *mut BTreeMap<usize, Terms>,
}
unsafe fn graph<'a, 'b>(slot: Slot) -> &'a mut Graph<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn count(slot: Slot) -> usize {
    unsafe { graph(slot) }.nodes.len()
}
unsafe extern "C" fn inspect(slot: Slot, index: usize) -> condition_graph::SourceNode {
    condition_graph::source_node(unsafe { graph(slot) }.nodes[index])
}
unsafe extern "C" fn lookup(slot: Slot, source: Borrowed) -> Borrowed {
    let key = unsafe { &*source.cast::<Clause>() }.key();
    let values = unsafe { graph(slot) }.values;
    borrowed(&unsafe { &*values }[&key])
}
fn store(slot: Slot, index: usize, value: Terms) {
    let c = unsafe { graph(slot) };
    let key = c.nodes[index].key();
    unsafe { &mut *c.values }.insert(key, value);
}
unsafe extern "C" fn empty(slot: Slot, index: usize) {
    store(slot, index, Terms::new());
}
unsafe extern "C" fn leaf(slot: Slot, index: usize, source: Borrowed) {
    let mut output = Terms::new();
    accept(unsafe { &*source.cast::<Facts>() }, &mut output);
    store(slot, index, output);
}
unsafe extern "C" fn either(slot: Slot, index: usize, left: Borrowed, right: Borrowed) {
    let (left, right) = unsafe { (&*left.cast::<Terms>(), &*right.cast::<Terms>()) };
    let mut output = left.clone();
    {
        let mut c = EitherTerms {
            input: right.iter(),
            output: &mut output,
        };
        unsafe {
            b_either(
                (&mut c as *mut EitherTerms<'_>).cast(),
                EitherHost {
                    next: either_next,
                    insert: either_insert,
                },
            )
        };
    }
    store(slot, index, output);
}
unsafe extern "C" fn both(slot: Slot, index: usize, left: Borrowed, right: Borrowed) {
    let (left, right) = unsafe { (&*left.cast::<Terms>(), &*right.cast::<Terms>()) };
    let mut output = Terms::new();
    {
        let mut c = Product {
            left: left.iter(),
            right_source: right,
            right: right.iter(),
            output: &mut output,
        };
        unsafe {
            b_product(
                (&mut c as *mut Product<'_>).cast(),
                ProductHost {
                    left: product_left,
                    reset: product_reset,
                    right: product_right,
                    union: product_union,
                },
            )
        };
    }
    store(slot, index, output);
}
#[repr(C)]
struct GraphHost {
    count: unsafe extern "C" fn(Slot) -> usize,
    inspect: unsafe extern "C" fn(Slot, usize) -> condition_graph::SourceNode,
    lookup: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    empty: unsafe extern "C" fn(Slot, usize),
    leaf: unsafe extern "C" fn(Slot, usize, Borrowed),
    either: unsafe extern "C" fn(Slot, usize, Borrowed, Borrowed),
    both: unsafe extern "C" fn(Slot, usize, Borrowed, Borrowed),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_graph"]
    fn b_graph(context: Slot, host: GraphHost);
}
struct Consensus<'a> {
    source: &'a Terms,
    left: btree_set::Iter<'a, Facts>,
    right: Option<btree_set::Range<'a, Facts>>,
    facts: Option<facts::Iter<'a>>,
    output: &'a mut Terms,
}
unsafe fn consensus<'a, 'b>(slot: Slot) -> &'a mut Consensus<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn next_left(slot: Slot) -> Term {
    term(unsafe { consensus(slot) }.left.next())
}
unsafe extern "C" fn right_begin(slot: Slot, left: Borrowed) {
    let c = unsafe { consensus(slot) };
    c.right = Some(
        c.source
            .range((Excluded(unsafe { &*left.cast::<Facts>() }), Unbounded)),
    );
}
unsafe extern "C" fn next_right(slot: Slot) -> Term {
    term(
        unsafe { consensus(slot) }
            .right
            .as_mut()
            .and_then(Iterator::next),
    )
}
unsafe extern "C" fn facts_begin(slot: Slot, left: Borrowed) {
    unsafe { consensus(slot) }.facts = Some(unsafe { &*left.cast::<Facts>() }.iter());
}
unsafe extern "C" fn next_fact(slot: Slot) -> fact_fields::Fields {
    unsafe { consensus(slot) }
        .facts
        .as_mut()
        .and_then(Iterator::next)
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn term_contains(source: Borrowed, fact: fact_fields::Fields) -> u32 {
    u32::from(unsafe { &*source.cast::<Facts>() }.contains(&unsafe { fact_fields::key(fact) }))
}
unsafe extern "C" fn resolve(
    slot: Slot,
    left: Borrowed,
    right: Borrowed,
    fact: fact_fields::Fields,
    other: fact_fields::Fields,
) {
    let (fact, other) = unsafe { (fact_fields::key(fact), fact_fields::key(other)) };
    let (left, right) = unsafe { (&*left.cast::<Facts>(), &*right.cast::<Facts>()) };
    let facts = left
        .iter()
        .filter(|value| **value != fact)
        .chain(right.iter().filter(|value| **value != other))
        .copied()
        .collect();
    accept(&facts, unsafe { consensus(slot) }.output);
}
#[repr(C)]
struct ConsensusHost {
    left: unsafe extern "C" fn(Slot) -> Term,
    right_begin: unsafe extern "C" fn(Slot, Borrowed),
    right: unsafe extern "C" fn(Slot) -> Term,
    facts_begin: unsafe extern "C" fn(Slot, Borrowed),
    fact: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    contains: unsafe extern "C" fn(Borrowed, fact_fields::Fields) -> u32,
    resolve:
        unsafe extern "C" fn(Slot, Borrowed, Borrowed, fact_fields::Fields, fact_fields::Fields),
    facts: fact_fields::Host,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_consensus"]
    fn b_consensus(context: Slot, host: ConsensusHost);
}
struct Merge<'a> {
    input: btree_set::IntoIter<Facts>,
    current: Facts,
    output: &'a mut Terms,
}
unsafe fn merging<'a, 'b>(slot: Slot) -> &'a mut Merge<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn merge_next(slot: Slot) -> u32 {
    let c = unsafe { merging(slot) };
    match c.input.next() {
        Some(value) => {
            c.current = value;
            1
        }
        None => 0,
    }
}
unsafe extern "C" fn merge_insert(slot: Slot) -> u32 {
    let c = unsafe { merging(slot) };
    let value = std::mem::take(&mut c.current);
    u32::from(insert(c.output, value))
}
#[repr(C)]
struct MergeHost {
    next: unsafe extern "C" fn(Slot) -> u32,
    insert: unsafe extern "C" fn(Slot) -> u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_merge_additions"]
    fn b_merge(context: Slot, host: MergeHost) -> u32;
}
struct Closure<'a> {
    terms: &'a mut Terms,
}
unsafe extern "C" fn round(slot: Slot) -> u32 {
    let c = unsafe { &mut *slot.cast::<Closure<'_>>() };
    let mut additions = Terms::new();
    let unsigned32 = Scalar::U32;
    {
        let mut phase = Consensus {
            source: c.terms,
            left: c.terms.iter(),
            right: None,
            facts: None,
            output: &mut additions,
        };
        unsafe {
            b_consensus(
                (&mut phase as *mut Consensus<'_>).cast(),
                ConsensusHost {
                    left: next_left,
                    right_begin,
                    right: next_right,
                    facts_begin,
                    fact: next_fact,
                    contains: term_contains,
                    resolve,
                    facts: fact_fields::Host {
                        unsigned32: borrowed(&unsigned32),
                        compare: fact_fields::compare,
                    },
                },
            )
        };
    }
    let mut phase = Merge {
        input: additions.into_iter(),
        current: Facts::new(),
        output: c.terms,
    };
    unsafe {
        b_merge(
            (&mut phase as *mut Merge<'_>).cast(),
            MergeHost {
                next: merge_next,
                insert: merge_insert,
            },
        )
    }
}
#[repr(C)]
struct ClosureHost {
    round: unsafe extern "C" fn(Slot) -> u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_closure"]
    fn b_closure(context: Slot, host: ClosureHost);
}
struct Fold {
    input: btree_set::IntoIter<Facts>,
    current: Facts,
    result: Clause,
}
unsafe extern "C" fn fold_next(slot: Slot) -> u32 {
    let c = unsafe { &mut *slot.cast::<Fold>() };
    match c.input.next() {
        Some(value) => {
            c.current = value;
            1
        }
        None => 0,
    }
}
unsafe extern "C" fn append(slot: Slot) {
    let c = unsafe { &mut *slot.cast::<Fold>() };
    let facts = std::mem::take(&mut c.current);
    let leaf = Clause::facts(facts);
    c.result = Clause::either(&c.result, &leaf);
}
#[repr(C)]
struct FoldHost {
    next: unsafe extern "C" fn(Slot) -> u32,
    append: unsafe extern "C" fn(Slot),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::canonical_fold"]
    fn b_fold(context: Slot, host: FoldHost);
}
pub(super) fn canonical(root: &Clause) -> Clause {
    let mut values = BTreeMap::new();
    {
        let mut c = Graph {
            nodes: Clause::ordered([root]),
            values: &mut values,
        };
        unsafe {
            b_graph(
                (&mut c as *mut Graph<'_>).cast(),
                GraphHost {
                    count,
                    inspect,
                    lookup,
                    empty,
                    leaf,
                    either,
                    both,
                },
            )
        };
    }
    let mut terms = values.remove(&root.key()).unwrap();
    drop(values);
    {
        let mut c = Closure { terms: &mut terms };
        unsafe { b_closure((&mut c as *mut Closure<'_>).cast(), ClosureHost { round }) };
    }
    let mut c = Fold {
        input: terms.into_iter(),
        current: Facts::new(),
        result: Clause::impossible(),
    };
    unsafe {
        b_fold(
            (&mut c as *mut Fold).cast(),
            FoldHost {
                next: fold_next,
                append,
            },
        )
    };
    c.result
}
