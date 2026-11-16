//! B owns the complete shared-Clause overlap search. The original query owns
//! its physical containers and borrows immutable Clause/Fact fields until return.
use super::{Clause, ClauseNode, Fact, Facts, contradictory};
use std::{
    collections::{HashMap, btree_set},
    ffi::c_void,
};

type Slot = *mut c_void;
type Borrowed = *const c_void;
#[derive(Clone, Copy)]
#[repr(C)]
struct Span {
    data: *const usize,
    count: usize,
}
#[repr(C)]
struct SourceNode {
    kind: u32,
    left: Borrowed,
    right: Borrowed,
    no: Borrowed,
    truth: u32,
    empty: u32,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Gate {
    left: usize,
    right: usize,
    kind: u32,
    parents: Span,
}
#[repr(C)]
struct Graph {
    nodes: *const Gate,
    stride: usize,
    values: *mut u8,
}
#[repr(C)]
struct Assignment {
    present: u32,
    node: usize,
    truth: u32,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Checkpoint {
    nodes: usize,
    facts: usize,
    negative: usize,
}
#[repr(C)]
struct AlternativeView {
    kind: u32,
    node: usize,
    truth: u32,
    checkpoint: Checkpoint,
}

struct Node<'a> {
    clause: &'a Clause,
    view: Gate,
    parents: Vec<usize>,
}
enum Alternative<'a> {
    Clause(usize, bool),
    Fact(&'a Fact, btree_set::Iter<'a, Fact>),
}
struct Context<'a> {
    indices: HashMap<usize, usize>,
    nodes: Vec<Node<'a>>,
    pending: Vec<(usize, bool)>,
    values: Vec<u8>,
    trail: Vec<usize>,
    alternatives: Vec<(Alternative<'a>, Checkpoint)>,
    facts: Facts,
    additions: Vec<Fact>,
    required: Option<btree_set::Iter<'a, Fact>>,
    next_fact: Option<(Fact, Option<btree_set::Iter<'a, Fact>>)>,
}
impl Context<'_> {
    fn add(&mut self, fact: Fact) {
        if self.facts.insert(fact) {
            self.additions.push(fact);
        }
    }
}
unsafe fn context<'a, 'b>(slot: Slot) -> &'a mut Context<'b> {
    unsafe { &mut *slot.cast::<Context<'b>>() }
}

unsafe extern "C" fn index(slot: Slot, clause: Borrowed) -> usize {
    let context = unsafe { context(slot) };
    let clause = unsafe { &*clause.cast::<Clause>() };
    *context.indices.entry(clause.key()).or_insert_with(|| {
        let id = context.nodes.len();
        context.nodes.push(Node {
            clause,
            view: Gate {
                left: 0,
                right: 0,
                kind: 0,
                parents: Span {
                    data: std::ptr::null(),
                    count: 0,
                },
            },
            parents: Vec::new(),
        });
        id
    })
}
unsafe extern "C" fn node_count(slot: Slot) -> usize {
    unsafe { context(slot) }.nodes.len()
}
unsafe extern "C" fn inspect(slot: Slot, id: usize) -> SourceNode {
    let clause = unsafe { context(slot) }.nodes[id].clause;
    let mut out = SourceNode {
        kind: 0,
        left: std::ptr::null(),
        right: std::ptr::null(),
        no: std::ptr::null(),
        truth: 0,
        empty: 0,
    };
    match &**clause.0 {
        ClauseNode::Impossible => {}
        ClauseNode::Facts(facts) => {
            out.kind = 1;
            out.empty = u32::from(facts.is_empty());
        }
        ClauseNode::Both(a, b) => {
            out.kind = 2;
            out.left = std::ptr::from_ref(a).cast();
            out.right = std::ptr::from_ref(b).cast();
        }
        ClauseNode::Either(a, b) => {
            out.kind = 3;
            out.left = std::ptr::from_ref(a).cast();
            out.right = std::ptr::from_ref(b).cast();
        }
        ClauseNode::Branch(parent, condition, truth) => {
            out.kind = 4;
            out.left = std::ptr::from_ref(parent).cast();
            out.right = std::ptr::from_ref(&condition.cases[1]).cast();
            out.no = std::ptr::from_ref(&condition.cases[0]).cast();
            out.truth = u32::from(*truth);
        }
    }
    out
}
unsafe extern "C" fn define(slot: Slot, id: usize, kind: u32, left: usize, right: usize) {
    let context = unsafe { context(slot) };
    let view = &mut context.nodes[id].view;
    view.kind = kind;
    view.left = left;
    view.right = right;
}
unsafe extern "C" fn parent_push(slot: Slot, child: usize, parent: usize) {
    unsafe { context(slot) }.nodes[child].parents.push(parent);
}
unsafe extern "C" fn prepare(slot: Slot) -> Graph {
    let context = unsafe { context(slot) };
    drop(std::mem::take(&mut context.indices));
    for node in &mut context.nodes {
        node.view.parents = Span {
            data: node.parents.as_ptr(),
            count: node.parents.len(),
        };
    }
    context.values = vec![0; context.nodes.len()];
    Graph {
        nodes: std::ptr::addr_of!(context.nodes[0].view),
        stride: std::mem::size_of::<Node<'_>>(),
        values: context.values.as_mut_ptr(),
    }
}
unsafe extern "C" fn pending_push(slot: Slot, id: usize, truth: u32) {
    unsafe { context(slot) }.pending.push((id, truth != 0));
}
unsafe extern "C" fn pending_pop(slot: Slot) -> Assignment {
    match unsafe { context(slot) }.pending.pop() {
        Some((node, truth)) => Assignment {
            present: 1,
            node,
            truth: u32::from(truth),
        },
        None => Assignment {
            present: 0,
            node: 0,
            truth: 0,
        },
    }
}
unsafe extern "C" fn pending_clear(slot: Slot) {
    unsafe { context(slot) }.pending.clear();
}
unsafe extern "C" fn trail_push(slot: Slot, id: usize) {
    unsafe { context(slot) }.trail.push(id);
}
unsafe extern "C" fn trail_pop(slot: Slot) -> usize {
    unsafe { context(slot) }.trail.pop().unwrap()
}
unsafe extern "C" fn trail(slot: Slot) -> Span {
    let trail = &unsafe { context(slot) }.trail;
    Span {
        data: trail.as_ptr(),
        count: trail.len(),
    }
}
unsafe extern "C" fn additions(slot: Slot) -> usize {
    unsafe { context(slot) }.additions.len()
}
unsafe extern "C" fn undo_addition(slot: Slot) {
    let context = unsafe { context(slot) };
    let fact = context.additions.pop().unwrap();
    context.facts.remove(&fact);
}
unsafe extern "C" fn add_leaf(slot: Slot, id: usize) {
    let context = unsafe { context(slot) };
    let clause = context.nodes[id].clause;
    if let ClauseNode::Facts(required) = &**clause.0 {
        required.iter().copied().for_each(|fact| context.add(fact));
    }
}
unsafe extern "C" fn conflict(slot: Slot) -> u32 {
    u32::from(contradictory(&unsafe { context(slot) }.facts))
}
unsafe extern "C" fn add_next(slot: Slot) -> u32 {
    let context = unsafe { context(slot) };
    context.required = context.next_fact.take().and_then(|(fact, remaining)| {
        context.add(fact);
        remaining
    });
    u32::from(context.required.is_some())
}
unsafe extern "C" fn begin_facts(slot: Slot, id: usize) {
    let context = unsafe { context(slot) };
    if let ClauseNode::Facts(required) = &**context.nodes[id].clause.0 {
        context.required = Some(required.iter());
    }
}
unsafe extern "C" fn next_fact(slot: Slot) -> Borrowed {
    unsafe { context(slot) }
        .required
        .as_mut()
        .unwrap()
        .next()
        .map_or(std::ptr::null(), |fact| std::ptr::from_ref(fact).cast())
}
unsafe extern "C" fn negate(slot: Slot, fact: Borrowed) -> u32 {
    if let Some(negated) = Clause::negated_fact(unsafe { *fact.cast::<Fact>() }) {
        unsafe { context(slot) }.next_fact = Some((negated, None));
        1
    } else {
        0
    }
}
unsafe extern "C" fn remaining(slot: Slot) -> usize {
    unsafe { context(slot) }.required.as_ref().unwrap().len()
}
unsafe extern "C" fn push_clause(slot: Slot, id: usize, truth: u32, checkpoint: Checkpoint) {
    unsafe { context(slot) }
        .alternatives
        .push((Alternative::Clause(id, truth != 0), checkpoint));
}
unsafe extern "C" fn push_fact(slot: Slot, fact: Borrowed, checkpoint: Checkpoint) {
    let context = unsafe { context(slot) };
    let required = context.required.take().unwrap();
    context.alternatives.push((
        Alternative::Fact(unsafe { &*fact.cast::<Fact>() }, required),
        checkpoint,
    ));
}
unsafe extern "C" fn pop_alternative(slot: Slot) -> AlternativeView {
    let context = unsafe { context(slot) };
    match context.alternatives.pop() {
        Some((Alternative::Clause(node, truth), checkpoint)) => {
            context.next_fact = None;
            AlternativeView {
                kind: 1,
                node,
                truth: u32::from(truth),
                checkpoint,
            }
        }
        Some((Alternative::Fact(fact, required), checkpoint)) => {
            context.next_fact = Some((*fact, Some(required)));
            AlternativeView {
                kind: 2,
                node: 0,
                truth: 0,
                checkpoint,
            }
        }
        None => AlternativeView {
            kind: 0,
            node: 0,
            truth: 0,
            checkpoint: Checkpoint {
                nodes: 0,
                facts: 0,
                negative: 0,
            },
        },
    }
}

#[repr(C)]
struct Host {
    index: unsafe extern "C" fn(Slot, Borrowed) -> usize,
    node_count: unsafe extern "C" fn(Slot) -> usize,
    inspect: unsafe extern "C" fn(Slot, usize) -> SourceNode,
    define: unsafe extern "C" fn(Slot, usize, u32, usize, usize),
    parent_push: unsafe extern "C" fn(Slot, usize, usize),
    prepare: unsafe extern "C" fn(Slot) -> Graph,
    pending_push: unsafe extern "C" fn(Slot, usize, u32),
    pending_pop: unsafe extern "C" fn(Slot) -> Assignment,
    pending_clear: unsafe extern "C" fn(Slot),
    trail_push: unsafe extern "C" fn(Slot, usize),
    trail_pop: unsafe extern "C" fn(Slot) -> usize,
    trail: unsafe extern "C" fn(Slot) -> Span,
    additions: unsafe extern "C" fn(Slot) -> usize,
    undo_addition: unsafe extern "C" fn(Slot),
    add_leaf: unsafe extern "C" fn(Slot, usize),
    contradictory: unsafe extern "C" fn(Slot) -> u32,
    add_next: unsafe extern "C" fn(Slot) -> u32,
    begin_facts: unsafe extern "C" fn(Slot, usize),
    next_fact: unsafe extern "C" fn(Slot) -> Borrowed,
    negate: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    remaining: unsafe extern "C" fn(Slot) -> usize,
    push_clause: unsafe extern "C" fn(Slot, usize, u32, Checkpoint),
    push_fact: unsafe extern "C" fn(Slot, Borrowed, Checkpoint),
    pop_alternative: unsafe extern "C" fn(Slot) -> AlternativeView,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::overlap"]
    fn query(left: Borrowed, right: Borrowed, truth: u32, context: Slot, host: Host) -> u32;
}
pub(super) fn overlap(left: &Clause, right: &Clause, truth: bool) -> bool {
    let mut context = Context {
        indices: HashMap::new(),
        nodes: Vec::new(),
        pending: Vec::new(),
        values: Vec::new(),
        trail: Vec::new(),
        alternatives: Vec::new(),
        facts: Facts::new(),
        additions: Vec::new(),
        required: None,
        next_fact: None,
    };
    unsafe {
        query(
            std::ptr::from_ref(left).cast(),
            std::ptr::from_ref(right).cast(),
            u32::from(truth),
            (&mut context as *mut Context<'_>).cast(),
            Host {
                index,
                node_count,
                inspect,
                define,
                parent_push,
                prepare,
                pending_push,
                pending_pop,
                pending_clear,
                trail_push,
                trail_pop,
                trail,
                additions,
                undo_addition,
                add_leaf,
                contradictory: conflict,
                add_next,
                begin_facts,
                next_fact,
                negate,
                remaining,
                push_clause,
                push_fact,
                pop_alternative,
            },
        ) != 0
    }
}
