//! B owns the complete shared-Clause overlap search. The original query owns
//! its physical containers and borrows immutable Clause/Fact fields until return.
use super::{Clause, ClauseNode, Fact, Facts, Proof, ValueIdentity, contradictory};
use std::{
    borrow::Borrow,
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
#[derive(Default)]
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
impl<'a> Context<'a> {
    fn new(left: &'a Clause, right: &'a Clause, indices: BTreeMap<usize, usize>) -> Self {
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
        Self {
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
        }
    }
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
    let mut out = SourceNode::default();
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

#[derive(Default)]
#[repr(C)]
struct CallProjectionNode {
    original: Borrowed,
    node: SourceNode,
}

struct CallProjection<'a, P> {
    ordered: std::slice::Iter<'a, &'a Clause>,
    bindings: &'a BTreeMap<ValueIdentity, P>,
    results: BTreeMap<usize, Clause>,
}

unsafe extern "C" fn call_next<P>(slot: Slot) -> CallProjectionNode {
    let ctx = unsafe { &mut *slot.cast::<CallProjection<'_, P>>() };
    ctx.ordered.next().map_or_else(CallProjectionNode::default, |&clause| CallProjectionNode {
        original: std::ptr::from_ref(clause).cast(),
        node: source_node(clause),
    })
}

unsafe fn call_key(source: Borrowed) -> usize {
    unsafe { &*source.cast::<Clause>() }.key()
}

unsafe extern "C" fn call_impossible<P>(slot: Slot, source: Borrowed) {
    let ctx = unsafe { &mut *slot.cast::<CallProjection<'_, P>>() };
    ctx.results.insert(unsafe { call_key(source) }, Clause::impossible());
}

unsafe extern "C" fn call_leaf<P: Borrow<Proof>>(slot: Slot, source: Borrowed, facts: Borrowed, need_private: u32) -> u32 {
    let ctx = unsafe { &mut *slot.cast::<CallProjection<'_, P>>() };
    let facts = unsafe { &*facts.cast::<Facts>() };
    let (mapped, private) = Clause::project_call_leaf(facts, ctx.bindings, need_private != 0);
    ctx.results.insert(unsafe { call_key(source) }, mapped);
    u32::from(private)
}

unsafe extern "C" fn call_pair<P, const EITHER: bool>(slot: Slot, source: Borrowed, left: Borrowed, right: Borrowed) {
    let ctx = unsafe { &mut *slot.cast::<CallProjection<'_, P>>() };
    let left = &ctx.results[&unsafe { call_key(left) }];
    let right = &ctx.results[&unsafe { call_key(right) }];
    let mapped = if EITHER { Clause::either(left, right) } else { Clause::both(left, right) };
    ctx.results.insert(unsafe { call_key(source) }, mapped);
}

#[repr(C)]
struct CallProjectionHost {
    next: unsafe extern "C" fn(Slot) -> CallProjectionNode,
    impossible: unsafe extern "C" fn(Slot, Borrowed),
    facts: unsafe extern "C" fn(Slot, Borrowed, Borrowed, u32) -> u32,
    both: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed),
    either: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed),
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::project_call_graph"]
    fn project_call_graph(slot: Slot, host: CallProjectionHost) -> u32;
}

pub(super) fn project_call<P: Borrow<Proof>>(
    ordered: &[&Clause], bindings: &BTreeMap<ValueIdentity, P>,
) -> (BTreeMap<usize, Clause>, bool) {
    let mut ctx = CallProjection { ordered: ordered.iter(), bindings, results: BTreeMap::new() };
    let private = unsafe {
        project_call_graph(std::ptr::from_mut(&mut ctx).cast(), CallProjectionHost {
            next: call_next::<P>,
            impossible: call_impossible::<P>,
            facts: call_leaf::<P>,
            both: call_pair::<P, false>,
            either: call_pair::<P, true>,
        })
    };
    (ctx.results, private != 0)
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
unsafe extern "C" fn reset_search(slot: Slot) {
    let context = unsafe { context(slot) };
    context.pending.clear();
    context.trail.clear();
    context.alternatives.clear();
    context.facts.clear();
    context.additions.clear();
    context.required = None;
    context.next_fact = None;
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
    #[link_name = "bootstrap.condition_graph::equivalent"]
    fn query_equivalent(
        left: Borrowed,
        right: Borrowed,
        context: Slot,
        host: Host,
        reset: unsafe extern "C" fn(Slot),
    ) -> u32;
}
pub(super) fn overlap(left: &Clause, right: &Clause, truth: bool) -> bool {
    let unsigned32 = super::Scalar::U32;
    let indices = super::condition_node_ids([left, right], |_| {});
    let mut context = Context::new(left, right, indices);
    unsafe {
        query(
            std::ptr::from_ref(left).cast(),
            std::ptr::from_ref(right).cast(),
            u32::from(truth),
            (&mut context as *mut Context<'_>).cast(),
            host(&unsigned32),
        ) != 0
    }
}
pub(super) fn equivalent(left: &Clause, right: &Clause) -> bool {
    if left.key() == right.key() {
        return true;
    }
    let indices = super::condition_node_ids([left, right], |_| {});
    if indices[&left.key()] == indices[&right.key()] {
        return true;
    }
    let unsigned32 = super::Scalar::U32;
    let mut context = Context::new(left, right, indices);
    unsafe {
        query_equivalent(
            std::ptr::from_ref(left).cast(),
            std::ptr::from_ref(right).cast(),
            (&mut context as *mut Context<'_>).cast(),
            host(&unsigned32),
            reset_search,
        ) != 0
    }
}
fn host(unsigned32: &super::Scalar) -> Host {
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
            unsigned32: std::ptr::from_ref(unsigned32).cast(),
            compare: super::fact_fields::compare,
        },
        remaining,
        push_clause,
        push_fact,
        pop_alternative,
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
    root: usize,
}
struct RequiredResult<'a> {
    facts: Option<&'a Facts>,
    node: RequiredNode,
    value: Option<Option<super::QueryFacts<'a>>>,
}
pub(super) struct RequiredQuery<'a> {
    nodes: Vec<RequiredResult<'a>>,
    root: usize,
    pending: Vec<usize>,
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
    if let Some(required) = evaluation.query.nodes[id].facts {
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
        let value = if super::cross_conflict(evaluation.environment, &required, false) {
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
    #[link_name = "bootstrap.condition_graph::required_evaluate"]
    fn evaluate_required(graph: RequiredGraph, context: Slot, host: RequiredHost);
}
impl<'a> RequiredQuery<'a> {
    pub(super) fn new(root: &'a Clause) -> Self {
        use super::EncodedClause;
        let mut nodes = Vec::<RequiredResult<'a>>::new();
        let indices = super::codec::conditions::node_ids([root], true, |shape| {
            let (kind, facts, left, right) = match shape {
                EncodedClause::Impossible => (0, None, 0, 0),
                EncodedClause::Facts(facts) => (1, Some(facts), 0, 0),
                EncodedClause::Both(left, right) => (2, None, left, right),
                EncodedClause::Either(left, right) => (3, None, left, right),
            };
            if kind >= 2 {
                nodes[left].node.users += 1;
                nodes[right].node.users += 1;
            }
            nodes.push(RequiredResult {
                facts,
                node: RequiredNode { left, right, kind, users: 0, remaining: 0 },
                value: None,
            });
        });
        let root = indices[&root.key()];
        nodes[root].node.users += 1;
        Self { nodes, root, pending: Vec::new() }
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
            root: self.root,
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
        self.nodes[self.root].value.take().unwrap()
    }
}
