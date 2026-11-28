//! B chooses the complete Clause composition. Rust owns the original typed
//! output and necessary intermediate slots; borrowed inputs remain pinned.
use super::{Clause, Condition, SharedCondition, Domains, Fact, Facts, IndexDomain, Proof, Scalar, Ty, ValueIdentity, condition_graph, contradictory, fact_fields};
use std::{ffi::c_void, mem::MaybeUninit, ptr};
type Slot = *mut c_void;
type Borrowed = *const c_void;
fn borrowed<T>(value: &T) -> Borrowed {
    ptr::from_ref(value).cast()
}
unsafe extern "C" fn inspect(source: Borrowed) -> condition_graph::SourceNode {
    condition_graph::source_node(unsafe { &*source.cast::<Clause>() })
}
unsafe extern "C" fn same(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe { (*left.cast::<Clause>()).same_node(&*right.cast::<Clause>()) })
}
unsafe extern "C" fn same_condition(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe {
        (&*left.cast::<SharedCondition>()).same_node(&*right.cast::<SharedCondition>())
    })
}
unsafe extern "C" fn subset(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe { (&*left.cast::<Facts>()).is_subset(&*right.cast::<Facts>()) })
}
unsafe extern "C" fn copy(output: Slot, source: Borrowed) {
    unsafe {
        output
            .cast::<Clause>()
            .write((&*source.cast::<Clause>()).clone())
    };
}
unsafe extern "C" fn impossible(output: Slot) {
    unsafe { output.cast::<Clause>().write(Clause::impossible()) };
}
unsafe extern "C" fn make_both(output: Slot, left: Borrowed, right: Borrowed) {
    let value = Clause::both_node(
        unsafe { (&*left.cast::<Clause>()).clone() },
        unsafe { (&*right.cast::<Clause>()).clone() },
    );
    unsafe { output.cast::<Clause>().write(value) };
}
unsafe extern "C" fn make_either(output: Slot, left: Borrowed, right: Borrowed) {
    let value = Clause::either_node(
        unsafe { (&*left.cast::<Clause>()).clone() },
        unsafe { (&*right.cast::<Clause>()).clone() },
    );
    unsafe { output.cast::<Clause>().write(value) };
}
unsafe extern "C" fn conflict(source: Borrowed) -> u32 {
    u32::from(contradictory(unsafe { &*source.cast::<Facts>() }))
}
unsafe extern "C" fn make_facts(output: Slot, source: Slot) {
    let value = Clause::raw_facts(std::mem::take(unsafe {
        &mut *source.cast::<Facts>()
    }));
    unsafe { output.cast::<Clause>().write(value) };
}
#[repr(C)]
struct LeafHost {
    conflict: unsafe extern "C" fn(Borrowed) -> u32,
    impossible: unsafe extern "C" fn(Slot),
    make: unsafe extern "C" fn(Slot, Slot),
}
fn leaf_host() -> LeafHost {
    LeafHost {
        conflict,
        impossible,
        make: make_facts,
    }
}
unsafe extern "C" fn unite(output: Slot, left: Borrowed, right: Borrowed) {
    let (left, right) = unsafe { (&*left.cast::<Facts>(), &*right.cast::<Facts>()) };
    let mut facts = left.union(right).into_facts();
    unsafe { b_facts((&mut facts as *mut Facts).cast(), output, leaf_host()) };
}
#[repr(C)]
struct BothHost {
    inspect: unsafe extern "C" fn(Borrowed) -> condition_graph::SourceNode,
    same: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    unite: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
    impossible: unsafe extern "C" fn(Slot),
    copy: unsafe extern "C" fn(Slot, Borrowed),
    make: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
}
fn both_host() -> BothHost {
    BothHost {
        inspect,
        same,
        unite,
        impossible,
        copy,
        make: make_both,
    }
}
unsafe extern "C" fn factor(output: Slot, shared: Borrowed, left: Borrowed, right: Borrowed) {
    // This is the actual temporary Clause required by B's selected factoring.
    // Its two recursive operations both enter the B implementation; no logical
    // decision or second graph is kept in the native stack frame.
    let nested = either(unsafe { &*left.cast::<Clause>() }, unsafe {
        &*right.cast::<Clause>()
    });
    unsafe { b_both(shared, borrowed(&nested), output, both_host()) };
}
#[repr(C)]
struct EitherHost {
    inspect: unsafe extern "C" fn(Borrowed) -> condition_graph::SourceNode,
    same: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    same_condition: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    subset: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    copy: unsafe extern "C" fn(Slot, Borrowed),
    make: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
    factor: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed),
}
fn either_host() -> EitherHost {
    EitherHost {
        inspect,
        same,
        same_condition,
        subset,
        copy,
        make: make_either,
        factor,
    }
}
unsafe extern "C" fn case(condition: Borrowed, truth: u32) -> Borrowed {
    borrowed(&unsafe { &*condition.cast::<SharedCondition>() }.cases[truth as usize])
}
unsafe extern "C" fn make_branch(output: Slot, parent: Borrowed, condition: Borrowed, truth: u32) {
    let value = Clause::branch_node(
        unsafe { (&*parent.cast::<Clause>()).clone() },
        unsafe { (&*condition.cast::<SharedCondition>()).clone() },
        truth != 0,
    );
    unsafe { output.cast::<Clause>().write(value) };
}
#[repr(C)]
struct BranchHost {
    inspect: unsafe extern "C" fn(Borrowed) -> condition_graph::SourceNode,
    case: unsafe extern "C" fn(Borrowed, u32) -> Borrowed,
    impossible: unsafe extern "C" fn(Slot),
    make: unsafe extern "C" fn(Slot, Borrowed, Borrowed, u32),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::clause_facts"]
    fn b_facts(facts: Slot, output: Slot, host: LeafHost);
    #[link_name = "bootstrap.condition_graph::clause_both"]
    fn b_both(left: Borrowed, right: Borrowed, output: Slot, host: BothHost);
    #[link_name = "bootstrap.condition_graph::clause_either"]
    fn b_either(left: Borrowed, right: Borrowed, output: Slot, host: EitherHost);
    #[link_name = "bootstrap.condition_graph::clause_branch"]
    fn b_branch(parent: Borrowed, condition: Borrowed, truth: u32, output: Slot, host: BranchHost);
}
pub(super) fn facts(mut value: Facts) -> Clause {
    let mut output = MaybeUninit::<Clause>::uninit();
    unsafe {
        b_facts(
            (&mut value as *mut Facts).cast(),
            output.as_mut_ptr().cast(),
            leaf_host(),
        );
        output.assume_init()
    }
}
pub(super) fn both(left: &Clause, right: &Clause) -> Clause {
    let mut output = MaybeUninit::<Clause>::uninit();
    unsafe {
        b_both(
            borrowed(left),
            borrowed(right),
            output.as_mut_ptr().cast(),
            both_host(),
        );
        output.assume_init()
    }
}
pub(super) fn either(left: &Clause, right: &Clause) -> Clause {
    let mut output = MaybeUninit::<Clause>::uninit();
    unsafe {
        b_either(
            borrowed(left),
            borrowed(right),
            output.as_mut_ptr().cast(),
            either_host(),
        );
        output.assume_init()
    }
}
pub(super) fn branch(parent: &Clause, condition: &SharedCondition, truth: bool) -> Clause {
    let mut output = MaybeUninit::<Clause>::uninit();
    unsafe {
        b_branch(
            borrowed(parent),
            borrowed(condition),
            u32::from(truth),
            output.as_mut_ptr().cast(),
            BranchHost {
                inspect,
                case,
                impossible,
                make: make_branch,
            },
        );
        output.assume_init()
    }
}

#[repr(C)]
struct PathComponent {
    existing: u32,
    boolean: u32,
    integer: u32,
}

#[repr(C)]
struct PathHost {
    component: unsafe extern "C" fn(Borrowed, usize) -> PathComponent,
    identity: unsafe extern "C" fn(Borrowed, usize) -> Borrowed,
    merge: unsafe extern "C" fn(Slot, usize),
    share: unsafe extern "C" fn(Slot, usize),
    boolean: unsafe extern "C" fn(Slot, usize, Borrowed),
    integer: unsafe extern "C" fn(Slot, usize, Borrowed),
}

struct PathBinding<'a> {
    scalars: &'a [(Scalar, u32)],
    origins: &'a [super::OriginList],
    conditions: &'a mut [Option<Box<Condition>>],
    path: &'a Clause,
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::bind_proof_path"]
    fn b_bind_proof_path(context: Slot, count: usize, host: PathHost);
}

unsafe extern "C" fn path_component(context: Borrowed, index: usize) -> PathComponent {
    let binding = unsafe { &*context.cast::<PathBinding<'_>>() };
    let scalar = binding.scalars[index].0;
    PathComponent {
        existing: u32::from(binding.conditions[index].is_some()),
        boolean: u32::from(scalar == Scalar::Bool),
        integer: u32::from(scalar.is_integer()),
    }
}

unsafe extern "C" fn path_identity(context: Borrowed, index: usize) -> Borrowed {
    let binding = unsafe { &*context.cast::<PathBinding<'_>>() };
    binding.origins.get(index).and_then(|origins| origins.first()).map_or(ptr::null(), borrowed)
}

unsafe extern "C" fn merge_path(context: Slot, index: usize) {
    let binding = unsafe { &mut *context.cast::<PathBinding<'_>>() };
    let condition = binding.conditions[index].as_mut().unwrap();
    for case in &mut condition.cases {
        *case = Clause::both(case, binding.path);
    }
}

unsafe extern "C" fn share_path(context: Slot, index: usize) {
    let binding = unsafe { &mut *context.cast::<PathBinding<'_>>() };
    let path = binding.path.clone();
    binding.conditions[index] = Some(Box::new(Condition::paths(path.clone(), path)));
}

fn bind_path_facts(binding: &mut PathBinding<'_>, index: usize, facts: [Fact; 2]) {
    let cases = facts.map(|fact| Clause::both(&Clause::facts([fact].into()), binding.path));
    binding.conditions[index] = Some(Box::new(Condition { cases }));
}

unsafe extern "C" fn bind_path_boolean(context: Slot, index: usize, origin: Borrowed) {
    let binding = unsafe { &mut *context.cast::<PathBinding<'_>>() };
    let origin = unsafe { *origin.cast::<ValueIdentity>() };
    bind_path_facts(binding, index, [Fact::Boolean(origin, false), Fact::Boolean(origin, true)]);
}

unsafe extern "C" fn bind_path_integer(context: Slot, index: usize, origin: Borrowed) {
    let binding = unsafe { &mut *context.cast::<PathBinding<'_>>() };
    let origin = unsafe { *origin.cast::<ValueIdentity>() };
    bind_path_facts(binding, index, [Fact::Zero(origin), Fact::NonZero(origin)]);
}

pub(super) fn bind_proof_path(proof: &mut Proof, ty: &Ty, path: &Clause) {
    bind_path_conditions(&proof.origins, &mut proof.conditions, ty, path);
}

fn bind_path_conditions(
    origins: &[super::OriginList], conditions: &mut Vec<Option<Box<Condition>>>,
    ty: &Ty, path: &Clause,
) {
    conditions.resize_with(conditions.len().max(ty.components.len()), || None);
    let mut binding = PathBinding { scalars: &ty.components, origins, conditions, path };
    unsafe {
        b_bind_proof_path(
            ptr::from_mut(&mut binding).cast(), ty.components.len(),
            PathHost { component: path_component, identity: path_identity, merge: merge_path,
                share: share_path, boolean: bind_path_boolean, integer: bind_path_integer },
        );
    }
}


#[repr(C)]
struct ConditionJoinHost {
    present: unsafe extern "C" fn(Borrowed, usize) -> u32,
    empty: unsafe extern "C" fn(Slot),
    common: unsafe extern "C" fn(Slot, usize),
    selected: unsafe extern "C" fn(Slot, usize),
}

struct ConditionOperand<'a> {
    proof: &'a Proof,
    bound: Option<Vec<Option<Box<Condition>>>>,
}

impl<'a> ConditionOperand<'a> {
    fn new(proof: &'a Proof, model: Option<&Ty>) -> Self {
        let bound = model.and_then(|ty| {
            let facts = proof.facts(ty);
            if facts.is_empty() {
                return None;
            }
            let mut conditions = proof.conditions.clone();
            bind_path_conditions(&proof.origins, &mut conditions, ty, &Clause::facts(facts));
            Some(conditions)
        });
        Self { proof, bound }
    }

    fn has_condition(&self, component: usize) -> bool {
        self.bound.as_deref().unwrap_or(&self.proof.conditions)
            .get(component).is_some_and(Option::is_some)
    }

    fn take_condition(&mut self, component: usize) -> Condition {
        self.bound.as_mut()
            .and_then(|conditions| conditions.get_mut(component))
            .and_then(Option::take)
            .map_or_else(|| self.proof.condition_for_join(component), |condition| *condition)
    }
}

struct ConditionJoin<'a> {
    operands: [ConditionOperand<'a>; 2],
    branch: Option<&'a Condition>,
    shared: Option<SharedCondition>,
    result: Vec<Option<Box<Condition>>>,
}


#[derive(Clone, Copy)]
#[repr(C)]
struct UpperRow {
    identity: Borrowed,
    distance: u32,
}

#[repr(C)]
struct DomainJoinHost {
    next: unsafe extern "C" fn(Slot, u32) -> UpperRow,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    origin: unsafe extern "C" fn(Borrowed, u32, Borrowed) -> u32,
    upper: unsafe extern "C" fn(Slot, Borrowed, u32),
    bits: unsafe extern "C" fn(Slot) -> u32,
}

struct DomainJoin<'a> {
    proofs: [&'a Proof; 2],
    iterators: [std::collections::btree_set::Iter<'a, IndexDomain>; 2],
    component: usize,
    scalar: Option<Scalar>,
    result: Domains,
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::join_proof_conditions"]
    fn b_join_proof_conditions(context: Slot, width: usize, selected: u32, host: ConditionJoinHost);
    #[link_name = "bootstrap.condition_graph::join_proof_domains"]
    fn b_join_proof_domains(context: Slot, bits: u32, host: DomainJoinHost) -> u32;
}

unsafe extern "C" fn condition_at_join(context: Borrowed, component: usize) -> u32 {
    let state = unsafe { &*context.cast::<ConditionJoin<'_>>() };
    u32::from(state.operands.iter().any(|operand| operand.has_condition(component)))
}

unsafe extern "C" fn empty_joined_condition(context: Slot) {
    let state = unsafe { &mut *context.cast::<ConditionJoin<'_>>() };
    state.result.push(None);
}

unsafe extern "C" fn common_joined_condition(context: Slot, component: usize) {
    let state = unsafe { &mut *context.cast::<ConditionJoin<'_>>() };
    let [left, right] = &mut state.operands;
    let left = left.take_condition(component);
    let right = right.take_condition(component);
    state.result.push(Some(Box::new(Condition::common(left, right))));
}

unsafe extern "C" fn selected_joined_condition(context: Slot, component: usize) {
    let state = unsafe { &mut *context.cast::<ConditionJoin<'_>>() };
    let [left, right] = &mut state.operands;
    let left = left.take_condition(component);
    let right = right.take_condition(component);
    let branch = state.branch.unwrap();
    let shared = state.shared.get_or_insert_with(|| branch.clone().shared());
    state.result.push(Some(Box::new(Condition::branch_on(shared, left, right))));
}

pub(super) fn join_conditions(
    left: &Proof, right: &Proof, width: usize, branch: Option<&Condition>,
    model: Option<&Ty>,
) -> Vec<Option<Box<Condition>>> {
    let mut state = ConditionJoin {
        operands: [ConditionOperand::new(left, model), ConditionOperand::new(right, model)],
        branch, shared: None, result: Vec::with_capacity(width),
    };
    unsafe {
        b_join_proof_conditions(
            ptr::from_mut(&mut state).cast(), width, u32::from(branch.is_some()),
            ConditionJoinHost { present: condition_at_join, empty: empty_joined_condition,
                common: common_joined_condition, selected: selected_joined_condition },
        );
    }
    state.result
}

unsafe extern "C" fn next_upper_row(context: Slot, side: u32) -> UpperRow {
    let state = unsafe { &mut *context.cast::<DomainJoin<'_>>() };
    state.iterators[side as usize].find_map(|row| match row {
        IndexDomain::Upper(identity, distance) => Some(UpperRow {
            identity: borrowed(identity), distance: *distance,
        }),
        _ => None,
    }).unwrap_or(UpperRow { identity: ptr::null(), distance: 0 })
}

unsafe extern "C" fn upper_origin(context: Borrowed, side: u32, identity: Borrowed) -> u32 {
    let state = unsafe { &*context.cast::<DomainJoin<'_>>() };
    let limit = unsafe { &*identity.cast::<ValueIdentity>() };
    u32::from(state.proofs[side as usize].origins.get(state.component)
        .is_some_and(|origins| origins.contains(limit)))
}

unsafe extern "C" fn insert_joined_upper(context: Slot, identity: Borrowed, distance: u32) {
    let state = unsafe { &mut *context.cast::<DomainJoin<'_>>() };
    state.result.insert(IndexDomain::Upper(unsafe { *identity.cast::<ValueIdentity>() }, distance));
}

unsafe extern "C" fn join_known_bits(context: Slot) -> u32 {
    let state = unsafe { &mut *context.cast::<DomainJoin<'_>>() };
    let scalar = state.scalar.unwrap();
    let [left, right] = state.proofs;
    if let Some(bits) = left.known_bits(state.component, scalar)
        .and_then(|left| left.common(right.known_bits(state.component, scalar)?))
    {
        state.result.insert(bits.domain());
        return u32::from(bits.value != 0);
    }
    0
}

fn join_domains(left: &Proof, right: &Proof, component: usize) -> (Domains, u32) {
    let a = left.domains(component);
    let b = right.domains(component);
    let scalar = a.union(&b).find_map(|domain| match domain {
        IndexDomain::KnownBits(scalar, ..) => Some(*scalar),
        _ => None,
    });
    let mut state = DomainJoin {
        proofs: [left, right], iterators: [a.iter(), b.iter()], component, scalar,
        result: a.intersection(&b).filter(|domain| !matches!(domain, IndexDomain::KnownBits(..))).copied().collect(),
    };
    let nonzero = unsafe {
        b_join_proof_domains(
            ptr::from_mut(&mut state).cast(), u32::from(scalar.is_some()),
            DomainJoinHost { next: next_upper_row, compare: fact_fields::compare, origin: upper_origin,
                upper: insert_joined_upper, bits: join_known_bits },
        )
    };
    (state.result, nonzero)
}

#[repr(C)]
struct NumericJoinFacts {
    left_nonzero: u32,
    right_nonzero: u32,
    bits_nonzero: u32,
}

#[repr(C)]
struct NumericJoinHost {
    domains: unsafe extern "C" fn(Slot, usize) -> NumericJoinFacts,
    store: unsafe extern "C" fn(Slot, usize, u32),
}

struct NumericJoin<'a> {
    left: &'a mut Proof,
    right: &'a Proof,
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::join_proof_numeric"]
    fn b_join_proof_numeric(context: Slot, width: usize, host: NumericJoinHost);
}

unsafe extern "C" fn joined_domains(context: Slot, component: usize) -> NumericJoinFacts {
    let state = unsafe { &mut *context.cast::<NumericJoin<'_>>() };
    let (domains, bits) = join_domains(state.left, state.right, component);
    state.left.domains[component] = domains;
    NumericJoinFacts {
        left_nonzero: u32::from(state.left.nonzero[component]),
        right_nonzero: u32::from(state.right.nonzero.get(component) == Some(&true)),
        bits_nonzero: bits,
    }
}

unsafe extern "C" fn store_joined_numeric(context: Slot, component: usize, nonzero: u32) {
    let state = unsafe { &mut *context.cast::<NumericJoin<'_>>() };
    let other = state.right.origins.get(component).map(super::OriginList::as_slice).unwrap_or_default();
    state.left.origins[component].retain(|identity| other.contains(identity));
    state.left.ranges[component] = state.left.ranges[component].union(&state.right.range_set(component));
    state.left.nonzero[component] = nonzero != 0;
}

pub(super) fn join_numeric(left: &mut Proof, right: &Proof, width: usize) {
    left.origins.resize(width);
    left.domains.resize_with(width, Domains::new);
    left.ranges.resize_with(width, super::ScalarRanges::default);
    left.nonzero.resize(width, false);

    let mut state = NumericJoin { left, right };
    unsafe {
        b_join_proof_numeric(
            ptr::from_mut(&mut state).cast(),
            width,
            NumericJoinHost { domains: joined_domains, store: store_joined_numeric },
        )
    }
}
