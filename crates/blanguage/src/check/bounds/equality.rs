//! B controls equality closure and alias projection. The original query owns
//! roots, typed expression keys, Facts and the final borrowed/owned result.
use super::{Fact, Facts, IndexDomain, IntegerExpression, IntegerOperation, Operand, QueryFacts, ValueIdentity, fact_fields};
use std::{
    borrow::Cow,
    collections::{BTreeMap, btree_set},
    ffi::c_void,
    mem::MaybeUninit,
    ptr,
};
type Roots = BTreeMap<ValueIdentity, ValueIdentity>;
type Slot = *mut c_void;
type Borrowed = *const c_void;
fn borrowed<T>(value: &T) -> Borrowed {
    ptr::from_ref(value).cast()
}
#[repr(C)]
struct Lookup {
    present: u32,
    root: Borrowed,
}
#[repr(C)]
struct IdentityHost {
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    assign: unsafe extern "C" fn(Slot, Borrowed),
}
unsafe extern "C" fn assign(output: Slot, value: Borrowed) {
    let value = unsafe { *value.cast::<ValueIdentity>() };
    unsafe { output.cast::<ValueIdentity>().write(value) };
}
struct EqualContext<'a, I> {
    roots: &'a mut Roots,
    input: I,
    groups: [*mut ValueIdentity; 2],
}
unsafe fn equal_context<'a, 'b, I>(slot: Slot) -> &'a mut EqualContext<'b, I> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn equal_next<'a, I: Iterator<Item = &'a Fact>>(
    slot: Slot,
) -> fact_fields::Fields {
    unsafe { equal_context::<I>(slot) }
        .input
        .next()
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn equal_lookup<I>(slot: Slot, side: u32, identity: Borrowed) -> Lookup {
    let id = unsafe { *identity.cast::<ValueIdentity>() };
    let context = unsafe { equal_context::<I>(slot) };
    let group = context.roots.get(&id).copied();
    let snapshot = context.groups[side as usize];
    unsafe { snapshot.write(group.unwrap_or(id)) };
    Lookup {
        present: u32::from(group.is_some()),
        root: snapshot.cast(),
    }
}
unsafe extern "C" fn merge<I>(slot: Slot, merged: Borrowed, root: Borrowed) {
    let (merged, root) = unsafe {
        (
            *merged.cast::<ValueIdentity>(),
            *root.cast::<ValueIdentity>(),
        )
    };
    // The native range iterator borrows the original mutable map only here.
    // B decides whether each actual member is assigned to the selected root.
    for (_, current) in unsafe { equal_context::<I>(slot) }
        .roots
        .range_mut(merged..)
    {
        unsafe {
            b_merge(
                (current as *mut ValueIdentity).cast(),
                borrowed(&merged),
                borrowed(&root),
                IdentityHost {
                    compare: fact_fields::compare,
                    assign,
                },
            )
        };
    }
}
unsafe extern "C" fn insert<I>(slot: Slot, identity: Borrowed, root: Borrowed) {
    let (identity, root) = unsafe {
        (
            *identity.cast::<ValueIdentity>(),
            *root.cast::<ValueIdentity>(),
        )
    };
    unsafe { equal_context::<I>(slot) }
        .roots
        .insert(identity, root);
}
#[repr(C)]
struct EqualHost {
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    lookup: unsafe extern "C" fn(Slot, u32, Borrowed) -> Lookup,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    merge: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
    insert: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::equality_merge_value"]
    fn b_merge(current: Slot, merged: Borrowed, root: Borrowed, host: IdentityHost);
    #[link_name = "bootstrap.condition_graph::equality_extend"]
    fn b_extend(context: Slot, host: EqualHost) -> u32;
}
pub(super) fn extend<'a, I: IntoIterator<Item = &'a Fact>>(roots: &mut Roots, facts: I) -> bool {
    let mut left = MaybeUninit::<ValueIdentity>::uninit();
    let mut right = MaybeUninit::<ValueIdentity>::uninit();
    let mut context = EqualContext {
        roots,
        input: facts.into_iter(),
        groups: [left.as_mut_ptr(), right.as_mut_ptr()],
    };
    unsafe {
        b_extend(
            (&mut context as *mut EqualContext<'_, I::IntoIter>).cast(),
            EqualHost {
                next: equal_next::<I::IntoIter>,
                lookup: equal_lookup::<I::IntoIter>,
                compare: fact_fields::compare,
                merge: merge::<I::IntoIter>,
                insert: insert::<I::IntoIter>,
            },
        ) != 0
    }
}
struct ComputedContext<'a> {
    roots: &'a mut Roots,
    expressions: Vec<(&'a ValueIdentity, &'a IntegerExpression)>,
    index: usize,
    produced: BTreeMap<IntegerExpression, ValueIdentity>,
    inverse: Option<(ValueIdentity, IntegerExpression)>,
    equalities: Facts,
    expression: *mut IntegerExpression,
    result: *mut ValueIdentity,
}
unsafe fn computed_context<'a, 'b>(slot: Slot) -> &'a mut ComputedContext<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn expression(domain: Borrowed) -> Borrowed {
    match unsafe { &*domain.cast::<IndexDomain>() } {
        IndexDomain::Computed(value) => borrowed(value),
        _ => ptr::null(),
    }
}
unsafe extern "C" fn begin(slot: Slot) {
    let context = unsafe { computed_context(slot) };
    context.index = 0;
    context.inverse = None;
    context.produced.clear();
    context.equalities.clear();
}
#[repr(C)]
struct ComputedStep {
    present: u32,
    result: Borrowed,
}
unsafe extern "C" fn step(slot: Slot) -> ComputedStep {
    let context = unsafe { computed_context(slot) };
    let (result, expression) = if let Some(inverse) = context.inverse.take() {
        inverse
    } else {
        let Some(&(result, expression)) = context.expressions.get(context.index) else {
            return ComputedStep {
                present: 0,
                result: ptr::null(),
            };
        };
        context.index += 1;
        let root = |id| context.roots.get(&id).copied().unwrap_or(id);
        let expression = expression.map(|id| Some(root(id))).unwrap();
        let resolved = root(*result);
        if context.roots.contains_key(result) {
            use IntegerOperation::{Add, Subtract};
            use Operand::{Constant, Value};
            let IntegerExpression::Binary(op, scalar, a, b) = expression;
            context.inverse = match (op, a, b) {
                (Add, Value(input), constant @ Constant(..))
                | (Add, constant @ Constant(..), Value(input)) => Some((
                    input,
                    IntegerExpression::Binary(Subtract, scalar, Value(resolved), constant),
                )),
                (Subtract, Value(input), constant @ Constant(..)) => Some((
                    input,
                    IntegerExpression::Binary(Add, scalar, Value(resolved), constant),
                )),
                _ => None,
            };
        }
        (resolved, expression)
    };
    unsafe {
        context.result.write(result);
        context.expression.write(expression);
    };
    ComputedStep {
        present: 1,
        result: context.result.cast(),
    }
}
unsafe extern "C" fn produced(slot: Slot) -> Borrowed {
    let context = unsafe { computed_context(slot) };
    context
        .produced
        .get(unsafe { &*context.expression })
        .map_or(ptr::null(), borrowed)
}
unsafe extern "C" fn remember(slot: Slot) {
    let context = unsafe { computed_context(slot) };
    context
        .produced
        .insert(unsafe { *context.expression }, unsafe { *context.result });
}
unsafe extern "C" fn equate(slot: Slot, left: Borrowed, right: Borrowed) {
    let (left, right) = unsafe {
        (
            *left.cast::<ValueIdentity>(),
            *right.cast::<ValueIdentity>(),
        )
    };
    unsafe { computed_context(slot) }
        .equalities
        .insert(Fact::equal(left, right));
}
unsafe extern "C" fn has_equalities(slot: Slot) -> u32 {
    u32::from(!unsafe { computed_context(slot) }.equalities.is_empty())
}
unsafe extern "C" fn extend_computed_equalities(slot: Slot) {
    let context = unsafe { computed_context(slot) };
    extend(context.roots, &context.equalities);
}
#[repr(C)]
struct ComputedHost {
    begin: unsafe extern "C" fn(Slot),
    step: unsafe extern "C" fn(Slot) -> ComputedStep,
    produced: unsafe extern "C" fn(Slot) -> Borrowed,
    remember: unsafe extern "C" fn(Slot),
    equate: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
    has_equalities: unsafe extern "C" fn(Slot) -> u32,
    extend: unsafe extern "C" fn(Slot),
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::computed_equalities"]
    fn b_computed(context: Slot, host: ComputedHost);
}
pub(super) fn computed<'a, I: IntoIterator<Item = &'a Fact>>(roots: &mut Roots, facts: I) {
    let expressions: Vec<_> = facts
        .into_iter()
        .filter_map(|fact| match fact {
            Fact::Index(result, IndexDomain::Computed(expression)) => Some((result, expression)),
            _ => None,
        })
        .collect();
    if expressions.is_empty() {
        return;
    }
    let mut expression_slot = MaybeUninit::<IntegerExpression>::uninit();
    let mut result = MaybeUninit::<ValueIdentity>::uninit();
    let mut context = ComputedContext {
        roots,
        expressions,
        index: 0,
        produced: BTreeMap::new(),
        inverse: None,
        equalities: Facts::new(),
        expression: expression_slot.as_mut_ptr(),
        result: result.as_mut_ptr(),
    };
    unsafe {
        b_computed(
            (&mut context as *mut ComputedContext<'_>).cast(),
            ComputedHost {
                begin: begin,
                step: step,
                produced: produced,
                remember: remember,
                equate: equate,
                has_equalities: has_equalities,
                extend: extend_computed_equalities,
                compare: fact_fields::compare,
            },
        )
    };
}
pub(super) fn roots<'a, I: IntoIterator<Item = &'a Fact>>(facts: I) -> Roots
where
    I::IntoIter: Clone,
{
    let facts = facts.into_iter();
    let mut roots = Roots::new();
    extend(&mut roots, facts.clone());
    computed(&mut roots, facts);
    roots
}
struct Additional<'a> {
    environment: &'a Facts,
    environment_roots: &'a Roots,
    additions: &'a Facts,
    cursor: btree_set::Iter<'a, Fact>,
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
        context.environment.iter().chain(context.additions),
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
struct Projection<'a> {
    environment: &'a Facts,
    facts: &'a Facts,
    roots: &'a Roots,
    members: &'a [(&'a ValueIdentity, &'a ValueIdentity)],
    previous: Option<&'a Roots>,
    cursors: [btree_set::Iter<'a, Fact>; 2],
    group: std::slice::Iter<'a, (&'a ValueIdentity, &'a ValueIdentity)>,
    inferred: Facts,
}
unsafe fn projection<'a, 'b>(slot: Slot) -> &'a mut Projection<'b> {
    unsafe { &mut *slot.cast() }
}
unsafe extern "C" fn alias_next(slot: Slot, side: u32) -> fact_fields::Fields {
    unsafe { projection(slot) }.cursors[side as usize]
        .next()
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn alias_root(slot: Slot, identity: Borrowed) -> Borrowed {
    unsafe { projection(slot) }
        .roots
        .get(unsafe { &*identity.cast::<ValueIdentity>() })
        .map_or(ptr::null(), borrowed)
}
unsafe extern "C" fn previous(slot: Slot, identity: Borrowed) -> Borrowed {
    unsafe { projection(slot) }
        .previous
        .and_then(|roots| roots.get(unsafe { &*identity.cast::<ValueIdentity>() }))
        .map_or(ptr::null(), borrowed)
}
unsafe extern "C" fn group(slot: Slot, root: Borrowed) {
    let root = unsafe { *root.cast::<ValueIdentity>() };
    let context = unsafe { projection(slot) };
    let begin = context.members.partition_point(|(group, _)| **group < root);
    let end = context
        .members
        .partition_point(|(group, _)| **group <= root);
    context.group = context.members[begin..end].iter();
}
unsafe extern "C" fn member(slot: Slot) -> Borrowed {
    unsafe { projection(slot) }
        .group
        .next()
        .map_or(ptr::null(), |(_, id)| borrowed(*id))
}
unsafe extern "C" fn emit(slot: Slot, fields: fact_fields::Fields, identity: Borrowed) {
    let fact = unsafe { fact_fields::key(fields) }.at(unsafe { *identity.cast::<ValueIdentity>() });
    let context = unsafe { projection(slot) };
    if !context.environment.contains(&fact) && !context.facts.contains(&fact) {
        context.inferred.insert(fact);
    }
}
#[repr(C)]
struct AliasHost {
    next: unsafe extern "C" fn(Slot, u32) -> fact_fields::Fields,
    root: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    previous: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    group: unsafe extern "C" fn(Slot, Borrowed),
    member: unsafe extern "C" fn(Slot) -> Borrowed,
    emit: unsafe extern "C" fn(Slot, fact_fields::Fields, Borrowed),
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::alias_facts"]
    fn b_alias(context: Slot, inherited: u32, host: AliasHost);
}
pub(super) fn equivalent<'a>(
    environment: &Facts,
    mut facts: QueryFacts<'a>,
    roots: &Roots,
    members: &[(&ValueIdentity, &ValueIdentity)],
    inherited: Option<&Roots>,
) -> QueryFacts<'a> {
    if members.is_empty() {
        return facts;
    }
    let inferred = {
        let mut context = Projection {
            environment,
            facts: &facts,
            roots,
            members,
            previous: inherited,
            cursors: [facts.iter(), environment.iter()],
            group: members[0..0].iter(),
            inferred: Facts::new(),
        };
        unsafe {
            b_alias(
                (&mut context as *mut Projection<'_>).cast(),
                u32::from(inherited.is_some()),
                AliasHost {
                    next: alias_next,
                    root: alias_root,
                    previous,
                    group,
                    member,
                    emit,
                    compare: fact_fields::compare,
                },
            )
        };
        context.inferred
    };
    if !inferred.is_empty() {
        facts.extend(inferred);
    }
    facts
}
