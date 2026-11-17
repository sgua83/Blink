//! B controls common fact production. Original query inputs, roots and result own their storage.
use super::{
    Fact, Facts, IndexDomain, KnownBits, QueryFacts, Scalar, ValueIdentity,
    additional_equality_roots, equality_members, equivalent_facts, fact_fields,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, btree_map, btree_set},
    ffi::c_void,
    ptr,
    sync::Arc,
};
type Borrowed = *const c_void;
type Slot = *mut c_void;

struct Context<'a> {
    environment: &'a Facts,
    sides: [&'a Facts; 2],
    roots: btree_map::Iter<'a, ValueIdentity, ValueIdentity>,
    right_roots: &'a BTreeMap<ValueIdentity, ValueIdentity>,
    groups: BTreeMap<(ValueIdentity, ValueIdentity), ValueIdentity>,
    union: btree_set::Union<'a, Fact>,
    cursors: [btree_set::Iter<'a, Fact>; 2],
    common: Facts,
}
unsafe fn context<'a, 'b>(slot: Slot) -> &'a mut Context<'b> {
    unsafe { &mut *slot.cast::<Context<'b>>() }
}
unsafe extern "C" fn intersection(slot: Slot) {
    let context = unsafe { context(slot) };
    context.common = context.sides[0]
        .intersection(context.sides[1])
        .copied()
        .collect();
}
#[repr(C)]
struct Root {
    present: u32,
    matched: u32,
    identity: Borrowed,
    left: Borrowed,
    right: Borrowed,
}
unsafe extern "C" fn root_next(slot: Slot) -> Root {
    let context = unsafe { context(slot) };
    match context.roots.next() {
        Some((id, left)) => {
            let right = context.right_roots.get(id);
            Root {
                present: 1,
                matched: u32::from(right.is_some()),
                identity: ptr::from_ref(id).cast(),
                left: ptr::from_ref(left).cast(),
                right: right.map_or(ptr::null(), |right| ptr::from_ref(right).cast()),
            }
        }
        None => Root {
            present: 0,
            matched: 0,
            identity: ptr::null(),
            left: ptr::null(),
            right: ptr::null(),
        },
    }
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
unsafe extern "C" fn union_next(slot: Slot) -> fact_fields::Fields {
    unsafe { context(slot) }
        .union
        .next()
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
#[repr(C)]
struct Known {
    scalar: Borrowed,
    present: u32,
}
unsafe extern "C" fn known(value: Borrowed) -> Known {
    match unsafe { &*value.cast::<IndexDomain>() } {
        IndexDomain::KnownBits(scalar, ..) => Known {
            scalar: ptr::from_ref(scalar).cast(),
            present: 1,
        },
        _ => Known {
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
    match KnownBits::facts(
        unsafe { *scalar.cast::<Scalar>() },
        unsafe { *identity.cast::<ValueIdentity>() },
        [context.environment, context.sides[side as usize]],
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
unsafe extern "C" fn side_next(slot: Slot, side: u32) -> fact_fields::Fields {
    unsafe { context(slot) }.cursors[side as usize]
        .next()
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
    root_next: unsafe extern "C" fn(Slot) -> Root,
    representative: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed) -> Borrowed,
    contains: unsafe extern "C" fn(Slot, fact_fields::Fields) -> u32,
    emit: unsafe extern "C" fn(Slot, fact_fields::Fields),
    union_next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    known: unsafe extern "C" fn(Borrowed) -> Known,
    known_bits: unsafe extern "C" fn(Slot, u32, Borrowed, Borrowed) -> Bits,
    contains_known: unsafe extern "C" fn(Slot, Borrowed, Borrowed, u64, u64) -> u32,
    emit_known: unsafe extern "C" fn(Slot, Borrowed, Borrowed, u64, u64),
    side_next: unsafe extern "C" fn(Slot, u32) -> fact_fields::Fields,
    range: unsafe extern "C" fn(Slot, fact_fields::Fields, fact_fields::Fields) -> u32,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    scalar_compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    scalar: unsafe extern "C" fn(Borrowed) -> fact_fields::ScalarView,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::common_facts"]
    fn evaluate(context: Slot, host: Host);
}
pub(super) fn either<'a>(
    environment: &Facts,
    environment_roots: &BTreeMap<ValueIdentity, ValueIdentity>,
    environment_members: &[(&ValueIdentity, &ValueIdentity)],
    a: Option<QueryFacts<'a>>,
    b: Option<QueryFacts<'a>>,
) -> Option<QueryFacts<'a>> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let left = additional_equality_roots(environment, environment_roots, &a);
            let right = additional_equality_roots(environment, environment_roots, &b);
            let expand =
                |facts, roots: &Cow<'_, BTreeMap<ValueIdentity, ValueIdentity>>| match roots {
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
            let mut context = Context {
                environment,
                sides: [&a, &b],
                roots: left.iter(),
                right_roots: &right,
                groups: BTreeMap::new(),
                union: a.union(&b),
                cursors: [a.iter(), b.iter()],
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
                        union_next,
                        known,
                        known_bits,
                        contains_known,
                        emit_known,
                        side_next,
                        range,
                        compare: fact_fields::compare,
                        scalar_compare: fact_fields::compare_scalars,
                        scalar: fact_fields::scalar,
                    },
                );
            }
            Some(QueryFacts::Owned(Arc::new(context.common)))
        }
        (a, None) | (None, a) => a,
    }
}
