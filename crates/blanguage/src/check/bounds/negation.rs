//! B owns graph negation; original containers own source borrows and Clause results.
use super::facts;
use super::{Clause, ClauseNode, condition_graph::SourceNode, fact_fields};
use std::{
    collections::{BTreeMap},
    ffi::c_void,
    ptr,
};

type Borrowed = *const c_void;
type Slot = *mut c_void;

#[derive(Default)]
#[repr(C)]
struct Pending {
    present: u32,
    ready: u32,
    source: Borrowed,
}
struct Context<'a> {
    pending: Vec<(&'a Clause, bool)>,
    values: BTreeMap<usize, Clause>,
    facts: facts::Iter<'a>,
}
unsafe fn context<'a, 'b>(slot: Slot) -> &'a mut Context<'b> {
    unsafe { &mut *slot.cast::<Context<'b>>() }
}
unsafe fn clause<'a>(value: Borrowed) -> &'a Clause {
    unsafe { &*value.cast::<Clause>() }
}
unsafe extern "C" fn inspect(value: Borrowed) -> SourceNode {
    super::condition_graph::source_node(unsafe { clause(value) })
}
unsafe extern "C" fn push(slot: Slot, value: Borrowed, ready: u32) {
    unsafe { context(slot) }
        .pending
        .push((unsafe { clause(value) }, ready != 0));
}
unsafe extern "C" fn pop(slot: Slot) -> Pending {
    unsafe { context(slot) }
        .pending
        .pop()
        .map_or_else(Pending::default, |(source, ready)| Pending {
            present: 1,
            ready: u32::from(ready),
            source: ptr::from_ref(source).cast(),
        })
}
unsafe extern "C" fn has_result(slot: Slot, source: Borrowed) -> u32 {
    u32::from(
        unsafe { context(slot) }
            .values
            .contains_key(&unsafe { clause(source) }.key()),
    )
}
unsafe extern "C" fn unconstrained(slot: Slot, source: Borrowed) {
    unsafe { context(slot) }
        .values
        .insert(unsafe { clause(source) }.key(), Clause::default());
}
unsafe extern "C" fn impossible(slot: Slot, source: Borrowed) {
    unsafe { context(slot) }
        .values
        .insert(unsafe { clause(source) }.key(), Clause::impossible());
}
unsafe extern "C" fn begin_facts(slot: Slot, source: Borrowed) {
    if let ClauseNode::Facts(facts) = &**unsafe { clause(source) }.0 {
        unsafe { context(slot) }.facts = facts.iter();
    }
}
unsafe extern "C" fn next_fact(slot: Slot) -> fact_fields::Fields {
    unsafe { context(slot) }
        .facts
        .next()
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn either_fact(slot: Slot, source: Borrowed, fields: fact_fields::Fields) {
    let fact = unsafe { fact_fields::key(fields) };
    let value = Clause::facts([fact].into());
    let current = unsafe { context(slot) }
        .values
        .get_mut(&unsafe { clause(source) }.key())
        .unwrap();
    *current = Clause::either(current, &value);
}
unsafe extern "C" fn either(slot: Slot, source: Borrowed, left: Borrowed, right: Borrowed) {
    let context = unsafe { context(slot) };
    let value = Clause::either(
        &context.values[&unsafe { clause(left) }.key()],
        &context.values[&unsafe { clause(right) }.key()],
    );
    context
        .values
        .insert(unsafe { clause(source) }.key(), value);
}
unsafe extern "C" fn both(slot: Slot, source: Borrowed, left: Borrowed, right: Borrowed) {
    let context = unsafe { context(slot) };
    let value = Clause::both(
        &context.values[&unsafe { clause(left) }.key()],
        &context.values[&unsafe { clause(right) }.key()],
    );
    context
        .values
        .insert(unsafe { clause(source) }.key(), value);
}

#[repr(C)]
struct Host {
    inspect: unsafe extern "C" fn(Borrowed) -> SourceNode,
    push: unsafe extern "C" fn(Slot, Borrowed, u32),
    pop: unsafe extern "C" fn(Slot) -> Pending,
    has_result: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    unconstrained: unsafe extern "C" fn(Slot, Borrowed),
    impossible: unsafe extern "C" fn(Slot, Borrowed),
    begin_facts: unsafe extern "C" fn(Slot, Borrowed),
    next_fact: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    either_fact: unsafe extern "C" fn(Slot, Borrowed, fact_fields::Fields),
    either: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed),
    both: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed),
    facts: fact_fields::Host,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::negate_graph"]
    fn evaluate(root: Borrowed, context: Slot, host: Host);
}
pub(super) fn negate(root: &Clause) -> Clause {
    let mut context = Context {
        pending: Vec::new(),
        values: BTreeMap::new(),
        facts: facts::Iter::default(),
    };
    let unsigned32 = super::Scalar::U32;
    unsafe {
        evaluate(
            ptr::from_ref(root).cast(),
            (&mut context as *mut Context<'_>).cast(),
            Host {
                inspect,
                push,
                pop,
                has_result,
                unconstrained,
                impossible,
                begin_facts,
                next_fact,
                either_fact,
                either,
                both,
                facts: fact_fields::Host {
                    unsigned32: ptr::from_ref(&unsigned32).cast(),
                    compare: fact_fields::compare,
                },
            },
        );
    }
    context.values.remove(&root.key()).unwrap()
}
