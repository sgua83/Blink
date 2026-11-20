//! B produces comparison cases from original values and borrowed proof domains.
//! Native types and containers own identities, facts and final clauses.
use super::*;
use std::{collections::btree_set, ffi::c_void, mem::MaybeUninit, ptr};
type Slot = *mut c_void;
type Borrowed = *const c_void;
unsafe fn frame<'a, T>(slot: Slot) -> &'a mut T {
    unsafe { &mut *slot.cast() }
}
fn operation(op: Comparison) -> u32 {
    match op {
        Comparison::Equal => 10,
        Comparison::NotEqual => 11,
        Comparison::Less => 12,
        Comparison::Greater => 13,
        Comparison::LessEqual => 14,
        Comparison::GreaterEqual => 15,
    }
}
#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct Bounds {
    lower: u64,
    upper: u64,
}
impl From<(Bits, Bits)> for Bounds {
    fn from((lower, upper): (Bits, Bits)) -> Self {
        Self { lower, upper }
    }
}
#[derive(Clone, Copy)]
#[repr(u32)]
enum Output {
    Minimum,
    Maximum,
    NonZero,
    Equal,
    NotEqual,
}
struct State<'a> {
    values: [&'a Value; 2],
    ids: [ValueIdentity; 2],
    scalar: Scalar,
    empty: &'a Domains,
    input: btree_set::Iter<'a, IndexDomain>,
    facts: [Facts; 2],
    cases: [MaybeUninit<Clause>; 2],
    ordered: [MaybeUninit<Clause>; 2],
}
impl<'a> State<'a> {
    fn new(values: [&'a Value; 2], empty: &'a Domains) -> Self {
        Self {
            values,
            ids: values.map(Value::identity),
            scalar: values[0].ty.components[0].0,
            empty,
            input: empty.iter(),
            facts: std::array::from_fn(|_| Facts::new()),
            cases: std::array::from_fn(|_| MaybeUninit::uninit()),
            ordered: std::array::from_fn(|_| MaybeUninit::uninit()),
        }
    }
    fn domains(&self, side: u32) -> &'a Domains {
        self.values[side as usize]
            .proof
            .domains
            .get(0)
            .unwrap_or(self.empty)
    }
    unsafe fn finish(self, invert: u32) -> Condition {
        let mut cases = self.cases.map(|case| unsafe { case.assume_init() });
        if invert != 0 {
            cases.swap(0, 1);
        }
        Condition { cases }
    }
}
unsafe extern "C" fn emit(s: Slot, truth: u32, kind: Output, side: u32, bits: u64) {
    let c = unsafe { frame::<State<'_>>(s) };
    let id = c.ids[side as usize];
    let fact = match kind {
        Output::Minimum => Fact::Minimum(id, c.scalar, bits),
        Output::Maximum => Fact::Maximum(id, c.scalar, bits),
        Output::NonZero => Fact::NonZero(id),
        Output::Equal => Fact::equal(c.ids[0], c.ids[1]),
        Output::NotEqual => Fact::NotEqual(c.ids[0].min(c.ids[1]), c.ids[0].max(c.ids[1])),
    };
    c.facts[truth as usize].insert(fact);
}
unsafe extern "C" fn select(s: Slot, side: u32) {
    let c = unsafe { frame::<State<'_>>(s) };
    c.input = c.domains(side).iter();
}
unsafe extern "C" fn next(s: Slot) -> fact_fields::ConditionDomain {
    unsafe { frame::<State<'_>>(s) }.input.next().map(fact_fields::ConditionDomain::from).unwrap_or_default()
}
unsafe extern "C" fn domain(s: Slot, truth: u32, side: u32, value: Borrowed) {
    let value = unsafe { *value.cast::<IndexDomain>() };
    let c = unsafe { frame::<State<'_>>(s) };
    c.facts[truth as usize].insert(Fact::Index(c.ids[side as usize], value));
}
unsafe extern "C" fn finish(s: Slot, truth: u32, possible: u32) {
    let c = unsafe { frame::<State<'_>>(s) };
    c.cases[truth as usize].write(if possible != 0 {
        Clause::facts(std::mem::take(&mut c.facts[truth as usize]))
    } else {
        Clause::impossible()
    });
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CaseHost {
    emit: unsafe extern "C" fn(Slot, u32, Output, u32, u64),
    select: unsafe extern "C" fn(Slot, u32),
    next: unsafe extern "C" fn(Slot) -> fact_fields::ConditionDomain,
    domain: unsafe extern "C" fn(Slot, u32, u32, Borrowed),
    finish: unsafe extern "C" fn(Slot, u32, u32),
}
const CASE_HOST: CaseHost = CaseHost {
    emit,
    select,
    next,
    domain,
    finish,
};
#[derive(Clone, Copy)]
#[repr(u32)]
enum ConstraintOutput {
    Below,
    Upper,
    Minimum,
    Maximum,
}
struct Constraint<'a> {
    facts: &'a mut Facts,
    ids: [ValueIdentity; 2],
    scalar: Scalar,
    input: btree_set::Iter<'a, IndexDomain>,
}
unsafe extern "C" fn constraint_emit(s: Slot, kind: ConstraintOutput, value: u64) {
    let c = unsafe { frame::<Constraint<'_>>(s) };
    let [a, b] = c.ids;
    c.facts.insert(match kind {
        ConstraintOutput::Below => Fact::Below(a, b, value != 0, c.scalar),
        ConstraintOutput::Upper => Fact::Index(a, IndexDomain::Upper(b, value as u32)),
        ConstraintOutput::Minimum => Fact::Minimum(b, c.scalar, value),
        ConstraintOutput::Maximum => Fact::Maximum(a, c.scalar, value),
    });
}
unsafe extern "C" fn constraint_next(s: Slot) -> index_domains::Fields {
    unsafe { frame::<Constraint<'_>>(s) }
        .input
        .next()
        .map(index_domains::fields)
        .unwrap_or_default()
}
unsafe extern "C" fn constraint_domain(
    s: Slot,
    kind: index_domains::Output,
    value: index_domains::Fields,
) {
    let value = unsafe { index_domains::materialize(kind, value) };
    let c = unsafe { frame::<Constraint<'_>>(s) };
    c.facts.insert(Fact::Index(c.ids[0], value));
}
#[repr(C)]
struct ConstraintHost {
    emit: unsafe extern "C" fn(Slot, ConstraintOutput, u64),
    next: unsafe extern "C" fn(Slot) -> index_domains::Fields,
    domain: unsafe extern "C" fn(Slot, index_domains::Output, index_domains::Fields),
}
const CONSTRAINT_HOST: ConstraintHost = ConstraintHost {
    emit: constraint_emit,
    next: constraint_next,
    domain: constraint_domain,
};
pub(super) fn constrain_parts(
    facts: &mut Facts,
    ids: [ValueIdentity; 2],
    bounds: [Bounds; 2],
    domains: &Domains,
    strict: bool,
    scalar: Scalar,
) {
    let mut c = Constraint {
        facts,
        ids,
        scalar,
        input: domains.iter(),
    };
    unsafe {
        b_constraint(
            (&mut c as *mut Constraint<'_>).cast(),
            u32::from(strict),
            fact_fields::scalar(ptr::from_ref(&scalar).cast()),
            bounds[0],
            bounds[1],
            CONSTRAINT_HOST,
        )
    };
}
#[repr(C)]
struct CallComparisonRanges {
    left: Borrowed,
    left_count: u64,
    right: Borrowed,
    right_count: u64,
    get: unsafe extern "C" fn(Borrowed, u64) -> Bounds,
}
unsafe extern "C" fn call_range(input: Borrowed, at: u64) -> Bounds {
    let ranges = unsafe { *input.cast::<&[(Bits, Bits)]>() };
    ranges[at as usize].into()
}

pub(super) fn call_below(
    ids: [ValueIdentity; 2], [left, right]: [&Proof; 2], strict: bool, scalar: Scalar,
) -> Clause {
    let same = left.origins[0].iter().any(|id| right.origins[0].contains(id));
    let left_ranges = left.interval_source(0);
    let right_ranges = right.interval_source(0);
    let ranges = CallComparisonRanges {
        left: ptr::from_ref(&left_ranges).cast(),
        left_count: left_ranges.len() as u64,
        right: ptr::from_ref(&right_ranges).cast(),
        right_count: right_ranges.len() as u64,
        get: call_range,
    };
    let mut facts = Facts::new();
    let empty = Domains::new();
    let mut c = Constraint {
        facts: &mut facts,
        ids,
        scalar,
        input: right.domains.get(0).unwrap_or(&empty).iter(),
    };
    let possible = unsafe {
        b_call_below(
            ptr::from_mut(&mut c).cast(),
            u32::from(strict),
            u32::from(same),
            fact_fields::scalar(ptr::from_ref(&scalar).cast()),
            ranges,
            CONSTRAINT_HOST,
        )
    };
    if possible { Clause::facts(facts) } else { Clause::impossible() }
}
struct Integer<'a> {
    ranges: &'a [ScalarRanges; 2],
    bounds: [Bounds; 2],
}
#[repr(C)]
struct IntegerInfo {
    scalar: fact_fields::ScalarView,
    counts: [u64; 2],
    bounds: [Bounds; 2],
    same: u32,
    nonzero: [u32; 2],
}
unsafe extern "C" fn range(i: Borrowed, side: u32, at: u64) -> Bounds {
    unsafe { (&*i.cast::<Integer<'_>>()).ranges[side as usize].0[at as usize].into() }
}
unsafe extern "C" fn constrain_case(s: Slot, i: Borrowed, truth: u32, side: u32, strict: u32) {
    let i = unsafe { &*i.cast::<Integer<'_>>() };
    let c = unsafe { frame::<State<'_>>(s) };
    let a = side as usize;
    let b = 1 - a;
    let domains = c.domains(b as u32);
    constrain_parts(
        &mut c.facts[truth as usize],
        [c.ids[a], c.ids[b]],
        [i.bounds[a], i.bounds[b]],
        domains,
        strict != 0,
        c.scalar,
    );
}
fn info(c: &State<'_>, i: &Integer<'_>) -> IntegerInfo {
    IntegerInfo {
        scalar: unsafe { fact_fields::scalar(ptr::from_ref(&c.scalar).cast()) },
        counts: i.ranges.each_ref().map(|r| r.0.len() as u64),
        bounds: i.bounds,
        same: u32::from(c.values[0].same_component(0, c.values[1], 0)),
        nonzero: c.values.map(|v| u32::from(v.is_nonzero())),
    }
}
unsafe extern "C" fn ordered(s: Slot, i: Borrowed, slot: u32, op: u32, metadata: IntegerInfo) {
    let c = unsafe { frame::<State<'_>>(s) };
    let mut child = State::new(c.values, c.empty);
    unsafe {
        b_integer_case(
            (&mut child as *mut State<'_>).cast(),
            i,
            op,
            1,
            metadata,
            INTEGER_HOST,
        )
    };
    c.ordered[slot as usize].write(unsafe { child.cases[1].assume_init_read() });
}
unsafe extern "C" fn refine(s: Slot) {
    let c = unsafe { frame::<State<'_>>(s) };
    let [a, b] = c
        .ordered
        .each_mut()
        .map(|v| unsafe { v.assume_init_read() });
    let old = unsafe { c.cases[0].assume_init_read() };
    c.cases[0].write(Clause::both(&old, &Clause::either(&a, &b)));
}
#[repr(C)]
#[derive(Clone, Copy)]
struct IntegerHost {
    cases: CaseHost,
    range: unsafe extern "C" fn(Borrowed, u32, u64) -> Bounds,
    constrain: unsafe extern "C" fn(Slot, Borrowed, u32, u32, u32),
    ordered: unsafe extern "C" fn(Slot, Borrowed, u32, u32, IntegerInfo),
    refine: unsafe extern "C" fn(Slot),
}
const INTEGER_HOST: IntegerHost = IntegerHost {
    cases: CASE_HOST,
    range,
    constrain: constrain_case,
    ordered,
    refine,
};
#[derive(Clone, Copy)]
#[repr(u32)]
enum PredicateOperation {
    Equal = 10,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
}
unsafe extern "C" fn predicate(s: Slot, truth: u32, op: PredicateOperation) {
    let c = unsafe { frame::<State<'_>>(s) };
    let op = match op {
        PredicateOperation::Equal => Comparison::Equal,
        PredicateOperation::NotEqual => Comparison::NotEqual,
        PredicateOperation::Less => Comparison::Less,
        PredicateOperation::Greater => Comparison::Greater,
        PredicateOperation::LessEqual => Comparison::LessEqual,
        PredicateOperation::GreaterEqual => Comparison::GreaterEqual,
    };
    c.facts[truth as usize].insert(Fact::Predicate(
        ScalarPredicate::new(
            op,
            c.scalar,
            Operand::of(c.values[0], 0),
            Operand::of(c.values[1], 0),
        ),
        truth != 0,
    ));
}
#[repr(C)]
struct FloatHost {
    cases: CaseHost,
    predicate: unsafe extern "C" fn(Slot, u32, PredicateOperation),
}
struct Boolean {
    inputs: [Condition; 2],
    pairs: [MaybeUninit<Clause>; 2],
    cases: [MaybeUninit<Clause>; 2],
}
unsafe extern "C" fn boolean_pair(s: Slot, slot: u32, a: u32, b: u32) {
    let c = unsafe { frame::<Boolean>(s) };
    c.pairs[slot as usize].write(Clause::both(
        &c.inputs[0].cases[a as usize],
        &c.inputs[1].cases[b as usize],
    ));
}
unsafe extern "C" fn boolean_case(s: Slot, truth: u32) {
    let c = unsafe { frame::<Boolean>(s) };
    let [a, b] = c.pairs.each_mut().map(|v| unsafe { v.assume_init_read() });
    c.cases[truth as usize].write(Clause::either(&a, &b));
}
#[repr(C)]
struct BooleanHost {
    pair: unsafe extern "C" fn(Slot, u32, u32, u32),
    finish: unsafe extern "C" fn(Slot, u32),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::comparison_constraint"]
    fn b_constraint(
        s: Slot,
        strict: u32,
        info: fact_fields::ScalarView,
        left: Bounds,
        right: Bounds,
        host: ConstraintHost,
    );
    #[link_name = "bootstrap.condition_graph::comparison_call_below"]
    fn b_call_below(
        context: Slot, strict: u32, same: u32, info: fact_fields::ScalarView,
        ranges: CallComparisonRanges, host: ConstraintHost,
    ) -> bool;
    #[link_name = "bootstrap.condition_graph::comparison_integer_case"]
    fn b_integer_case(
        s: Slot,
        i: Borrowed,
        op: u32,
        truth: u32,
        info: IntegerInfo,
        host: IntegerHost,
    );
    #[link_name = "bootstrap.condition_graph::comparison_integer"]
    fn b_integer(s: Slot, i: Borrowed, op: u32, info: IntegerInfo, host: IntegerHost) -> u32;
    #[link_name = "bootstrap.condition_graph::comparison_float"]
    fn b_float(s: Slot, op: u32, finite: Bounds, host: FloatHost) -> u32;
    #[link_name = "bootstrap.condition_graph::comparison_boolean"]
    fn b_boolean(s: Slot, op: u32, host: BooleanHost) -> u32;
}
pub(super) fn compare(op: Comparison, a: &Value, b: &Value) -> Condition {
    let op = operation(op);
    if a.ty == Ty::scalar(Scalar::Bool) && b.ty == a.ty {
        let mut c = Boolean {
            inputs: [Condition::of(a), Condition::of(b)],
            pairs: std::array::from_fn(|_| MaybeUninit::uninit()),
            cases: std::array::from_fn(|_| MaybeUninit::uninit()),
        };
        let invert = unsafe {
            b_boolean(
                (&mut c as *mut Boolean).cast(),
                op,
                BooleanHost {
                    pair: boolean_pair,
                    finish: boolean_case,
                },
            )
        };
        let mut result = Condition {
            cases: c.cases.map(|c| unsafe { c.assume_init() }),
        };
        if invert != 0 {
            result = result.negate();
        }
        return result;
    }
    let scalar = a.ty.components[0].0;
    if a.ty.components.len() != 1 || b.ty != a.ty {
        return Condition::default();
    }
    let empty = Domains::new();
    let mut c = State::new([a, b], &empty);
    if scalar.is_float() {
        let infinity = if scalar == Scalar::F32 {
            f32::INFINITY.to_bits() as Bits
        } else {
            f64::INFINITY.to_bits()
        };
        let finite = (
            floating::order(scalar, infinity | (1 << (scalar.bit_width() - 1))),
            floating::order(scalar, infinity),
        )
            .into();
        let invert = unsafe {
            b_float(
                (&mut c as *mut State<'_>).cast(),
                op,
                finite,
                FloatHost {
                    cases: CASE_HOST,
                    predicate,
                },
            )
        };
        return unsafe { c.finish(invert) };
    }
    if !scalar.is_integer() {
        return Condition::default();
    }
    let ranges = [a.component_ranges(0), b.component_ranges(0)];
    let i = Integer {
        ranges: &ranges,
        bounds: ranges.each_ref().map(|r| r.hull().into()),
    };
    let metadata = info(&c, &i);
    let invert = unsafe {
        b_integer(
            (&mut c as *mut State<'_>).cast(),
            ptr::from_ref(&i).cast(),
            op,
            metadata,
            INTEGER_HOST,
        )
    };
    unsafe { c.finish(invert) }
}
