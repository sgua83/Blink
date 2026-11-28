//! B owns execution-condition traversal and formulas; native storage owns cases.
use super::{Clause, Condition, condition_graph::SourceNode};
use std::{collections::BTreeMap, ffi::c_void, ptr};

type Borrowed = *const c_void;
type Slot = *mut c_void;

#[derive(Default)]
#[repr(C)]
struct Pending {
    present: u32,
    ready: u32,
    source: Borrowed,
}
#[repr(C)]
struct Case {
    source: Borrowed,
    truth: u32,
    original: u32,
}
#[repr(C)]
struct Join {
    left: Case,
    right: Case,
    any: u32,
}
struct Context<'a> {
    pending: Vec<(&'a Clause, bool)>,
    values: BTreeMap<usize, Condition>,
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
    unsafe { context(slot) }.pending.push((unsafe { clause(value) }, ready != 0));
}
unsafe extern "C" fn pop(slot: Slot) -> Pending {
    unsafe { context(slot) }.pending.pop().map_or_else(Pending::default, |(source, ready)| Pending {
        present: 1, ready: u32::from(ready), source: ptr::from_ref(source).cast(),
    })
}
unsafe extern "C" fn has_result(slot: Slot, source: Borrowed) -> u32 {
    u32::from(unsafe { context(slot) }.values.contains_key(&unsafe { clause(source) }.key()))
}
unsafe extern "C" fn impossible(slot: Slot, source: Borrowed) {
    unsafe { context(slot) }.values.insert(unsafe { clause(source) }.key(), Condition::constant(false));
}
unsafe extern "C" fn guard(slot: Slot, source: Borrowed) {
    let value = Condition::guard(unsafe { clause(source) }.clone());
    unsafe { context(slot) }.values.insert(unsafe { clause(source) }.key(), value);
}
unsafe fn case<'a>(context: &'a Context<'_>, input: &Case) -> &'a Clause {
    let condition = if input.original != 0 {
        &**unsafe { &*input.source.cast::<super::SharedCondition>() }
    } else {
        &context.values[&unsafe { clause(input.source) }.key()]
    };
    &condition.cases[input.truth as usize]
}
unsafe extern "C" fn compose(slot: Slot, source: Borrowed, no: Join, yes: Join) {
    let context = unsafe { context(slot) };
    let cases = [no, yes].map(|join| {
        let left = unsafe { case(context, &join.left) };
        let right = unsafe { case(context, &join.right) };
        if join.any != 0 { Clause::either(left, right) } else { Clause::both(left, right) }
    });
    context.values.insert(unsafe { clause(source) }.key(), Condition { cases });
}
#[repr(C)]
struct Host {
    inspect: unsafe extern "C" fn(Borrowed) -> SourceNode,
    push: unsafe extern "C" fn(Slot, Borrowed, u32),
    pop: unsafe extern "C" fn(Slot) -> Pending,
    has_result: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    impossible: unsafe extern "C" fn(Slot, Borrowed),
    guard: unsafe extern "C" fn(Slot, Borrowed),
    compose: unsafe extern "C" fn(Slot, Borrowed, Join, Join),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::execution_condition"]
    fn evaluate(root: Borrowed, context: Slot, host: Host);
}
pub(super) fn execution(root: &Clause) -> Condition {
    let mut context = Context { pending: Vec::new(), values: BTreeMap::new() };
    unsafe {
        evaluate(ptr::from_ref(root).cast(), (&mut context as *mut Context<'_>).cast(),
            Host { inspect, push, pop, has_result, impossible, guard, compose });
    }
    context.values.remove(&root.key()).unwrap()
}
