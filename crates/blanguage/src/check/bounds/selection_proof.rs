//! The native proof store lends one aggregate candidate at a time to Blink.
use super::*;
use std::{ffi::c_void, ptr};

pub(super) enum Choices<'a> {
    Fixed { count: u32, index: ownership::ApplicationIndex },
    Owned(&'a [(Vec<usize>, Clause)]),
}

struct Candidate {
    proof: Proof,
    condition: Clause,
    facts: Option<RequiredFacts>,
}

struct Selection<'a> {
    path: &'a Clause,
    environment: Facts,
    proof: &'a Proof,
    source: &'a Ty,
    element: &'a Ty,
    registers: Option<&'a [Reg]>,
    choices: Choices<'a>,
    components: Vec<usize>,
    current: Option<Candidate>,
    selected: Option<Proof>,
    related: Option<Clause>,
}

#[repr(C)]
struct Host {
    project: unsafe extern "C" fn(*mut c_void, usize),
    relate: unsafe extern "C" fn(*mut c_void),
    require: unsafe extern "C" fn(*mut c_void) -> u32,
    refine: unsafe extern "C" fn(*mut c_void, usize),
    join: unsafe extern "C" fn(*mut c_void),
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::select_aggregate_proof"]
    fn select_aggregate_proof(
        context: *mut c_void, choices: usize, components: usize, publish: u32, host: Host,
    );
}

unsafe extern "C" fn project(context: *mut c_void, choice: usize) {
    let state = unsafe { &mut *context.cast::<Selection<'_>>() };
    let width = state.element.components.len();
    let derived;
    let (components, condition): (&[usize], &Clause) = match &state.choices {
        Choices::Fixed { index, .. } => {
            let first = choice * width;
            state.components.clear();
            state.components.extend(first..first + width);
            derived = Clause::selection(*index, choice);
            (&state.components, &derived)
        }
        Choices::Owned(choices) => {
            let (components, condition) = &choices[choice];
            (components, condition)
        }
    };
    let mut proof = state.proof.project(components, state.source.components.len());
    if let Some(registers) = state.registers {
        proof.origins.resize_with(width, Vec::new);
        for (origins, register) in proof.origins.iter_mut().zip(registers) {
            let identity = ValueIdentity::Register(*register);
            origins.retain(|origin| *origin != identity);
            origins.insert(0, identity);
        }
    }
    let condition = Clause::both(condition, &Clause::facts(proof.facts(state.element)));
    state.current = Some(Candidate { proof, condition, facts: None });
}

unsafe extern "C" fn relate(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<Selection<'_>>() };
    let related = state.related.as_mut().unwrap();
    *related = Clause::either(related, &state.current.as_ref().unwrap().condition);
}

unsafe extern "C" fn require(context: *mut c_void) -> u32 {
    let state = unsafe { &mut *context.cast::<Selection<'_>>() };
    let mut current = state.current.take().unwrap();
    current.condition = Clause::both(state.path, &current.condition);
    let Some(required) = current.condition.required_in(&state.environment) else {
        return 0;
    };
    current.proof.extend_equalities(&required.roots);
    current.facts = Some(required);
    state.current = Some(current);
    1
}

unsafe extern "C" fn refine(context: *mut c_void, component: usize) {
    let state = unsafe { &mut *context.cast::<Selection<'_>>() };
    let current = state.current.as_mut().unwrap();
    for fact in &current.facts.as_ref().unwrap().facts {
        if fact.identity().is_some_and(|identity| {
            current.proof.origins.get(component).is_some_and(|origins| origins.contains(&identity))
        }) {
            current.proof.refine(state.element, component, *fact);
        }
    }
}

unsafe extern "C" fn join(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<Selection<'_>>() };
    let Candidate { mut proof, condition, .. } = state.current.take().unwrap();
    proof.under_path(state.element, &condition);
    match &mut state.selected {
        Some(selected) => selected.join(&proof, state.element.components.len(), None),
        None => state.selected = Some(proof),
    }
}

pub(super) fn run(
    path: &Clause, mut environment: Facts, proof: &Proof, source: &Ty, element: &Ty,
    choices: Choices<'_>, registers: Option<&[Reg]>,
) -> (Proof, Clause) {
    environment.extend(proof.facts(source));
    let count = match &choices {
        Choices::Fixed { count, .. } => *count as usize,
        Choices::Owned(choices) => choices.len(),
    };
    let publish = u32::from(registers.is_some());
    let mut state = Selection {
        path, environment, proof, source, element, registers, choices,
        components: Vec::new(), current: None, selected: None,
        related: registers.map(|_| Clause::impossible()),
    };
    unsafe {
        select_aggregate_proof(
            ptr::from_mut(&mut state).cast(), count, element.components.len(), publish,
            Host { project, relate, require, refine, join },
        );
    }
    let path = state.related.map_or_else(|| path.clone(), |related| Clause::both(path, &related));
    (state.selected.unwrap_or_default(), path)
}

struct CaptureSelection<'a> {
    value: &'a Value,
    result: Proof,
}

#[repr(C)]
struct CaptureHost {
    all: unsafe extern "C" fn(*mut c_void),
    component: unsafe extern "C" fn(*mut c_void, usize, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::capture_selected_proof"]
    fn capture_selected_proof(
        context: *mut c_void, indices: *const usize, count: usize,
        width: usize, registers: usize, host: CaptureHost,
    );
}

unsafe extern "C" fn capture_all(context: *mut c_void) {
    let state = unsafe { &mut *context.cast::<CaptureSelection<'_>>() };
    state.result = Proof::capture(state.value);
}

unsafe extern "C" fn capture_component(context: *mut c_void, component: usize, registered: u32) {
    let state = unsafe { &mut *context.cast::<CaptureSelection<'_>>() };
    let value = state.value;
    let (range, nonzero, origins, condition) = if registered != 0 {
        let (range, nonzero) = value.component_numeric_facts(component);
        (range, nonzero, value.component_origins(component), Proof::value_condition(value, component))
    } else {
        (ScalarRanges::default(), false, Vec::new(), None)
    };

    let proof = &value.proof;
    let result = &mut state.result;
    result.ranges.push(range);
    result.nonzero.push(nonzero);
    result.origins.push(origins);
    result.conditions.push(condition);
    result.domains.push(proof.domains(component).into_owned());
    result.views.push(proof.view_borrow(component));
    if !proof.addresses.is_empty() {
        result.addresses.push(proof.addresses.get(component).cloned().flatten());
    }
}

pub(super) fn capture(value: &Value, components: &[usize], width: usize) -> Proof {
    let mut state = CaptureSelection { value, result: Proof::default() };
    unsafe {
        capture_selected_proof(
            ptr::from_mut(&mut state).cast(), components.as_ptr(), components.len(),
            width, value.regs.len(), CaptureHost { all: capture_all, component: capture_component },
        );
    }
    state.result
}
