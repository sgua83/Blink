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
#[derive(Default)]
struct OrderedTree {
    records: *mut c_void,
    root: usize,
    count: usize,
    capacity: usize,
}

#[repr(C)]
#[derive(Default)]
struct IdentityTable {
    entries: *mut c_void,
    count: usize,
    capacity: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(in super::super) struct ConditionNodes {
    fields: *const c_void,
    count: usize,
}

#[repr(C)]
#[derive(Default)]
pub(in super::super) struct CanonicalGraph<'a> {
    nodes: OrderedTree,
    identities: IdentityTable,
    roots: std::marker::PhantomData<&'a Clause>,
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::condition_node_ids"]
    fn canonical_ids(stack: &mut TraversalStack, graph: Slot, keep_pairs: u32);
    #[link_name = "bootstrap.condition_index::condition_node_index"]
    fn canonical_index(graph: *const c_void, key: usize) -> usize;
    #[link_name = "bootstrap.condition_index::condition_node_at"]
    fn canonical_node(graph: *const c_void, index: usize) -> NodeFields;
    #[link_name = "bootstrap.condition_index::condition_seal"]
    fn canonical_seal(graph: Slot) -> ConditionNodes;
    #[link_name = "bootstrap.condition_index::condition_graph_drop"]
    fn canonical_drop(graph: Slot);
}

impl<'a> CanonicalGraph<'a> {
    pub(in super::super) fn index(&self, key: usize) -> usize {
        unsafe { canonical_index(ptr::from_ref(self).cast(), key) }
    }

    pub(in super::super) fn len(&self) -> usize {
        self.nodes.count
    }

    pub(in super::super) fn seal(&mut self) -> ConditionNodes {
        unsafe { canonical_seal(slot(self)) }
    }

    pub(in super::super) fn nodes(&self) -> impl ExactSizeIterator<Item = EncodedClause<&'a Facts>> + '_ {
        (0..self.len()).map(|index| {
            let value = unsafe { canonical_node(ptr::from_ref(self).cast(), index) };
            match value.kind {
                NodeKind::Impossible => EncodedClause::Impossible,
                NodeKind::Facts => EncodedClause::Facts(unsafe { &*value.facts.cast::<Facts>() }),
                NodeKind::Both => EncodedClause::Both(value.first, value.second),
                NodeKind::Either => EncodedClause::Either(value.first, value.second),
            }
        })
    }
}

impl Drop for CanonicalGraph<'_> {
    fn drop(&mut self) {
        unsafe { canonical_drop(slot(self)) };
    }
}

pub(in super::super) fn node_ids<'a>(
    roots: impl IntoIterator<Item = &'a Clause>,
    keep_pairs: bool,
) -> CanonicalGraph<'a> {
    let mut pending = ClausePostorder::new(roots);
    let mut graph = CanonicalGraph::default();
    unsafe { canonical_ids(&mut pending.stack, slot(&mut graph), u32::from(keep_pairs)) };
    graph
}

pub(in crate::check::bounds) enum Conditions<'a>{Components(&'a [Option<SharedCondition>]),One(&'a Condition)}
impl<'a> Conditions<'a>{fn len(&self)->usize{match self{Self::Components(v)=>v.len(),Self::One(_)=>1}}fn at(&self,index:usize)->Option<&'a Condition>{match self{Self::Components(v)=>v[index].as_deref(),Self::One(v)=>Some(*v)}}}
#[repr(C)]
struct WriteHost {
    append: unsafe extern "C" fn(Slot, *const u8, usize),
    condition: unsafe extern "C" fn(*const c_void, usize) -> RootFields,
}

impl<'a> Conditions<'a> {
    fn graph(&self) -> CanonicalGraph<'a> {
        let roots = (0..self.len()).filter_map(|index| self.at(index))
            .flat_map(|condition| condition.cases.iter());
        node_ids(roots, false)
    }
}

unsafe extern "C" fn condition_at(context: *const c_void, index: usize) -> RootFields {
    unsafe { &*context.cast::<Conditions<'_>>() }.at(index)
        .map_or_else(RootFields::default, |condition| RootFields {
            present: 1,
            first: condition.cases[0].key(),
            second: condition.cases[1].key(),
        })
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
        graph: *const c_void,
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
        left_graph: *const c_void,
        right_graph: *const c_void,
        root: unsafe extern "C" fn(*const c_void, usize) -> RootFields,
        facts: unsafe extern "C" fn(*const c_void, *const c_void) -> i32,
    ) -> i32;
}

pub(in crate::check::bounds) fn order(left: &Condition, right: &Condition) -> std::cmp::Ordering {
    let left = Conditions::One(left);
    let right = Conditions::One(right);
    let left_graph = left.graph();
    let right_graph = right.graph();
    unsafe {
        compare(ptr::from_ref(&left).cast(), ptr::from_ref(&right).cast(),
            ptr::from_ref(&left_graph).cast(), ptr::from_ref(&right_graph).cast(), condition_at, compare_facts)
    }.cmp(&0)
}
pub(in crate::check::bounds) fn write_borrowed(conditions:Conditions<'_>,writer:&mut Writer){
    let graph = conditions.graph();
    unsafe {
        write_conditions(
            ptr::from_ref(&conditions).cast(),
            conditions.len(),
            ptr::from_ref(&graph).cast(),
            slot(writer),
            WriteHost { append, condition: condition_at },
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
pub(in crate::check::bounds) unsafe fn read_borrowed(context:crate::codec::contracts::Slot)->Result<ConditionSlots>{
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
            return Ok(ConditionSlots::new());
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
                    Clause::both_node(node(&nodes, a)?, node(&nodes, b)?)
                }
                EncodedClause::Either(a, b) => {
                    Clause::either_node(node(&nodes, a)?, node(&nodes, b)?)
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
                        Ok(Condition {
                            cases: [node(&nodes, a)?, node(&nodes, b)?],
                        }.shared())
                    })
                    .transpose()
            })
            .collect()
    }
}
