//! The B Clause owner composes the original immutable nodes.
use super::{Clause, Condition, ConditionSlots, SharedCondition, StoredClause, Domains, Fact, Facts, Proof, Scalar, Ty, ValueIdentity, contradictory};
use std::{ffi::c_void, mem::ManuallyDrop, ptr::{self, NonNull}};
type Slot = *mut c_void;
type Borrowed = *const c_void;

fn borrowed<T>(value: &T) -> Borrowed {
    ptr::from_ref(value).cast()
}

pub(in crate::check) unsafe extern "C" fn conflict(source: Borrowed) -> u32 {
    u32::from(contradictory(unsafe { &*source.cast::<Facts>() }))
}

type Conflict = unsafe extern "C" fn(Borrowed) -> u32;

unsafe extern "C" {
    #[link_name = "bootstrap.clause_storage::checked_clause_facts"]
    fn checked_facts(value: *mut Facts, conflict: Conflict) -> NonNull<StoredClause>;
    #[link_name = "bootstrap.clause_storage::combine_clause_both"]
    fn combine_both(left: Borrowed, right: Borrowed, conflict: Conflict) -> NonNull<StoredClause>;
    #[link_name = "bootstrap.clause_storage::combine_clause_either"]
    fn combine_either(left: Borrowed, right: Borrowed, conflict: Conflict) -> NonNull<StoredClause>;
    #[link_name = "bootstrap.clause_storage::combine_clause_branch"]
    fn combine_branch(parent: Borrowed, condition: Borrowed, truth: u32) -> NonNull<StoredClause>;
}

pub(super) fn facts(value: Facts) -> Clause {
    let mut owner = ManuallyDrop::new(value);
    Clause(unsafe { checked_facts(&mut *owner, conflict) })
}

pub(super) fn both(left: &Clause, right: &Clause) -> Clause {
    Clause(unsafe { combine_both(left.raw(), right.raw(), conflict) })
}

pub(super) fn either(left: &Clause, right: &Clause) -> Clause {
    Clause(unsafe { combine_either(left.raw(), right.raw(), conflict) })
}

pub(super) fn branch(parent: &Clause, condition: &SharedCondition, truth: bool) -> Clause {
    Clause(unsafe { combine_branch(parent.raw(), condition.raw(), u32::from(truth)) })
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
    conditions: &'a mut ConditionSlots,
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
    binding.conditions.constrain(index, binding.path);
}

unsafe extern "C" fn share_path(context: Slot, index: usize) {
    let binding = unsafe { &mut *context.cast::<PathBinding<'_>>() };
    let path = binding.path.clone();
    binding.conditions.set(index, Some(Condition::paths(path.clone(), path).shared()));
}

fn bind_path_facts(binding: &mut PathBinding<'_>, index: usize, facts: [Fact; 2]) {
    let cases = facts.map(|fact| Clause::both(&Clause::facts([fact].into()), binding.path));
    binding.conditions.set(index, Some(Condition { cases }.shared()));
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
    origins: &[super::OriginList], conditions: &mut ConditionSlots,
    ty: &Ty, path: &Clause,
) {
    conditions.resize(conditions.len().max(ty.components.len()));
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
    bound: Option<ConditionSlots>,
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
        self.bound.as_ref().unwrap_or(&self.proof.conditions)
            .get(component).is_some_and(Option::is_some)
    }

    fn take_condition(&mut self, component: usize) -> Condition {
        self.bound.as_mut()
            .and_then(|conditions| conditions.take(component))
            .map_or_else(|| self.proof.condition_for_join(component), |condition| (*condition).clone())
    }
}

struct ConditionJoin<'a> {
    operands: [ConditionOperand<'a>; 2],
    branch: Option<&'a Condition>,
    shared: Option<SharedCondition>,
    result: ConditionSlots,
}


unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::join_proof_conditions"]
    fn b_join_proof_conditions(context: Slot, width: usize, selected: u32, host: ConditionJoinHost);
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
    state.result.push(Some(Condition::common(left, right).shared()));
}

unsafe extern "C" fn selected_joined_condition(context: Slot, component: usize) {
    let state = unsafe { &mut *context.cast::<ConditionJoin<'_>>() };
    let [left, right] = &mut state.operands;
    let left = left.take_condition(component);
    let right = right.take_condition(component);
    let branch = state.branch.unwrap();
    let shared = state.shared.get_or_insert_with(|| branch.clone().shared());
    state.result.push(Some(Condition::branch_on(shared, left, right).shared()));
}

pub(super) fn join_conditions(
    left: &Proof, right: &Proof, width: usize, branch: Option<&Condition>,
    model: Option<&Ty>,
) -> ConditionSlots {
    let mut state = ConditionJoin {
        operands: [ConditionOperand::new(left, model), ConditionOperand::new(right, model)],
        branch, shared: None, result: ConditionSlots::new(),
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

#[repr(C)]
struct ProofDomainOperand<'a> {
    domains: &'a Domains,
    origins: *const ValueIdentity,
    origin_count: usize,
    ranges: *const super::BitRange,
    range_count: usize,
}

impl<'a> ProofDomainOperand<'a> {
    fn new(proof: &'a Proof, domains: &'a Domains, component: usize) -> Self {
        let origins = proof.origins.get(component).map(super::OriginList::as_slice).unwrap_or_default();
        let ranges = proof.interval_source(component);
        Self {
            domains,
            origins: origins.as_ptr(),
            origin_count: origins.len(),
            ranges: ranges.as_ptr(),
            range_count: ranges.len(),
        }
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.proof_domain_join::join_proof_domain_sets"]
    fn join_proof_domain_sets(output: &mut Domains, left: ProofDomainOperand<'_>, right: ProofDomainOperand<'_>) -> u32;
}

fn join_domains(left: &Proof, right: &Proof, component: usize) -> (Domains, u32) {
    let a = left.domains(component);
    let b = right.domains(component);
    let mut result = Domains::new();
    let nonzero = unsafe {
        join_proof_domain_sets(&mut result, ProofDomainOperand::new(left, &a, component),
                              ProofDomainOperand::new(right, &b, component))
    };
    (result, nonzero)
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
