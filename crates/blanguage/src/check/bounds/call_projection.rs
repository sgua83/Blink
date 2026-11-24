//! Single-fact control lives in B. This synchronous frame borrows the original
//! fact and input proofs; only transformed keys and the final Clause are owned.
use super::*;
use std::{collections::btree_set, ffi::c_void, ptr};

type Slot = *mut c_void;
type Borrowed = *const c_void;
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Input { original: Borrowed, identity: Borrowed, nonzero: u32 }
#[repr(C)]
struct Range { lower: u64, upper: u64 }
#[repr(C)]
struct Case { original: Borrowed, empty: u32 }
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Common { source: Borrowed, from: Borrowed, low: Borrowed, extended: Borrowed, to: Borrowed }
#[repr(C)]
struct Converted { lower: u64, upper: u64, contains_zero: u32, known: u32, value: u64 }
#[derive(Default)]
#[repr(C)]
struct Origin { original: Borrowed, element: u32 }
struct Frame<'a, P> {
    bindings: &'a BTreeMap<ValueIdentity, P>,
    domains: btree_set::Iter<'a, IndexDomain>,
    domain: Option<IndexDomain>, predicate: Option<ScalarPredicate>,
    facts: Facts, clause: Option<Clause>,
}
unsafe fn frame<'a, 'b, P>(slot: Slot) -> &'a mut Frame<'b, P> { unsafe { &mut *slot.cast() } }
unsafe fn proof<'a>(value: Borrowed) -> &'a Proof { unsafe { &*value.cast() } }
unsafe fn common_scalars(value: Common) -> [Scalar; 4] {
    [value.from, value.low, value.extended, value.to].map(|value| unsafe { *value.cast() })
}
unsafe extern "C" fn input<P: std::borrow::Borrow<Proof>>(slot: Slot, id: Borrowed) -> Input {
    let c = unsafe { frame::<P>(slot) };
    Proof::bound_input(c.bindings, unsafe { &*id.cast() }).map(|value| Input {
        original: ptr::from_ref(value).cast(),
        identity: value.origins.first().and_then(|v| v.first()).map_or(ptr::null(), |v| ptr::from_ref(v).cast()),
        nonzero: u32::from(value.nonzero.first() == Some(&true)),
    }).unwrap_or_default()
}
unsafe extern "C" fn range(value: Borrowed, mask: u64, restricted: u32) -> Range {
    let value = unsafe { proof(value) };
    let (lower, upper) = if restricted != 0 { value.range_set(0).restrict(0, mask).hull() } else { value.range(0) };
    Range { lower, upper }
}
unsafe extern "C" fn boolean(value: Borrowed) -> u32 {
    unsafe { proof(value) }.component_condition(0).and_then(Condition::boolean_constant).map_or(2, u32::from)
}
unsafe extern "C" fn case(value: Borrowed, truth: u32) -> Case {
    unsafe { proof(value) }.component_condition(0).map(|value| {
        let value = &value.cases[truth as usize];
        Case { original: ptr::from_ref(value).cast(), empty: u32::from(matches!(&**value.0, ClauseNode::Facts(facts) if facts.is_empty())) }
    }).unwrap_or(Case { original: ptr::null(), empty: 1 })
}
unsafe extern "C" fn include<P>(slot: Slot, value: Borrowed) {
    let c = unsafe { frame::<P>(slot) }; let value = unsafe { &*value.cast::<Clause>() };
    c.clause = Some(match c.clause.take() { Some(old) => Clause::both(&old, value), None => value.clone() });
}
unsafe extern "C" fn include_cases<P>(slot: Slot, value: Borrowed) {
    if let Some(value) = unsafe { proof(value) }.component_condition(0) {
        let value = Clause::either(&value.cases[0], &value.cases[1]);
        unsafe { include::<P>(slot, ptr::from_ref(&value).cast()) };
    }
}
unsafe extern "C" fn impossible<P>(slot: Slot) { unsafe { frame::<P>(slot) }.clause = Some(Clause::impossible()); }
unsafe extern "C" fn emit<P>(slot: Slot, fields: fact_fields::Fields) { unsafe { frame::<P>(slot) }.facts.insert(unsafe { fact_fields::key(fields) }); }
unsafe extern "C" fn commit<P>(slot: Slot) {
    let value = Clause::facts(std::mem::take(&mut unsafe { frame::<P>(slot) }.facts));
    unsafe { include::<P>(slot, ptr::from_ref(&value).cast()) };
}
unsafe extern "C" fn count<P>(slot: Slot) -> usize { unsafe { frame::<P>(slot) }.facts.len() }
unsafe extern "C" fn below<P>(slot: Slot, a: Input, b: Input, strict: u32, scalar: Borrowed) {
    let value = comparison_conditions::call_below(
        [unsafe { *a.identity.cast() }, unsafe { *b.identity.cast() }],
        [unsafe { proof(a.original) }, unsafe { proof(b.original) }], strict != 0, unsafe { *scalar.cast() },
    );
    unsafe { frame::<P>(slot) }.clause = Some(value);
}
unsafe extern "C" fn mapped_domain<P: std::borrow::Borrow<Proof>>(slot: Slot, value: Borrowed) -> Borrowed {
    let c = unsafe { frame::<P>(slot) };
    c.domain = Proof::call_domain(unsafe { *value.cast() }, c.bindings);
    c.domain.as_ref().map_or(ptr::null(), |v| ptr::from_ref(v).cast())
}
unsafe extern "C" fn common(value: Borrowed) -> Common {
    match unsafe { &*value.cast::<IndexDomain>() } {
        IndexDomain::CommonRange(source, from, low, extended, to) => Common {
            source: ptr::from_ref(source).cast(), from: ptr::from_ref(from).cast(), low: ptr::from_ref(low).cast(),
            extended: ptr::from_ref(extended).cast(), to: ptr::from_ref(to).cast(),
        }, _ => Common::default(),
    }
}
unsafe extern "C" fn converted(value: Borrowed, common: Common) -> Converted {
    let input = unsafe { proof(value) }; let [from, low, extended, to] = unsafe { common_scalars(common) };
    let range = range_sets::cast_chain(input.interval_source(0), [from, low, extended, to], input.nonzero.first() == Some(&true));
    let bits = Proof::call_common_bits(input, from, low, extended, to);
    let (lower, upper) = range.hull();
    Converted { lower, upper, contains_zero: u32::from(range.contains(to.order_bias())), known: u32::from(bits.is_some()), value: bits.map_or(0, |v| v.value) }
}
unsafe extern "C" fn begin<P>(slot: Slot, value: Borrowed) {
    unsafe { frame::<P>(slot) }.domains = unsafe { proof(value) }.domains.first().map(|v| v.iter()).unwrap_or_default();
}
unsafe extern "C" fn next<P>(slot: Slot) -> fact_fields::ConditionDomain {
    unsafe { frame::<P>(slot) }.domains.next().map(fact_fields::ConditionDomain::from).unwrap_or_default()
}
unsafe extern "C" fn cast_domain<P>(slot: Slot, value: Borrowed, common: Common) -> Borrowed {
    let c = unsafe { frame::<P>(slot) }; c.domain = super::cast_domain(unsafe { *value.cast() }, unsafe { common_scalars(common) });
    c.domain.as_ref().map_or(ptr::null(), |v| ptr::from_ref(v).cast())
}
unsafe extern "C" fn origin(value: Borrowed, index: usize) -> Origin {
    unsafe { proof(value) }.origins.first().and_then(|v| v.get(index)).map(|value| Origin {
        original: ptr::from_ref(value).cast(),
        element: u32::from(matches!(value, ValueIdentity::ElementComponent(_) | ValueIdentity::ElementParent(..) | ValueIdentity::ElementIndex(_))),
    }).unwrap_or_default()
}
unsafe extern "C" fn emit_common<P>(slot: Slot, target: Borrowed, source: Borrowed, common: Common) {
    let [from, low, extended, to] = unsafe { common_scalars(common) };
    unsafe { frame::<P>(slot) }.facts.insert(Fact::Index(unsafe { *target.cast() }, IndexDomain::CommonRange(unsafe { *source.cast() }, from, low, extended, to)));
}
unsafe extern "C" fn predicate<P: std::borrow::Borrow<Proof>>(slot: Slot, value: Borrowed) -> Borrowed {
    let c = unsafe { frame::<P>(slot) }; c.predicate = unsafe { *value.cast::<ScalarPredicate>() }.at_call(c.bindings);
    c.predicate.as_ref().map_or(ptr::null(), |v| ptr::from_ref(v).cast())
}
unsafe extern "C" fn comparison<P: std::borrow::Borrow<Proof>>(slot: Slot, scalar: Borrowed, a: Borrowed, b: Borrowed, strict: u32) -> Borrowed {
    let c = unsafe { frame::<P>(slot) };
    let op = if strict != 0 { Comparison::Less } else { Comparison::LessEqual };
    c.predicate = ScalarPredicate::new(op, unsafe { *scalar.cast() }, Operand::Value(unsafe { *a.cast() }), Operand::Value(unsafe { *b.cast() })).at_call(c.bindings);
    c.predicate.as_ref().map_or(ptr::null(), |v| ptr::from_ref(v).cast())
}
#[repr(C)]
struct Host {
    input: unsafe extern "C" fn(Slot, Borrowed) -> Input,
    range: unsafe extern "C" fn(Borrowed, u64, u32) -> Range,
    boolean: unsafe extern "C" fn(Borrowed) -> u32,
    select_case: unsafe extern "C" fn(Borrowed, u32) -> Case,
    include: unsafe extern "C" fn(Slot, Borrowed),
    include_cases: unsafe extern "C" fn(Slot, Borrowed),
    impossible: unsafe extern "C" fn(Slot),
    emit: unsafe extern "C" fn(Slot, fact_fields::Fields),
    commit: unsafe extern "C" fn(Slot), count: unsafe extern "C" fn(Slot) -> usize,
    below: unsafe extern "C" fn(Slot, Input, Input, u32, Borrowed),
    mapped_domain: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    common: unsafe extern "C" fn(Borrowed) -> Common,
    converted: unsafe extern "C" fn(Borrowed, Common) -> Converted,
    begin: unsafe extern "C" fn(Slot, Borrowed),
    next: unsafe extern "C" fn(Slot) -> fact_fields::ConditionDomain,
    cast_domain: unsafe extern "C" fn(Slot, Borrowed, Common) -> Borrowed,
    origin: unsafe extern "C" fn(Borrowed, usize) -> Origin,
    emit_common: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Common),
    predicate: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    comparison: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed, u32) -> Borrowed,
    inspect_predicate: unsafe extern "C" fn(Borrowed) -> fact_fields::PredicateView,
    scalar: unsafe extern "C" fn(Borrowed) -> fact_fields::ScalarView,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::call_fact"]
    fn project(context: Slot, fact: fact_fields::Fields, host: Host);
}
impl Proof {
    pub(super) fn call_fact<P: std::borrow::Borrow<Proof>>(value: Fact, bindings: &BTreeMap<ValueIdentity, P>) -> Option<Clause> {
        let mut c = Frame { bindings, domains: btree_set::Iter::default(), domain: None, predicate: None, facts: Facts::new(), clause: None };
        unsafe { project(ptr::from_mut(&mut c).cast(), fact_fields::Fields::from(&value), Host {
            input: input::<P>, range, boolean, select_case: case, include: include::<P>, include_cases: include_cases::<P>,
            impossible: impossible::<P>, emit: emit::<P>, commit: commit::<P>, count: count::<P>, below: below::<P>,
            mapped_domain: mapped_domain::<P>, common, converted, begin: begin::<P>, next: next::<P>,
            cast_domain: cast_domain::<P>, origin, emit_common: emit_common::<P>, predicate: predicate::<P>, comparison: comparison::<P>,
            inspect_predicate: fact_fields::predicate, scalar: fact_fields::scalar, compare: fact_fields::compare,
        }) };
        c.clause
    }
}
