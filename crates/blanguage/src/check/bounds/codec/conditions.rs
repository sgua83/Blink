//! Synchronous projections of the original condition graph and containers.
//! B owns canonical node IDs and the existing conditions wire grammar.
use super::*;
use crate::codec::{Result, Writer};
use std::{ffi::c_void, ptr};

#[repr(u32)]
#[derive(Clone, Copy, Default)]
enum NodeKind {
    #[default]
    Impossible = 0,
    Facts = 1,
    Both = 2,
    Either = 3,
}
#[repr(u32)]
#[derive(Clone, Copy, Default)]
enum ClauseKind {
    #[default]
    Impossible = 0,
    Facts = 1,
    Both = 2,
    Either = 3,
    Branch = 4,
}
#[repr(C)]
#[derive(Default)]
struct ClauseFields {
    present: u32,
    kind: ClauseKind,
    source: *const c_void,
    facts: *const c_void,
    first: usize,
    second: usize,
}
#[repr(C)]
#[derive(Default)]
struct NodeFields {
    kind: NodeKind,
    facts: *const c_void,
    first: usize,
    second: usize,
}
#[repr(C)]
#[derive(Default)]
struct RootFields {
    present: u32,
    first: usize,
    second: usize,
}
#[repr(C)]
struct Inserted {
    index: usize,
    inserted: u32,
}
#[repr(C)]
struct CanonicalHost {
    next: unsafe extern "C" fn(Slot) -> ClauseFields,
    index: unsafe extern "C" fn(Slot, usize) -> usize,
    len: unsafe extern "C" fn(Slot) -> usize,
    intern: unsafe extern "C" fn(Slot, NodeKind, *const c_void, usize, usize, usize) -> Inserted,
    record: unsafe extern "C" fn(Slot, *const c_void, NodeKind, *const c_void, usize, usize),
    bind: unsafe extern "C" fn(Slot, *const c_void, usize),
}
struct Canonical<'a, F> {
    pending: ClausePostorder<'a>,
    unique: BTreeMap<EncodedClause<&'a Facts>, usize>,
    indices: HashMap<usize, usize>,
    record: F,
}
unsafe fn borrowed_node<'a>(
    kind: NodeKind,
    facts: *const c_void,
    a: usize,
    b: usize,
) -> EncodedClause<&'a Facts> {
    match kind {
        NodeKind::Impossible => EncodedClause::Impossible,
        NodeKind::Facts => EncodedClause::Facts(unsafe { &*facts.cast::<Facts>() }),
        NodeKind::Both => EncodedClause::Both(a, b),
        NodeKind::Either => EncodedClause::Either(a, b),
    }
}
unsafe extern "C" fn next<'a, F>(context: Slot) -> ClauseFields {
    let state = unsafe { &mut *context.cast::<Canonical<'a, F>>() };
    let indices = &state.indices;
    let Some(clause) = state.pending.next(|key| indices.contains_key(&key)) else {
        return ClauseFields::default();
    };
    let mut result = ClauseFields {
        present: 1,
        source: ptr::from_ref(clause).cast(),
        ..Default::default()
    };
    match &**clause.0 {
        ClauseNode::Impossible => result.kind = ClauseKind::Impossible,
        ClauseNode::Facts(facts) => {
            result.kind = ClauseKind::Facts;
            result.facts = ptr::from_ref(facts).cast();
        }
        ClauseNode::Both(a, b) => {
            result.kind = ClauseKind::Both;
            result.first = a.key();
            result.second = b.key();
        }
        ClauseNode::Either(a, b) => {
            result.kind = ClauseKind::Either;
            result.first = a.key();
            result.second = b.key();
        }
        ClauseNode::Branch(parent, condition, truth) => {
            result.kind = ClauseKind::Branch;
            result.first = parent.key();
            result.second = condition.cases[usize::from(*truth)].key();
        }
    }
    result
}
unsafe extern "C" fn index<'a, F>(context: Slot, key: usize) -> usize {
    unsafe { &*context.cast::<Canonical<'a, F>>() }.indices[&key]
}
unsafe extern "C" fn len<'a, F>(context: Slot) -> usize {
    unsafe { &*context.cast::<Canonical<'a, F>>() }.unique.len()
}
unsafe extern "C" fn intern<'a, F>(
    context: Slot,
    kind: NodeKind,
    facts: *const c_void,
    a: usize,
    b: usize,
    next: usize,
) -> Inserted {
    use std::collections::btree_map::Entry;
    let state = unsafe { &mut *context.cast::<Canonical<'a, F>>() };
    match state
        .unique
        .entry(unsafe { borrowed_node(kind, facts, a, b) })
    {
        Entry::Occupied(entry) => Inserted {
            index: *entry.get(),
            inserted: 0,
        },
        Entry::Vacant(entry) => {
            entry.insert(next);
            Inserted {
                index: next,
                inserted: 1,
            }
        }
    }
}
unsafe extern "C" fn record<'a, F: FnMut(&'a Clause, EncodedClause<&'a Facts>)>(
    context: Slot,
    source: *const c_void,
    kind: NodeKind,
    facts: *const c_void,
    a: usize,
    b: usize,
) {
    let state = unsafe { &mut *context.cast::<Canonical<'a, F>>() };
    (state.record)(unsafe { &*source.cast::<Clause>() }, unsafe { borrowed_node(kind, facts, a, b) });
}
unsafe extern "C" fn bind<'a, F>(context: Slot, source: *const c_void, index: usize) {
    let key = unsafe { &*source.cast::<Clause>() }.key();
    unsafe { &mut *context.cast::<Canonical<'a, F>>() }.indices.insert(key, index);
}
unsafe extern "C" {
    #[link_name = "bootstrap.contract_codec::condition_node_ids"]
    fn canonical_ids(context: Slot, host: CanonicalHost, keep_pairs: u32);
}
pub(in super::super) fn node_ids<'a, F: FnMut(&'a Clause, EncodedClause<&'a Facts>)>(
    roots: impl IntoIterator<Item = &'a Clause>,
    keep_pairs: bool,
    record: F,
) -> HashMap<usize, usize> {
    let mut state = Canonical {
        pending: ClausePostorder::new(roots),
        unique: BTreeMap::new(),
        indices: HashMap::new(),
        record,
    };
    unsafe {
        canonical_ids(
            slot(&mut state),
            CanonicalHost {
                next: next::<F>,
                index: index::<F>,
                len: len::<F>,
                intern: intern::<F>,
                record: self::record::<F>,
                bind: bind::<F>,
            },
            u32::from(keep_pairs),
        )
    };
    state.indices
}

pub(in crate::check::bounds) enum Conditions<'a>{Components(&'a [Option<Box<Condition>>]),One(&'a Condition)}
impl<'a> Conditions<'a>{fn len(&self)->usize{match self{Self::Components(v)=>v.len(),Self::One(_)=>1}}fn at(&self,index:usize)->Option<&'a Condition>{match self{Self::Components(v)=>v[index].as_deref(),Self::One(v)=>Some(*v)}}}
struct WriteContext<'a> {
    conditions: Conditions<'a>,
    nodes: Vec<EncodedClause<&'a Facts>>,
    indices: HashMap<usize, usize>,
}
impl<'a> WriteContext<'a> {
    fn new(conditions: Conditions<'a>) -> Self {
        let roots = (0..conditions.len())
            .filter_map(|i| conditions.at(i))
            .flat_map(|condition| condition.cases.iter());
        let mut nodes = Vec::new();
        let indices = node_ids(roots, false, |_, node| nodes.push(node));
        Self { conditions, nodes, indices }
    }
}
#[repr(C)]
struct WriteHost {
    append: unsafe extern "C" fn(Slot, *const u8, usize),
    condition: unsafe extern "C" fn(*const c_void, usize) -> RootFields,
    node: unsafe extern "C" fn(*const c_void, usize) -> NodeFields,
    index: unsafe extern "C" fn(*const c_void, usize) -> usize,
}
unsafe extern "C" fn condition_at(context: *const c_void, index: usize) -> RootFields {
    unsafe { &*context.cast::<WriteContext<'_>>() }.conditions.at(index)
        .map_or_else(RootFields::default, |condition| RootFields {
            present: 1,
            first: condition.cases[0].key(),
            second: condition.cases[1].key(),
        })
}
unsafe extern "C" fn node_at(context: *const c_void, index: usize) -> NodeFields {
    match unsafe { &*context.cast::<WriteContext<'_>>() }.nodes[index] {
        EncodedClause::Impossible => NodeFields::default(),
        EncodedClause::Facts(facts) => NodeFields {
            kind: NodeKind::Facts,
            facts: ptr::from_ref(facts).cast(),
            ..Default::default()
        },
        EncodedClause::Both(a, b) => NodeFields {
            kind: NodeKind::Both,
            first: a,
            second: b,
            ..Default::default()
        },
        EncodedClause::Either(a, b) => NodeFields {
            kind: NodeKind::Either,
            first: a,
            second: b,
            ..Default::default()
        },
    }
}
unsafe extern "C" fn root_index(context: *const c_void, key: usize) -> usize {
    unsafe { &*context.cast::<WriteContext<'_>>() }.indices[&key]
}
unsafe extern "C" fn compare_facts(left: *const c_void, right: *const c_void) -> i32 {
    let a = unsafe { &*left.cast::<Facts>() };
    let b = unsafe { &*right.cast::<Facts>() };
    if a == b {
        return 0;
    }
    let mut a = Writer(Vec::new());
    let mut b = Writer(Vec::new());
    unsafe {
        write_fact_sequence(left, slot(&mut a), append);
        write_fact_sequence(right, slot(&mut b), append);
    }
    a.0.cmp(&b.0) as i32
}
unsafe extern "C" {
    #[link_name = "bootstrap.contract_codec::write_proof_conditions"]
    fn write_conditions(
        context: *const c_void,
        width: usize,
        count: usize,
        output: Slot,
        host: WriteHost,
    );
    #[link_name = "bootstrap.contract_codec::write_fact_sequence"]
    fn write_fact_sequence(
        facts: *const c_void,
        output: Slot,
        append: Append,
    );
    #[link_name = "bootstrap.contract_codec::compare_proof_condition"]
    fn compare(
        left: *const c_void,
        right: *const c_void,
        left_count: usize,
        right_count: usize,
        node: unsafe extern "C" fn(*const c_void, usize) -> NodeFields,
        root: unsafe extern "C" fn(*const c_void, usize) -> RootFields,
        index: unsafe extern "C" fn(*const c_void, usize) -> usize,
        facts: unsafe extern "C" fn(*const c_void, *const c_void) -> i32,
    ) -> i32;
}
pub(in crate::check::bounds) fn order(left: &Condition, right: &Condition) -> std::cmp::Ordering {
    let left = WriteContext::new(Conditions::One(left));
    let right = WriteContext::new(Conditions::One(right));
    unsafe {
        compare(ptr::from_ref(&left).cast(), ptr::from_ref(&right).cast(),
            left.nodes.len(), right.nodes.len(), node_at, condition_at, root_index, compare_facts)
    }.cmp(&0)
}
pub(in crate::check::bounds) fn write_borrowed(conditions:Conditions<'_>,writer:&mut Writer){
    let state = WriteContext::new(conditions);
    unsafe {
        write_conditions(
            ptr::from_ref(&state).cast(),
            state.conditions.len(),
            state.nodes.len(),
            slot(writer),
            WriteHost {
                append,
                condition: condition_at,
                node: node_at,
                index: root_index,
            },
        )
    };
}

#[repr(C)]
struct ReadHost {
    impossible: unsafe extern "C" fn(Slot),
    facts: unsafe extern "C" fn(Slot, Slot),
    both: unsafe extern "C" fn(Slot, usize, usize),
    either: unsafe extern "C" fn(Slot, usize, usize),
}
unsafe extern "C" fn push_impossible(output: Slot) {
    unsafe { &mut *output.cast::<Vec<EncodedClause<Facts>>>() }.push(EncodedClause::Impossible);
}
unsafe extern "C" fn push_both(output: Slot, a: usize, b: usize) {
    unsafe { &mut *output.cast::<Vec<EncodedClause<Facts>>>() }.push(EncodedClause::Both(a, b));
}
unsafe extern "C" fn push_either(output: Slot, a: usize, b: usize) {
    unsafe { &mut *output.cast::<Vec<EncodedClause<Facts>>>() }.push(EncodedClause::Either(a, b));
}
unsafe extern "C" fn take_facts(output: Slot, source: Slot) {
    let facts = unsafe { source.cast::<Facts>().read() };
    unsafe { &mut *output.cast::<Vec<EncodedClause<Facts>>>() }.push(EncodedClause::Facts(facts));
}
unsafe extern "C" fn push_root(output: Slot, present: u32, a: usize, b: usize) {
    unsafe { &mut *output.cast::<Vec<Option<[usize; 2]>>>() }
        .push((present != 0).then_some([a, b]));
}
unsafe extern "C" {
    #[link_name = "bootstrap.contract_codec::read_condition_nodes"]
    fn read_nodes(
        input: Bytes,
        position: *mut usize,
        present: *mut u32,
        output: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_condition_roots"]
    fn read_roots(
        input: Bytes,
        position: *mut usize,
        output: Slot,
        push: unsafe extern "C" fn(Slot, u32, usize, usize),
    ) -> ReadStatus;
}
fn checked(status: ReadStatus) -> Result<()> {
    match status.message() {
        Some(message) => Err(message.into()),
        None => Ok(()),
    }
}
pub(in crate::check::bounds) unsafe fn read_borrowed(context:crate::codec::contracts::Slot)->Result<Vec<Option<Box<Condition>>>>{
    let (bytes, position) = unsafe { contracts::input(context) };
    {
        let mut present = 0;
        let mut encoded = Vec::<EncodedClause<Facts>>::new();
        checked(unsafe {
            read_nodes(
                bytes,
                position,
                &mut present,
                slot(&mut encoded),
                ReadHost {
                    impossible: push_impossible,
                    facts: take_facts,
                    both: push_both,
                    either: push_either,
                },
            )
        })?;
        if present == 0 {
            return Ok(Vec::new());
        }
        fn node(nodes: &[Clause], index: usize) -> Result<Clause> {
            nodes
                .get(index)
                .cloned()
                .ok_or_else(|| "模块条件节点引用无效".into())
        }
        // The original semantic owner validates references after the complete
        // node vector has been read, and before the root vector is read.
        let mut nodes = Vec::new();
        for encoded in encoded {
            nodes.push(match encoded {
                EncodedClause::Impossible => Clause::impossible(),
                EncodedClause::Facts(facts) => Clause::facts(facts),
                EncodedClause::Both(a, b) => {
                    Clause::node(ClauseNode::Both(node(&nodes, a)?, node(&nodes, b)?))
                }
                EncodedClause::Either(a, b) => {
                    Clause::node(ClauseNode::Either(node(&nodes, a)?, node(&nodes, b)?))
                }
            });
        }
        let mut roots = Vec::<Option<[usize; 2]>>::new();
        checked(unsafe { read_roots(bytes, position, slot(&mut roots), push_root) })?;
        roots
            .into_iter()
            .map(|roots| {
                roots
                    .map(|[a, b]| {
                        Ok(Box::new(Condition {
                            cases: [node(&nodes, a)?, node(&nodes, b)?],
                        }))
                    })
                    .transpose()
            })
            .collect()
    }
}
