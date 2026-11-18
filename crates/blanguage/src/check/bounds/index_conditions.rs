//! B produces index relations. Original operands, Proof and Clause keep ownership.
use super::*;
use std::{ffi::c_void, mem::MaybeUninit, ptr};
type Slot = *mut c_void;
type Borrowed = *const c_void;
type Bounds = comparison_conditions::Bounds;
#[derive(Clone, Copy)]
#[repr(C)]
struct Fields {
    kind: u32,
    identity: Borrowed,
    bits: u64,
}
fn project(value: &Option<Operand>) -> Fields {
    match value {
        Some(Operand::Value(id)) => Fields {
            kind: 1,
            identity: ptr::from_ref(id).cast(),
            bits: 0,
        },
        Some(Operand::Constant(_, bits)) => Fields {
            kind: 2,
            identity: ptr::null(),
            bits: *bits,
        },
        None => Fields {
            kind: 0,
            identity: ptr::null(),
            bits: 0,
        },
    }
}
#[repr(C)]
struct Input {
    operands: [Fields; 2],
    nonzero: [u32; 2],
}
#[derive(Clone, Copy)]
#[repr(u32)]
enum Output {
    Minimum,
    Maximum,
    Equal,
    NotEqual,
}
#[derive(Clone, Copy)]
#[repr(u32)]
enum Finish {
    Impossible,
    Empty,
    Facts,
}
struct Cases {
    facts: [Facts; 2],
    clauses: [MaybeUninit<Clause>; 2],
}
impl Cases {
    fn new() -> Self {
        Self {
            facts: std::array::from_fn(|_| Facts::new()),
            clauses: std::array::from_fn(|_| MaybeUninit::uninit()),
        }
    }
    unsafe fn finish(self) -> Condition {
        Condition {
            cases: self.clauses.map(|c| unsafe { c.assume_init() }),
        }
    }
}
unsafe fn cases<'a>(slot: Slot) -> &'a mut Cases {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn emit(
    s: Slot,
    truth: u32,
    kind: Output,
    first: Borrowed,
    second: Borrowed,
    bits: u64,
) {
    let first = unsafe { *first.cast::<ValueIdentity>() };
    let fact = match kind {
        Output::Minimum => Fact::Minimum(first, Scalar::U32, bits),
        Output::Maximum => Fact::Maximum(first, Scalar::U32, bits),
        Output::Equal => Fact::equal(first, unsafe { *second.cast::<ValueIdentity>() }),
        Output::NotEqual => {
            let second = unsafe { *second.cast::<ValueIdentity>() };
            Fact::NotEqual(first.min(second), first.max(second))
        }
    };
    unsafe { cases(s) }.facts[truth as usize].insert(fact);
}
unsafe extern "C" fn finish(s: Slot, truth: u32, mode: Finish) {
    let c = unsafe { cases(s) };
    let at = truth as usize;
    c.clauses[at].write(match mode {
        Finish::Impossible => Clause::impossible(),
        Finish::Empty => Clause::default(),
        Finish::Facts => Clause::facts(std::mem::take(&mut c.facts[at])),
    });
}
unsafe extern "C" fn predicate(s: Slot, a: Borrowed, b: Borrowed) {
    let a = unsafe { *a.cast::<ValueIdentity>() };
    let b = unsafe { *b.cast::<ValueIdentity>() };
    let predicate = ScalarPredicate::new(
        Comparison::Equal,
        Scalar::U32,
        Operand::Value(a),
        Operand::Value(b),
    );
    let c = unsafe { cases(s) };
    c.facts[0].insert(Fact::Predicate(predicate, false));
    c.facts[1].insert(Fact::Predicate(predicate, true));
}
unsafe extern "C" fn guard(s: Slot) {
    let c = unsafe { cases(s) };
    let path = Clause::facts(std::mem::take(&mut c.facts[1]));
    c.clauses[0].write(path.negate());
    c.clauses[1].write(path);
}
#[derive(Clone, Copy)]
#[repr(C)]
struct CaseHost {
    emit: unsafe extern "C" fn(Slot, u32, Output, Borrowed, Borrowed, u64),
    finish: unsafe extern "C" fn(Slot, u32, Finish),
}
const CASE_HOST: CaseHost = CaseHost { emit, finish };
#[repr(C)]
struct EqualHost {
    cases: CaseHost,
    predicate: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
    guard: unsafe extern "C" fn(Slot),
}
struct Proofs<'a> {
    items: [&'a Proof; 2],
    empty: &'a Domains,
}
unsafe extern "C" fn bounds(input: Borrowed, side: u32) -> Bounds {
    let p = unsafe { &*input.cast::<Proofs<'_>>() };
    p.items[side as usize]
        .range_set(0)
        .restrict(0, u32::MAX as Bits)
        .hull()
        .into()
}
unsafe extern "C" fn constrain(
    s: Slot,
    input: Borrowed,
    truth: u32,
    limit_side: u32,
    index: Borrowed,
    limit: Borrowed,
    index_bounds: Bounds,
    limit_bounds: Bounds,
    strict: u32,
) {
    let p = unsafe { &*input.cast::<Proofs<'_>>() };
    let ids = unsafe {
        [
            *index.cast::<ValueIdentity>(),
            *limit.cast::<ValueIdentity>(),
        ]
    };
    comparison_conditions::constrain_parts(
        &mut unsafe { cases(s) }.facts[truth as usize],
        ids,
        [index_bounds, limit_bounds],
        p.items[limit_side as usize]
            .domains
            .get(0)
            .unwrap_or(p.empty),
        strict != 0,
        Scalar::U32,
    );
}
#[repr(C)]
struct BelowHost {
    cases: CaseHost,
    bounds: unsafe extern "C" fn(Borrowed, u32) -> Bounds,
    constrain:
        unsafe extern "C" fn(Slot, Borrowed, u32, u32, Borrowed, Borrowed, Bounds, Bounds, u32),
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::index_condition_equal"]
    fn b_equal(s: Slot, a: Fields, b: Fields, host: EqualHost);
    #[link_name = "bootstrap.condition_graph::index_condition_below"]
    fn b_below(s: Slot, input: Borrowed, operands: Input, strict: u32, host: BelowHost);
}
pub(super) fn equal(a: Operand, b: Operand) -> Condition {
    let operands = [Some(a), Some(b)];
    let fields = operands.each_ref().map(project);
    let mut c = Cases::new();
    unsafe {
        b_equal(
            (&mut c as *mut Cases).cast(),
            fields[0],
            fields[1],
            EqualHost {
                cases: CASE_HOST,
                predicate,
                guard,
            },
        )
    };
    unsafe { c.finish() }
}
pub(super) fn below(
    a: Option<Operand>,
    ap: &Proof,
    b: Option<Operand>,
    bp: &Proof,
    strict: bool,
) -> Condition {
    let operands = [a, b];
    let input = Input {
        operands: operands.each_ref().map(project),
        nonzero: [ap, bp].map(|p| u32::from(p.nonzero.get(0) == Some(&true))),
    };
    let empty = Domains::new();
    let proofs = Proofs {
        items: [ap, bp],
        empty: &empty,
    };
    let mut c = Cases::new();
    unsafe {
        b_below(
            (&mut c as *mut Cases).cast(),
            ptr::from_ref(&proofs).cast(),
            input,
            u32::from(strict),
            BelowHost {
                cases: CASE_HOST,
                bounds,
                constrain,
                compare: fact_fields::compare,
            },
        )
    };
    unsafe { c.finish() }
}
