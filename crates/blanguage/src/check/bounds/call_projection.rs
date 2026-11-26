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
    bindings: &'a P,
    domains: btree_set::Iter<'a, IndexDomain>,
    domain: Option<IndexDomain>, predicate: Option<ScalarPredicate>,
    facts: Facts, clause: Option<Clause>,
}
unsafe fn frame<'a, 'b, P>(slot: Slot) -> &'a mut Frame<'b, P> { unsafe { &mut *slot.cast() } }
unsafe fn proof<'a>(value: Borrowed) -> &'a Proof { unsafe { &*value.cast() } }
unsafe fn common_scalars(value: Common) -> [Scalar; 4] {
    [value.from, value.low, value.extended, value.to].map(|value| unsafe { *value.cast() })
}
fn describe(value: CallInput<'_>, id: &ValueIdentity) -> Input {
    match value {
        CallInput::Proof(proof) => Input {
            original: ptr::from_ref(proof).cast(),
            identity: proof.origins.first().and_then(|v| v.first()).map_or(ptr::null(), |v| ptr::from_ref(v).cast()),
            nonzero: u32::from(value.nonzero()),
        },
        CallInput::Original(_) => Input { identity: ptr::from_ref(id).cast(), ..Input::default() },
    }
}

unsafe fn optional_proof<'a>(value: Borrowed) -> Option<&'a Proof> {
    unsafe { value.cast::<Proof>().as_ref() }
}

unsafe fn comparison_input<'a>(input: Input) -> CallInput<'a> {
    match unsafe { optional_proof(input.original) } {
        Some(proof) => CallInput::Proof(proof),
        None => CallInput::Original(unsafe { *input.identity.cast::<ValueIdentity>() }),
    }
}

unsafe extern "C" fn input<P: CallBindings>(slot: Slot, id: Borrowed) -> Input {
    let c = unsafe { frame::<P>(slot) };
    let id = unsafe { &*id.cast::<ValueIdentity>() };
    c.bindings.input(*id).map_or_else(Input::default, |input| describe(input, id))
}
unsafe extern "C" fn range(value: Borrowed, mask: u64, restricted: u32) -> Range {
    let values = unsafe { optional_proof(value) }.map_or(FULL_INTERVALS, |proof| proof.interval_source(0));
    let (lower, upper) = if restricted != 0 {
        range_sets::restrict(values, 0, mask).hull()
    } else {
        range_sets::hull(values)
    };
    Range { lower, upper }
}

unsafe extern "C" fn boolean(value: Borrowed) -> u32 {
    unsafe { optional_proof(value) }.and_then(|proof| proof.component_condition(0))
        .and_then(Condition::boolean_constant).map_or(2, u32::from)
}

unsafe extern "C" fn case(value: Borrowed, truth: u32) -> Case {
    unsafe { optional_proof(value) }.and_then(|proof| proof.component_condition(0)).map(|value| {
        let value = &value.cases[truth as usize];
        Case { original: ptr::from_ref(value).cast(), empty: u32::from(matches!(&**value.0, ClauseNode::Facts(facts) if facts.is_empty())) }
    }).unwrap_or(Case { original: ptr::null(), empty: 1 })
}
unsafe extern "C" fn include<P>(slot: Slot, value: Borrowed) {
    let c = unsafe { frame::<P>(slot) }; let value = unsafe { &*value.cast::<Clause>() };
    c.clause = Some(match c.clause.take() { Some(old) => Clause::both(&old, value), None => value.clone() });
}
unsafe extern "C" fn include_cases<P>(slot: Slot, value: Borrowed) {
    if let Some(value) = unsafe { optional_proof(value) }.and_then(|proof| proof.component_condition(0)) {
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
        [unsafe { comparison_input(a) }, unsafe { comparison_input(b) }], strict != 0, unsafe { *scalar.cast() },
    );
    unsafe { frame::<P>(slot) }.clause = Some(value);
}
unsafe extern "C" fn mapped_domain<P: CallBindings>(slot: Slot, value: Borrowed) -> Borrowed {
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
    let input = unsafe { optional_proof(value) };
    let [from, low, extended, to] = unsafe { common_scalars(common) };
    let ranges = input.map_or(FULL_INTERVALS, |proof| proof.interval_source(0));
    let nonzero = input.is_some_and(|proof| proof.nonzero.first() == Some(&true));
    let ((lower, upper), contains_zero) = range_sets::cast_summary(ranges, [from, low, extended, to], nonzero);
    let bits = Proof::call_common_bits(input, from, low, extended, to);
    Converted { lower, upper, contains_zero, known: u32::from(bits.is_some()), value: bits.map_or(0, |v| v.value) }
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
unsafe extern "C" fn origin(value: Input, index: usize) -> Origin {
    let identity = match unsafe { optional_proof(value.original) } {
        Some(proof) => proof.origins.first().and_then(|origins| origins.get(index)),
        None if index == 0 => unsafe { value.identity.cast::<ValueIdentity>().as_ref() },
        None => None,
    };
    identity.map(|value| Origin {
        original: ptr::from_ref(value).cast(),
        element: u32::from(matches!(value, ValueIdentity::ElementComponent(_) | ValueIdentity::ElementParent(..) | ValueIdentity::ElementIndex(_))),
    }).unwrap_or_default()
}
unsafe extern "C" fn emit_common<P>(slot: Slot, target: Borrowed, source: Borrowed, common: Common) {
    let [from, low, extended, to] = unsafe { common_scalars(common) };
    unsafe { frame::<P>(slot) }.facts.insert(Fact::Index(unsafe { *target.cast() }, IndexDomain::CommonRange(unsafe { *source.cast() }, from, low, extended, to)));
}
unsafe extern "C" fn predicate<P: CallBindings>(slot: Slot, value: Borrowed) -> Borrowed {
    let c = unsafe { frame::<P>(slot) }; c.predicate = unsafe { *value.cast::<ScalarPredicate>() }.at_call(c.bindings);
    c.predicate.as_ref().map_or(ptr::null(), |v| ptr::from_ref(v).cast())
}
unsafe extern "C" fn comparison<P: CallBindings>(slot: Slot, scalar: Borrowed, a: Borrowed, b: Borrowed, strict: u32) -> Borrowed {
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
    origin: unsafe extern "C" fn(Input, usize) -> Origin,
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
    pub(super) fn call_fact<P: CallBindings>(value: Fact, bindings: &P) -> Option<Clause> {
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

#[repr(C)]
struct ProofWidths { origins: usize, domains: usize, conditions: usize }

#[repr(C)]
struct ProofDomain {
    original: Borrowed,
    kind: u32,
}

struct ProofFrame<'a, P> {
    source: &'a Proof,
    bindings: &'a P,
    components: Option<&'a [usize]>,
    output: Proof,
    component: usize,
    origins: std::slice::Iter<'a, ValueIdentity>,
    domains: btree_set::Iter<'a, IndexDomain>,
}

unsafe fn proof_frame<'a, 'b, P>(slot: Slot) -> &'a mut ProofFrame<'b, P> {
    unsafe { &mut *slot.cast() }
}

impl<P> ProofFrame<'_, P> {
    fn intersect(&mut self, range: &ScalarRanges) {
        let ranges = &mut self.output.ranges;
        ranges.resize_with(ranges.len().max(self.component + 1), ScalarRanges::default);
        ranges[self.component] = ranges[self.component].intersection(range);
    }
}

unsafe extern "C" fn proof_begin<P>(slot: Slot, component: usize, origins: u32, domains: u32) {
    let state = unsafe { proof_frame::<P>(slot) };
    let source = state.components.map_or(component, |components| components[component]);
    state.component = component;
    state.origins = state.source.origins.get(source).map(|origins| origins.iter()).unwrap_or_default();
    state.domains = state.source.domains.get(source).map(|domains| domains.iter()).unwrap_or_default();
    if origins != 0 {
        state.output.origins.push(Vec::new());
        state.output.ranges.resize_with(state.output.ranges.len().max(component + 1), ScalarRanges::default);
        state.output.nonzero.resize(state.output.nonzero.len().max(component + 1), false);
    }
    if domains != 0 {
        state.output.domains.push(Domains::new());
    }
}

unsafe extern "C" fn proof_input<P: CallBindings>(slot: Slot) -> Input {
    let state = unsafe { proof_frame::<P>(slot) };
    state.origins.find_map(|id| state.bindings.input(*id).map(|input| describe(input, id))).unwrap_or_default()
}

unsafe extern "C" fn proof_keep_origin<P>(slot: Slot, id: Borrowed) {
    let state = unsafe { proof_frame::<P>(slot) };
    state.output.origins[state.component].push(unsafe { *id.cast::<ValueIdentity>() });
}
unsafe extern "C" fn proof_merge_input<P>(slot: Slot, input: Borrowed) {
    let state = unsafe { proof_frame::<P>(slot) };
    let value = unsafe { proof(input) };
    let index = state.component;
    let output = &mut state.output;
    output.origins[index].extend(value.origins.first().into_iter().flatten().copied());
    output.ranges[index] = range_sets::intersection(&output.ranges[index].0, value.interval_source(0));
    output.nonzero[index] |= value.nonzero.first() == Some(&true);
    output.domains[index].extend(value.domains.first().into_iter().flatten().copied());
    // An incoming condition is already in the caller's identities.
    if let Some(condition) = value.component_condition(0) {
        output.conditions.resize_with(output.conditions.len().max(index + 1), || None);
        let current = output.conditions[index].get_or_insert_with(|| Box::new(Condition::default()));
        for (case, incoming) in current.cases.iter_mut().zip(&condition.cases) {
            *case = Clause::both(case, incoming);
        }
    }
}

unsafe extern "C" fn proof_domain<P>(slot: Slot) -> ProofDomain {
    let state = unsafe { proof_frame::<P>(slot) };
    let Some(domain) = state.domains.next() else {
        return ProofDomain { original: ptr::null(), kind: 0 };
    };
    let kind = match domain {
        IndexDomain::CommonRange(..) => 1,
        IndexDomain::Floating(..) | IndexDomain::Computed(..) => 2,
        _ => 0,
    };
    ProofDomain { original: ptr::from_ref(domain).cast(), kind }
}

unsafe extern "C" fn proof_direct<P: CallBindings>(slot: Slot, domain: Borrowed) {
    let state = unsafe { proof_frame::<P>(slot) };
    if let Some(domain) = Proof::call_domain(unsafe { *domain.cast() }, state.bindings) {
        state.output.domains[state.component].insert(domain);
    }
}

unsafe extern "C" fn proof_common<P: CallBindings>(slot: Slot, domain: Borrowed) {
    let state = unsafe { proof_frame::<P>(slot) };
    let IndexDomain::CommonRange(source, from, low, extended, to) = (unsafe { *domain.cast() }) else {
        unsafe { std::hint::unreachable_unchecked() };
    };
    let Some(incoming) = state.bindings.input(source) else { return; };
    let scalars = [from, low, extended, to];
    let nonzero = incoming.nonzero();
    let range = range_sets::cast_chain(incoming.intervals(), scalars, nonzero);
    let component = state.component;
    if let Some(bits) = Proof::call_common_bits(incoming.proof(), from, low, extended, to) {
        state.output.mark_domain(component, bits.domain());
    }
    state.intersect(&range);
    let output = &mut state.output;
    output.nonzero.resize(output.nonzero.len().max(component + 1), false);
    output.nonzero[component] |= if to.is_integer() { !range.contains(to.order_bias()) } else { nonzero };
    let domains = &mut output.domains[component];
    domains.extend(incoming.domains().copied().filter_map(|domain| super::cast_domain(domain, scalars)));
    domains.extend(incoming.origins().iter()
        .filter(|id| !matches!(id, ValueIdentity::ElementComponent(_) | ValueIdentity::ElementParent(..) | ValueIdentity::ElementIndex(_)))
        .map(|id| IndexDomain::CommonRange(*id, from, low, extended, to)));
}

unsafe extern "C" fn proof_derived<P: CallBindings>(slot: Slot, domain: Borrowed) {
    let state = unsafe { proof_frame::<P>(slot) };
    let result = match unsafe { *domain.cast::<IndexDomain>() } {
        IndexDomain::Floating(bound) => bound.project_at_call(state.bindings)
            .map(|(domain, range)| (domain.map(IndexDomain::Floating), range)),
        IndexDomain::Computed(expression) => expression.project_at_call(state.bindings)
            .map(|(domain, range)| (domain.map(IndexDomain::Computed), range)),
        _ => unsafe { std::hint::unreachable_unchecked() },
    };
    if let Some((domain, range)) = result {
        if let Some(domain) = domain {
            state.output.domains[state.component].insert(domain);
        }
        if let Some(range) = range {
            state.intersect(&range);
        }
    }
}

unsafe extern "C" fn proof_false_case<P>(slot: Slot) -> Borrowed {
    let state = unsafe { proof_frame::<P>(slot) };
    if state.output.nonzero.get(state.component) == Some(&true) {
        return ptr::null();
    }
    state.output.component_condition(state.component)
        .map_or(ptr::null(), |condition| ptr::from_ref(&condition.cases[0]).cast())
}

unsafe extern "C" fn proof_possible(clause: Borrowed) -> u32 {
    u32::from(unsafe { &*clause.cast::<Clause>() }.possible())
}

unsafe extern "C" fn proof_nonzero<P>(slot: Slot) {
    let state = unsafe { proof_frame::<P>(slot) };
    state.output.mark_nonzero(state.component);
}

#[repr(C)]
struct ProofHost {
    begin: unsafe extern "C" fn(Slot, usize, u32, u32),
    input: unsafe extern "C" fn(Slot) -> Input,
    merge_input: unsafe extern "C" fn(Slot, Borrowed),
    keep_origin: unsafe extern "C" fn(Slot, Borrowed),
    domain: unsafe extern "C" fn(Slot) -> ProofDomain,
    direct: unsafe extern "C" fn(Slot, Borrowed),
    common: unsafe extern "C" fn(Slot, Borrowed),
    derived: unsafe extern "C" fn(Slot, Borrowed),
    false_case: unsafe extern "C" fn(Slot) -> Borrowed,
    possible: unsafe extern "C" fn(Borrowed) -> u32,
    nonzero: unsafe extern "C" fn(Slot),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::project_call_proof"]
    fn b_project_proof(context: Slot, widths: ProofWidths, host: ProofHost);
}

pub(super) fn project_proof<P: CallBindings>(
    proof: &Proof, bindings: &P, components: Option<&[usize]>,
) -> Proof {
    let count = |width| components.map_or(width, |selected| selected.len());
    let widths = ProofWidths {
        origins: count(proof.origins.len()), domains: count(proof.domains.len()),
        conditions: count(proof.conditions.len()),
    };
    let source = |component| components.map_or(component, |selected| selected[component]);
    let conditions = (0..widths.conditions).map(|i| proof.component_condition(source(i)));
    let roots: Vec<_> = conditions.clone().flatten().flat_map(|condition| condition.cases.iter()).collect();
    let mut rewritten = if roots.is_empty() { Vec::new() } else {
        Clause::at_calls(&roots, bindings, || false, Clause::required)
    }.into_iter();
    let conditions = conditions.map(|condition| condition.map(|_| {
        Box::new(Condition { cases: [rewritten.next().unwrap(), rewritten.next().unwrap()] })
    })).collect();
    let output = Proof {
        origins: Vec::with_capacity(widths.origins),
        domains: Vec::with_capacity(widths.origins.max(widths.domains)),
        conditions,
        ranges: components.map_or_else(|| proof.ranges.clone(), |selected| {
            selected.iter().map(|&i| proof.range_set(i).into_owned()).collect()
        }),
        nonzero: components.map_or_else(|| proof.nonzero.clone(), |selected| {
            selected.iter().map(|&i| proof.nonzero.get(i) == Some(&true)).collect()
        }),
        ..Proof::default()
    };
    let mut state = ProofFrame {
        source: proof, bindings, components, output, component: 0,
        origins: Default::default(), domains: Default::default(),
    };
    unsafe { b_project_proof(ptr::from_mut(&mut state).cast(), widths, ProofHost {
        begin: proof_begin::<P>, input: proof_input::<P>, merge_input: proof_merge_input::<P>, keep_origin: proof_keep_origin::<P>,
        domain: proof_domain::<P>, direct: proof_direct::<P>, common: proof_common::<P>, derived: proof_derived::<P>,
        false_case: proof_false_case::<P>, possible: proof_possible, nonzero: proof_nonzero::<P>,
    }) };
    state.output
}

