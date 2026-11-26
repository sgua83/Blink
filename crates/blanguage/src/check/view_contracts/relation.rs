use super::*;
use bounds::RelationPath;
use std::{borrow::Cow, ffi::c_void, ptr};

type Borrowed = *const c_void;
type Slot = *mut c_void;

const PLAN_REACHABLE: u64 = 1;
const PLAN_VARIABLE: u64 = 2;
const PLAN_READS: u64 = 4;

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct PlanNode {
    first: usize,
    end: usize,
    flags: u64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct PlanState {
    pending: usize,
    users_start: usize,
    users_count: usize,
    changes: u64,
}

#[repr(C)]
struct OrderCount {
    completed: usize,
    stable: usize,
}

#[repr(C)]
struct PlanInput {
    rows: *const PlanNode,
    dependencies: *const usize,
    count: usize,
}

#[repr(C)]
struct PlanWorkspace {
    states: *mut PlanState,
    users: *mut usize,
    ready: *mut usize,
}

#[repr(C)]
struct PlanOutput {
    ordered: *mut usize,
    ordered_count: usize,
    fixed_inputs: *mut u8,
}

struct EvaluationPlan {
    ordered: Vec<usize>,
    stable: usize,
    fixed_inputs: Vec<u8>,
    completed: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::plan"]
    fn plan_relation(input: PlanInput, work: PlanWorkspace, output: PlanOutput) -> OrderCount;
}

#[derive(Clone)]
pub(in crate::check) enum Epoch { Input, Local { memory:usize, snapshots:Option<Arc<[ValueSnapshot]>> } }
impl Epoch { fn key(&self)->Option<usize>{match self{Self::Input=>None,Self::Local{memory,..}=>Some(*memory)}} }
impl PartialEq for Epoch{fn eq(&self,b:&Self)->bool{self.key()==b.key()}}
impl Eq for Epoch{}
impl PartialOrd for Epoch{fn partial_cmp(&self,b:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(b))}}
impl Ord for Epoch{fn cmp(&self,b:&Self)->std::cmp::Ordering{self.key().cmp(&b.key())}}


#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
enum Op { Identity, Position(address::Origin), Initial{origin:address::Origin,frame:Option<usize>}, Read{word:u32,epoch:Epoch,frame:Option<usize>}, Element{fields:Vec<usize>,index:Option<bounds::Selector>}, Projection(Vec<usize>), Resources, Owners, Select(Vec<Option<bounds::Selector>>) }

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Edge { from:usize, op:Op, when:RelationPath }

#[derive(Clone,Default,PartialEq,Eq,PartialOrd,Ord)]
struct Node { seeds:Borrow, edges:Vec<Edge> }

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Write { target:usize, fields:Option<Vec<(u32,usize)>>, when:bounds::Condition }

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Frame { parent:Option<usize>, writes:Vec<Write> }

#[derive(Clone,Default,PartialEq,Eq,PartialOrd,Ord)]
pub(in crate::check) struct Graph { nodes:Vec<Node>, variables:BTreeSet<usize>, frames:Vec<Frame>, inputs:BTreeMap<LoopInput,usize> }


#[derive(Clone, Copy)]
#[repr(C)]
struct WalkSlice { data: Borrowed, count: usize }
impl WalkSlice {
    fn of<T>(values: &[T]) -> Self { Self { data: values.as_ptr().cast(), count: values.len() } }
}

#[repr(C)]
struct WalkVisit { node: usize, read: u32 }
#[repr(C)]
struct WalkEdge { source: usize, frame: usize, read: u32 }
#[repr(C)]
struct WalkFrame { parent: usize, writes: WalkSlice }
#[repr(C)]
struct WalkWrite { target: usize, fields: WalkSlice }

#[repr(C)]
struct RelationSource {
    edges: unsafe extern "C" fn(Borrowed, usize) -> WalkSlice,
    edge: unsafe extern "C" fn(Borrowed, usize) -> WalkEdge,
    frame: unsafe extern "C" fn(Borrowed, usize) -> WalkFrame,
    write: unsafe extern "C" fn(Borrowed, usize) -> WalkWrite,
    field: unsafe extern "C" fn(Borrowed, usize) -> usize,
}

impl RelationSource {
    const GRAPH: Self = Self {
        edges: walk_edges, edge: walk_edge, frame: walk_frame, write: walk_write, field: walk_field,
    };
}

#[repr(C)]
struct WalkEdges {
    source: RelationSource,
    mark_frame: unsafe extern "C" fn(Slot, WalkVisit) -> u32,
    publish: unsafe extern "C" fn(Slot, WalkVisit),
}

#[repr(C)]
struct WalkHost {
    edges: WalkEdges,
    pop: unsafe extern "C" fn(Slot) -> WalkVisit,
    visit: unsafe extern "C" fn(Slot, WalkVisit) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::walk_relation"]
    fn walk_relation(graph: Borrowed, work: Slot, root: usize, feedback: u32, host: WalkHost) -> u32;
    #[link_name = "bootstrap.relation_order::relation_dependencies"]
    fn relation_dependencies(graph: Borrowed, work: Slot, root: usize, host: WalkEdges);
    #[link_name = "bootstrap.relation_order::historical_relation_frames"]
    fn historical_relation_frames(
        graph: Borrowed, work: Slot, roots: Slot, next: unsafe extern "C" fn(Slot) -> usize, host: WalkEdges,
    );
}

#[derive(Clone, Copy)]
struct RelationView<'g> {
    base: &'g Graph,
    tail: &'g [Node],
    residual: Option<&'g ResidualRelation>,
}

struct RelationNode<'g> {
    seeds: &'g Borrow,
    edges: &'g [Edge],
}

impl<'g> RelationView<'g> {
    fn node(self, id: usize) -> RelationNode<'g> {
        if id >= self.base.nodes.len() {
            let node = &self.tail[id - self.base.nodes.len()];
            return RelationNode { seeds: &node.seeds, edges: &node.edges };
        }
        let original = &self.base.nodes[id];
        match self.residual.and_then(|residual| residual.nodes.get(&id)) {
            Some(node) => RelationNode {
                seeds: &node.seeds,
                edges: node.edges.as_deref().unwrap_or(&original.edges),
            },
            None => RelationNode { seeds: &original.seeds, edges: &original.edges },
        }
    }

    fn reachable(self, root: usize) -> (BTreeSet<usize>, BTreeSet<usize>) {
        let mut work = RelationWalk::from_view(self, |_| false);
        work.walk(root, false);
        let [nodes, _] = work.nodes;
        let [frames, _] = work.frames;
        (nodes, frames)
    }

    fn selected(self, root: usize) -> Graph {
        let (nodes, frames) = self.reachable(root);
        let nodes: Vec<_> = nodes.into_iter().collect();
        let frames: Vec<_> = frames.into_iter().collect();
        let selection = RelationSelection {
            nodes: WalkSlice::of(&nodes), frames: WalkSlice::of(&frames), root,
        };
        let mut selected = SelectedRelation {
            source: self,
            result: Graph { nodes: Vec::with_capacity(nodes.len()), frames: Vec::with_capacity(frames.len()), ..Graph::default() },
        };
        unsafe {
            compact_relation(
                ptr::from_ref(&self).cast(), ptr::from_mut(&mut selected).cast(),
                selection, RelationSource::GRAPH, SelectedRelation::OUTPUT,
            );
        }
        selected.result.inputs.extend(self.base.inputs.iter().filter_map(|(&input, &id)| {
            let id = unsafe { compact_relation_index(selection, id) };
            (id != usize::MAX).then_some((input, id))
        }));
        selected.result
    }
}

unsafe extern "C" fn walk_edges(graph: Borrowed, node: usize) -> WalkSlice {
    let graph = unsafe { *graph.cast::<RelationView<'_>>() };
    WalkSlice::of(graph.node(node).edges)
}

unsafe extern "C" fn walk_edge(edges: Borrowed, index: usize) -> WalkEdge {
    let edge = unsafe { &*edges.cast::<Edge>().add(index) };
    let (frame, read) = match edge.op {
        Op::Read { frame, .. } => (frame, 1),
        Op::Initial { frame, .. } => (frame, 0),
        _ => (None, 0),
    };
    WalkEdge { source: edge.from, frame: frame.unwrap_or(usize::MAX), read }
}

unsafe extern "C" fn walk_frame(graph: Borrowed, frame: usize) -> WalkFrame {
    let graph = unsafe { *graph.cast::<RelationView<'_>>() };
    let original = &graph.base.frames[frame];
    let writes = graph.residual.and_then(|residual| residual.writes.get(&frame))
        .map_or(original.writes.as_slice(), Vec::as_slice);
    WalkFrame { parent: original.parent.unwrap_or(usize::MAX), writes: WalkSlice::of(writes) }
}

unsafe extern "C" fn walk_write(writes: Borrowed, index: usize) -> WalkWrite {
    let write = unsafe { &*writes.cast::<Write>().add(index) };
    WalkWrite { target: write.target, fields: WalkSlice::of(write.fields.as_deref().unwrap_or_default()) }
}

unsafe extern "C" fn walk_field(fields: Borrowed, index: usize) -> usize {
    unsafe { (*fields.cast::<(u32, usize)>().add(index)).1 }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct RelationSelection {
    nodes: WalkSlice,
    frames: WalkSlice,
    root: usize,
}

#[repr(C)]
struct RelationOutput {
    node: unsafe extern "C" fn(Slot, usize, usize, usize) -> Slot,
    edge: unsafe extern "C" fn(Slot, Borrowed, usize, usize, usize, Borrowed),
    frame: unsafe extern "C" fn(Slot, usize, usize) -> Slot,
    write: unsafe extern "C" fn(Slot, Borrowed, usize, usize) -> Slot,
    field: unsafe extern "C" fn(Slot, Borrowed, usize, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::compact"]
    fn compact_relation(
        source: Borrowed, destination: Slot, selection: RelationSelection,
        graph: RelationSource, output: RelationOutput,
    );
    #[link_name = "bootstrap.relation_order::compact_index"]
    fn compact_relation_index(selection: RelationSelection, original: usize) -> usize;
}

struct SelectedRelation<'g> {
    source: RelationView<'g>,
    result: Graph,
}

impl SelectedRelation<'_> {
    const OUTPUT: RelationOutput = RelationOutput {
        node: Self::node, edge: emit_relation_edge, frame: Self::frame,
        write: emit_relation_write, field: emit_relation_field,
    };

    unsafe extern "C" fn node(destination: Slot, id: usize, index: usize, count: usize) -> Slot {
        let selected = unsafe { &mut *destination.cast::<Self>() };
        let graph = &mut selected.result;
        if selected.source.base.variables.contains(&id) {
            graph.variables.insert(index);
        }
        graph.nodes.push(Node {
            seeds: selected.source.node(id).seeds.clone(),
            edges: Vec::with_capacity(count),
        });
        ptr::from_mut(&mut graph.nodes[index]).cast()
    }

    unsafe extern "C" fn frame(destination: Slot, parent: usize, count: usize) -> Slot {
        let selected = unsafe { &mut *destination.cast::<Self>() };
        append_relation_frame(&mut selected.result.frames, parent, count)
    }
}

fn append_relation_frame(frames: &mut Vec<Frame>, parent: usize, count: usize) -> Slot {
    let index = frames.len();
    frames.push(Frame {
        parent: (parent != usize::MAX).then_some(parent),
        writes: Vec::with_capacity(count),
    });
    ptr::from_mut(&mut frames[index]).cast()
}

unsafe extern "C" fn emit_relation_edge(
    destination: Slot, edges: Borrowed, index: usize, from: usize, frame: usize, entry: Borrowed,
) {
    let node = unsafe { &mut *destination.cast::<Node>() };
    let old = unsafe { &*edges.cast::<Edge>().add(index) };
    let mut op = old.op.clone();
    let parent = (frame != usize::MAX).then_some(frame);
    match &mut op {
        Op::Read { frame, epoch, .. } => {
            *frame = parent;
            if let Some(local) = unsafe { entry.cast::<Epoch>().as_ref() } {
                *epoch = local.clone();
            }
        }
        Op::Initial { frame, .. } => *frame = parent,
        _ => {}
    }
    node.edges.push(Edge { from, op, when: old.when.clone() });
}

unsafe extern "C" fn emit_relation_write(destination: Slot, writes: Borrowed, index: usize, target: usize) -> Slot {
    let frame = unsafe { &mut *destination.cast::<Frame>() };
    let old = unsafe { &*writes.cast::<Write>().add(index) };
    let index = frame.writes.len();
    frame.writes.push(Write {
        target,
        fields: old.fields.as_ref().map(|fields| Vec::with_capacity(fields.len())),
        when: old.when.clone(),
    });
    frame.writes[index].fields.as_mut()
        .map_or(ptr::null_mut(), |fields| ptr::from_mut(fields).cast())
}

unsafe extern "C" fn emit_relation_field(destination: Slot, fields: Borrowed, index: usize, target: usize) {
    let output = unsafe { &mut *destination.cast::<Vec<(u32, usize)>>() };
    let (word, _) = unsafe { *fields.cast::<(u32, usize)>().add(index) };
    output.push((word, target));
}

#[repr(C)]
struct RelationOffsets {
    nodes: usize,
    frames: usize,
    prefix: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::import_relation"]
    fn import_relation_graph(
        graph: Borrowed, destination: Slot, root: usize, nodes: usize, frames: usize,
        offsets: RelationOffsets, entry: Borrowed, source: RelationSource, output: RelationOutput,
        bind_inputs: unsafe extern "C" fn(Slot, usize),
    ) -> usize;
}

struct RelationImport<'work, 'source> {
    builder: &'work mut Builder<'source>,
    graph: &'work Graph,
}

impl RelationImport<'_, '_> {
    const OUTPUT: RelationOutput = RelationOutput {
        node: Self::node, edge: emit_relation_edge, frame: Self::frame,
        write: emit_relation_write, field: emit_relation_field,
    };

    unsafe extern "C" fn node(context: Slot, id: usize, node: usize, count: usize) -> Slot {
        let import = unsafe { &mut *context.cast::<Self>() };
        if import.graph.variables.contains(&id) {
            import.builder.graph.variables.insert(node);
        }
        import.builder.fill(node, &import.graph.nodes[id].seeds, &Epoch::Input);
        let output = &mut import.builder.graph.nodes[node];
        output.edges.reserve(count);
        ptr::from_mut(output).cast()
    }

    unsafe extern "C" fn frame(context: Slot, parent: usize, count: usize) -> Slot {
        let import = unsafe { &mut *context.cast::<Self>() };
        append_relation_frame(&mut import.builder.graph.frames, parent, count)
    }

    unsafe extern "C" fn bind_inputs(context: Slot, offset: usize) {
        let import = unsafe { &mut *context.cast::<Self>() };
        for (input, id) in &import.graph.inputs {
            import.builder.graph.inputs.entry(*input).or_insert(offset + id);
        }
    }
}


struct PublicRelationValues<'a> {
    function: &'a Function,
    exposed: &'a BTreeMap<Origin, Borrow>,
    guards: &'a bounds::PublicGuards<'a>,
    relation: Option<&'a bounds::RelationGuards>,
    parameters: &'a BTreeMap<ValueIdentity, bounds::Proof>,
}

#[derive(Clone, Copy)]
enum RelationValues<'a> {
    Arguments(&'a BTreeMap<ValueIdentity, bounds::Proof>),
    Substitution(&'a BTreeMap<ValueIdentity, bounds::Proof>),
    Outputs(&'a BTreeMap<ValueIdentity, ValueIdentity>),
    Public(&'a PublicRelationValues<'a>),
}

impl RelationValues<'_> {
    fn seed(self, seed: &Borrow) -> Borrow {
        match self {
            Self::Arguments(values) => seed.at_call(values),
            Self::Substitution(values) => seed.substitute_values(values),
            Self::Outputs(values) => seed.output_bound(values),
            Self::Public(public) => canonical_with_guards(
                seed.clone(), public.function, None, public.exposed, public.guards, BorrowProjection::Input,
            ),
        }
    }

    fn guard(self, guard: &RelationPath) -> RelationPath {
        match self {
            Self::Arguments(values) => RelationPath::Guard(guard.project(values)),
            Self::Substitution(values) => RelationPath::Cases(guard.condition().substitute_values(values)),
            Self::Outputs(values) => RelationPath::Guard(guard.path().output_bound(values)),
            Self::Public(public) => RelationPath::Guard(match public.relation {
                Some(scope) => scope.edge(guard),
                None => guard.project(public.parameters),
            }),
        }
    }

    fn condition(self, condition: &bounds::Condition) -> bounds::Condition {
        match self {
            Self::Arguments(values) => condition.at_call(values),
            Self::Substitution(values) => condition.substitute_values(values),
            Self::Outputs(values) => condition.output_bound(values),
            Self::Public(public) => condition.at_call(public.parameters),
        }
    }

    fn operation(self, op: &Op) -> Op {
        match (self, op) {
            (Self::Arguments(values), Op::Position(origin)) => Op::Position(origin.bound_at_call(values)),
            (Self::Substitution(values), Op::Position(origin)) => Op::Position(origin.substitute_values(values)),
            (Self::Outputs(values), Op::Position(origin)) => Op::Position(origin.bind_outputs(values)),
            (Self::Public(public), Op::Position(origin)) => {
                Op::Position(canonical_address(origin, public.function, public.exposed, public.guards))
            }
            (Self::Public(_), Op::Read { word, frame, .. }) => {
                Op::Read { word: *word, epoch: Epoch::Input, frame: *frame }
            }
            (Self::Arguments(values), _) => Self::selectors(op, |index| Origin::index_at_call(index, values)),
            (Self::Substitution(values), _) => Self::selectors(op, |index| Origin::index_substitute(index, values)),
            _ => op.clone(),
        }
    }

    fn selectors(op: &Op, bind: impl Fn(Option<bounds::Selector>) -> Option<bounds::Selector>) -> Op {
        match op {
            Op::Element { fields, index } => Op::Element { fields: fields.clone(), index: bind(*index) },
            Op::Select(indices) => Op::Select(indices.iter().copied().map(bind).collect()),
            _ => op.clone(),
        }
    }
}

#[repr(C)]
struct RelationBindSlice {
    source: WalkSlice,
    destination: Slot,
}

#[repr(C)]
struct RelationBindOutput {
    node: unsafe extern "C" fn(Slot, usize) -> RelationBindSlice,
    edge: unsafe extern "C" fn(Slot, Slot, Borrowed, usize),
    frame: unsafe extern "C" fn(Slot, usize) -> RelationBindSlice,
    write: unsafe extern "C" fn(Slot, Slot, Borrowed, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::bind_relation_values"]
    fn bind_relation_values(context: Slot, nodes: usize, frames: usize, output: RelationBindOutput);
}

struct BindRelation<'a> {
    source: &'a Graph,
    values: RelationValues<'a>,
    result: Graph,
}

impl BindRelation<'_> {
    const OUTPUT: RelationBindOutput = RelationBindOutput {
        node: Self::node, edge: Self::edge, frame: Self::frame, write: Self::write,
    };

    unsafe extern "C" fn node(context: Slot, index: usize) -> RelationBindSlice {
        let binding = unsafe { &mut *context.cast::<Self>() };
        let original = &binding.source.nodes[index];
        binding.result.nodes.push(Node {
            seeds: binding.values.seed(&original.seeds),
            edges: Vec::with_capacity(original.edges.len()),
        });
        RelationBindSlice {
            source: WalkSlice::of(&original.edges),
            destination: ptr::from_mut(&mut binding.result.nodes[index]).cast(),
        }
    }

    unsafe extern "C" fn edge(context: Slot, node: Slot, source: Borrowed, index: usize) {
        let values = unsafe { ptr::addr_of!((*context.cast::<Self>()).values).read() };
        let node = unsafe { &mut *node.cast::<Node>() };
        let edge = unsafe { &*source.cast::<Edge>().add(index) };
        let when = values.guard(&edge.when);
        node.edges.push(Edge { from: edge.from, op: values.operation(&edge.op), when });
    }

    unsafe extern "C" fn frame(context: Slot, index: usize) -> RelationBindSlice {
        let binding = unsafe { &mut *context.cast::<Self>() };
        let original = &binding.source.frames[index];
        RelationBindSlice {
            source: WalkSlice::of(&original.writes),
            destination: append_relation_frame(
                &mut binding.result.frames, original.parent.unwrap_or(usize::MAX), original.writes.len(),
            ),
        }
    }

    unsafe extern "C" fn write(context: Slot, frame: Slot, source: Borrowed, index: usize) {
        let values = unsafe { ptr::addr_of!((*context.cast::<Self>()).values).read() };
        let frame = unsafe { &mut *frame.cast::<Frame>() };
        let write = unsafe { &*source.cast::<Write>().add(index) };
        frame.writes.push(Write {
            target: write.target,
            fields: write.fields.clone(),
            when: values.condition(&write.when),
        });
    }
}

struct RelationWalk<'g, F> {
    graph: RelationView<'g>,
    accepts: F,
    pending: Vec<WalkVisit>,
    nodes: [BTreeSet<usize>; 2],
    frames: [BTreeSet<usize>; 2],
}

impl<'g, F: FnMut(RelationNode<'g>) -> bool> RelationWalk<'g, F> {
    fn new(graph: &'g Graph, accepts: F) -> Self {
        Self::from_view(RelationView { base: graph, tail: &[], residual: None }, accepts)
    }

    fn from_view(graph: RelationView<'g>, accepts: F) -> Self {
        Self { graph, accepts, pending: Vec::new(), nodes: Default::default(), frames: Default::default() }
    }

    fn edges(publish: unsafe extern "C" fn(Slot, WalkVisit)) -> WalkEdges {
        WalkEdges { source: RelationSource::GRAPH, mark_frame: Self::mark_frame, publish }
    }

    unsafe extern "C" fn mark_frame(work: Slot, visit: WalkVisit) -> u32 {
        let work = unsafe { &mut *work.cast::<Self>() };
        u32::from(work.frames[visit.read as usize].insert(visit.node))
    }

    unsafe extern "C" fn push(work: Slot, visit: WalkVisit) {
        unsafe { &mut *work.cast::<Self>() }.pending.push(visit);
    }

    unsafe extern "C" fn pop(work: Slot) -> WalkVisit {
        unsafe { &mut *work.cast::<Self>() }.pending.pop()
            .unwrap_or(WalkVisit { node: usize::MAX, read: 0 })
    }

    unsafe extern "C" fn visit(work: Slot, visit: WalkVisit) -> u32 {
        let work = unsafe { &mut *work.cast::<Self>() };
        if !work.nodes[visit.read as usize].insert(visit.node) {
            return 0;
        }
        if (work.accepts)(work.graph.node(visit.node)) { 2 } else { 1 }
    }

    unsafe extern "C" fn collect(work: Slot, visit: WalkVisit) {
        unsafe { &mut *work.cast::<Self>() }.nodes[0].insert(visit.node);
    }

    fn walk(&mut self, root: usize, feedback: bool) -> bool {
        let graph = self.graph;
        let host = WalkHost { edges: Self::edges(Self::push), pop: Self::pop, visit: Self::visit };
        unsafe {
            walk_relation(ptr::from_ref(&graph).cast(), ptr::from_mut(self).cast(), root, u32::from(feedback), host) != 0
        }
    }

    fn historical_frames<I: Iterator<Item = usize>>(&mut self, mut roots: I) {
        unsafe extern "C" fn next<I: Iterator<Item = usize>>(roots: Slot) -> usize {
            unsafe { &mut *roots.cast::<I>() }.next().unwrap_or(usize::MAX)
        }
        let graph = self.graph;
        unsafe {
            historical_relation_frames(
                ptr::from_ref(&graph).cast(), ptr::from_mut(self).cast(), ptr::from_mut(&mut roots).cast(),
                next::<I>, Self::edges(Self::collect),
            );
        }
    }

    fn dependencies(&mut self, root: usize) {
        let graph = self.graph;
        unsafe {
            relation_dependencies(ptr::from_ref(&graph).cast(), ptr::from_mut(self).cast(), root, Self::edges(Self::collect));
        }
    }
}

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
enum Key { Loop(LoopInput), Read(ContentRead), Call(CallResult) }
#[repr(C)]
struct FeedbackEdge { from: usize, identity: u32 }
#[repr(C)]
struct FeedbackNode { edges: *const std::ffi::c_void, count: usize, variable: u32, seeded: u32 }

#[repr(C)]
struct IdentityFeedbackHost {
    edge: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> FeedbackEdge,
    node: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> FeedbackNode,
    exchange: unsafe extern "C" fn(*mut std::ffi::c_void, usize, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::prune_identity_feedback"]
    fn prune_identity_feedback(
        graph: *const std::ffi::c_void, edges: *mut std::ffi::c_void, count: usize,
        variable: usize, host: IdentityFeedbackHost,
    ) -> usize;
}

unsafe extern "C" fn identity_feedback_edge(edges: *const std::ffi::c_void, index: usize) -> FeedbackEdge {
    let edge = unsafe { &*edges.cast::<Edge>().add(index) };
    FeedbackEdge { from: edge.from, identity: u32::from(matches!(edge.op, Op::Identity)) }
}

unsafe extern "C" fn identity_feedback_node(graph: *const std::ffi::c_void, index: usize) -> FeedbackNode {
    let graph = unsafe { &*graph.cast::<Graph>() };
    let node = &graph.nodes[index];
    FeedbackNode {
        edges: node.edges.as_ptr().cast(),
        count: node.edges.len(),
        variable: u32::from(graph.variables.contains(&index)),
        seeded: u32::from(!node.seeds.empty()),
    }
}

unsafe extern "C" fn exchange_feedback_edges(edges: *mut std::ffi::c_void, retained: usize, next: usize) {
    let edges = edges.cast::<Edge>();
    unsafe { std::ptr::swap(edges.add(retained), edges.add(next)) };
}

#[derive(Default)]
struct FramePrefix {
    through: usize,
    runs: Vec<PrefixBoundary>,
}

#[repr(C)]
struct PrefixBoundary {
    memory: usize,
    frame: usize,
}

#[repr(C)]
struct PrefixState {
    through: usize,
    runs: WalkSlice,
}

#[repr(C)]
struct PrefixHost {
    state: unsafe extern "C" fn(Slot) -> PrefixState,
    emit: unsafe extern "C" fn(Slot, usize, usize) -> usize,
    publish: unsafe extern "C" fn(Slot, usize, PrefixBoundary, u32, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::lexical_frame"]
    fn lexical_frame(context: Slot, memory: usize, host: PrefixHost) -> usize;
    #[link_name = "bootstrap.relation_order::label_lexical_frames"]
    fn label_lexical_frames(runs: WalkSlice, frames: *mut PrefixBoundary, count: usize);
}

struct Builder<'a> { function:&'a Function,state:LoopScope<'a>,known:&'a States<'a>,keys:BTreeMap<Key,usize>,prefix:FramePrefix,graph:Graph,parameters:BTreeMap<ValueIdentity,usize> }
impl Builder<'_> {
    fn empty(&mut self)->usize{let id=self.graph.nodes.len();self.graph.nodes.push(Node::default());id}
    fn edge(&mut self,to:usize,from:usize,op:Op,when:bounds::Clause){self.graph.nodes[to].edges.push(Edge{from,op,when:RelationPath::Guard(when)})}
    fn borrow(&mut self,value:&Borrow,epoch:&Epoch)->usize{let id=self.empty();self.fill(id,value,epoch);id}
    fn read(&mut self, to: usize, from: usize, word: u32, epoch: Epoch, when: RelationPath) {
        let frame = match &epoch {
            Epoch::Input => None,
            Epoch::Local { memory, .. } => self.read_frame(*memory),
        };
        self.graph.nodes[to].edges.push(Edge { from, op: Op::Read { word, epoch, frame }, when });
    }
    fn fill(&mut self,id:usize,value:&Borrow,epoch:&Epoch){
        self.fill_with_seeds(id, value, epoch, |seeds, source, when| {
            seeds.sources.insert_when(source.clone(), when);
        });
    }
    fn fill_with_seeds(
        &mut self, id: usize, value: &Borrow, epoch: &Epoch,
        mut add_seed: impl FnMut(&mut Borrow, &Origin, bounds::Clause),
    ) {
        for(source,when)in value.sources.entries(){
            let target=match source {
                Origin::Parameter(input,indices)if self.parameters.contains_key(input)=>{let from=self.parameters[input];let n=self.empty();self.edge(n,from,Op::Select(indices.clone()),bounds::Clause::default());Some(n)},
                Origin::Position(origin)=>Some(self.position(origin,epoch)),
                Origin::Contents{origin,word}=>{let from=self.position(origin,epoch);let n=self.empty();self.read(n,from,*word,epoch.clone(),RelationPath::Guard(bounds::Clause::default()));Some(n)},
                Origin::Element{parent,fields,index}=>{let from=self.borrow(parent,epoch);let n=self.empty();self.edge(n,from,Op::Element{fields:fields.clone(),index:*index},bounds::Clause::default());Some(n)},
                Origin::Projection{parent,fields}=>{let from=self.borrow(parent,epoch);let n=self.empty();self.edge(n,from,Op::Projection(fields.clone()),bounds::Clause::default());Some(n)},
                Origin::Relation(graph,root)=>Some(self.import(graph,*root,None,&Epoch::Input)),
                _=>None,
            };
            if let Some(from) = target {
                self.edge(id, from, Op::Identity, when);
            } else {
                add_seed(&mut self.graph.nodes[id].seeds, source, when);
            }
        }
        for(input,when)in value.inputs.entries(){let from=self.key(Key::Loop(*input));self.edge(id,from,Op::Identity,when);}
        for(read,when)in value.reads.entries(){let from=self.key(Key::Read(read.clone()));self.edge(id,from,Op::Identity,when);}
        for(call,when)in value.calls.entries(){let from=self.key(Key::Call(call.clone()));self.edge(id,from,Op::Identity,when);}
    }
    fn position(&mut self,value:&address::Origin,epoch:&Epoch)->usize{
        if let address::Root::Parameter(parameter)=&value.root && let Some(&from)=self.parameters.get(&ValueIdentity::ParameterStorage(*parameter)){
            let id=self.empty();let mut template=value.clone();template.root=address::Root::Derived(Borrow::default());template.positions=Arc::from([]);self.edge(id,from,Op::Position(template),bounds::Clause::default());return id;
        }
        let parent=match &value.root{address::Root::Derived(b)|address::Root::Resource{borrow:b,..}|address::Root::Dataset{borrow:b,..}|address::Root::External(address::Identity::Storage(b))=>Some(b),_=>None};
        let id=self.empty();
        if let Some(parent)=parent{let from=self.borrow(parent,epoch);let mut template=value.map_borrows(|_|Borrow::default());template.positions=Arc::from([]);self.edge(id,from,Op::Position(template),bounds::Clause::default());}
        else{self.graph.nodes[id].seeds.sources.insert(Origin::Position(Arc::new(value.clone())));}
        id
    }
    fn writes(&mut self, memory: usize, names: &BTreeMap<ValueIdentity, ValueIdentity>) -> Vec<Write> {
        let usage = &self.function.uses[memory];
        let Requirement::Memory { borrow, access, written, address_fields, .. } = &usage.requirement else {
            return Vec::new();
        };
        let mut writes = Vec::new();
        let bind = |value: Borrow| {
            if names.is_empty() { value } else { value.output_bound(names) }
        };
        if let Some(call) = self.function.calls.iter().find(|call| call.memory == memory) {
            for access in &self.known[&call.function].interface.access {
                if let Access::Stored { borrows, word, value, when } = access {
                    let target = self.borrow(&bind(call_content_sources(borrows, call, &call.arguments)
                        .at_call(&call.arguments)), &Epoch::Input);
                    let value = self.borrow(&bind(call_content_sources(value, call, &call.arguments)
                        .at_call(&call.arguments)), &Epoch::Input);
                    writes.push(Write {
                        target,
                        fields: Some(vec![(*word, value)]),
                        when: when.at_call(&call.arguments).output_bound(names),
                    });
                }
            }
        } else if access.is_some_and(super::super::Access::writes) {
            let target = if names.is_empty() {
                self.borrow(borrow, &Epoch::Input)
            } else {
                self.borrow(&borrow.output_bound(names), &Epoch::Input)
            };
            let fields = written.as_ref().map(|value| {
                address_fields.iter().map(|&(word, component)| {
                    (word, self.borrow(&bind(value.view_borrow(component)), &Epoch::Input))
                }).collect()
            });
            writes.push(Write { target, fields, when: bounds::Condition::constant(true) });
        }
        writes
    }

    fn frame(&mut self, memory: usize) -> Option<usize> {
        let host = PrefixHost {
            state: Self::prefix_state, emit: Self::emit_prefix_frame, publish: Self::publish_prefix_frame,
        };
        let frame = unsafe { lexical_frame(ptr::from_mut(self).cast(), memory, host) };
        (frame != usize::MAX).then_some(frame)
    }

    unsafe extern "C" fn prefix_state(context: Slot) -> PrefixState {
        let builder = unsafe { &*context.cast::<Self>() };
        let prefix = &builder.prefix;
        PrefixState { through: prefix.through, runs: WalkSlice::of(&prefix.runs) }
    }

    unsafe extern "C" fn emit_prefix_frame(context: Slot, at: usize, parent: usize) -> usize {
        let builder = unsafe { &mut *context.cast::<Self>() };
        builder.prefix.through = at + 1;
        let mut writes = builder.writes(at, &BTreeMap::new());
        if writes.is_empty() {
            return usize::MAX;
        }
        let execution = bounds::Condition::execution(&builder.function.uses[at].path);
        for write in &mut writes {
            write.when = write.when.clone().logical(execution.clone(), false);
        }
        let id = builder.graph.frames.len();
        builder.graph.frames.push(Frame { parent: (parent != usize::MAX).then_some(parent), writes });
        id
    }

    unsafe extern "C" fn publish_prefix_frame(
        context: Slot,
        position: usize,
        boundary: PrefixBoundary,
        restore: u32,
        previous: usize,
    ) {
        let builder = unsafe { &mut *context.cast::<Self>() };
        let runs = &mut builder.prefix.runs;
        let memory = boundary.memory;
        runs.insert(position, boundary);
        if restore != 0 {
            runs.insert(position + 1, PrefixBoundary { memory: memory + 1, frame: previous });
        }
    }

    fn read_frame(&mut self, memory: usize) -> Option<usize> {
        let mut current = self.frame(memory);
        let mut frames = Vec::new();
        while let Some(frame) = current {
            frames.push(PrefixBoundary { memory: usize::MAX, frame });
            current = self.graph.frames[frame].parent;
        }
        unsafe {
            label_lexical_frames(WalkSlice::of(&self.prefix.runs), frames.as_mut_ptr(), frames.len());
        }
        let function = self.function;
        let path = &function.uses[memory].path;
        let mut parent = None;
        let mut normal = bounds::Normalizer::new();
        for (scope, at, prior) in function.carried_writes(memory) {
            let names = function.history_names(at, memory, Some(scope));
            let prior = prior.output_bound(&names);
            let mut writes = self.writes(at, &names);
            for write in &mut writes {
                // An earlier iteration may contribute this write. The current
                // read can still be reached without that iteration's write.
                write.when = bounds::Condition::paths(
                    write.when.assume(&prior, true), bounds::Clause::default(),
                );
            }
            if !writes.is_empty() {
                let id = self.graph.frames.len();
                self.graph.frames.push(Frame { parent, writes });
                parent = Some(id);
            }
        }
        for PrefixBoundary { memory: at, frame: id } in frames.into_iter().rev() {
            let names = function.history_names(at, memory, None);
            let mut writes = if names.is_empty() {
                self.graph.frames[id].writes.clone()
            } else {
                let execution = bounds::Condition::execution(&function.uses[at].path).output_bound(&names);
                let mut writes = self.writes(at, &names);
                for write in &mut writes {
                    write.when = write.when.clone().logical(execution.clone(), false);
                }
                writes
            };
            for write in &mut writes {
                write.when = normal.condition(&bounds::Condition::paths(
                    write.when.assume(path, true), write.when.assume(path, false),
                ));
            }
            let frame=Frame{parent,writes};let id=match self.graph.frames.iter().position(|f|*f==frame){Some(id)=>id,None=>{let id=self.graph.frames.len();self.graph.frames.push(frame);id}};parent=Some(id);
        }
        if let Some(observe) = self.known.observe {
            observe(self.state.name, crate::compiler::BorrowSolveStorage::ReadFrame {
                memory, tables: normal.storage(),
            });
        }
        parent
    }
    fn key(&mut self,key:Key)->usize{
        if let Some(&id)=self.keys.get(&key){return id;}
        let id=self.empty();self.keys.insert(key.clone(),id);
        if let Key::Read(ContentRead::Value { memory, .. } | ContentRead::Call { memory, .. }) = &key
            && self.function.repeated_read(*memory)
        {
            self.graph.variables.insert(id);
        }
        match key{
            Key::Loop(input)=>{self.graph.inputs.insert(input,id);self.graph.variables.insert(id);self.fill(id,&self.function.loops[&input].borrow.clone(),&Epoch::Input)},
            Key::Read(ContentRead::Relation { memory, graph, root, snapshots }) => {
                let prefix = self.read_frame(memory);
                let from = self.import(&graph, root, prefix, &Epoch::Local { memory, snapshots });
                self.edge(id, from, Op::Identity, bounds::Clause::default());
            },
            Key::Read(ContentRead::Value { memory, word, borrow, snapshots }) => {
                let from = self.borrow(&borrow, &Epoch::Input);
                let when = RelationPath::Cases(bounds::Condition::execution(&self.function.uses[memory].path));
                self.read(id, from, word, Epoch::Local { memory, snapshots: Some(snapshots) }, when);
            },
            Key::Read(ContentRead::Call { memory, origin, word, snapshots }) => {
                let from = self.position(&origin, &Epoch::Input);
                let when = RelationPath::Cases(bounds::Condition::execution(&self.function.uses[memory].path));
                self.read(id, from, word, Epoch::Local { memory, snapshots }, when);
            },
            Key::Read(ContentRead::Snapshot{memory,position:Position::Local(slot),initial,..})=>{
                let from=self.borrow(&initial.view_borrow(0),&Epoch::Input);
                let writes = self.writes(memory, &BTreeMap::new());
                let frame = if writes.is_empty() {
                    None
                } else {
                    let id = self.graph.frames.len();
                    self.graph.frames.push(Frame { parent: None, writes });
                    Some(id)
                };
                let layout=address::Layout::of(&Ty::scalar(Scalar::U64));
                let origin=address::Origin{root:address::Root::Local{base:slot,len:1},layout:layout.clone(),base:layout,words:Arc::from([0]),positions:Arc::from([Position::Local(slot)]),writable:true,initialized:Arc::from([true])};
                let path = self.function.uses[memory].path.clone();
                self.graph.nodes[id].edges.push(Edge {
                    from,
                    op: Op::Initial { origin, frame },
                    when: RelationPath::Cases(bounds::Condition::paths(path, bounds::Clause::impossible())),
                });
            },
            Key::Read(read)=>{
                let epoch=match &read{ContentRead::Snapshot{memory,..}|ContentRead::Returned{memory}=>Epoch::Local{memory:*memory,snapshots:None},_=>Epoch::Input};
                let mut evaluation=BorrowEvaluation{function:self.function,state:self.state,known:self.known,expand_loops:false,deferred_reads:true,loop_bindings:None,values:BTreeMap::new(),stored: None, exit_writes: None,stored_paths:Default::default()};let value=evaluation.content_read(&read,LoopSources::Deferred);self.fill(id,&value,&epoch);
            },
            Key::Call(result)=>{let call=&self.function.calls[result.call];let value=self.known[&call.function].interface.outputs.get(&result.output).map(|output|call_content_sources(output,call,&call.arguments).at_call(&call.arguments)).unwrap_or_else(||Borrow::source(Origin::Unresolved));self.fill(id,&value,&Epoch::Local{memory:call.memory,snapshots:call.snapshots.clone()});},
        }id
    }
    fn import(&mut self, graph: &Graph, root: usize, prefix: Option<usize>, entry: &Epoch) -> usize {
        let offsets = RelationOffsets {
            nodes: self.graph.nodes.len(),
            frames: self.graph.frames.len(),
            prefix: prefix.unwrap_or(usize::MAX),
        };
        self.graph.nodes.resize_with(offsets.nodes + graph.nodes.len(), Node::default);
        let source = RelationView { base: graph, tail: &[], residual: None };
        let entry: Borrowed = match entry {
            Epoch::Input => ptr::null(),
            Epoch::Local { .. } => ptr::from_ref(entry).cast(),
        };
        let mut import = RelationImport { builder: self, graph };
        unsafe {
            import_relation_graph(ptr::from_ref(&source).cast(), ptr::from_mut(&mut import).cast(),
                root, graph.nodes.len(), graph.frames.len(), offsets, entry, RelationSource::GRAPH,
                RelationImport::OUTPUT, RelationImport::bind_inputs)
        }
    }
}
impl Graph {
    fn builder<'a>(function:&'a Function,state:LoopScope<'a>,known:&'a States<'a>)->Builder<'a>{Builder{function,state,known,keys:BTreeMap::new(),prefix:FramePrefix::default(),graph:Self::default(),parameters:BTreeMap::new()}}
    pub(super) fn recursive_inputs(name:&str,function:&Function,state:LoopScope<'_>,known:&States)->Option<BTreeMap<ValueIdentity,Borrow>>{
        if !function.calls.iter().any(|call|call.function==name){return None;}
        let mut b=Self::builder(function,state,known);let mut roots=BTreeMap::new();
        for(&local,&formal)in &function.parameters{if function.input(formal){
            let id=*roots.entry(formal).or_insert_with(||{let id=b.empty();b.graph.variables.insert(id);b.graph.nodes[id].seeds=Borrow::source(Origin::Parameter(local,Vec::new()));id});
            b.parameters.insert(local,id);
        }}
        // Feedback depends only on edges and writes. Merge the seeds once
        // the graph has a reader, in the original argument order.
        let mut pending_seeds = Vec::new();
        for call in function.calls.iter().filter(|call| call.function == name) {
            for (input, value) in &call.arguments {
                if let Some(&id) = roots.get(input) {
                    let value = value.view_borrow(0).restricted(&call.path);
                    let epoch = Epoch::Local { memory: call.memory, snapshots: call.snapshots.clone() };
                    b.fill_with_seeds(id, &value, &epoch, |_, source, when| {
                        pending_seeds.push((id, source.clone(), when));
                    });
                }
            }
        }
        if !roots.values().any(|root| b.graph.read_feedback(*root)) {
            return None;
        }
        for (id, source, when) in pending_seeds {
            b.graph.nodes[id].seeds.sources.insert_when(source, when);
        }
        let graph=Arc::new(b.graph);let mut evaluation=BorrowEvaluation{function,state,known,expand_loops:true,deferred_reads:false,loop_bindings:None,values:BTreeMap::new(),stored: None, exit_writes: None,stored_paths:Default::default()};
        let parameters=residency::input_parameters(&function.parameters,function.input_width);
        Some(roots.into_iter().map(|(input,root)|{let value=graph.solve(root,&mut evaluation);(input,function.address_borrows(&parameters,value).unwrap_or_default())}).collect())
    }
    pub(super) fn component_inputs<'name, 'state, 'bindings>(
        entry: &'name str,
        members: &[&'name str],
        functions: &'state BTreeMap<String, &Function>,
        known: &'state States<'state>,
        public_parameters: impl Fn(&'name str) -> &'bindings BTreeMap<ValueIdentity, bounds::Proof>,
    ) -> Vec<(&'name str, BTreeMap<ValueIdentity, bounds::Proof>, BTreeMap<ValueIdentity, Borrow>)> {
        fn roots(builder: &mut Builder<'_>, scalars: &BTreeMap<ValueIdentity, bounds::Proof>, initial: bool)
            -> BTreeMap<ValueIdentity,usize>
        {
            scalars.keys().map(|&input| {
                let root = builder.empty();
                builder.graph.variables.insert(root);
                if initial {
                    builder.graph.nodes[root].seeds = Borrow::source(Origin::Parameter(input, Vec::new()));
                }
                (input, root)
            }).collect()
        }

        let function = functions[entry];
        let state = known[entry].loop_scope();
        let mut builder = Self::builder(function, state, known);
        let formals = members.iter().map(|&name| {
            let function = functions[name];
            (name, function.parameters.values().copied().filter(|&formal| function.input(formal)).collect::<BTreeSet<_>>())
        }).collect::<BTreeMap<_,_>>();
        let entry_scalars = formals[entry].iter().map(|&input| {
            (input, bounds::Proof::default().with_identity(input))
        }).collect::<BTreeMap<_,_>>();
        let entry_roots = roots(&mut builder, &entry_scalars, true);
        let mut ports = vec![(entry, entry_scalars, entry_roots)];
        let mut port = 0;
        while port < ports.len() {
            let (caller, scalars, parameters) = ports[port].clone();
            port += 1;
            let function = functions[caller];
            let state = known[caller].loop_scope();
            let public = public_parameters(caller);
            let mut local = Self::builder(function, state, known);
            let mut operands = Vec::new();
            let empty = BTreeMap::new();
            for call in &function.calls {
                let Some((&callee, inputs)) = formals.get_key_value(call.function.as_str()) else { continue; };
                let when = call.path.project_guard(public).project_guard(&scalars);
                if !when.possible() { continue; }
                let incoming = inputs.iter().map(|&input| {
                    let scalar = call.arguments.get(&input).map_or_else(bounds::Proof::default, |proof| {
                        let local = proof.numeric_at_call(&empty);
                        if local.known_boolean_value(1).is_some() {
                            return local;
                        }
                        proof.known_identity(0).and_then(|id| function.parameters.get(&id))
                            .and_then(|id| scalars.get(id)).cloned().unwrap_or(local)
                    });
                    (input, scalar)
                }).collect::<BTreeMap<_,_>>();
                let target = match ports.iter().position(|(name, known, _)| *name == callee && *known == incoming) {
                    Some(index) => index,
                    None => {
                        let nodes = roots(&mut builder, &incoming, false);
                        let index = ports.len();
                        ports.push((callee, incoming, nodes));
                        index
                    }
                };
                let epoch = Epoch::Local { memory: call.memory, snapshots: call.snapshots.clone() };
                for (input, proof) in &call.arguments {
                    let Some(&node) = ports[target].2.get(input) else { continue; };
                    let actual = proof.view_borrow(0).restricted(&call.path);
                    if actual.active() {
                        let source = local.borrow(&actual, &epoch);
                        operands.push((node, source, when.clone()));
                    }
                }
            }
            let local = Arc::new(local.graph);
            let mut evaluation = BorrowEvaluation {
                function, state, known, expand_loops: true, deferred_reads: false,
                loop_bindings: None, values: BTreeMap::new(), stored: None, exit_writes: None,
                stored_paths: Default::default(),
            };
            let identities = scalars.into_iter().map(|(input, mut proof)| {
                proof.set_view_borrow(0, Borrow::source(Origin::Parameter(input, Vec::new())));
                (input, proof)
            }).collect::<BTreeMap<_,_>>();
            builder.parameters = parameters;
            for (target, source, when) in operands {
                let value = local.solve(source, &mut evaluation);
                if let Some(actual) = function.address_borrows(public, value) {
                    let source = builder.borrow(&actual.at_call(&identities), &Epoch::Input);
                    builder.edge(target, source, Op::Identity, when);
                }
            }
        }
        let graph = Arc::new(builder.graph);
        let mut evaluation = BorrowEvaluation {
            function, state, known, expand_loops: true, deferred_reads: false,
            loop_bindings: None, values: BTreeMap::new(), stored: None, exit_writes: None,
            stored_paths: Default::default(),
        };
        ports.into_iter().map(|(name, scalars, nodes)| {
            let inputs = nodes.into_iter().map(|(input, node)| (input, graph.solve(node, &mut evaluation))).collect();
            (name, scalars, inputs)
        }).collect()
    }

    pub(super) fn loops(
        inputs: &BTreeSet<LoopInput>, function: &Function, name: &str, known: &States,
    ) -> Option<Arc<Self>> {
        if inputs.is_empty() {
            return None;
        }
        let mut builder = Self::builder(function, LoopScope { name, loops: None }, known);
        for &input in inputs {
            builder.key(Key::Loop(input));
        }
        builder.graph.remove_identity_feedback();
        Some(Arc::new(builder.graph))
    }

    pub(super) fn input_root(&self, input: LoopInput) -> usize {
        self.inputs[&input]
    }
    pub(super) fn bind(&self,root:usize,function:&Function,state:LoopScope<'_>,known:&States)->(Arc<Self>,usize){let mut b=Self::builder(function,state,known);let root=b.import(self,root,None,&Epoch::Input);(Arc::new(b.graph),root)}
    pub(super) fn loop_content(read: &ContentRead, function: &Function, state: LoopScope<'_>, known: &States)
        -> (Arc<Self>, usize)
    {
        let mut builder = Self::builder(function, state, known);
        let root = builder.key(Key::Read(read.clone()));
        (Arc::new(builder.graph), root)
    }

    pub(super) fn bind_incoming(
        &self, root: usize, memory: usize, snapshots: Option<Arc<[ValueSnapshot]>>,
        function: &Function, state: LoopScope<'_>, known: &States,
    ) -> (Arc<Self>, usize) {
        let mut builder = Self::builder(function, state, known);
        let prefix = builder.read_frame(memory);
        let root = builder.import(self, root, prefix, &Epoch::Local { memory, snapshots });
        (Arc::new(builder.graph), root)
    }
    fn remove_identity_feedback(&mut self) {
        for &variable in &self.variables {
            let mut edges = std::mem::take(&mut self.nodes[variable].edges);
            let retained = unsafe {
                prune_identity_feedback(
                    std::ptr::from_ref(self).cast(), edges.as_mut_ptr().cast(), edges.len(), variable,
                    IdentityFeedbackHost {
                        edge: identity_feedback_edge, node: identity_feedback_node, exchange: exchange_feedback_edges,
                    },
                )
            };
            edges.truncate(retained);
            self.nodes[variable].edges = edges;
        }
    }

    fn read_feedback(&self, root: usize) -> bool {
        RelationWalk::new(self, |_| false).walk(root, true)
    }

    fn reachable(&self, root: usize) -> (BTreeSet<usize>, BTreeSet<usize>) {
        RelationView { base: self, tail: &[], residual: None }.reachable(root)
    }

    fn any_reachable<'g>(&'g self, root: usize, accepts: impl FnMut(RelationNode<'g>) -> bool) -> bool {
        RelationWalk::new(self, accepts).walk(root, false)
    }

    fn evaluation_dependencies(&self, id: usize) -> BTreeSet<usize> {
        let mut work = RelationWalk::new(self, |_| false);
        work.dependencies(id);
        let [nodes, _] = work.nodes;
        nodes
    }

    fn evaluation_plan(
        &self,
        reachable: impl IntoIterator<Item = usize>,
        materialize: bool,
    ) -> EvaluationPlan {
        let mut rows = vec![PlanNode::default(); self.nodes.len()];
        let mut dependencies = Vec::new();
        let mut ordinary_nodes = 0;
        for id in reachable {
            let variable = self.variables.contains(&id);
            ordinary_nodes += usize::from(!variable);
            let first = dependencies.len();
            if materialize || !variable {
                dependencies.extend(self.evaluation_dependencies(id));
            }
            let mut flags = PLAN_REACHABLE;
            if variable {
                flags |= PLAN_VARIABLE;
            }
            if materialize && self.nodes[id].edges.iter().any(|edge| matches!(edge.op, Op::Read { .. })) {
                flags |= PLAN_READS;
            }
            rows[id] = PlanNode { first, end: dependencies.len(), flags };
        }

        let mut states = vec![PlanState::default(); rows.len()];
        let mut users = Vec::<usize>::with_capacity(dependencies.len());
        let mut ready = Vec::<usize>::with_capacity(ordinary_nodes);
        let mut ordered = Vec::new();
        let mut fixed_inputs = Vec::new();
        let output = if materialize {
            ordered.reserve(ordinary_nodes);
            fixed_inputs.resize(rows.len(), 0);
            PlanOutput { ordered: ordered.as_mut_ptr(), ordered_count: ordinary_nodes, fixed_inputs: fixed_inputs.as_mut_ptr() }
        } else {
            PlanOutput { ordered: std::ptr::null_mut(), ordered_count: 0, fixed_inputs: std::ptr::null_mut() }
        };
        // Each dependency can have one reverse entry, and each ordinary node is
        // queued and emitted at most once. Only the emitted order is read here.
        let OrderCount { completed, stable } = unsafe {
            plan_relation(
                PlanInput { rows: rows.as_ptr(), dependencies: dependencies.as_ptr(), count: rows.len() },
                PlanWorkspace { states: states.as_mut_ptr(), users: users.as_mut_ptr(), ready: ready.as_mut_ptr() },
                output,
            )
        };
        if materialize {
            unsafe { ordered.set_len(completed) };
        }
        EvaluationPlan { ordered, stable, fixed_inputs, completed }
    }
    pub(in crate::check) fn selected(&self, root: usize) -> Self {
        RelationView { base: self, tail: &[], residual: None }.selected(root)
    }
    pub(in crate::check) fn has_local(&self, root: usize) -> bool {
        self.any_reachable(root, |node| node.seeds.has_local())
    }
    pub(in crate::check) fn unresolved(&self, root: usize) -> bool {
        self.any_reachable(root, |node| node.seeds.unresolved())
    }
    pub(in crate::check) fn symbolic(&self, root: usize) -> bool {
        self.any_reachable(root, |node| {
            node.seeds.sources.iter().any(|source| {
                matches!(source, Origin::Parameter(..) | Origin::Contents { .. } | Origin::Relation(..))
            })
        })
    }
    pub(in crate::check) fn content_inputs(&self, root: usize) -> bool {
        self.any_reachable(root, |node| {
            node.edges.iter().any(|edge| matches!(edge.op, Op::Read { .. }))
                || node.seeds.content_inputs()
        })
    }
    pub(in crate::check) fn pending(&self, root: usize) -> bool {
        self.any_reachable(root, |node| {
            node.seeds.pending() || node.seeds.sources.iter().any(|source| {
                matches!(source, Origin::Relation(..) | Origin::Element { .. } | Origin::Projection { .. })
                    || matches!(source, Origin::Position(position) if matches!(
                        position.root, address::Root::Derived(_) | address::Root::Resource { .. }
                            | address::Root::Dataset { .. } | address::Root::External(address::Identity::Storage(_))
                    ))
            })
        })
    }
    fn project(self: &Arc<Self>, root: usize, op: Op) -> Borrow {
        let node = &self.nodes[root];
        if matches!(op, Op::Resources | Op::Owners)
            && node.seeds.empty()
            && node.edges.len() == 1
            && node.edges[0].op == op
            && node.edges[0].when.condition().boolean_constant() == Some(true)
        {
            return Borrow::source(Origin::Relation(Arc::clone(self), root));
        }

        let selected = Node {
            seeds: Borrow::default(),
            edges: vec![Edge { from: root, op, when: RelationPath::Guard(bounds::Clause::default()) }],
        };
        if let Some(id) = self.nodes.iter().position(|node| *node == selected) {
            return Borrow::source(Origin::Relation(Arc::clone(self), id));
        }

        let mut graph = self.as_ref().clone();
        let id = graph.nodes.len();
        graph.nodes.push(selected);
        Borrow::source(Origin::Relation(Arc::new(graph), id))
    }
    pub(in crate::check) fn resources(self: &Arc<Self>, root: usize) -> Borrow {
        self.project(root, Op::Resources)
    }
    pub(in crate::check) fn owners(self: &Arc<Self>, root: usize) -> Borrow {
        self.project(root, Op::Owners)
    }
    pub(in crate::check) fn select(self: &Arc<Self>, root: usize, indices: &[Option<bounds::Selector>]) -> Borrow {
        self.project(root, Op::Select(indices.to_vec()))
    }
    pub(in crate::check) fn seed_borrows(&self, root: usize) -> Vec<&Borrow> {
        let (reachable, _) = self.reachable(root);
        reachable.into_iter().map(|id| &self.nodes[id].seeds).collect()
    }
    pub(in crate::check) fn at_call(&self, values: &BTreeMap<ValueIdentity, bounds::Proof>) -> Self {
        self.bind_values(RelationValues::Arguments(values))
    }

    pub(in crate::check) fn substitute_values(&self, values: &BTreeMap<ValueIdentity, bounds::Proof>) -> Self {
        self.bind_values(RelationValues::Substitution(values))
    }

    pub(in crate::check) fn output_bound(&self, values: &BTreeMap<ValueIdentity, ValueIdentity>) -> Self {
        self.bind_values(RelationValues::Outputs(values))
    }

    fn bind_values(&self, values: RelationValues<'_>) -> Self {
        let mut binding = BindRelation {
            source: self,
            values,
            result: Self {
                nodes: Vec::with_capacity(self.nodes.len()),
                variables: self.variables.clone(),
                frames: Vec::with_capacity(self.frames.len()),
                inputs: match values {
                    RelationValues::Public(_) => BTreeMap::new(),
                    _ => self.inputs.clone(),
                },
            },
        };
        unsafe {
            bind_relation_values(
                ptr::from_mut(&mut binding).cast(), self.nodes.len(), self.frames.len(), BindRelation::OUTPUT,
            );
        }
        binding.result
    }

    pub(in crate::check) fn normalize(&mut self,normal:&mut bounds::Normalizer){for n in &mut self.nodes{n.seeds.normalize_contract(normal);for e in &mut n.edges{e.when.normalize(normal);if let Op::Position(p)=&mut e.op{normal.origin(p);}}}for f in &mut self.frames{for w in &mut f.writes{w.when=normal.condition(&w.when);}}}
    pub(super) fn canonical(
        &self, function: &Function, exposed: &BTreeMap<Origin, Borrow>,
        guards: &bounds::PublicGuards<'_>,
    ) -> Self {
        let input_parameters = || residency::input_parameters(&function.parameters, function.input_width);
        let relation = guards.relation(input_parameters);
        let local;
        let parameters = match relation {
            Some(scope) => &scope.bindings,
            None => {
                local = input_parameters();
                &local
            }
        };
        let public = PublicRelationValues { function, exposed, guards, relation, parameters };
        self.bind_values(RelationValues::Public(&public))
    }
    fn extension(&self,other:&Graph)->bool{other.nodes.get(..self.nodes.len())==Some(self.nodes.as_slice())&&other.frames==self.frames&&other.variables==self.variables&&other.inputs==self.inputs}
    fn references(&self,source:&Origin)->bool{
        let borrow=|b:&Borrow|b.sources.iter().any(|s|self.references(s));
        match source{
            Origin::Relation(g,_)=>self.extension(g),
            Origin::Element{parent,..}|Origin::Projection{parent,..}=>borrow(parent),
            Origin::Position(p)|Origin::Contents{origin:p,..}=>match &p.root{address::Root::Derived(b)|address::Root::Resource{borrow:b,..}|address::Root::Dataset{borrow:b,..}|address::Root::External(address::Identity::Storage(b))=>borrow(b),_=>false},
            _=>false,
        }
    }
    fn private(&self, source: &Origin) -> bool {
        !self.references(source) && Borrow::source_has_local(source)
    }
    fn residual(&self, query: &Query<'_>) -> ResidualRelation {
        let mut history = RelationWalk::new(self, |_| false);
        history.historical_frames(query.values.keys().chain(query.variables.keys()).copied());
        let [frames, _] = history.frames;
        let frames: Vec<_> = frames.into_iter().collect();
        let graph = RelationView { base: self, tail: &[], residual: None };
        let mut projection = ResidualProjection { query, result: ResidualRelation::default() };
        unsafe {
            project_relation_return(
                ptr::from_ref(&graph).cast(), ptr::from_mut(&mut projection).cast(),
                self.nodes.len(), WalkSlice::of(&frames), RelationSource::GRAPH,
                ResidualProjection::OUTPUT,
            );
        }
        projection.result
    }
    fn rebind<'r>(&self, value: &Borrow, residual: &impl Fn() -> &'r ResidualRelation) -> Borrow {
        value.map_sources(|source|match source{
            Origin::Relation(graph, root) if self.extension(&graph) => {
                let tail = &graph.nodes[self.nodes.len()..];
                let selected = RelationView { base: self, tail, residual: Some(residual()) }.selected(root);
                Borrow::source(Origin::Relation(Arc::new(selected), 0))
            },
            Origin::Element{parent,fields,index}=>Borrow::selected_element(self.rebind(&parent,residual),fields,index),
            Origin::Projection{parent,fields}=>Borrow::projection(self.rebind(&parent,residual),fields),
            Origin::Position(p)=>p.map_borrows(|b|self.rebind(b,residual)).normalize(),
            Origin::Contents{origin,word}=>Borrow::source(Origin::Contents{origin:Arc::new(origin.map_borrows(|b|self.rebind(b,residual))),word}),
            other=>Borrow::source(other),
        })
    }
    pub(super) fn solve(self: &Arc<Self>, root: usize, evaluation: &mut BorrowEvaluation<'_>) -> Borrow {
        let (reachable, _) = self.reachable(root);
        let loop_scopes = self.inputs.iter().filter_map(|(input, &node)| {
            evaluation.function.history.get(&input.level).map(|history| (node, (input.level, history)))
        }).collect::<BTreeMap<_, _>>();
        let plan = self.evaluation_plan(reachable.iter().copied(), true);
        let mut variables = self.variables.intersection(&reachable)
            .map(|&id| (id, Borrow::default())).collect::<BTreeMap<_, _>>();
        let mut values = BTreeMap::new();
        let mut initial = true;
        loop {
            let bindings = LoopBindings {
                outer: evaluation.loop_bindings,
                inputs: &self.inputs,
                values: &variables,
            };
            let mut scope = BorrowEvaluation {
                function: evaluation.function,
                state: evaluation.state,
                known: evaluation.known,
                expand_loops: evaluation.expand_loops,
                deferred_reads: evaluation.deferred_reads,
                loop_bindings: Some(&bindings),
                values: BTreeMap::new(), stored: None, exit_writes: None, stored_paths: super::StoredPaths::Borrowed(evaluation.stored_paths.paths()),
            };
            let mut query = Query {
                graph: self,
                relation: Arc::clone(self),
                variables: &variables,
                values,
            };
            let (converged, next) = {
                let mut round = RelationRound {
                    root, query: &mut query, scope: &mut scope, loop_scopes: &loop_scopes,
                    variables: variables.keys(), next: Vec::new(),
                };
                let converged = round.run(&plan, initial);
                (converged, round.next)
            };
            if converged {
                let value = query.node(root);
                let residual = std::cell::OnceCell::new();
                return self.rebind(value, &|| residual.get_or_init(|| self.residual(&query)));
            }
            values = query.values;
            variables.extend(next);
            initial = false;
        }
    }
}
#[repr(C)]
struct RoundSchedule {
    ordered: *const usize,
    count: usize,
    stable: usize,
    fixed_inputs: *const u8,
}

#[repr(C)]
struct RoundHost {
    evaluate: unsafe extern "C" fn(Slot, usize, u32),
    next_variable: unsafe extern "C" fn(Slot) -> usize,
    observe: unsafe extern "C" fn(Slot),
    equivalent: unsafe extern "C" fn(Slot, usize, u32) -> u32,
    forget: unsafe extern "C" fn(Slot, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::solve_round"]
    fn solve_relation_round(context: Slot, schedule: RoundSchedule, initial: u32, host: RoundHost) -> u32;
}

struct RelationRound<'round, 'graph, 'scope> {
    root: usize,
    query: &'round mut Query<'graph>,
    scope: &'round mut BorrowEvaluation<'scope>,
    loop_scopes: &'round BTreeMap<usize, (usize, &'scope LoopHistory)>,
    variables: std::collections::btree_map::Keys<'graph, usize, Borrow>,
    next: Vec<(usize, Borrow)>,
}

impl RelationRound<'_, '_, '_> {
    fn run(&mut self, plan: &EvaluationPlan, initial: bool) -> bool {
        let schedule = RoundSchedule {
            ordered: plan.ordered.as_ptr(), count: plan.ordered.len(),
            stable: plan.stable, fixed_inputs: plan.fixed_inputs.as_ptr(),
        };
        let host = RoundHost {
            evaluate: Self::evaluate, next_variable: Self::next_variable, observe: Self::observe,
            equivalent: Self::equivalent, forget: Self::forget,
        };
        unsafe { solve_relation_round(ptr::from_mut(self).cast(), schedule, u32::from(initial), host) != 0 }
    }

    unsafe extern "C" fn evaluate(context: Slot, id: usize, input: u32) {
        let round = unsafe { &mut *context.cast::<Self>() };
        let mut value = round.query.definition(id, round.scope);
        if input == 0 {
            round.query.values.insert(id, value);
            return;
        }
        if let Some((level, history)) = round.loop_scopes.get(&id) {
            // A carried source can come from any earlier iteration.
            let bindings = bounds::Clause::scalar_identities(value.sources.paths()).into_iter()
                .filter(|identity| !match identity {
                    ValueIdentity::Register(reg) | ValueIdentity::TaskControl(reg)
                    | ValueIdentity::TaskCompletion(reg, _) => history.registers.contains(reg),
                    ValueIdentity::MemoryValue(memory, _) => history.uses.contains(memory),
                    ValueIdentity::LoopValue(scope, _) => scope == level || round.scope.function.history.get(scope)
                        .is_some_and(|inner| inner.registers.start >= history.registers.start
                            && inner.registers.end <= history.registers.end),
                    _ => false,
                })
                .map(|identity| (identity, bounds::Proof::default().with_identity(identity)))
                .collect();
            value = value.guards_at_call(&bindings);
        }
        round.next.push((id, value));
    }

    unsafe extern "C" fn next_variable(context: Slot) -> usize {
        let round = unsafe { &mut *context.cast::<Self>() };
        round.variables.next().copied().unwrap_or(usize::MAX)
    }

    unsafe extern "C" fn observe(context: Slot) {
        let round = unsafe { &*context.cast::<Self>() };
        if let Some(observe) = round.scope.known.observe {
            use crate::compiler::{AnalysisStorage, BorrowSolveStorage};
            let query = &round.query;
            observe(round.scope.state.name, BorrowSolveStorage::Relation {
                root: round.root,
                nodes: AnalysisStorage::vector(&query.graph.nodes),
                frames: AnalysisStorage::vector(&query.graph.frames),
                resolved: AnalysisStorage::entries::<usize, Borrow>(query.values.len(), None),
                previous_inputs: AnalysisStorage::entries::<usize, Borrow>(query.variables.len(), None),
                next_inputs: AnalysisStorage::vector(&round.next),
            });
        }
    }

    unsafe extern "C" fn equivalent(context: Slot, index: usize, paths: u32) -> u32 {
        let round = unsafe { &*context.cast::<Self>() };
        let (id, value) = &round.next[index];
        let before = &round.query.variables[id].sources;
        u32::from(if paths == 0 {
            value.sources.same_sources(before)
        } else {
            value.sources.equivalent_paths(before)
        })
    }

    unsafe extern "C" fn forget(context: Slot, id: usize) {
        let round = unsafe { &mut *context.cast::<Self>() };
        round.query.values.remove(&id);
    }
}

#[derive(Default)]
struct ResidualRelation {
    nodes: BTreeMap<usize, ResidualNode>,
    writes: BTreeMap<usize, Vec<Write>>,
}

struct ResidualNode {
    seeds: Borrow,
    edges: Option<Vec<Edge>>,
}

impl ResidualNode {
    fn seeds_of(node: &Node) -> Self {
        Self { seeds: node.seeds.clone(), edges: None }
    }
}

#[repr(C)]
struct ResidualOutput {
    node_facts: unsafe extern "C" fn(Slot, usize) -> u32,
    node: unsafe extern "C" fn(Slot, usize, u32, u32),
    keep: unsafe extern "C" fn(Slot, usize) -> u32,
    write: unsafe extern "C" fn(Slot, usize, usize, u32),
    field: unsafe extern "C" fn(Slot, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::project_return"]
    fn project_relation_return(
        graph: Borrowed, output: Slot, nodes: usize, frames: WalkSlice,
        source: RelationSource, host: ResidualOutput,
    );
}

struct ResidualProjection<'q, 'g> {
    query: &'q Query<'g>,
    result: ResidualRelation,
}

impl ResidualProjection<'_, '_> {
    const OUTPUT: ResidualOutput = ResidualOutput {
        node_facts: Self::node_facts, node: Self::node, keep: Self::keep,
        write: Self::write, field: Self::field,
    };

    unsafe extern "C" fn node_facts(context: Slot, id: usize) -> u32 {
        let query = unsafe { &*context.cast::<Self>() }.query;
        let graph = query.graph;
        let resolved = u32::from(query.values.contains_key(&id));
        let bound = u32::from(query.variables.contains_key(&id));
        let variable = graph.variables.contains(&id);
        let reads = !variable && graph.nodes[id].edges.iter().any(|edge| matches!(edge.op, Op::Read { .. }));
        resolved | (bound << 1) | (u32::from(variable) << 2) | (u32::from(reads) << 3)
    }

    unsafe extern "C" fn node(context: Slot, id: usize, expand: u32, publish: u32) {
        let projection = unsafe { &mut *context.cast::<Self>() };
        let query = projection.query;
        let graph = query.graph;
        let original = &graph.nodes[id];
        let mut patch = if expand != 0 {
            match query.local_definition(id) {
                Cow::Owned(node) => Some(ResidualNode { seeds: node.seeds, edges: Some(node.edges) }),
                Cow::Borrowed(_) => None,
            }
        } else {
            None
        };
        if patch.is_none() && original.seeds.sources.iter().any(|source| graph.private(source)) {
            patch = Some(ResidualNode::seeds_of(original));
        }
        if let Some(node) = &mut patch {
            node.seeds.retain_sources(|source| !graph.private(source));
        }
        if publish != 0 {
            for (source, when) in query.node(id).sources.entries() {
                if !graph.references(source) && !Borrow::source_has_local(source) {
                    patch.get_or_insert_with(|| ResidualNode::seeds_of(original))
                        .seeds.sources.union_when(source.clone(), &when);
                }
            }
        }
        if let Some(node) = patch {
            projection.result.nodes.insert(id, node);
        }
    }

    unsafe extern "C" fn keep(context: Slot, target: usize) -> u32 {
        let query = unsafe { &*context.cast::<Self>() }.query;
        u32::from(query.node(target).sources.entries()
            .any(|(source, when)| when.possible() && !query.graph.private(source)))
    }

    unsafe extern "C" fn write(context: Slot, frame: usize, index: usize, keep: u32) {
        let projection = unsafe { &mut *context.cast::<Self>() };
        let original = &projection.query.graph.frames[frame].writes;
        let writes = &mut projection.result.writes;
        if keep == 0 {
            writes.entry(frame).or_insert_with(|| original[..index].to_vec());
        } else if let Some(selected) = writes.get_mut(&frame) {
            selected.push(original[index].clone());
        }
    }

    unsafe extern "C" fn field(context: Slot, id: usize) {
        let projection = unsafe { &mut *context.cast::<Self>() };
        let query = projection.query;
        let graph = query.graph;
        for (source, when) in query.node(id).sources.entries() {
            if graph.private(source) {
                projection.result.nodes.entry(id)
                    .or_insert_with(|| ResidualNode::seeds_of(&graph.nodes[id]))
                    .seeds.sources.union_when(source.clone(), &when);
            }
        }
    }
}

#[repr(u32)]
enum WriteReplay {
    Continue = 0,
    FinishFrame = 1,
    Stop = 2,
}

unsafe extern "C" {
    #[link_name = "bootstrap.relation_order::replay_writes"]
    fn replay_relation_writes(
        graph: Borrowed,
        context: Slot,
        frame: usize,
        frame_at: unsafe extern "C" fn(Borrowed, usize) -> WalkFrame,
        write: unsafe extern "C" fn(Slot, Borrowed, usize) -> WriteReplay,
    ) -> u32;
}

struct Query<'g> {
    graph: &'g Graph,
    relation: Arc<Graph>,
    variables: &'g BTreeMap<usize, Borrow>,
    values: BTreeMap<usize, Borrow>,
}
impl<'g> Query<'g> {
    fn node(&self, id: usize) -> &Borrow {
        self.variables.get(&id).unwrap_or_else(|| &self.values[&id])
    }

    fn definition(&self, id: usize, evaluation: &mut BorrowEvaluation<'_>) -> Borrow {
        let mut definition = self.local_definition(id);
        let mut sources = match &mut definition {
            Cow::Borrowed(node) => node.seeds.sources.clone(),
            Cow::Owned(node) => std::mem::take(&mut node.seeds.sources),
        };
        for edge in &definition.edges {
            let from = self.node(edge.from);
            let incoming = self.apply(id, edge.from, &edge.op, from, evaluation);
            sources.union_restricted(&incoming.sources, edge.when.path());
        }
        Borrow { sources, ..Borrow::default() }
    }
    fn deferred(&self,id:usize)->Borrow{Borrow::source(Origin::Relation(self.relation.clone(),id))}
    fn apply<'a>(
        &self, id: usize, from_id: usize, op: &Op, from: &'a Borrow, evaluation: &mut BorrowEvaluation<'_>,
    ) -> Cow<'a, Borrow> {
        let value = match op {
            Op::Identity => return Cow::Borrowed(from),
            Op::Read { word, epoch, frame } => self.read(id, from_id, from, *word, epoch, *frame, evaluation),
            Op::Resources => from.map_sources(|source| Borrow::source(source).resources()),
            Op::Owners => from.map_sources(|source| Borrow::source(source).owners()),
            Op::Select(indices) => from.map_sources(|source| Borrow::source(source).select_elements(indices)),
            Op::Element { fields, index } => from.map_sources(|source| {
                Borrow::selected_element(Borrow::source(source), fields.clone(), *index)
            }),
            Op::Projection(fields) => from.map_sources(|source| {
                Borrow::projection(Borrow::source(source), fields.clone())
            }),
            Op::Position(position) => from.map_sources(|source| {
                position.map_borrows(|_| Borrow::source(source)).normalize()
            }),
            Op::Initial { .. } => from.map_sources(|_| Borrow::default()),
        };
        Cow::Owned(value)
    }

    fn replay_writes<F: FnMut(&Write) -> WriteReplay>(&self, frame: Option<usize>, mut write: F) -> bool {
        unsafe extern "C" fn visit<F: FnMut(&Write) -> WriteReplay>(
            context: Slot,
            writes: Borrowed,
            index: usize,
        ) -> WriteReplay {
            let write = unsafe { &*writes.cast::<Write>().add(index) };
            let apply = unsafe { &mut *context.cast::<F>() };
            apply(write)
        }

        let graph = RelationView { base: self.graph, tail: &[], residual: None };
        unsafe {
            replay_relation_writes(ptr::from_ref(&graph).cast(), ptr::from_mut(&mut write).cast(),
                frame.unwrap_or(usize::MAX), walk_frame, visit::<F>) != 0
        }
    }

    fn local_definition(&self, id: usize) -> Cow<'g, Node> {
        let node = &self.graph.nodes[id];
        if !node.edges.iter().any(|edge| matches!(edge.op, Op::Initial { .. })) {
            return Cow::Borrowed(node);
        }
        let mut result = Node { seeds: node.seeds.clone(), edges: Vec::with_capacity(node.edges.len()) };
        for edge in &node.edges {
            let Op::Initial{origin,frame}=&edge.op else{result.edges.push(edge.clone());continue;};
            let path = edge.when.path();
            let execution = edge.when.condition();
            let position = content_position(origin, 0);
            let mut earlier = bounds::Condition::constant(true);
            self.replay_writes(*frame, |write| {
                let targets = self.node(write.target);
                let targets = targets.sources.entries().filter_map(|(target, selected)| {
                    let yes = write.when.assume(&bounds::Clause::both(path, &selected), true);
                    yes.possible().then_some((target, yes))
                });
                let Some(fields) = &write.fields else {
                    // A possible external write contributes a source while retaining the earlier value.
                    for (target, yes) in targets {
                        let affected = match target {
                            Origin::Position(target) if target.symbolic_input() && !target.has_local() => false,
                            Origin::Position(target) => address::MemoryEffect::of(
                                &Borrow::source(Origin::Position(target.clone())), super::super::Access::Write, None,
                            ).written_bytes(position.as_ref().unwrap(), Scalar::U64, None) != 0,
                            Origin::External => true,
                            _ => false,
                        };
                        if affected {
                            result.seeds.sources.union_when(Origin::External, &earlier.assume(&yes, true));
                        }
                    }
                    return WriteReplay::Continue;
                };

                let mut hit = bounds::Clause::impossible();
                let mut miss = bounds::Clause::impossible();
                for (target, yes) in targets {
                    let mut complete = false;
                    if let Origin::Position(target) = target {
                        // A formal input cannot reach this invocation's private slots.
                        if target.symbolic_input() && !target.has_local() {
                            miss = bounds::Clause::either(&miss, &yes);
                            continue;
                        }
                        for (word, value) in fields {
                            if let Some(full) = content_write_overlap(
                                Some((origin, 0)), position.as_ref(), target, *word, || Some(&yes),
                            ) {
                                result.edges.push(Edge {
                                    from: *value,
                                    op: Op::Identity,
                                    when: {
                                        let visible = earlier.clone()
                                            .logical(bounds::Condition::guard(yes.clone()), false);
                                        // The selected write already includes the read path.
                                        RelationPath::Cases(bounds::Condition::paths(
                                            visible.assume(&bounds::Clause::default(), true),
                                            bounds::Clause::either(
                                                &visible.assume(&bounds::Clause::default(), false),
                                                &execution.assume(&bounds::Clause::default(), false),
                                            ),
                                        ))
                                    },
                                });
                                hit = bounds::Clause::either(&hit, &yes);
                                complete |= full;
                            }
                        }
                    } else if matches!(target, Origin::External) {
                        result.seeds.sources.union_when(Origin::External, &earlier.assume(&yes, true));
                    }
                    if !complete {
                        miss = bounds::Clause::either(&miss, &yes);
                    }
                }
                let no = bounds::Clause::either(&write.when.assume(path, false), &miss);
                let previous = std::mem::take(&mut earlier);
                earlier = previous.logical(bounds::Condition::paths(hit, no).negate(), false);
                WriteReplay::Continue
            });
            result.edges.push(Edge { from: edge.from, op: Op::Identity, when: RelationPath::Cases(earlier.logical(execution.into_owned(), false)) });
        }
        Cow::Owned(result)
    }
    fn binding(&self, mut id: usize) -> (usize, Cow<'g, Node>) {
        loop {
            let node = self.local_definition(id);
            if !self.graph.variables.contains(&id)
                && node.seeds.empty()
                && let [edge] = node.edges.as_slice()
                && edge.op == Op::Identity
            {
                id = edge.from;
            } else {
                return (id, node);
            }
        }
    }

    fn location(&self, id: usize) -> Option<(usize, address::Origin)> {
        let (_, node) = self.binding(id);
        if node.seeds.empty()
            && let [edge] = node.edges.as_slice()
            && let Op::Position(origin) = &edge.op
        {
            Some((self.binding(edge.from).0, origin.clone()))
        } else {
            None
        }
    }
    fn read(
        &self, id: usize, from_id: usize, from: &Borrow, word: u32, epoch: &Epoch, frame: Option<usize>,
        evaluation: &mut BorrowEvaluation<'_>,
    ) -> Borrow {
        let mut output=Borrow::default();
        let read_location = std::cell::LazyCell::new(|| {
            let (binding, origin) = self.location(from_id)?;
            let (_, word) = content_word(&origin, word)?;
            Some((binding, origin, word))
        });
        for (source, path) in from.sources.entries() {
            let Origin::Position(origin) = source else { continue; };
            let position = content_position(origin, word);
            let mut content = ContentAtRead::default();
            let mut delayed = false;
            let ready = self.replay_writes(frame, |write| {
                let yes = write.when.assume(&path, true);
                if !yes.possible() { return WriteReplay::Continue; }
                let not_written = std::cell::LazyCell::new(|| write.when.assume(&path, false));
                let mut decision = WriteReplay::Continue;
                let matches = write.fields.as_deref().filter(|fields| !fields.is_empty()).and_then(|fields| {
                    let (read_binding, read, read_word) = read_location.as_ref()?;
                    let (write_binding, target) = self.location(write.target)?;
                    let same_binding = *read_binding == write_binding
                        || (!self.graph.variables.contains(read_binding)
                            && !self.graph.variables.contains(&write_binding)
                            && self.graph.nodes[*read_binding] == self.graph.nodes[write_binding]);
                    if !same_binding || read.root != target.root || read.base != target.base {
                        return None;
                    }
                    let read_word = *read_word;
                    Some(fields.iter().filter_map(move |&(field, value)| {
                        let (_, word) = content_word(&target, field)?;
                        (word == read_word).then_some(value)
                    }).peekable())
                });
                if let Some(mut values) = matches && values.peek().is_some() {
                    let mut incoming = Borrow::default();
                    for value in values {
                        let value = self.node(value);
                        if value.empty() && !not_written.possible() {
                            decision = WriteReplay::FinishFrame;
                            break;
                        }
                        incoming.sources.union(&value.sources);
                    }
                    content.write(&incoming, &write.when, true);
                    return decision;
                }
                let targets = self.node(write.target);
                if targets.empty() { return WriteReplay::Stop; }
                let mut hit=bounds::Clause::impossible();let mut miss=bounds::Clause::impossible();let mut incoming=Borrow::default();
                for(target,selected)in targets.sources.entries(){
                    let selected=bounds::Clause::both(&yes,&selected);if !selected.possible(){continue;}
                    let mut complete=false;let mut possible=false;
                    if let Origin::Position(target)=target{
                        if let Some(fields)=&write.fields{
                            for (field,value)in fields{
                                if let Some(full) = content_write_overlap(
                                    Some((origin, word)), position.as_ref(), target, *field, || Some(&selected),
                                ) {
                                    let v = self.node(*value);
                                    if v.empty() && !not_written.possible() {
                                        decision = WriteReplay::FinishFrame;
                                        break;
                                    }
                                    incoming.sources.union_restricted(&v.sources, &selected);
                                    possible = true;
                                    complete |= full;
                                } else if origin.symbolic_input() || target.symbolic_input()
                                    || matches!(origin.root, address::Root::Parameter(_))
                                    || matches!(target.root, address::Root::Parameter(_))
                                {
                                    delayed = true;
                                }
                            }
                        }else if position.as_ref().is_some_and(|p|address::MemoryEffect::of(&Borrow::source(Origin::Position(target.clone())).restricted(&selected),super::super::Access::Write,None).written_bytes(p,Scalar::U64,None)!=0){possible=true;incoming.sources.union_when(Origin::External, &selected);}
                    }else if matches!(target,Origin::External){possible=true;incoming.sources.union_when(Origin::External, &selected);}
                    if possible{hit=bounds::Clause::either(&hit,&selected);}if !complete{miss=bounds::Clause::either(&miss,&selected);}
                }
                let no = bounds::Clause::either(&not_written, &miss);
                content.write(&incoming, &bounds::Condition::paths(hit, no), true);
                decision
            });
            let value=if !ready{Borrow::default()}else if delayed{self.deferred(id)}else if content.earlier.as_ref().is_some_and(|p|!p.possible()){content.with_initial(Borrow::default())}else if origin.symbolic_input(){self.deferred(id)}else{
                let base=match epoch{Epoch::Input=>input_contents(origin,word),Epoch::Local{memory,snapshots}=>evaluation.content_before(origin,word,*memory,snapshots.as_deref())};
                if base.sources.iter().any(|s|matches!(s,Origin::Contents{..})){self.deferred(id)}else{content.with_initial(base)}
            };output.sources.union_restricted(&value.sources, &path);
        }output
    }
}

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Op);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        identity: ReadCallback,
        position: ReadCallback,
        read: ReadCallback,
        element: ReadCallback,
        projection: ReadCallback,
        resources: ReadCallback,
        owners: ReadCallback,
        select: ReadCallback,
        initial: ReadCallback,
    }
    unsafe extern "C" fn build_identity(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Op>() = Op::Identity;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_position(context: Slot, destination: Slot) -> ReadStatus {
        let mut origin = <address::Origin as Native>::empty();
        #[repr(C)]
        struct Fields {
            origin: Slot,
        }
        #[repr(C)]
        struct Host {
            origin: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_position"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    origin: contracts::slot(&mut origin),
                },
                Host {
                    origin: contracts::read_value::<address::Origin>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Position(origin);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_read(context: Slot, destination: Slot) -> ReadStatus {
        let mut word = <u32 as Native>::empty();
        let mut frame = <Option<usize> as Native>::empty();
        #[repr(C)]
        struct Fields {
            word: Slot,
            frame: Slot,
        }
        #[repr(C)]
        struct Host {
            word: ReadCallback,
            frame: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_read"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    word: contracts::slot(&mut word),
                    frame: contracts::slot(&mut frame),
                },
                Host {
                    word: contracts::read_value::<u32>,
                    frame: contracts::read_value::<Option<usize>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Read {
                word,
                epoch: Epoch::Input,
                frame,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_element(context: Slot, destination: Slot) -> ReadStatus {
        let mut fields = <Vec<usize> as Native>::empty();
        let mut index = <Option<bounds::Selector> as Native>::empty();
        #[repr(C)]
        struct Fields {
            fields: Slot,
            index: Slot,
        }
        #[repr(C)]
        struct Host {
            fields: ReadCallback,
            index: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_element"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    fields: contracts::slot(&mut fields),
                    index: contracts::slot(&mut index),
                },
                Host {
                    fields: contracts::read_value::<Vec<usize>>,
                    index: contracts::read_value::<Option<bounds::Selector>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Element { fields, index };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_projection(context: Slot, destination: Slot) -> ReadStatus {
        let mut fields = <Vec<usize> as Native>::empty();
        #[repr(C)]
        struct Fields {
            fields: Slot,
        }
        #[repr(C)]
        struct Host {
            fields: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_projection"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    fields: contracts::slot(&mut fields),
                },
                Host {
                    fields: contracts::read_value::<Vec<usize>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Projection(fields);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_resources(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Op>() = Op::Resources;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_owners(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Op>() = Op::Owners;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_select(context: Slot, destination: Slot) -> ReadStatus {
        let mut indices = <Vec<Option<bounds::Selector>> as Native>::empty();
        #[repr(C)]
        struct Fields {
            indices: Slot,
        }
        #[repr(C)]
        struct Host {
            indices: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_select"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    indices: contracts::slot(&mut indices),
                },
                Host {
                    indices: contracts::read_value::<Vec<Option<bounds::Selector>>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Select(indices);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_initial(context: Slot, destination: Slot) -> ReadStatus {
        let mut origin = <address::Origin as Native>::empty();
        let mut frame = <Option<usize> as Native>::empty();
        #[repr(C)]
        struct Fields {
            origin: Slot,
            frame: Slot,
        }
        #[repr(C)]
        struct Host {
            origin: ReadCallback,
            frame: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_initial"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    origin: contracts::slot(&mut origin),
                    frame: contracts::slot(&mut frame),
                },
                Host {
                    origin: contracts::read_value::<address::Origin>,
                    frame: contracts::read_value::<Option<usize>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Initial { origin, frame };
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_relation_op"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Op {
        fn empty() -> Self {
            Self::Identity
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Identity => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_identity"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Position(origin) => {
                    #[repr(C)]
                    struct Fields {
                        origin: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        origin: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_position"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                origin: contracts::address(origin).cast_mut(),
                            },
                            Host {
                                origin: contracts::write_value::<address::Origin>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Read { word, frame, .. } => {
                    #[repr(C)]
                    struct Fields {
                        word: Slot,
                        frame: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        word: WriteCallback,
                        frame: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_read"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                word: contracts::address(word).cast_mut(),
                                frame: contracts::address(frame).cast_mut(),
                            },
                            Host {
                                word: contracts::write_value::<u32>,
                                frame: contracts::write_value::<Option<usize>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Element { fields, index } => {
                    #[repr(C)]
                    struct Fields {
                        fields: Slot,
                        index: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        fields: WriteCallback,
                        index: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_element"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                fields: contracts::address(fields).cast_mut(),
                                index: contracts::address(index).cast_mut(),
                            },
                            Host {
                                fields: contracts::write_value::<Vec<usize>>,
                                index: contracts::write_value::<Option<bounds::Selector>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Projection(fields) => {
                    #[repr(C)]
                    struct Fields {
                        fields: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        fields: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_projection"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                fields: contracts::address(fields).cast_mut(),
                            },
                            Host {
                                fields: contracts::write_value::<Vec<usize>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Resources => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_resources"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Owners => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_owners"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Select(indices) => {
                    #[repr(C)]
                    struct Fields {
                        indices: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        indices: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_select"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                indices: contracts::address(indices).cast_mut(),
                            },
                            Host {
                                indices: contracts::write_value::<Vec<Option<bounds::Selector>>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Initial { origin, frame } => {
                    #[repr(C)]
                    struct Fields {
                        origin: Slot,
                        frame: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        origin: WriteCallback,
                        frame: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_initial"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                origin: contracts::address(origin).cast_mut(),
                                frame: contracts::address(frame).cast_mut(),
                            },
                            Host {
                                origin: contracts::write_value::<address::Origin>,
                                frame: contracts::write_value::<Option<usize>>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        identity: build_identity,
                        position: build_position,
                        read: build_read,
                        element: build_element,
                        projection: build_projection,
                        resources: build_resources,
                        owners: build_owners,
                        select: build_select,
                        initial: build_initial,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Edge);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        from: Slot,
        op: Slot,
        when: Slot,
    }
    #[repr(C)]
    struct Writer {
        from: WriteCallback,
        op: WriteCallback,
        when: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        from: ReadCallback,
        op: ReadCallback,
        when: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_edge"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_edge"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Edge {
        fn empty() -> Self {
            Self {
                from: <usize as Native>::empty(),
                op: <Op as Native>::empty(),
                when: RelationPath::Cases(<bounds::Condition as Native>::empty()),
            }
        }
        unsafe fn write(&self, output: Slot) {
            let when = self.when.condition();
            unsafe {
                write(
                    output,
                    Fields {
                        from: contracts::address(&self.from).cast_mut(),
                        op: contracts::address(&self.op).cast_mut(),
                        when: contracts::address(when.as_ref()).cast_mut(),
                    },
                    Writer {
                        from: contracts::write_value::<usize>,
                        op: contracts::write_value::<Op>,
                        when: contracts::write_value::<bounds::Condition>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut when = <bounds::Condition as Native>::empty();
            let status = unsafe {
                read(
                    context,
                    Fields {
                        from: contracts::slot(&mut output.from),
                        op: contracts::slot(&mut output.op),
                        when: contracts::slot(&mut when),
                    },
                    Reader {
                        from: contracts::read_value::<usize>,
                        op: contracts::read_value::<Op>,
                        when: contracts::read_value::<bounds::Condition>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            output.when = RelationPath::Cases(when);
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Node);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        seeds: Slot,
        edges: Slot,
    }
    #[repr(C)]
    struct Writer {
        seeds: WriteCallback,
        edges: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        seeds: ReadCallback,
        edges: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_node"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_node"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Node {
        fn empty() -> Self {
            Self {
                seeds: <Borrow as Native>::empty(),
                edges: <Vec<Edge> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        seeds: contracts::address(&self.seeds).cast_mut(),
                        edges: contracts::address(&self.edges).cast_mut(),
                    },
                    Writer {
                        seeds: contracts::write_value::<Borrow>,
                        edges: contracts::write_value::<Vec<Edge>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        seeds: contracts::slot(&mut output.seeds),
                        edges: contracts::slot(&mut output.edges),
                    },
                    Reader {
                        seeds: contracts::read_value::<Borrow>,
                        edges: contracts::read_value::<Vec<Edge>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Write);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        target: Slot,
        fields: Slot,
        when: Slot,
    }
    #[repr(C)]
    struct Writer {
        target: WriteCallback,
        fields: WriteCallback,
        when: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        target: ReadCallback,
        fields: ReadCallback,
        when: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_write"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_write"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Write {
        fn empty() -> Self {
            Self {
                target: <usize as Native>::empty(),
                fields: <Option<Vec<(u32, usize)>> as Native>::empty(),
                when: <bounds::Condition as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        target: contracts::address(&self.target).cast_mut(),
                        fields: contracts::address(&self.fields).cast_mut(),
                        when: contracts::address(&self.when).cast_mut(),
                    },
                    Writer {
                        target: contracts::write_value::<usize>,
                        fields: contracts::write_value::<Option<Vec<(u32, usize)>>>,
                        when: contracts::write_value::<bounds::Condition>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        target: contracts::slot(&mut output.target),
                        fields: contracts::slot(&mut output.fields),
                        when: contracts::slot(&mut output.when),
                    },
                    Reader {
                        target: contracts::read_value::<usize>,
                        fields: contracts::read_value::<Option<Vec<(u32, usize)>>>,
                        when: contracts::read_value::<bounds::Condition>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Frame);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        parent: Slot,
        writes: Slot,
    }
    #[repr(C)]
    struct Writer {
        parent: WriteCallback,
        writes: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        parent: ReadCallback,
        writes: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_frame"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_frame"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Frame {
        fn empty() -> Self {
            Self {
                parent: <Option<usize> as Native>::empty(),
                writes: <Vec<Write> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        parent: contracts::address(&self.parent).cast_mut(),
                        writes: contracts::address(&self.writes).cast_mut(),
                    },
                    Writer {
                        parent: contracts::write_value::<Option<usize>>,
                        writes: contracts::write_value::<Vec<Write>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        parent: contracts::slot(&mut output.parent),
                        writes: contracts::slot(&mut output.writes),
                    },
                    Reader {
                        parent: contracts::read_value::<Option<usize>>,
                        writes: contracts::read_value::<Vec<Write>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Graph);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        nodes: Slot,
        variables: Slot,
        frames: Slot,
    }
    #[repr(C)]
    struct Writer {
        nodes: WriteCallback,
        variables: WriteCallback,
        frames: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        nodes: ReadCallback,
        variables: ReadCallback,
        frames: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_graph"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_graph"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Graph {
        fn empty() -> Self {
            Self {
                nodes: <Vec<Node> as Native>::empty(),
                variables: <BTreeSet<usize> as Native>::empty(),
                frames: <Vec<Frame> as Native>::empty(),
                inputs: BTreeMap::new(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        nodes: contracts::address(&self.nodes).cast_mut(),
                        variables: contracts::address(&self.variables).cast_mut(),
                        frames: contracts::address(&self.frames).cast_mut(),
                    },
                    Writer {
                        nodes: contracts::write_value::<Vec<Node>>,
                        variables: contracts::write_value::<BTreeSet<usize>>,
                        frames: contracts::write_value::<Vec<Frame>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        nodes: contracts::slot(&mut output.nodes),
                        variables: contracts::slot(&mut output.variables),
                        frames: contracts::slot(&mut output.frames),
                    },
                    Reader {
                        nodes: contracts::read_value::<Vec<Node>>,
                        variables: contracts::read_value::<BTreeSet<usize>>,
                        frames: contracts::read_value::<Vec<Frame>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            if output.nodes.is_empty()||output.variables.iter().any(|id|*id>=output.nodes.len())||output.nodes.iter().any(|n|n.edges.iter().any(|e|e.from>=output.nodes.len()||matches!(&e.op,Op::Read{frame:Some(f),..}|Op::Initial{frame:Some(f),..}if *f>=output.frames.len())))||output.frames.iter().enumerate().any(|(i,f)|f.parent.is_some_and(|p|p>=i)||f.writes.iter().any(|w|w.target>=output.nodes.len()||w.fields.iter().flatten().any(|(_,v)|*v>=output.nodes.len()))){return unsafe{contracts::fail(context,"invalid bound read relation")};}
            if output.evaluation_plan(0..output.nodes.len(), false).completed
                != output.nodes.len() - output.variables.len()
            {
                return unsafe {
                    contracts::fail(context, "cyclic read expression without an input binding")
                };
            }
            ReadStatus::Success
        }
    }
};
