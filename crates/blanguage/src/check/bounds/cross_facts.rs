//! B selects cross-set conflicts; original fact containers own keys and storage.
use super::{Fact, Facts, IndexDomain, Scalar, ScalarPredicate, ValueIdentity};
use std::{collections::btree_set, ffi::c_void, ops::Bound, ptr};

type Borrowed = *const c_void;
type Slot = *mut c_void;

#[derive(Clone, Copy, Default)]
#[repr(u32)]
enum Kind {
    #[default]
    Equal = 1,
    Index,
    Minimum,
    Maximum,
    NonZero,
    Boolean,
    Below,
    Outside,
    Zero,
    NotEqual,
    Predicate,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Fields {
    present: u32,
    kind: Kind,
    first: Borrowed,
    second: Borrowed,
    payload: Borrowed,
    scalar: Borrowed,
    value: u64,
    mask: u64,
    flag: u32,
}

impl From<&Fact> for Fields {
    fn from(fact: &Fact) -> Self {
        let mut view = Self {
            present: 1,
            ..Self::default()
        };
        match fact {
            Fact::Equal(a, b) => {
                view.kind = Kind::Equal;
                view.first = ptr::from_ref(a).cast();
                view.second = ptr::from_ref(b).cast();
            }
            Fact::NotEqual(a, b) => {
                view.kind = Kind::NotEqual;
                view.first = ptr::from_ref(a).cast();
                view.second = ptr::from_ref(b).cast();
            }
            Fact::Index(id, domain) => {
                view.kind = Kind::Index;
                view.first = ptr::from_ref(id).cast();
                view.payload = ptr::from_ref(domain).cast();
            }
            Fact::Outside(id, domain) => {
                view.kind = Kind::Outside;
                view.first = ptr::from_ref(id).cast();
                view.payload = ptr::from_ref(domain).cast();
            }
            Fact::Minimum(id, scalar, value) => {
                view.kind = Kind::Minimum;
                view.first = ptr::from_ref(id).cast();
                view.scalar = ptr::from_ref(scalar).cast();
                view.value = *value;
                view.mask = scalar.bit_mask();
            }
            Fact::Maximum(id, scalar, value) => {
                view.kind = Kind::Maximum;
                view.first = ptr::from_ref(id).cast();
                view.scalar = ptr::from_ref(scalar).cast();
                view.value = *value;
                view.mask = scalar.bit_mask();
            }
            Fact::NonZero(id) => {
                view.kind = Kind::NonZero;
                view.first = ptr::from_ref(id).cast();
            }
            Fact::Zero(id) => {
                view.kind = Kind::Zero;
                view.first = ptr::from_ref(id).cast();
            }
            Fact::Boolean(id, truth) => {
                view.kind = Kind::Boolean;
                view.first = ptr::from_ref(id).cast();
                view.flag = u32::from(*truth);
            }
            Fact::Below(a, b, strict, scalar) => {
                view.kind = Kind::Below;
                view.first = ptr::from_ref(a).cast();
                view.second = ptr::from_ref(b).cast();
                view.scalar = ptr::from_ref(scalar).cast();
                view.flag = u32::from(*strict);
            }
            Fact::Predicate(predicate, truth) => {
                view.kind = Kind::Predicate;
                view.payload = ptr::from_ref(predicate).cast();
                view.flag = u32::from(*truth);
            }
        }
        view
    }
}

// These keys only live during the native container operation that consumes them.
unsafe fn key(view: Fields) -> Fact {
    unsafe {
        match view.kind {
            Kind::Equal => Fact::Equal(
                *view.first.cast::<ValueIdentity>(),
                *view.second.cast::<ValueIdentity>(),
            ),
            Kind::NotEqual => Fact::NotEqual(
                *view.first.cast::<ValueIdentity>(),
                *view.second.cast::<ValueIdentity>(),
            ),
            Kind::Index => Fact::Index(
                *view.first.cast::<ValueIdentity>(),
                *view.payload.cast::<IndexDomain>(),
            ),
            Kind::Outside => Fact::Outside(
                *view.first.cast::<ValueIdentity>(),
                *view.payload.cast::<IndexDomain>(),
            ),
            Kind::Minimum => Fact::Minimum(
                *view.first.cast::<ValueIdentity>(),
                *view.scalar.cast::<Scalar>(),
                view.value,
            ),
            Kind::Maximum => Fact::Maximum(
                *view.first.cast::<ValueIdentity>(),
                *view.scalar.cast::<Scalar>(),
                view.value,
            ),
            Kind::NonZero => Fact::NonZero(*view.first.cast::<ValueIdentity>()),
            Kind::Zero => Fact::Zero(*view.first.cast::<ValueIdentity>()),
            Kind::Boolean => Fact::Boolean(*view.first.cast::<ValueIdentity>(), view.flag != 0),
            Kind::Below => Fact::Below(
                *view.first.cast::<ValueIdentity>(),
                *view.second.cast::<ValueIdentity>(),
                view.flag != 0,
                *view.scalar.cast::<Scalar>(),
            ),
            Kind::Predicate => {
                Fact::Predicate(*view.payload.cast::<ScalarPredicate>(), view.flag != 0)
            }
        }
    }
}

struct Context<'a> {
    sets: [&'a Facts; 2],
    iterator: btree_set::Iter<'a, Fact>,
}

unsafe fn context<'a, 'b>(slot: Slot) -> &'a mut Context<'b> {
    unsafe { &mut *slot.cast::<Context<'b>>() }
}
unsafe extern "C" fn count(slot: Slot, side: u32) -> usize {
    unsafe { context(slot) }.sets[side as usize].len()
}
unsafe extern "C" fn begin(slot: Slot, side: u32) {
    let context = unsafe { context(slot) };
    context.iterator = context.sets[side as usize].iter();
}
unsafe extern "C" fn next(slot: Slot) -> Fields {
    unsafe { context(slot) }
        .iterator
        .next()
        .map(Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn contains(slot: Slot, side: u32, view: Fields) -> u32 {
    u32::from(unsafe { context(slot) }.sets[side as usize].contains(&unsafe { key(view) }))
}
unsafe extern "C" fn range(
    slot: Slot,
    side: u32,
    lower: Fields,
    upper: Fields,
    exclude_lower: u32,
    include_upper: u32,
) -> u32 {
    let lower = unsafe { key(lower) };
    let upper = unsafe { key(upper) };
    let lower = if exclude_lower != 0 {
        Bound::Excluded(lower)
    } else {
        Bound::Included(lower)
    };
    let upper = if include_upper != 0 {
        Bound::Included(upper)
    } else {
        Bound::Excluded(upper)
    };
    u32::from(
        unsafe { context(slot) }.sets[side as usize]
            .range((lower, upper))
            .next()
            .is_some(),
    )
}
unsafe extern "C" fn compare(a: Borrowed, b: Borrowed) -> i32 {
    unsafe { (&*a.cast::<ValueIdentity>()).cmp(&*b.cast::<ValueIdentity>()) as i32 }
}

#[repr(C)]
struct Scalars {
    values: [Borrowed; 4],
}
#[repr(C)]
struct Host {
    count: unsafe extern "C" fn(Slot, u32) -> usize,
    begin: unsafe extern "C" fn(Slot, u32),
    next: unsafe extern "C" fn(Slot) -> Fields,
    contains: unsafe extern "C" fn(Slot, u32, Fields) -> u32,
    range: unsafe extern "C" fn(Slot, u32, Fields, Fields, u32, u32) -> u32,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::cross_conflict"]
    fn evaluate(slot: Slot, scalars: Scalars, host: Host) -> u32;
}

pub(super) fn conflict(a: &Facts, b: &Facts) -> bool {
    let mut context = Context {
        sets: [a, b],
        iterator: btree_set::Iter::default(),
    };
    let numeric = [Scalar::U32, Scalar::I32, Scalar::U64, Scalar::I64];
    let scalars = Scalars {
        values: numeric
            .each_ref()
            .map(|scalar| ptr::from_ref(scalar).cast()),
    };
    unsafe {
        evaluate(
            (&mut context as *mut Context<'_>).cast(),
            scalars,
            Host {
                count,
                begin,
                next,
                contains,
                range,
                compare,
            },
        ) != 0
    }
}
