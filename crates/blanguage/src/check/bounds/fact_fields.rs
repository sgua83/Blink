//! Physical projections borrow original Fact fields until the synchronous consumer returns.
use super::{Fact, IndexDomain, Scalar, ScalarPredicate, ValueIdentity};
use std::{ffi::c_void, ptr};
type Borrowed = *const c_void;

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
pub(super) struct Fields {
    present: u32,
    kind: Kind,
    first: Borrowed,
    second: Borrowed,
    payload: Borrowed,
    scalar: Borrowed,
    value: u64,
    mask: u64,
    flag: u32,
    fixed: u32,
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
                if let IndexDomain::Fixed(length) = domain {
                    view.fixed = 1;
                    view.value = u64::from(*length);
                }
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
pub(super) unsafe fn key(view: Fields) -> Fact {
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

pub(super) unsafe extern "C" fn compare(a: Borrowed, b: Borrowed) -> i32 {
    unsafe { (&*a.cast::<ValueIdentity>()).cmp(&*b.cast::<ValueIdentity>()) as i32 }
}

pub(super) unsafe extern "C" fn project(fact: Borrowed) -> Fields {
    Fields::from(unsafe { &*fact.cast::<Fact>() })
}

#[repr(C)]
pub(super) struct Host {
    pub(super) unsigned32: Borrowed,
    pub(super) compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::negated_fact"]
    fn evaluate(fact: Fields, host: Host) -> Fields;
}
pub(super) fn negated(fact: Fact) -> Option<Fact> {
    let unsigned32 = Scalar::U32;
    let fields = unsafe {
        evaluate(
            Fields::from(&fact),
            Host {
                unsigned32: ptr::from_ref(&unsigned32).cast(),
                compare,
            },
        )
    };
    (fields.present != 0).then(|| unsafe { key(fields) })
}
