//! B owns the complete shared-Clause overlap search. The original query owns
//! its physical containers and borrows immutable Clause/Fact fields until return.
use super::{Clause, ClauseNode, Fact, Facts, contradictory};
use std::{
    collections::{BTreeMap, HashMap, btree_set},
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
pub(super) struct SourceNode {
    kind: u32,
    left: Borrowed,
    right: Borrowed,
    no: Borrowed,
    truth: u32,
    empty: u32,
    condition: Borrowed,
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
    indices: BTreeMap<usize, usize>,
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
    context.indices[&clause.key()]
}
unsafe extern "C" fn node_count(slot: Slot) -> usize {
    unsafe { context(slot) }.nodes.len()
}
unsafe extern "C" fn inspect(slot: Slot, id: usize) -> SourceNode {
    source_node(unsafe { context(slot) }.nodes[id].clause)
}
pub(super) fn source_node(clause: &Clause) -> SourceNode {
    let mut out = SourceNode {
        kind: 0,
        left: std::ptr::null(),
        right: std::ptr::null(),
        no: std::ptr::null(),
        truth: 0,
        empty: 0,
        condition: std::ptr::null(),
    };
    match &**clause.0 {
        ClauseNode::Impossible => {}
        ClauseNode::Facts(facts) => {
            out.kind = 1;
            out.left = std::ptr::from_ref(facts).cast();
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
            out.condition = std::ptr::from_ref(condition).cast();
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
unsafe extern "C" fn set_next_fact(slot: Slot, fields: super::fact_fields::Fields) {
    let fact = unsafe { super::fact_fields::key(fields) };
    unsafe { context(slot) }.next_fact = Some((fact, None));
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
    inspect_fact: unsafe extern "C" fn(Borrowed) -> super::fact_fields::Fields,
    set_next_fact: unsafe extern "C" fn(Slot, super::fact_fields::Fields),
    facts: super::fact_fields::Host,
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
    let unsigned32 = super::Scalar::U32;
    let indices = super::condition_node_ids([left, right], |_| {});
    let mut nodes = Vec::new();
    for clause in Clause::ordered([left, right]) {
        if indices[&clause.key()] == nodes.len() {
            nodes.push(Node {
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
        }
    }
    let mut context = Context {
        indices,
        nodes,
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
                inspect_fact: super::fact_fields::project,
                set_next_fact,
                facts: super::fact_fields::Host {
                    unsigned32: std::ptr::from_ref(&unsigned32).cast(),
                    compare: super::fact_fields::compare,
                },
                remaining,
                push_clause,
                push_fact,
                pop_alternative,
            },
        ) != 0
    }
}

// The original required-facts query retains its physical nodes and values.
// B owns their build/evaluation order and the count of actual consumers.
#[derive(Clone, Copy)]
#[repr(C)]
struct RequiredNode {
    left: usize,
    right: usize,
    kind: u32,
    users: usize,
    remaining: usize,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct RequiredGraph {
    nodes: *mut RequiredNode,
    stride: usize,
    count: usize,
}
struct RequiredResult<'a> {
    clause: &'a Clause,
    node: RequiredNode,
    value: Option<Option<super::QueryFacts<'a>>>,
}
pub(super) struct RequiredQuery<'a> {
    nodes: Vec<RequiredResult<'a>>,
    pending: Vec<usize>,
}
struct RequiredBuilder<'a> {
    indices: HashMap<usize, usize>,
    query: RequiredQuery<'a>,
}
unsafe fn required_builder<'a, 'b>(slot: Slot) -> &'a mut RequiredBuilder<'b> {
    unsafe { &mut *slot.cast::<RequiredBuilder<'b>>() }
}
unsafe extern "C" fn required_index(slot: Slot, clause: Borrowed) -> usize {
    let builder = unsafe { required_builder(slot) };
    let clause = unsafe { &*clause.cast::<Clause>() };
    match builder.indices.entry(clause.key()) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            let id = builder.query.nodes.len();
            entry.insert(id);
            builder.query.nodes.push(RequiredResult {
                clause,
                node: RequiredNode {
                    left: 0,
                    right: 0,
                    kind: 0,
                    users: 1,
                    remaining: 0,
                },
                value: None,
            });
            id
        }
        std::collections::hash_map::Entry::Occupied(entry) => {
            let id = *entry.get();
            builder.query.nodes[id].node.users += 1;
            id
        }
    }
}
unsafe extern "C" fn required_count(slot: Slot) -> usize {
    unsafe { required_builder(slot) }.query.nodes.len()
}
unsafe extern "C" fn required_inspect(slot: Slot, id: usize) -> SourceNode {
    source_node(unsafe { required_builder(slot) }.query.nodes[id].clause)
}
unsafe extern "C" fn required_define(slot: Slot, id: usize, kind: u32, left: usize, right: usize) {
    let node = &mut unsafe { required_builder(slot) }.query.nodes[id].node;
    node.left = left;
    node.right = right;
    node.kind = kind;
}
#[repr(C)]
struct RequiredBuildHost {
    index: unsafe extern "C" fn(Slot, Borrowed) -> usize,
    count: unsafe extern "C" fn(Slot) -> usize,
    inspect: unsafe extern "C" fn(Slot, usize) -> SourceNode,
    define: unsafe extern "C" fn(Slot, usize, u32, usize, usize),
}
struct RequiredEvaluation<'a, 'b, 'c> {
    query: &'a mut RequiredQuery<'b>,
    environment: &'c Facts,
    roots: &'c std::collections::BTreeMap<super::ValueIdentity, super::ValueIdentity>,
    members: &'c [(&'c super::ValueIdentity, &'c super::ValueIdentity)],
}
unsafe fn required_evaluation<'a, 'b, 'c, 'd>(
    slot: Slot,
) -> &'a mut RequiredEvaluation<'b, 'c, 'd> {
    unsafe { &mut *slot.cast::<RequiredEvaluation<'b, 'c, 'd>>() }
}
#[repr(C)]
struct RequiredPending {
    present: u32,
    index: usize,
}
unsafe extern "C" fn required_reset(slot: Slot, id: usize) {
    unsafe { required_evaluation(slot) }.query.nodes[id].value = None;
}
unsafe extern "C" fn required_status(slot: Slot, id: usize) -> u32 {
    match &unsafe { required_evaluation(slot) }.query.nodes[id].value {
        None => 0,
        Some(None) => 1,
        Some(Some(_)) => 2,
    }
}
unsafe extern "C" fn required_push(slot: Slot, id: usize) {
    unsafe { required_evaluation(slot) }.query.pending.push(id);
}
unsafe extern "C" fn required_pop(slot: Slot) -> RequiredPending {
    match unsafe { required_evaluation(slot) }.query.pending.pop() {
        Some(index) => RequiredPending { present: 1, index },
        None => RequiredPending {
            present: 0,
            index: 0,
        },
    }
}
unsafe extern "C" fn required_pending_count(slot: Slot) -> usize {
    unsafe { required_evaluation(slot) }.query.pending.len()
}
unsafe extern "C" fn required_impossible(slot: Slot, id: usize) {
    unsafe { required_evaluation(slot) }.query.nodes[id].value = Some(None);
}
unsafe extern "C" fn required_leaf(slot: Slot, id: usize) {
    let evaluation = unsafe { required_evaluation(slot) };
    if let ClauseNode::Facts(required) = &**evaluation.query.nodes[id].clause.0 {
        let required = if required.is_disjoint(evaluation.environment) {
            super::QueryFacts::Borrowed(required)
        } else {
            super::QueryFacts::Owned(std::sync::Arc::new(
                required
                    .difference(evaluation.environment)
                    .copied()
                    .collect(),
            ))
        };
        let value = if super::cross_conflict(evaluation.environment, &required) {
            None
        } else {
            Some(required)
        };
        evaluation.query.nodes[id].value = Some(value);
    }
}
fn take_required<'a>(result: &mut RequiredResult<'a>, last: u32) -> Option<super::QueryFacts<'a>> {
    if last != 0 {
        result.value.take().unwrap()
    } else {
        result.value.as_ref().unwrap().clone()
    }
}
unsafe extern "C" fn required_union(slot: Slot, id: usize, left_last: u32, right_last: u32) {
    let query = &mut unsafe { required_evaluation(slot) }.query;
    let node = query.nodes[id].node;
    let a = take_required(&mut query.nodes[node.left], left_last).unwrap();
    let b = take_required(&mut query.nodes[node.right], right_last).unwrap();
    query.nodes[id].value = Some(super::combine_facts(a, b));
}
unsafe extern "C" fn required_common(slot: Slot, id: usize, left_last: u32, right_last: u32) {
    let evaluation = unsafe { required_evaluation(slot) };
    let node = evaluation.query.nodes[id].node;
    let a = take_required(&mut evaluation.query.nodes[node.left], left_last);
    let b = take_required(&mut evaluation.query.nodes[node.right], right_last);
    evaluation.query.nodes[id].value = Some(super::either_facts(
        evaluation.environment,
        evaluation.roots,
        evaluation.members,
        a,
        b,
    ));
}
unsafe extern "C" fn required_valid_additions(slot: Slot, id: usize) -> u32 {
    let evaluation = unsafe { required_evaluation(slot) };
    let facts = evaluation.query.nodes[id]
        .value
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap();
    u32::from(!super::additions_conflict(evaluation.environment, facts))
}
unsafe extern "C" fn required_release(slot: Slot, id: usize, last: u32) -> u32 {
    let result = &mut unsafe { required_evaluation(slot) }.query.nodes[id];
    let computed = result.value.is_some();
    if last != 0 {
        result.value = None;
    }
    u32::from(computed)
}
#[repr(C)]
struct RequiredHost {
    reset: unsafe extern "C" fn(Slot, usize),
    status: unsafe extern "C" fn(Slot, usize) -> u32,
    push: unsafe extern "C" fn(Slot, usize),
    pop: unsafe extern "C" fn(Slot) -> RequiredPending,
    pending_count: unsafe extern "C" fn(Slot) -> usize,
    impossible: unsafe extern "C" fn(Slot, usize),
    leaf: unsafe extern "C" fn(Slot, usize),
    union: unsafe extern "C" fn(Slot, usize, u32, u32),
    common: unsafe extern "C" fn(Slot, usize, u32, u32),
    valid_additions: unsafe extern "C" fn(Slot, usize) -> u32,
    release: unsafe extern "C" fn(Slot, usize, u32) -> u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::required_build"]
    fn build_required(root: Borrowed, context: Slot, host: RequiredBuildHost);
    #[link_name = "bootstrap.condition_graph::required_evaluate"]
    fn evaluate_required(graph: RequiredGraph, context: Slot, host: RequiredHost);
}
impl<'a> RequiredQuery<'a> {
    pub(super) fn new(root: &'a Clause) -> Self {
        let mut builder = RequiredBuilder {
            indices: HashMap::new(),
            query: Self {
                nodes: Vec::new(),
                pending: Vec::new(),
            },
        };
        unsafe {
            build_required(
                std::ptr::from_ref(root).cast(),
                (&mut builder as *mut RequiredBuilder<'_>).cast(),
                RequiredBuildHost {
                    index: required_index,
                    count: required_count,
                    inspect: required_inspect,
                    define: required_define,
                },
            );
        }
        builder.query
    }
    pub(super) fn evaluate(
        &mut self,
        environment: &Facts,
        roots: &std::collections::BTreeMap<super::ValueIdentity, super::ValueIdentity>,
        members: &[(&super::ValueIdentity, &super::ValueIdentity)],
    ) -> Option<super::QueryFacts<'a>> {
        let graph = RequiredGraph {
            nodes: std::ptr::addr_of_mut!(self.nodes[0].node),
            stride: std::mem::size_of::<RequiredResult<'a>>(),
            count: self.nodes.len(),
        };
        let mut evaluation = RequiredEvaluation {
            query: self,
            environment,
            roots,
            members,
        };
        unsafe {
            evaluate_required(
                graph,
                (&mut evaluation as *mut RequiredEvaluation<'_, '_, '_>).cast(),
                RequiredHost {
                    reset: required_reset,
                    status: required_status,
                    push: required_push,
                    pop: required_pop,
                    pending_count: required_pending_count,
                    impossible: required_impossible,
                    leaf: required_leaf,
                    union: required_union,
                    common: required_common,
                    valid_additions: required_valid_additions,
                    release: required_release,
                },
            );
        }
        self.nodes[0].value.take().unwrap()
    }
}
