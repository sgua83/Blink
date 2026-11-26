//! B selects cross-set conflicts; original fact containers own keys and storage.
use super::facts;
use super::{
    Facts, Scalar,
    fact_fields::{Fields, compare, key},
};
use std::{ffi::c_void, ops::Bound, ptr};

type Borrowed = *const c_void;
type Slot = *mut c_void;

struct Context<'a> {
    sets: [&'a Facts; 2],
    iterator: facts::Iter<'a>,
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
    fn evaluate(slot: Slot, scalars: Scalars, host: Host, may_share: u32) -> u32;
}

pub(super) fn conflict(a: &Facts, b: &Facts, may_share: bool) -> bool {
    let mut context = Context {
        sets: [a, b],
        iterator: facts::Iter::default(),
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
            u32::from(may_share),
        ) != 0
    }
}
