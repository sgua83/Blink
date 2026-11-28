//! B owns equality roots and computed expression closure. Rust lends the original
//! facts and passes borrowed or owned results to the query.
use super::facts;
use super::{Fact, Facts, IndexDomain, OutputBindingEntries, OutputBindings, QueryFacts, ValueIdentity, fact_fields};
use std::{
    borrow::Cow,
    ffi::c_void,
    ptr,
};
#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct Roots(OutputBindings);

impl Roots {
    pub(super) fn new() -> Self { Self(OutputBindings::new()) }
    pub(super) fn is_empty(&self) -> bool { self.0.is_empty() }
    pub(super) fn get(&self, id: &ValueIdentity) -> Option<&ValueIdentity> { self.0.get(id) }
    pub(super) fn bindings(&self) -> &OutputBindings { &self.0 }
    pub(super) fn iter(&self) -> OutputBindingEntries<'_> { self.0.iter() }
}

impl<'a> IntoIterator for &'a Roots {
    type Item = (&'a ValueIdentity, &'a ValueIdentity);
    type IntoIter = OutputBindingEntries<'a>;

    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

type Slot = *mut c_void;
type Borrowed = *const c_void;
fn borrowed<T>(value: &T) -> Borrowed {
    ptr::from_ref(value).cast()
}

unsafe extern "C" {
    #[link_name = "bootstrap.output_bindings::extend_equality_roots"]
    fn b_extend(roots: &mut Roots, context: Slot, next: unsafe extern "C" fn(Slot) -> Borrowed) -> u32;
}

pub(super) fn extend<'a, I: IntoIterator<Item = &'a Fact>>(roots: &mut Roots, facts: I) -> bool {
    unsafe extern "C" fn next<'a, I: Iterator<Item = &'a Fact>>(context: Slot) -> Borrowed {
        unsafe { &mut *context.cast::<I>() }.next().map_or(ptr::null(), borrowed)
    }
    let mut facts = facts.into_iter();
    unsafe { b_extend(roots, ptr::from_mut(&mut facts).cast(), next::<I::IntoIter>) != 0 }
}

unsafe extern "C" fn expression(domain: Borrowed) -> Borrowed {
    match unsafe { &*domain.cast::<IndexDomain>() } {
        IndexDomain::Computed(value) => borrowed(value),
        _ => ptr::null(),
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.output_bindings::resolve_computed_roots"]
    fn resolve_computed(roots: &mut Roots, sources: *const &Fact, count: usize);
}

pub(super) fn computed<'a>(roots: &mut Roots, fact_sets: impl IntoIterator<Item = &'a Facts>) {
    let sources: Vec<_> = fact_sets.into_iter()
        .flat_map(|facts| facts.iter().take_while(|fact| matches!(fact, Fact::Equal(..) | Fact::Index(..))))
        .filter(|fact| matches!(fact, Fact::Index(_, IndexDomain::Computed(_))))
        .collect();
    if !sources.is_empty() {
        unsafe { resolve_computed(roots, sources.as_ptr(), sources.len()); }
    }
}

pub(super) fn roots(facts: &Facts) -> Roots {
    let mut roots = Roots::new();
    extend(&mut roots, facts);
    computed(&mut roots, [facts]);
    roots
}
struct Additional<'a> {
    environment: &'a Facts,
    environment_roots: &'a Roots,
    additions: &'a Facts,
    cursor: facts::Iter<'a>,
    result: Cow<'a, Roots>,
}
unsafe fn additional_context<'a, 'b>(slot: Slot) -> &'a mut Additional<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn additional_next(slot: Slot) -> fact_fields::Fields {
    unsafe { additional_context(slot) }
        .cursor
        .next()
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn additional_lookup(slot: Slot, id: Borrowed) -> Borrowed {
    unsafe { additional_context(slot) }
        .environment_roots
        .get(unsafe { &*id.cast::<ValueIdentity>() })
        .map_or(ptr::null(), borrowed)
}
unsafe extern "C" fn own(slot: Slot) {
    let context = unsafe { additional_context(slot) };
    context.result = Cow::Owned(context.environment_roots.clone());
}
unsafe extern "C" fn additional_extend(slot: Slot) {
    let context = unsafe { additional_context(slot) };
    extend(context.result.to_mut(), context.additions);
}
unsafe extern "C" fn additional_computed(slot: Slot) {
    let context = unsafe { additional_context(slot) };
    computed(
        context.result.to_mut(),
        [context.environment, context.additions],
    );
}
unsafe extern "C" fn unchanged(slot: Slot) -> u32 {
    let context = unsafe { additional_context(slot) };
    u32::from(context.result.as_ref() == context.environment_roots)
}
unsafe extern "C" fn borrow(slot: Slot) {
    let context = unsafe { additional_context(slot) };
    context.result = Cow::Borrowed(context.environment_roots);
}
#[repr(C)]
struct AdditionalHost {
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    lookup: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    expression: unsafe extern "C" fn(Borrowed) -> Borrowed,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    own: unsafe extern "C" fn(Slot),
    extend: unsafe extern "C" fn(Slot),
    computed: unsafe extern "C" fn(Slot),
    unchanged: unsafe extern "C" fn(Slot) -> u32,
    borrow: unsafe extern "C" fn(Slot),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::additional_equalities"]
    fn b_additional(context: Slot, host: AdditionalHost);
}
pub(super) fn additional<'a>(
    environment: &Facts,
    environment_roots: &'a Roots,
    additions: &Facts,
) -> Cow<'a, Roots> {
    let mut context = Additional {
        environment,
        environment_roots,
        additions,
        cursor: additions.iter(),
        result: Cow::Borrowed(environment_roots),
    };
    unsafe {
        b_additional(
            (&mut context as *mut Additional<'_>).cast(),
            AdditionalHost {
                next: additional_next,
                lookup: additional_lookup,
                expression,
                compare: fact_fields::compare,
                own,
                extend: additional_extend,
                computed: additional_computed,
                unchanged,
                borrow,
            },
        )
    };
    match context.result {
        Cow::Borrowed(_) => Cow::Borrowed(environment_roots),
        Cow::Owned(value) => Cow::Owned(value),
    }
}
#[derive(Clone, Copy)]
#[repr(C)]
struct MemberStorage {
    records: Slot,
    count: usize,
}

pub(super) struct Members<'a> {
    roots: &'a Roots,
    storage: MemberStorage,
}

unsafe extern "C" {
    #[link_name = "bootstrap.output_bindings::build_equality_members"]
    fn build_members(roots: &Roots) -> MemberStorage;
    #[link_name = "bootstrap.output_bindings::release_equality_members"]
    fn release_members(records: Slot);
    #[link_name = "bootstrap.output_bindings::project_alias_facts"]
    fn project_aliases(environment: &Facts, facts: &Facts, roots: &Roots, members: MemberStorage,
        previous: *const Roots, output: &mut Facts);
}

impl<'a> Members<'a> {
    pub(super) fn new(roots: &'a Roots) -> Self {
        Self { roots, storage: unsafe { build_members(roots) } }
    }

    pub(super) fn roots(&self) -> &'a Roots { self.roots }
}

impl Drop for Members<'_> {
    fn drop(&mut self) { unsafe { release_members(self.storage.records) } }
}

pub(super) fn equivalent<'a>(environment: &Facts, mut facts: QueryFacts<'a>, members: &Members<'_>,
    inherited: Option<&Roots>) -> QueryFacts<'a> {
    let mut inferred = Facts::new();
    let previous = inherited.map_or(ptr::null(), ptr::from_ref);
    unsafe { project_aliases(environment, &facts, members.roots, members.storage, previous, &mut inferred) };
    if !inferred.is_empty() {
        facts.extend(inferred);
    }
    facts
}
