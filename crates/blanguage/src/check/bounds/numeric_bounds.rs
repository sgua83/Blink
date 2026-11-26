//! B folds actual bound facts directly into the original consumers' final maps.
use super::facts;
use super::*;
use std::{ffi::c_void, ptr};
type Slot = *mut c_void;
type Borrowed = *const c_void;
pub(super) type BoundMap = BTreeMap<(ValueIdentity, Scalar), Bits>;
#[derive(Clone, Copy)]
#[repr(u32)]
enum Side {
    Lower,
    Upper,
}
#[derive(Default)]
#[repr(C)]
struct Previous {
    present: u32,
    value: u64,
}
struct Context<'a> {
    input: facts::Iter<'a>,
    roots: Option<&'a BTreeMap<ValueIdentity, ValueIdentity>>,
    lower: BoundMap,
    upper: BoundMap,
}
unsafe extern "C" fn next(s: Slot) -> fact_fields::Fields {
    unsafe { &mut *s.cast::<Context<'_>>() }
        .input
        .next()
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn root(s: Slot, id: Borrowed) -> Borrowed {
    let c = unsafe { &*s.cast::<Context<'_>>() };
    c.roots
        .and_then(|roots| roots.get(unsafe { &*id.cast::<ValueIdentity>() }))
        .map(|id| ptr::from_ref(id).cast())
        .unwrap_or(id)
}
unsafe fn key(id: Borrowed, scalar: Borrowed) -> (ValueIdentity, Scalar) {
    unsafe { (*id.cast::<ValueIdentity>(), *scalar.cast::<Scalar>()) }
}
unsafe extern "C" fn lookup(s: Slot, side: Side, id: Borrowed, scalar: Borrowed) -> Previous {
    let c = unsafe { &*s.cast::<Context<'_>>() };
    let map = match side {
        Side::Lower => &c.lower,
        Side::Upper => &c.upper,
    };
    map.get(&unsafe { key(id, scalar) })
        .map(|value| Previous {
            present: 1,
            value: *value,
        })
        .unwrap_or_default()
}
unsafe extern "C" fn emit(s: Slot, side: Side, id: Borrowed, scalar: Borrowed, value: u64) {
    let key = unsafe { key(id, scalar) };
    let c = unsafe { &mut *s.cast::<Context<'_>>() };
    let map = match side {
        Side::Lower => &mut c.lower,
        Side::Upper => &mut c.upper,
    };
    map.insert(key, value);
}
#[repr(C)]
struct Host {
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    root: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    lookup: unsafe extern "C" fn(Slot, Side, Borrowed, Borrowed) -> Previous,
    emit: unsafe extern "C" fn(Slot, Side, Borrowed, Borrowed, u64),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::numeric_bounds"]
    fn b_collect(context: Slot, host: Host);
}
pub(super) fn collect(
    facts: &Facts,
    roots: Option<&BTreeMap<ValueIdentity, ValueIdentity>>,
) -> (BoundMap, BoundMap) {
    let mut c = Context {
        input: facts.iter(),
        roots,
        lower: BoundMap::new(),
        upper: BoundMap::new(),
    };
    unsafe {
        b_collect(
            (&mut c as *mut Context<'_>).cast(),
            Host {
                next,
                root,
                lookup,
                emit,
            },
        )
    };
    (c.lower, c.upper)
}
