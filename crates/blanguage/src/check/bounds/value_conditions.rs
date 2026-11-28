//! B selects live value cases and facts; original Proof and Clause own their data.
use super::*;
use std::{ffi::c_void, mem::MaybeUninit, ptr};
type Slot = *mut c_void;
type Borrowed = *const c_void;
#[repr(C)]
struct Constant {
    present: u32,
    bits: u64,
}
#[repr(C)]
struct Input {
    scalar: fact_fields::ScalarView,
    constant: Constant,
    base_cases: u32,
}
#[repr(C)]
struct Bounds {
    lower: u64,
    upper: u64,
}
#[derive(Clone, Copy)]
#[repr(u32)]
enum Output {
    BooleanFalse,
    BooleanTrue,
    Zero,
    NonZero,
    Minimum,
    Maximum,
}
#[derive(Clone, Copy)]
#[repr(u32)]
enum Finish {
    Impossible,
    Base,
    Facts,
}
struct Context<'a> {
    value: &'a Value,
    component: usize,
    scalar: Scalar,
    identity: ValueIdentity,
    base: Option<&'a Condition>,
    domains: super::domain_store::Iter<'a>,
    facts: [Facts; 2],
    cases: [MaybeUninit<Clause>; 2],
}
unsafe fn frame<'a>(slot: Slot) -> &'a mut Context<'a> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn next(s: Slot) -> fact_fields::ConditionDomain {
    unsafe { frame(s) }
        .domains
        .next()
        .map(fact_fields::ConditionDomain::from)
        .unwrap_or_default()
}
fn insert(c: &mut Context<'_>, mask: u32, fact: Fact) {
    for truth in 0..2 {
        if mask & (1 << truth) != 0 {
            c.facts[truth].insert(fact);
        }
    }
}
unsafe extern "C" fn domain(s: Slot, mask: u32, original: Borrowed) {
    let c = unsafe { frame(s) };
    let fact = Fact::Index(c.identity, unsafe { *original.cast::<IndexDomain>() });
    insert(c, mask, fact);
}
unsafe extern "C" fn bounds(s: Slot) -> Bounds {
    let c = unsafe { frame(s) };
    let (lower, upper) = c.value.component_range(c.component);
    Bounds { lower, upper }
}
unsafe extern "C" fn emit(s: Slot, mask: u32, output: Output, bits: u64) {
    let c = unsafe { frame(s) };
    let id = c.identity;
    let fact = match output {
        Output::BooleanFalse => Fact::Boolean(id, false),
        Output::BooleanTrue => Fact::Boolean(id, true),
        Output::Zero => Fact::Zero(id),
        Output::NonZero => Fact::NonZero(id),
        Output::Minimum => Fact::Minimum(id, c.scalar, bits),
        Output::Maximum => Fact::Maximum(id, c.scalar, bits),
    };
    insert(c, mask, fact);
}
unsafe extern "C" fn finish(s: Slot, truth: u32, mode: Finish) {
    let c = unsafe { frame(s) };
    let at = truth as usize;
    let clause = match mode {
        Finish::Impossible => Clause::impossible(),
        Finish::Base => c
            .base
            .map(|base| base.cases[at].clone())
            .unwrap_or_default(),
        Finish::Facts => {
            let facts = Clause::facts(std::mem::take(&mut c.facts[at]));
            match c.base {
                Some(base) => Clause::both(&base.cases[at], &facts),
                None => facts,
            }
        }
    };
    c.cases[at].write(clause);
}
#[repr(C)]
struct Host {
    next: unsafe extern "C" fn(Slot) -> fact_fields::ConditionDomain,
    domain: unsafe extern "C" fn(Slot, u32, Borrowed),
    bounds: unsafe extern "C" fn(Slot) -> Bounds,
    emit: unsafe extern "C" fn(Slot, u32, Output, u64),
    finish: unsafe extern "C" fn(Slot, u32, Finish),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::value_condition"]
    fn b_value_condition(context: Slot, input: Input, host: Host);
}
pub(super) fn component(value: &Value, component: usize) -> Condition {
    let scalar = value.ty.components[component].0;
    let base = value.proof.component_condition(component);
    let base_cases = base
        .map(|base| {
            base.cases
                .iter()
                .enumerate()
                .fold(0, |mask, (truth, case)| {
                    mask | if matches!(case.view(), ClauseNode::Impossible) {
                        0
                    } else {
                        1 << truth
                    }
                })
        })
        .unwrap_or(3);
    let constant = value
        .constant
        .as_ref()
        .map(|bits| Constant {
            present: 1,
            bits: bits[component],
        })
        .unwrap_or(Constant {
            present: 0,
            bits: 0,
        });
    let input = Input {
        scalar: unsafe { fact_fields::scalar(ptr::from_ref(&scalar).cast()) },
        constant,
        base_cases,
    };
    let empty = Domains::new();
    let mut context = Context {
        value,
        component,
        scalar,
        identity: value.component_identity(component),
        base,
        domains: value.proof.domains.get(component).unwrap_or(&empty).iter(),
        facts: std::array::from_fn(|_| Facts::new()),
        cases: std::array::from_fn(|_| MaybeUninit::uninit()),
    };
    unsafe {
        b_value_condition(
            (&mut context as *mut Context<'_>).cast(),
            input,
            Host {
                next,
                domain,
                bounds,
                emit,
                finish,
            },
        )
    };
    Condition {
        cases: context.cases.map(|case| unsafe { case.assume_init() }),
    }
}
