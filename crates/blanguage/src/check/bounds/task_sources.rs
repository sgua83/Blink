//! B evaluates task-return source sets. This synchronous query owns the original
//! node results; the final set moves back to the original native consumer.
use super::facts;
use super::{Clause, ClauseNode, Facts, Scalar, ValueIdentity, condition_graph, fact_fields};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    ptr,
};
type Slot = *mut c_void;
type Borrowed = *const c_void;
struct Context<'a> {
    nodes: Vec<&'a Clause>,
    values: BTreeMap<usize, Option<BTreeSet<u32>>>,
    leaf: Option<&'a Facts>,
    cursor: Option<facts::Iter<'a>>,
}
unsafe fn context<'a, 'b>(slot: Slot) -> &'a mut Context<'b> {
    unsafe { &mut *slot.cast::<Context<'b>>() }
}
unsafe extern "C" fn count(slot: Slot) -> usize {
    unsafe { context(slot) }.nodes.len()
}
unsafe extern "C" fn inspect(slot: Slot, index: usize) -> condition_graph::SourceNode {
    condition_graph::source_node(unsafe { context(slot) }.nodes[index])
}
unsafe extern "C" fn begin(slot: Slot, index: usize) {
    let context = unsafe { context(slot) };
    context.leaf = match &**context.nodes[index].0 {
        ClauseNode::Facts(facts) => Some(facts),
        _ => None,
    };
    context.cursor = context.leaf.map(Facts::iter);
}
unsafe extern "C" fn next(slot: Slot) -> fact_fields::Fields {
    unsafe { context(slot) }
        .cursor
        .as_mut()
        .and_then(Iterator::next)
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn contains(slot: Slot, fields: fact_fields::Fields) -> u32 {
    u32::from(
        unsafe { context(slot) }
            .leaf
            .is_some_and(|facts| facts.contains(&unsafe { fact_fields::key(fields) })),
    )
}
#[repr(C)]
struct Set {
    known: u32,
    values: Borrowed,
}
unsafe extern "C" fn lookup(slot: Slot, source: Borrowed) -> Set {
    let source = unsafe { &*source.cast::<Clause>() };
    match &unsafe { context(slot) }.values[&source.key()] {
        Some(values) => Set {
            known: 1,
            values: ptr::from_ref(values).cast(),
        },
        None => Set {
            known: 0,
            values: ptr::null(),
        },
    }
}
fn store(slot: Slot, index: usize, value: Option<BTreeSet<u32>>) {
    let context = unsafe { context(slot) };
    context.values.insert(context.nodes[index].key(), value);
}
unsafe extern "C" fn unknown(slot: Slot, index: usize) {
    store(slot, index, None);
}
unsafe extern "C" fn empty(slot: Slot, index: usize) {
    store(slot, index, Some(BTreeSet::new()));
}
unsafe extern "C" fn single(slot: Slot, index: usize, value: u32) {
    store(slot, index, Some(BTreeSet::from([value])));
}
unsafe extern "C" fn copy(slot: Slot, index: usize, values: Borrowed) {
    let values = unsafe { &*values.cast::<BTreeSet<u32>>() }.clone();
    store(slot, index, Some(values));
}
unsafe extern "C" fn intersection(slot: Slot, index: usize, left: Borrowed, right: Borrowed) {
    let (left, right) = unsafe {
        (
            &*left.cast::<BTreeSet<u32>>(),
            &*right.cast::<BTreeSet<u32>>(),
        )
    };
    let values = left.intersection(right).copied().collect();
    store(slot, index, Some(values));
}
unsafe extern "C" fn union(slot: Slot, index: usize, left: Borrowed, right: Borrowed) {
    let (left, right) = unsafe {
        (
            &*left.cast::<BTreeSet<u32>>(),
            &*right.cast::<BTreeSet<u32>>(),
        )
    };
    let values = left.union(right).copied().collect();
    store(slot, index, Some(values));
}
#[repr(C)]
struct Host {
    count: unsafe extern "C" fn(Slot) -> usize,
    inspect: unsafe extern "C" fn(Slot, usize) -> condition_graph::SourceNode,
    begin: unsafe extern "C" fn(Slot, usize),
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    contains: unsafe extern "C" fn(Slot, fact_fields::Fields) -> u32,
    lookup: unsafe extern "C" fn(Slot, Borrowed) -> Set,
    unknown: unsafe extern "C" fn(Slot, usize),
    empty: unsafe extern "C" fn(Slot, usize),
    single: unsafe extern "C" fn(Slot, usize, u32),
    copy: unsafe extern "C" fn(Slot, usize, Borrowed),
    intersection: unsafe extern "C" fn(Slot, usize, Borrowed, Borrowed),
    union: unsafe extern "C" fn(Slot, usize, Borrowed, Borrowed),
    same_scalar: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    identity: Borrowed,
    facts: fact_fields::Host,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::task_return_sources"]
    fn evaluate(context: Slot, host: Host);
}
pub(super) fn query(root: &Clause, component: usize) -> Option<BTreeSet<u32>> {
    let identity = ValueIdentity::TaskReturnSource(component);
    let unsigned32 = Scalar::U32;
    let mut context = Context {
        nodes: Clause::ordered(std::slice::from_ref(root)),
        values: BTreeMap::new(),
        leaf: None,
        cursor: None,
    };
    unsafe {
        evaluate(
            (&mut context as *mut Context<'_>).cast(),
            Host {
                count,
                inspect,
                begin,
                next,
                contains,
                lookup,
                unknown,
                empty,
                single,
                copy,
                intersection,
                union,
                same_scalar: fact_fields::same_scalar,
                identity: ptr::from_ref(&identity).cast(),
                facts: fact_fields::Host {
                    unsigned32: ptr::from_ref(&unsigned32).cast(),
                    compare: fact_fields::compare,
                },
            },
        );
    }
    context.values.remove(&root.key()).flatten()
}
