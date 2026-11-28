//! B controls common fact production. Original query inputs, roots and result own their storage.
use super::facts;
use super::{
    EqualityRoots, Fact, Facts, IndexDomain, OutputBindingEntries, QueryFacts, Scalar, ValueIdentity,
    additional_equality_roots, equality_members, equivalent_facts, fact_fields,
};
use std::{
    cell::OnceCell,
    borrow::Cow,
    collections::{BTreeMap, btree_map},
    ffi::c_void,
    ptr,
    sync::Arc,
};
type Borrowed = *const c_void;
type Slot = *mut c_void;

struct Context<'a> {
    environment: &'a Facts,
    sides: [&'a Facts; 2],
    roots: [OutputBindingEntries<'a>; 2],
    groups: BTreeMap<(ValueIdentity, ValueIdentity), ValueIdentity>,
    union: facts::Merge<'a>,
    cursors: [facts::Iter<'a>; 2],
    common: Facts,
}
unsafe fn context<'a, 'b>(slot: Slot) -> &'a mut Context<'b> {
    unsafe { &mut *slot.cast::<Context<'b>>() }
}
unsafe extern "C" fn intersection(slot: Slot) {
    let context = unsafe { context(slot) };
    context.common = context.sides[0].intersection(context.sides[1]).into_facts();
}
#[repr(C)]
struct Root {
    identity: Borrowed,
    root: Borrowed,
}
unsafe extern "C" fn root_next(slot: Slot, side: u32) -> Root {
    unsafe { context(slot) }.roots[side as usize].next().map_or(
        Root { identity: ptr::null(), root: ptr::null() },
        |(identity, root)| Root {
            identity: ptr::from_ref(identity).cast(),
            root: ptr::from_ref(root).cast(),
        },
    )
}
unsafe extern "C" fn representative(
    slot: Slot,
    left: Borrowed,
    right: Borrowed,
    identity: Borrowed,
) -> Borrowed {
    let pair = unsafe {
        (
            *left.cast::<ValueIdentity>(),
            *right.cast::<ValueIdentity>(),
        )
    };
    match unsafe { context(slot) }.groups.entry(pair) {
        btree_map::Entry::Vacant(entry) => {
            entry.insert(unsafe { *identity.cast::<ValueIdentity>() });
            ptr::null()
        }
        btree_map::Entry::Occupied(entry) => ptr::from_ref(entry.into_mut()).cast(),
    }
}
unsafe extern "C" fn contains(slot: Slot, fields: fact_fields::Fields) -> u32 {
    u32::from(
        unsafe { context(slot) }
            .environment
            .contains(&unsafe { fact_fields::key(fields) }),
    )
}
unsafe extern "C" fn emit(slot: Slot, fields: fact_fields::Fields) {
    unsafe { context(slot) }
        .common
        .insert(unsafe { fact_fields::key(fields) });
}
#[repr(C)]
struct Known {
    identity: Borrowed,
    scalar: Borrowed,
    present: u32,
}
unsafe extern "C" fn known_next(slot: Slot) -> Known {
    let context = unsafe { context(slot) };
    match context.union.find_map(|fact| match fact {
        Fact::Index(identity, IndexDomain::KnownBits(scalar, ..)) => Some((identity, scalar)),
        _ => None,
    }) {
        Some((identity, scalar)) => Known {
            identity: ptr::from_ref(identity).cast(),
            scalar: ptr::from_ref(scalar).cast(),
            present: 1,
        },
        None => Known {
            identity: ptr::null(),
            scalar: ptr::null(),
            present: 0,
        },
    }
}
#[repr(C)]
struct Bits {
    mask: u64,
    value: u64,
    present: u32,
}
unsafe extern "C" fn known_bits(
    slot: Slot,
    side: u32,
    identity: Borrowed,
    scalar: Borrowed,
) -> Bits {
    let context = unsafe { context(slot) };
    match super::known_bits::from_facts(
        unsafe { *scalar.cast::<Scalar>() },
        unsafe { *identity.cast::<ValueIdentity>() },
        &[context.environment, context.sides[side as usize]],
    ) {
        Some(bits) => Bits {
            mask: bits.mask,
            value: bits.value,
            present: 1,
        },
        None => Bits {
            mask: 0,
            value: 0,
            present: 0,
        },
    }
}
unsafe fn known_key(identity: Borrowed, scalar: Borrowed, mask: u64, value: u64) -> Fact {
    Fact::Index(
        unsafe { *identity.cast::<ValueIdentity>() },
        IndexDomain::KnownBits(unsafe { *scalar.cast::<Scalar>() }, mask, value),
    )
}
unsafe extern "C" fn contains_known(
    slot: Slot,
    identity: Borrowed,
    scalar: Borrowed,
    mask: u64,
    value: u64,
) -> u32 {
    u32::from(
        unsafe { context(slot) }
            .environment
            .contains(&unsafe { known_key(identity, scalar, mask, value) }),
    )
}
unsafe extern "C" fn emit_known(
    slot: Slot,
    identity: Borrowed,
    scalar: Borrowed,
    mask: u64,
    value: u64,
) {
    unsafe { context(slot) }
        .common
        .insert(unsafe { known_key(identity, scalar, mask, value) });
}
unsafe extern "C" fn bound_next(slot: Slot, side: u32) -> fact_fields::Fields {
    unsafe { context(slot) }.cursors[side as usize]
        .find(|fact| matches!(fact, Fact::Minimum(..) | Fact::Maximum(..)))
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn range(
    slot: Slot,
    lower: fact_fields::Fields,
    upper: fact_fields::Fields,
) -> u32 {
    let (lower, upper) = unsafe { (fact_fields::key(lower), fact_fields::key(upper)) };
    u32::from(
        unsafe { context(slot) }
            .environment
            .range(lower..=upper)
            .next()
            .is_some(),
    )
}
#[repr(C)]
struct Host {
    intersection: unsafe extern "C" fn(Slot),
    root_next: unsafe extern "C" fn(Slot, u32) -> Root,
    representative: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed) -> Borrowed,
    contains: unsafe extern "C" fn(Slot, fact_fields::Fields) -> u32,
    emit: unsafe extern "C" fn(Slot, fact_fields::Fields),
    known_next: unsafe extern "C" fn(Slot) -> Known,
    known_bits: unsafe extern "C" fn(Slot, u32, Borrowed, Borrowed) -> Bits,
    contains_known: unsafe extern "C" fn(Slot, Borrowed, Borrowed, u64, u64) -> u32,
    emit_known: unsafe extern "C" fn(Slot, Borrowed, Borrowed, u64, u64),
    bound_next: unsafe extern "C" fn(Slot, u32) -> fact_fields::Fields,
    range: unsafe extern "C" fn(Slot, fact_fields::Fields, fact_fields::Fields) -> u32,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    scalar_compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    scalar: unsafe extern "C" fn(Borrowed) -> fact_fields::ScalarView,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::common_facts"]
    fn evaluate(context: Slot, host: Host);
}
fn fold<'a>(
    environment: &'a Facts, sides: [&'a Facts; 2], roots: [OutputBindingEntries<'a>; 2],
) -> Facts {
    let mut context = Context {
        environment,
        sides,
        roots,
        groups: BTreeMap::new(),
        union: sides[0].union(sides[1]),
        cursors: [sides[0].iter(), sides[1].iter()],
        common: Facts::new(),
    };
    unsafe {
        evaluate(
            (&mut context as *mut Context<'_>).cast(),
            Host {
                intersection,
                root_next,
                representative,
                contains,
                emit,
                known_next,
                known_bits,
                contains_known,
                emit_known,
                bound_next,
                range,
                compare: fact_fields::compare,
                scalar_compare: fact_fields::compare_scalars,
                scalar: fact_fields::scalar,
            },
        );
    }
    context.common
}

pub(super) fn either<'a>(
    environment: &Facts,
    environment_roots: &EqualityRoots,
    environment_members: &[(&ValueIdentity, &ValueIdentity)],
    shared_equalities: Option<&OnceCell<Arc<Facts>>>,
    a: Option<QueryFacts<'a>>,
    b: Option<QueryFacts<'a>>,
) -> Option<QueryFacts<'a>> {
    match (a, b) {
        // With no established facts, an empty side supplies no equality,
        // range or known bits to the disjunction. Reuse its empty result.
        (Some(a), Some(_)) if environment.is_empty() && a.is_empty() => Some(a),
        (Some(_), Some(b)) if environment.is_empty() && b.is_empty() => Some(b),
        (Some(a), Some(b)) => {
            let left = additional_equality_roots(environment, environment_roots, &a);
            let right = additional_equality_roots(environment, environment_roots, &b);
            let expand =
                |facts, roots: &Cow<'_, EqualityRoots>| match roots {
                    Cow::Borrowed(_) => {
                        equivalent_facts(environment, facts, roots, environment_members, None)
                    }
                    Cow::Owned(_) => equivalent_facts(
                        environment,
                        facts,
                        roots,
                        &equality_members(roots),
                        Some(environment_roots),
                    ),
                };
            let a = expand(a, &left);
            let b = expand(b, &right);
            let common = if let Some(original) = shared_equalities
                && matches!(left, Cow::Borrowed(_))
                && matches!(right, Cow::Borrowed(_))
            {
                // Empty sides leave only the original equality result to produce.
                let shared = original.get_or_init(|| {
                    let empty = Facts::new();
                    Arc::new(fold(environment, [&empty, &empty], [environment_roots.iter(), environment_roots.iter()]))
                });
                let empty_roots = EqualityRoots::new();
                let mut facts = fold(environment, [&a, &b], [empty_roots.iter(), empty_roots.iter()]);
                if facts.is_empty() {
                    return Some(QueryFacts::Owned(Arc::clone(shared)));
                }
                facts.extend(shared.iter().copied());
                facts
            } else {
                fold(environment, [&a, &b], [left.iter(), right.iter()])
            };
            Some(QueryFacts::Owned(Arc::new(common)))
        }
        (a, None) | (None, a) => a,
    }
}
