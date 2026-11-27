//! B owns shared-Clause overlap and equivalence, including equivalence planning. The original query owns
//! its physical containers and borrows immutable Clause/Fact fields until return.
use super::facts;
use super::{
    CallBindings, Clause, ClauseNode, ClausePostorder, Condition, EncodedClause, Fact, Facts, IndexDomain, OutputBindings, Proof, Value, ValueIdentity,
    internal_conflicts, map_domain, output_identity,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    ffi::c_void,
    ptr,
    sync::Arc,
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
struct FactWalk<'a> {
    pending: Vec<&'a Clause>,
    seen: std::collections::HashSet<usize>,
}

impl<'a> FactWalk<'a> {
    fn new(roots: impl IntoIterator<Item = &'a Clause>) -> Self {
        let mut pending = roots.into_iter().collect::<Vec<_>>();
        pending.reverse();
        Self { pending, seen: Default::default() }
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct FactWalkHost {
    inspect: unsafe extern "C" fn(Slot, Borrowed) -> SourceNode,
    push: unsafe extern "C" fn(Slot, Borrowed),
    pop: unsafe extern "C" fn(Slot) -> Borrowed,
}

unsafe extern "C" fn inspect_fact_clause(walk: Slot, clause: Borrowed) -> SourceNode {
    let walk = unsafe { &mut *walk.cast::<FactWalk<'_>>() };
    let clause = unsafe { &*clause.cast::<Clause>() };
    if walk.seen.insert(clause.key()) {
        source_node(clause)
    } else {
        SourceNode::default()
    }
}

unsafe extern "C" fn push_fact_clause(walk: Slot, clause: Borrowed) {
    unsafe { &mut *walk.cast::<FactWalk<'_>>() }.pending.push(unsafe { &*clause.cast::<Clause>() });
}

unsafe extern "C" fn pop_fact_clause(walk: Slot) -> Borrowed {
    unsafe { &mut *walk.cast::<FactWalk<'_>>() }.pending.pop()
        .map_or(std::ptr::null(), |clause| std::ptr::from_ref(clause).cast())
}

const FACT_WALK: FactWalkHost = FactWalkHost {
    inspect: inspect_fact_clause,
    push: push_fact_clause,
    pop: pop_fact_clause,
};

struct ConditionInput<'a, F> {
    facts: Option<facts::Iter<'a>>,
    accept: F,
}

#[repr(C)]
struct ConditionInputHost {
    graph: FactWalkHost,
    begin: unsafe extern "C" fn(Slot, Borrowed),
    next: unsafe extern "C" fn(Slot) -> super::fact_fields::Fields,
    predicate: unsafe extern "C" fn(Borrowed) -> super::fact_fields::PredicateView,
    accept: unsafe extern "C" fn(Slot, Borrowed) -> u32,
}

unsafe extern "C" fn begin_input_facts<F>(slot: Slot, facts: Borrowed) {
    let input = unsafe { &mut *slot.cast::<ConditionInput<'_, F>>() };
    input.facts = Some(unsafe { &*facts.cast::<Facts>() }.iter());
}

unsafe extern "C" fn next_input_fact<F>(slot: Slot) -> super::fact_fields::Fields {
    let input = unsafe { &mut *slot.cast::<ConditionInput<'_, F>>() };
    input.facts.as_mut().unwrap().next().map_or_else(Default::default, super::fact_fields::Fields::from)
}

unsafe extern "C" fn accept_input<F: FnMut(&ValueIdentity) -> bool>(slot: Slot, id: Borrowed) -> u32 {
    let input = unsafe { &mut *slot.cast::<ConditionInput<'_, F>>() };
    u32::from((input.accept)(unsafe { &*id.cast::<ValueIdentity>() }))
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::next_fact_block"]
    fn next_fact_block(walk: Slot, host: FactWalkHost) -> Borrowed;

    #[link_name = "bootstrap.condition_graph::condition_inputs"]
    fn read_condition_inputs(walk: Slot, input: Slot, host: ConditionInputHost) -> u32;
}

pub(super) fn any_fact<'a>(
    roots: impl IntoIterator<Item = &'a Clause>,
    test: &impl Fn(&Fact) -> bool,
) -> bool {
    let mut walk = FactWalk::new(roots);
    let source = std::ptr::from_mut(&mut walk).cast();
    while let Some(facts) = unsafe { next_fact_block(source, FACT_WALK).cast::<Facts>().as_ref() } {
        if facts.iter().any(test) {
            return true;
        }
    }
    false
}

pub(super) fn condition_inputs<F: FnMut(&ValueIdentity) -> bool>(roots: &[Clause; 2], accept: F) -> bool {
    let mut walk = FactWalk::new(roots);
    let mut input = ConditionInput { facts: None, accept };
    let host = ConditionInputHost {
        graph: FACT_WALK,
        begin: begin_input_facts::<F>,
        next: next_input_fact::<F>,
        predicate: super::fact_fields::predicate,
        accept: accept_input::<F>,
    };
    unsafe {
        read_condition_inputs(
            std::ptr::from_mut(&mut walk).cast(), std::ptr::from_mut(&mut input).cast(), host,
        ) != 0
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Gate {
    left: usize,
    right: usize,
    kind: u32,
    parent_start: usize,
    parent_count: usize,
}
#[repr(C)]
struct Graph {
    nodes: *mut Gate,
    stride: usize,
    values: *mut u8,
    parents: *const usize,
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
    choice: usize,
}
#[repr(C)]
struct AlternativeView {
    kind: u32,
    node: usize,
    truth: u32,
    checkpoint: Checkpoint,
}

struct Node<'a> {
    facts: Option<&'a Facts>,
    view: Gate,
}
enum Alternative<'a> {
    Clause(usize, bool),
    Fact(&'a Fact, facts::Iter<'a>),
}
#[derive(Default)]
struct Context<'a> {
    nodes: Vec<Node<'a>>,
    parents: Vec<usize>,
    pending: Vec<(usize, bool)>,
    values: Vec<u8>,
    trail: Vec<usize>,
    alternatives: Vec<(Alternative<'a>, Checkpoint)>,
    facts: Facts,
    additions: Vec<Fact>,
    conflict_input: Option<internal_conflicts::Prepared>,
    required: Option<facts::Iter<'a>>,
    next_fact: Option<(Fact, Option<facts::Iter<'a>>)>,
}
impl<'a> Context<'a> {
    fn new(sources: impl ExactSizeIterator<Item = EncodedClause<&'a Facts>>) -> Self {
        let count = sources.len();
        let nodes = sources.map(|source| {
            let (kind, facts, left, right) = match source {
                EncodedClause::Impossible => (0, None, 0, 0),
                EncodedClause::Facts(facts) => (1, Some(facts), 0, 0),
                EncodedClause::Both(left, right) => (2, None, left, right),
                EncodedClause::Either(left, right) => (3, None, left, right),
            };
            Node {
                facts,
                view: Gate {
                    left,
                    right,
                    kind,
                    parent_start: 0,
                    parent_count: 0,
                },
            }
        }).collect();
        let mut context = Self {
            nodes,
            values: vec![0; count],
            ..Self::default()
        };
        let graph = context.graph();
        // The B constructor writes the declared Gate fields and the single parent
        // allocation. The source facts and all node addresses stay fixed.
        unsafe { connect_parents(graph, count, ptr::from_mut(&mut context.parents).cast(), allocate_parents) };
        context.seed_constants();
        context
    }

    fn seed_constants(&mut self) {
        for (id, node) in self.nodes.iter().enumerate() {
            if node.view.kind == 0 {
                self.pending.push((id, false));
            } else if node.facts.is_some_and(Facts::is_empty) {
                self.pending.push((id, true));
            }
        }
    }

    fn graph(&mut self) -> Graph {
        Graph {
            nodes: ptr::addr_of_mut!(self.nodes[0].view),
            stride: std::mem::size_of::<Node<'_>>(),
            values: self.values.as_mut_ptr(),
            parents: self.parents.as_ptr(),
        }
    }

    fn add(&mut self, fact: Fact) {
        let affects_aliases = Self::affects_aliases(fact);
        if !affects_aliases
            && self.conflict_input.as_mut().is_some_and(|input| !input.insert(fact))
        {
            // The older support and its alias roots survive every checkpoint that could keep this addition.
            return;
        }
        if self.facts.insert(fact) {
            self.additions.push(fact);
            if affects_aliases {
                self.conflict_input = None;
            }
        }
    }

    fn affects_aliases(fact: Fact) -> bool {
        matches!(fact, Fact::Equal(..) | Fact::Index(_, IndexDomain::Computed(_)))
    }
}
unsafe fn context<'a, 'b>(slot: Slot) -> &'a mut Context<'b> {
    unsafe { &mut *slot.cast::<Context<'b>>() }
}

unsafe extern "C" fn allocate_parents(slot: Slot, count: usize) -> *mut usize {
    let parents = unsafe { &mut *slot.cast::<Vec<usize>>() };
    *parents = vec![0; count];
    parents.as_mut_ptr()
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

// Source roots keep the borrowed addresses valid until the synchronous walk returns.
struct OutputRename<'a, 'out> {
    bindings: &'a OutputBindings,
    roots: &'out mut dyn Iterator<Item = (usize, &'a Clause)>,
    emit: &'out mut dyn FnMut(usize, Clause),
    output_index: usize,
    pending: Vec<(&'a Clause, bool)>,
    clauses: BTreeMap<usize, Clause>,
    conditions: BTreeMap<usize, Arc<Condition>>,
    facts: facts::Iter<'a>,
    renamed: Facts,
}

#[derive(Default)]
#[repr(C)]
struct OutputPending {
    present: u32,
    ready: u32,
    source: Borrowed,
}

#[repr(u32)]
enum OutputStatus {
    Pending,
    Original,
    Replaced,
}

#[derive(PartialEq)]
#[repr(u32)]
#[allow(dead_code, reason = "the B traversal constructs these discriminants")]
enum OutputBuild {
    Original,
    Facts,
    Both,
    Either,
    Branch,
    MappedBranch,
}

unsafe fn output_rename<'a, 'b, 'out>(slot: Slot) -> &'a mut OutputRename<'b, 'out> {
    unsafe { &mut *slot.cast::<OutputRename<'b, 'out>>() }
}

unsafe fn source_clause<'a>(source: Borrowed) -> &'a Clause {
    unsafe { &*source.cast::<Clause>() }
}

unsafe extern "C" fn output_root(slot: Slot) -> Borrowed {
    let ctx = unsafe { output_rename(slot) };
    let Some((index, root)) = ctx.roots.next() else { return ptr::null(); };
    ctx.output_index = index;
    ptr::from_ref(root).cast()
}

unsafe extern "C" fn output_inspect(source: Borrowed) -> SourceNode {
    source_node(unsafe { source_clause(source) })
}

unsafe extern "C" fn output_push(slot: Slot, source: Borrowed, ready: u32) {
    unsafe { output_rename(slot) }.pending.push((unsafe { source_clause(source) }, ready != 0));
}

unsafe extern "C" fn output_pop(slot: Slot) -> OutputPending {
    unsafe { output_rename(slot) }.pending.pop().map_or_else(OutputPending::default, |(source, ready)| {
        OutputPending { present: 1, ready: u32::from(ready), source: ptr::from_ref(source).cast() }
    })
}

unsafe extern "C" fn output_status(slot: Slot, source: Borrowed) -> OutputStatus {
    let ctx = unsafe { &*slot.cast::<OutputRename<'_, '_>>() };
    let source = unsafe { source_clause(source) };
    match ctx.clauses.get(&source.key()) {
        None => OutputStatus::Pending,
        Some(result) if Arc::ptr_eq(&source.0, &result.0) => OutputStatus::Original,
        Some(_) => OutputStatus::Replaced,
    }
}

unsafe extern "C" fn output_begin_facts(slot: Slot, facts: Borrowed) {
    let ctx = unsafe { output_rename(slot) };
    ctx.renamed.clear();
    ctx.facts = unsafe { &*facts.cast::<Facts>() }.iter();
}

unsafe extern "C" fn output_map_fact(slot: Slot) -> u32 {
    let ctx = unsafe { output_rename(slot) };
    let Some(fact) = ctx.facts.next() else { return 0; };
    ctx.renamed.insert(fact.map(&|id| output_identity(id, ctx.bindings)));
    1
}

unsafe extern "C" fn output_same_facts(slot: Slot, facts: Borrowed) -> u32 {
    let ctx = unsafe { &*slot.cast::<OutputRename<'_, '_>>() };
    u32::from(ctx.renamed == *unsafe { &*facts.cast::<Facts>() })
}

unsafe extern "C" fn output_build(slot: Slot, source: Borrowed, node: SourceNode, build: OutputBuild) {
    let ctx = unsafe { output_rename(slot) };
    let source = unsafe { source_clause(source) };
    let result = match build {
        OutputBuild::Original => source.clone(),
        OutputBuild::Facts => Clause::facts(std::mem::take(&mut ctx.renamed)),
        OutputBuild::Both | OutputBuild::Either => {
            let left = &ctx.clauses[&unsafe { source_clause(node.left) }.key()];
            let right = &ctx.clauses[&unsafe { source_clause(node.right) }.key()];
            if build == OutputBuild::Both {
                Clause::both(left, right)
            } else {
                Clause::either(left, right)
            }
        }
        OutputBuild::Branch | OutputBuild::MappedBranch => {
            let parent = &ctx.clauses[&unsafe { source_clause(node.left) }.key()];
            let original = unsafe { &*node.condition.cast::<Arc<Condition>>() };
            let condition = if build == OutputBuild::MappedBranch {
                ctx.conditions.entry(Arc::as_ptr(original) as usize).or_insert_with(|| {
                    Arc::new(Condition {
                        cases: original.cases.each_ref().map(|case| ctx.clauses[&case.key()].clone()),
                    })
                })
            } else {
                original
            };
            Clause::branch(parent, condition, node.truth != 0)
        }
    };
    ctx.clauses.insert(source.key(), result);
}

unsafe extern "C" fn output_emit(slot: Slot, source: Borrowed) {
    let ctx = unsafe { output_rename(slot) };
    (ctx.emit)(ctx.output_index, ctx.clauses[&unsafe { source_clause(source) }.key()].clone());
}

#[repr(C)]
struct OutputRenameHost {
    root: unsafe extern "C" fn(Slot) -> Borrowed,
    inspect: unsafe extern "C" fn(Borrowed) -> SourceNode,
    push: unsafe extern "C" fn(Slot, Borrowed, u32),
    pop: unsafe extern "C" fn(Slot) -> OutputPending,
    status: unsafe extern "C" fn(Slot, Borrowed) -> OutputStatus,
    begin_facts: unsafe extern "C" fn(Slot, Borrowed),
    map_fact: unsafe extern "C" fn(Slot) -> u32,
    same_facts: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    build: unsafe extern "C" fn(Slot, Borrowed, SourceNode, OutputBuild),
    emit: unsafe extern "C" fn(Slot, Borrowed),
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::rename_output_graph"]
    fn rename_output_graph(slot: Slot, host: OutputRenameHost);
}

pub(super) fn rename_outputs(roots: &[Clause], bindings: &OutputBindings) -> Vec<Clause> {
    let mut outputs = Vec::with_capacity(roots.len());
    rename_output_roots(roots.iter().enumerate(), bindings, |_, result| outputs.push(result));
    outputs
}

fn rename_output_roots<'a>(
    mut roots: impl Iterator<Item = (usize, &'a Clause)>,
    bindings: &'a OutputBindings,
    mut emit: impl FnMut(usize, Clause),
) {
    let mut ctx = OutputRename {
        bindings,
        roots: &mut roots,
        emit: &mut emit,
        output_index: 0,
        pending: Vec::new(),
        clauses: BTreeMap::new(),
        conditions: BTreeMap::new(),
        facts: facts::Iter::default(),
        renamed: Facts::new(),
    };
    unsafe {
        rename_output_graph(ptr::from_mut(&mut ctx).cast(), OutputRenameHost {
            root: output_root,
            inspect: output_inspect,
            push: output_push,
            pop: output_pop,
            status: output_status,
            begin_facts: output_begin_facts,
            map_fact: output_map_fact,
            same_facts: output_same_facts,
            build: output_build,
            emit: output_emit,
        });
    }
}

#[repr(C)]
struct ProofOutputShape {
    addresses: usize,
    views: usize,
    origins: usize,
    domains: usize,
}

#[repr(C)]
struct ProofOutputHost {
    address: unsafe extern "C" fn(Slot, usize),
    view: unsafe extern "C" fn(Slot, usize),
    origins: unsafe extern "C" fn(Slot, usize),
    domains: unsafe extern "C" fn(Slot, usize),
    conditions: unsafe extern "C" fn(Slot),
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::bind_output_proof"]
    fn source_bind_output_proof(slot: Slot, shape: ProofOutputShape, host: ProofOutputHost);
}

struct ProofOutput<'a> {
    source: &'a Proof,
    capture: Option<&'a Value>,
    bindings: &'a OutputBindings,
    result: Proof,
}

unsafe extern "C" fn bind_output_address(slot: Slot, component: usize) {
    let ctx = unsafe { &mut *slot.cast::<ProofOutput<'_>>() };
    ctx.result.addresses.push(ctx.source.addresses[component].as_ref().map(|value| value.output_bound(ctx.bindings)));
}

unsafe extern "C" fn bind_output_view(slot: Slot, component: usize) {
    let ctx = unsafe { &mut *slot.cast::<ProofOutput<'_>>() };
    ctx.result.views.push(ctx.source.views[component].output_bound(ctx.bindings));
}

unsafe extern "C" fn bind_output_origins(slot: Slot, component: usize) {
    let ctx = unsafe { &mut *slot.cast::<ProofOutput<'_>>() };
    let source = ctx.source.origins.get(component).map(Vec::as_slice).unwrap_or_default();
    let mut origins: Vec<_> = source.iter().map(|&id| output_identity(id, ctx.bindings)).collect();
    if let Some(value) = ctx.capture {
        if origins.is_empty() {
            origins.push(output_identity(ValueIdentity::Register(value.regs[component]), ctx.bindings));
        }
        let (range, nonzero) = value.component_numeric_facts(component);
        ctx.result.ranges.push(range);
        ctx.result.nonzero.push(nonzero);
    }
    ctx.result.origins.push(origins);
}

unsafe extern "C" fn bind_output_domains(slot: Slot, component: usize) {
    let ctx = unsafe { &mut *slot.cast::<ProofOutput<'_>>() };
    ctx.result.domains.push(ctx.source.domains[component].iter().map(|&domain| {
        map_domain(domain, &|id| output_identity(id, ctx.bindings))
    }).collect());
}

unsafe extern "C" fn bind_output_conditions(slot: Slot) {
    let ctx = unsafe { &mut *slot.cast::<ProofOutput<'_>>() };
    if let Some(value) = ctx.capture {
        let source: Vec<_> = (0..value.regs.len())
            .map(|component| Proof::captured_condition(value, component)).collect();
        bind_output_condition_slots(source.iter().map(Option::as_ref), ctx.bindings, &mut ctx.result.conditions);
    } else {
        bind_output_condition_slots(ctx.source.conditions.iter().map(Option::as_deref),
            ctx.bindings, &mut ctx.result.conditions);
    }
}

fn bind_output_condition_slots<'a>(
    source: impl ExactSizeIterator<Item = Option<&'a Condition>>,
    bindings: &'a OutputBindings,
    output: &mut Vec<Option<Box<Condition>>>,
) {
    let width = source.len();
    let roots = source.enumerate()
        .filter_map(|(index, condition)| condition.map(|condition| (index, &condition.cases)))
        .flat_map(|(index, cases)| cases.iter().map(move |case| (index, case)));
    let mut first = None;
    rename_output_roots(roots, bindings, |index, case| {
        if let Some(first) = first.take() {
            output.resize_with(index, || None);
            output.push(Some(Box::new(Condition { cases: [first, case] })));
        } else {
            first = Some(case);
        }
    });
    output.resize_with(width, || None);
}

pub(super) fn bind_output_proof(source: &Proof, capture: Option<&Value>, bindings: &OutputBindings) -> Proof {
    let shape = ProofOutputShape {
        addresses: source.addresses.len(),
        views: source.views.len(),
        origins: capture.map_or(source.origins.len(), |value| value.regs.len()),
        domains: source.domains.len(),
    };
    let (ranges, nonzero) = match capture {
        Some(value) => (Vec::with_capacity(value.regs.len()), Vec::with_capacity(value.regs.len())),
        None => (source.ranges.clone(), source.nonzero.clone()),
    };
    let result = Proof {
        ranges,
        nonzero,
        addresses: Vec::with_capacity(shape.addresses),
        views: Vec::with_capacity(shape.views),
        origins: Vec::with_capacity(shape.origins),
        domains: Vec::with_capacity(shape.domains),
        conditions: Vec::with_capacity(capture.map_or(source.conditions.len(), |value| value.regs.len())),
    };
    let mut ctx = ProofOutput { source, capture, bindings, result };
    unsafe {
        source_bind_output_proof(ptr::from_mut(&mut ctx).cast(), shape, ProofOutputHost {
            address: bind_output_address,
            view: bind_output_view,
            origins: bind_output_origins,
            domains: bind_output_domains,
            conditions: bind_output_conditions,
        })
    };
    ctx.result
}

#[derive(Default)]
#[repr(C)]
struct CallProjectionNode {
    original: Borrowed,
    node: SourceNode,
}

struct CallProjection<'a, P> {
    pending: ClausePostorder<'a>,
    bindings: &'a P,
    results: HashMap<usize, (Clause, bool)>,
}

unsafe extern "C" fn call_next<P>(slot: Slot) -> CallProjectionNode {
    let ctx = unsafe { &mut *slot.cast::<CallProjection<'_, P>>() };
    let results = &ctx.results;
    ctx.pending.next(|key| results.contains_key(&key))
        .map_or_else(CallProjectionNode::default, |clause| CallProjectionNode {
            original: std::ptr::from_ref(clause).cast(),
            node: source_node(clause),
        })
}

unsafe fn call_key(source: Borrowed) -> usize {
    unsafe { &*source.cast::<Clause>() }.key()
}

unsafe extern "C" fn call_impossible<P>(slot: Slot, source: Borrowed) {
    let ctx = unsafe { &mut *slot.cast::<CallProjection<'_, P>>() };
    ctx.results.insert(unsafe { call_key(source) }, (Clause::impossible(), false));
}

unsafe extern "C" fn call_leaf<P: CallBindings>(slot: Slot, source: Borrowed, facts: Borrowed) {
    let ctx = unsafe { &mut *slot.cast::<CallProjection<'_, P>>() };
    let facts = unsafe { &*facts.cast::<Facts>() };
    ctx.results.insert(unsafe { call_key(source) }, Clause::project_call_leaf(facts, ctx.bindings));
}

unsafe extern "C" fn call_pair<P, const EITHER: bool>(slot: Slot, source: Borrowed, left: Borrowed, right: Borrowed) {
    let ctx = unsafe { &mut *slot.cast::<CallProjection<'_, P>>() };
    let (left, left_private) = &ctx.results[&unsafe { call_key(left) }];
    let (right, right_private) = &ctx.results[&unsafe { call_key(right) }];
    let mapped = if EITHER { Clause::either(left, right) } else { Clause::both(left, right) };
    ctx.results.insert(unsafe { call_key(source) }, (mapped, *left_private || *right_private));
}

#[repr(C)]
struct CallProjectionHost {
    next: unsafe extern "C" fn(Slot) -> CallProjectionNode,
    impossible: unsafe extern "C" fn(Slot, Borrowed),
    facts: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
    both: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed),
    either: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed),
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::project_call_graph"]
    fn project_call_graph(slot: Slot, host: CallProjectionHost);
}

pub(super) fn project_call<'a, P: CallBindings>(
    roots: impl IntoIterator<Item = &'a Clause>, bindings: &'a P,
) -> HashMap<usize, (Clause, bool)> {
    let mut ctx = CallProjection {
        pending: ClausePostorder::new(roots),
        bindings,
        results: HashMap::new(),
    };
    unsafe {
        project_call_graph(std::ptr::from_mut(&mut ctx).cast(), CallProjectionHost {
            next: call_next::<P>,
            impossible: call_impossible::<P>,
            facts: call_leaf::<P>,
            both: call_pair::<P, false>,
            either: call_pair::<P, true>,
        })
    };
    ctx.results
}

struct CallBounds<'a> {
    ordered: std::slice::Iter<'a, &'a Clause>,
    candidates: &'a Facts,
    indices: HashMap<usize, usize>,
    words: usize,
    covered: Vec<u64>,
}
impl CallBounds<'_> {
    fn offset(&self, key: usize) -> usize {
        self.indices[&key] * self.words
    }
}
unsafe extern "C" fn bound_next(slot: Slot) -> CallProjectionNode {
    let ctx = unsafe { &mut *slot.cast::<CallBounds<'_>>() };
    ctx.ordered.next().map_or_else(CallProjectionNode::default, |&clause| CallProjectionNode {
        original: std::ptr::from_ref(clause).cast(),
        node: source_node(clause),
    })
}
unsafe extern "C" fn bound_impossible(slot: Slot, source: Borrowed) {
    let ctx = unsafe { &mut *slot.cast::<CallBounds<'_>>() };
    let at = ctx.offset(unsafe { call_key(source) });
    ctx.covered[at..at + ctx.words].fill(u64::MAX);
}
unsafe extern "C" fn bound_leaf(slot: Slot, source: Borrowed, facts: Borrowed) {
    let ctx = unsafe { &mut *slot.cast::<CallBounds<'_>>() };
    let at = ctx.offset(unsafe { call_key(source) });
    let present = unsafe { &*facts.cast::<Facts>() };
    for (index, fact) in ctx.candidates.iter().enumerate() {
        if present.contains(fact) {
            ctx.covered[at + index / 64] |= 1 << (index % 64);
        }
    }
}
unsafe extern "C" fn bound_pair<const EITHER: bool>(slot: Slot, source: Borrowed, left: Borrowed, right: Borrowed) {
    let ctx = unsafe { &mut *slot.cast::<CallBounds<'_>>() };
    let out = ctx.offset(unsafe { call_key(source) });
    let left = ctx.offset(unsafe { call_key(left) });
    let right = ctx.offset(unsafe { call_key(right) });
    for i in 0..ctx.words {
        let a = ctx.covered[left + i];
        let b = ctx.covered[right + i];
        ctx.covered[out + i] = if EITHER { a & b } else { a | b };
    }
}
pub(super) fn missing_call_bounds(root: &Clause, candidates: &Facts) -> Facts {
    let ordered = Clause::ordered([root]);
    let words = candidates.len().div_ceil(64);
    let mut ctx = CallBounds {
        ordered: ordered.iter(),
        candidates,
        indices: ordered.iter().enumerate().map(|(i, clause)| (clause.key(), i)).collect(),
        words,
        covered: vec![0; ordered.len() * words],
    };
    unsafe {
        project_call_graph(std::ptr::from_mut(&mut ctx).cast(), CallProjectionHost {
            next: bound_next,
            impossible: bound_impossible,
            facts: bound_leaf,
            both: bound_pair::<false>,
            either: bound_pair::<true>,
        });
    }
    let at = ctx.offset(root.key());
    candidates.iter().enumerate().filter_map(|(index, value)| {
        (ctx.covered[at + index / 64] & (1 << (index % 64)) == 0).then_some(*value)
    }).collect()
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
    context.conflict_input = None;
    context.required = None;
    context.next_fact = None;
    context.seed_constants();
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
    if Context::affects_aliases(fact) {
        context.conflict_input = None;
    } else if let Some(input) = &mut context.conflict_input {
        input.remove(fact);
    }
}
unsafe extern "C" fn add_leaf(slot: Slot, id: usize) {
    let context = unsafe { context(slot) };
    if let Some(facts) = context.nodes[id].facts {
        facts.iter().copied().for_each(|fact| context.add(fact));
    }
}
unsafe extern "C" fn conflict(slot: Slot) -> u32 {
    let context = unsafe { context(slot) };
    let input = context.conflict_input
        .get_or_insert_with(|| internal_conflicts::Prepared::new(&context.facts));
    u32::from(input.conflict(&context.facts))
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
    if let Some(facts) = context.nodes[id].facts {
        context.required = Some(facts.iter());
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
    usize::from(!unsafe { context(slot) }.required.as_ref().unwrap().is_empty())
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
                choice: 0,
            },
        },
    }
}

#[repr(C)]
struct Host {
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
    set_next_fact: unsafe extern "C" fn(Slot, super::fact_fields::Fields),
    facts: super::fact_fields::Host,
    remaining: unsafe extern "C" fn(Slot) -> usize,
    push_clause: unsafe extern "C" fn(Slot, usize, u32, Checkpoint),
    push_fact: unsafe extern "C" fn(Slot, Borrowed, Checkpoint),
    pop_alternative: unsafe extern "C" fn(Slot) -> AlternativeView,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::connect_parents"]
    fn connect_parents(graph: Graph, count: usize, storage: Slot,
                       allocate: unsafe extern "C" fn(Slot, usize) -> *mut usize);
    #[link_name = "bootstrap.condition_graph::overlap"]
    fn query(first: usize, second: usize, truth: u32, context: Slot, graph: Graph, host: Host) -> u32;
    #[link_name = "bootstrap.condition_graph::equivalent"]
    fn query_equivalent(
        first: usize,
        second: usize,
        plan: Slot,
        search: Slot,
        count: usize,
        preparation: EquivalenceHost,
        host: Host,
        reset: unsafe extern "C" fn(Slot),
    ) -> u32;
}

fn query_sources<'a>(
    left: &'a Clause,
    right: &'a Clause,
) -> (usize, usize, super::codec::conditions::CanonicalGraph<'a>) {
    let graph = super::codec::conditions::node_ids([left, right], false);
    (graph.index(left.key()), graph.index(right.key()), graph)
}

pub(super) fn overlap(left: &Clause, right: &Clause, truth: bool) -> bool {
    let unsigned32 = super::Scalar::U32;
    let (first, second, sources) = query_sources(left, right);
    let mut context = Context::new(sources.nodes());
    let graph = context.graph();
    unsafe {
        query(first, second, u32::from(truth), (&mut context as *mut Context<'_>).cast(),
            graph, host(&unsigned32)) != 0
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Conjunct<'a> {
    Fact(&'a Fact),
    Node(usize),
}

#[derive(Clone, Copy)]
#[repr(C)]
struct QueryNode {
    left: usize,
    right: usize,
    kind: u32,
}

#[derive(Default)]
struct Equivalence<'a> {
    sources: Vec<EncodedClause<&'a Facts>>,
    indices: Vec<usize>,
    seen: HashSet<usize>,
    pending: Vec<usize>,
    lists: Vec<Vec<usize>>,
    atoms: [BTreeSet<Conjunct<'a>>; 2],
    gates: HashMap<(u32, usize, usize), usize>,
}

unsafe fn equivalence<'a, 'b>(slot: Slot) -> &'a mut Equivalence<'b> {
    unsafe { &mut *slot.cast() }
}

unsafe extern "C" fn plan_node(slot: Slot, id: usize) -> QueryNode {
    let (kind, left, right) = match unsafe { equivalence(slot) }.sources[id] {
        EncodedClause::Impossible => (0, 0, 0),
        EncodedClause::Facts(_) => (1, 0, 0),
        EncodedClause::Both(left, right) => (2, left, right),
        EncodedClause::Either(left, right) => (3, left, right),
    };
    QueryNode { left, right, kind }
}

fn plan_gate(node: QueryNode) -> EncodedClause<&'static Facts> {
    if node.kind == 2 {
        EncodedClause::Both(node.left, node.right)
    } else {
        EncodedClause::Either(node.left, node.right)
    }
}

unsafe extern "C" fn plan_write(slot: Slot, from: usize, to: usize, node: QueryNode) {
    let plan = unsafe { equivalence(slot) };
    plan.sources[to] = if node.kind >= 2 { plan_gate(node) } else { plan.sources[from] };
}

unsafe extern "C" fn plan_append(slot: Slot, node: QueryNode) -> usize {
    let plan = unsafe { equivalence(slot) };
    let id = plan.sources.len();
    plan.sources.push(plan_gate(node));
    id
}

unsafe extern "C" fn plan_gate_get(slot: Slot, key: QueryNode) -> usize {
    unsafe { equivalence(slot) }.gates.get(&(key.kind, key.left, key.right)).copied().unwrap_or(usize::MAX)
}

unsafe extern "C" fn plan_gate_put(slot: Slot, key: QueryNode, id: usize) {
    unsafe { equivalence(slot) }.gates.insert((key.kind, key.left, key.right), id);
}

unsafe extern "C" fn plan_size(slot: Slot) -> usize {
    unsafe { equivalence(slot) }.sources.len()
}

unsafe extern "C" fn plan_indices(slot: Slot, count: usize) -> Span {
    let indices = &mut unsafe { equivalence(slot) }.indices;
    indices.clear();
    indices.resize(count, 0);
    Span { data: indices.as_mut_ptr(), count }
}

unsafe extern "C" fn plan_lists(slot: Slot, count: usize) {
    unsafe { equivalence(slot) }.lists.resize_with(count, Vec::new);
}

unsafe extern "C" fn plan_list(slot: Slot, list: usize) -> Span {
    let items = &unsafe { equivalence(slot) }.lists[list];
    Span { data: items.as_ptr(), count: items.len() }
}

unsafe extern "C" fn plan_list_push(slot: Slot, list: usize, id: usize) {
    unsafe { equivalence(slot) }.lists[list].push(id);
}

unsafe extern "C" fn plan_list_clear(slot: Slot, list: usize) {
    unsafe { equivalence(slot) }.lists[list].clear();
}

unsafe extern "C" fn plan_list_sort(slot: Slot, list: usize) {
    unsafe { equivalence(slot) }.lists[list].sort_unstable();
}

unsafe extern "C" fn plan_seen_clear(slot: Slot) {
    unsafe { equivalence(slot) }.seen.clear();
}

unsafe extern "C" fn plan_visit(slot: Slot, id: usize) -> u32 {
    u32::from(unsafe { equivalence(slot) }.seen.insert(id))
}

unsafe extern "C" fn plan_push(slot: Slot, id: usize) {
    unsafe { equivalence(slot) }.pending.push(id);
}

unsafe extern "C" fn plan_pop(slot: Slot) -> usize {
    unsafe { equivalence(slot) }.pending.pop().unwrap_or(usize::MAX)
}

unsafe extern "C" fn plan_atoms_clear(slot: Slot, side: usize) {
    unsafe { equivalence(slot) }.atoms[side].clear();
}

unsafe extern "C" fn plan_atom_insert(slot: Slot, side: usize, id: usize) {
    let plan = unsafe { equivalence(slot) };
    let atoms = &mut plan.atoms[side];
    match plan.sources[id] {
        EncodedClause::Facts(facts) => atoms.extend(facts.iter().map(Conjunct::Fact)),
        _ => { atoms.insert(Conjunct::Node(id)); }
    }
}

unsafe extern "C" fn plan_atoms_subset(slot: Slot, left: usize, right: usize) -> u32 {
    let atoms = &unsafe { equivalence(slot) }.atoms;
    u32::from(atoms[left].is_subset(&atoms[right]))
}

unsafe extern "C" fn plan_build(slot: Slot, search: Slot, count: usize) -> Graph {
    let Equivalence { mut sources, .. } = std::mem::take(unsafe { equivalence(slot) });
    sources.truncate(count);
    let search = unsafe { context(search) };
    *search = Context::new(sources.into_iter());
    search.graph()
}

#[repr(C)]
struct EquivalenceHost {
    node: unsafe extern "C" fn(Slot, usize) -> QueryNode,
    write: unsafe extern "C" fn(Slot, usize, usize, QueryNode),
    append: unsafe extern "C" fn(Slot, QueryNode) -> usize,
    gate_get: unsafe extern "C" fn(Slot, QueryNode) -> usize,
    gate_put: unsafe extern "C" fn(Slot, QueryNode, usize),
    size: unsafe extern "C" fn(Slot) -> usize,
    indices: unsafe extern "C" fn(Slot, usize) -> Span,
    lists: unsafe extern "C" fn(Slot, usize),
    list: unsafe extern "C" fn(Slot, usize) -> Span,
    list_push: unsafe extern "C" fn(Slot, usize, usize),
    list_clear: unsafe extern "C" fn(Slot, usize),
    list_sort: unsafe extern "C" fn(Slot, usize),
    seen_clear: unsafe extern "C" fn(Slot),
    visit: unsafe extern "C" fn(Slot, usize) -> u32,
    push: unsafe extern "C" fn(Slot, usize),
    pop: unsafe extern "C" fn(Slot) -> usize,
    atoms_clear: unsafe extern "C" fn(Slot, usize),
    atom_insert: unsafe extern "C" fn(Slot, usize, usize),
    atoms_subset: unsafe extern "C" fn(Slot, usize, usize) -> u32,
    build: unsafe extern "C" fn(Slot, Slot, usize) -> Graph,
}

pub(super) fn equivalent(left: &Clause, right: &Clause) -> bool {
    if left.key() == right.key() {
        return true;
    }
    let (first, second, sources) = query_sources(left, right);
    let count = sources.len();
    let mut plan = Equivalence { sources: sources.nodes().collect(), ..Equivalence::default() };
    let mut search = Context::default();
    let unsigned32 = super::Scalar::U32;
    let preparation = EquivalenceHost {
        node: plan_node,
        write: plan_write,
        append: plan_append,
        gate_get: plan_gate_get,
        gate_put: plan_gate_put,
        size: plan_size,
        indices: plan_indices,
        lists: plan_lists,
        list: plan_list,
        list_push: plan_list_push,
        list_clear: plan_list_clear,
        list_sort: plan_list_sort,
        seen_clear: plan_seen_clear,
        visit: plan_visit,
        push: plan_push,
        pop: plan_pop,
        atoms_clear: plan_atoms_clear,
        atom_insert: plan_atom_insert,
        atoms_subset: plan_atoms_subset,
        build: plan_build,
    };
    unsafe {
        query_equivalent(first, second, (&mut plan as *mut Equivalence<'_>).cast(),
            (&mut search as *mut Context<'_>).cast(), count, preparation, host(&unsigned32), reset_search) != 0
    }
}
fn host(unsigned32: &super::Scalar) -> Host {
    Host {
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
    valid_additions: Option<bool>,
}
pub(super) struct RequiredQuery<'a> {
    nodes: Vec<RequiredResult<'a>>,
    root: usize,
    pending: Vec<usize>,
}
struct RequiredEvaluation<'a, 'b, 'c> {
    common_equalities: Option<std::cell::OnceCell<Arc<Facts>>>,
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
    evaluation.query.nodes[id].valid_additions = None;
    evaluation.query.nodes[id].value = Some(super::common_facts::either(
        evaluation.environment,
        evaluation.roots,
        evaluation.members,
        evaluation.common_equalities.as_ref(),
        a,
        b,
    ));
}
unsafe extern "C" fn required_valid_additions(slot: Slot, id: usize) -> u32 {
    let evaluation = unsafe { required_evaluation(slot) };
    let node = &mut evaluation.query.nodes[id];
    let facts = node.value.as_ref().unwrap().as_ref().unwrap();
    if node.node.users == 1 {
        return u32::from(!super::additions_conflict(evaluation.environment, facts));
    }
    // All consumers of this result see the same immutable query environment.
    // A new common result resets this decision when the environment changes.
    u32::from(*node.valid_additions.get_or_insert_with(|| !super::additions_conflict(evaluation.environment, facts)))
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
        let graph = super::codec::conditions::node_ids([root], true);
        for shape in graph.nodes() {
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
                valid_additions: None,
            });
        }
        let root = graph.index(root.key());
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
            common_equalities: (!roots.is_empty()).then(std::cell::OnceCell::new),
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
