//! B selects index-domain projections and arithmetic relations. Inputs borrow
//! the original immutable Proof; only the actual result and max lookup are owned.
use super::*;
use std::{collections::btree_set, ffi::c_void, mem::MaybeUninit, ptr};
type Slot = *mut c_void;
type Borrowed = *const c_void;
fn borrow<T>(value: &T) -> Borrowed {
    ptr::from_ref(value).cast()
}
unsafe fn frame<'a, T>(slot: Slot) -> &'a mut T {
    unsafe { &mut *slot.cast() }
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub(super) struct Fields {
    present: u32,
    kind: u32,
    original: Borrowed,
    payload: Borrowed,
    first: Borrowed,
    second: Borrowed,
    distance: u64,
}
pub(super) fn fields(value: &IndexDomain) -> Fields {
    let mut out = Fields {
        present: 1,
        original: borrow(value),
        ..Fields::default()
    };
    match value {
        IndexDomain::Data(v) => {
            out.kind = 1;
            out.payload = borrow(v);
        }
        IndexDomain::Tile => out.kind = 2,
        IndexDomain::View(v) => {
            out.kind = 3;
            out.payload = borrow(v);
        }
        IndexDomain::ApplicationData(v) => {
            out.kind = 4;
            out.first = borrow(v);
        }
        IndexDomain::DataLength(v) => {
            out.kind = 5;
            out.payload = borrow(v);
        }
        IndexDomain::TileLength => out.kind = 6,
        IndexDomain::ViewLength(v) => {
            out.kind = 7;
            out.payload = borrow(v);
        }
        IndexDomain::ApplicationLength(v) => {
            out.kind = 8;
            out.first = borrow(v);
        }
        IndexDomain::Upper(v, d) => {
            out.kind = 9;
            out.first = borrow(v);
            out.distance = u64::from(*d);
        }
        IndexDomain::Room(v) => {
            out.kind = 10;
            out.first = borrow(v);
        }
        IndexDomain::Shifted(a, b, d) => {
            out.kind = 11;
            out.first = borrow(a);
            out.second = borrow(b);
            out.distance = u64::from(*d);
        }
        IndexDomain::Offset(v, d) => {
            out.kind = 12;
            out.first = borrow(v);
            out.distance = u64::from(*d);
        }
        IndexDomain::Lower(v, d) => {
            out.kind = 13;
            out.first = borrow(v);
            out.distance = u64::from(*d);
        }
        IndexDomain::Sum(a, b) => {
            out.kind = 14;
            out.first = borrow(a);
            out.second = borrow(b);
        }
        _ => {}
    }
    out
}
#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Output {
    Original = 0,
    Data,
    Tile,
    View,
    ApplicationData,
    Upper,
    Shifted,
    Sum,
    Offset,
    Room,
}
pub(super) unsafe fn materialize(kind: Output, v: Fields) -> IndexDomain {
    unsafe {
        match kind {
            Output::Original => *v.original.cast::<IndexDomain>(),
            Output::Data => IndexDomain::Data(*v.payload.cast::<usize>()),
            Output::Tile => IndexDomain::Tile,
            Output::View => IndexDomain::View(*v.payload.cast::<[ValueIdentity; 3]>()),
            Output::ApplicationData => {
                IndexDomain::ApplicationData(*v.first.cast::<ValueIdentity>())
            }
            Output::Upper => {
                IndexDomain::Upper(*v.first.cast::<ValueIdentity>(), v.distance as u32)
            }
            Output::Shifted => IndexDomain::Shifted(
                *v.first.cast::<ValueIdentity>(),
                *v.second.cast::<ValueIdentity>(),
                v.distance as u32,
            ),
            Output::Sum => IndexDomain::Sum(
                *v.first.cast::<ValueIdentity>(),
                *v.second.cast::<ValueIdentity>(),
            ),
            Output::Offset => {
                IndexDomain::Offset(*v.first.cast::<ValueIdentity>(), v.distance as u32)
            }
            Output::Room => IndexDomain::Room(*v.first.cast::<ValueIdentity>()),
        }
    }
}
struct LengthRelation<'a> {
    origins: [&'a [ValueIdentity]; 2],
    inputs: [btree_set::Iter<'a, IndexDomain>; 2],
    index: &'a Domains,
    length: &'a Domains,
}

#[repr(C)]
struct LengthRelationHost {
    next: unsafe extern "C" fn(Slot, u32) -> Fields,
    origin: unsafe extern "C" fn(Borrowed, u32, Borrowed) -> u32,
    resource: unsafe extern "C" fn(Borrowed, Fields) -> u32,
    range: unsafe extern "C" fn(Borrowed, Output, Fields, u32, u32) -> u32,
    sum: unsafe extern "C" fn(Borrowed, Borrowed, Borrowed) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::proof_length_relation"]
    fn b_length_relation(context: Slot, strict: u32, host: LengthRelationHost) -> u32;
}

unsafe extern "C" fn length_relation_next(context: Slot, side: u32) -> Fields {
    unsafe { frame::<LengthRelation<'_>>(context) }.inputs[side as usize]
        .next()
        .map(fields)
        .unwrap_or_default()
}

unsafe extern "C" fn length_relation_origin(context: Borrowed, side: u32, origin: Borrowed) -> u32 {
    let state = unsafe { &*context.cast::<LengthRelation<'_>>() };
    let identity = unsafe { &*origin.cast::<ValueIdentity>() };
    u32::from(state.origins[side as usize].contains(identity))
}

unsafe extern "C" fn length_relation_resource(context: Borrowed, resource: Fields) -> u32 {
    let state = unsafe { &*context.cast::<LengthRelation<'_>>() };
    let domain = unsafe { &*resource.original.cast::<IndexDomain>() };
    let identity = match *domain {
        IndexDomain::Data(binding) => ValueIdentity::DataLength(binding),
        IndexDomain::Tile => ValueIdentity::TileLength,
        IndexDomain::View(parts) => parts[2],
        _ => return 0,
    };
    u32::from(state.origins[1].contains(&identity))
}

unsafe extern "C" fn length_relation_range(
    context: Borrowed, kind: Output, row: Fields, min: u32, max: u32,
) -> u32 {
    let state = unsafe { &*context.cast::<LengthRelation<'_>>() };
    let start = unsafe { materialize(kind, Fields { distance: u64::from(min), ..row }) };
    let end = unsafe { materialize(kind, Fields { distance: u64::from(max), ..row }) };
    u32::from(state.index.range(start..=end).next().is_some())
}

unsafe extern "C" fn length_relation_sum(context: Borrowed, left: Borrowed, right: Borrowed) -> u32 {
    let state = unsafe { &*context.cast::<LengthRelation<'_>>() };
    let left = unsafe { *left.cast::<ValueIdentity>() };
    let right = unsafe { *right.cast::<ValueIdentity>() };
    u32::from(state.length.contains(&IndexDomain::Sum(left, right)))
}

pub(super) fn length_relation(
    (origins, index): (&[ValueIdentity], &Domains),
    (length_origins, length): (&[ValueIdentity], &Domains),
    strict: bool,
) -> bool {
    let mut relation = LengthRelation {
        origins: [origins, length_origins],
        inputs: [index.iter(), length.iter()],
        index,
        length,
    };
    unsafe {
        b_length_relation(
            ptr::from_mut(&mut relation).cast(),
            u32::from(strict),
            LengthRelationHost {
                next: length_relation_next,
                origin: length_relation_origin,
                resource: length_relation_resource,
                range: length_relation_range,
                sum: length_relation_sum,
            },
        ) != 0
    }
}


struct Below<'a> {
    input: btree_set::Iter<'a, IndexDomain>,
    output: Domains,
}
unsafe extern "C" fn below_next(s: Slot) -> Fields {
    unsafe { frame::<Below<'_>>(s) }
        .input
        .next()
        .map(fields)
        .unwrap_or_default()
}
unsafe extern "C" fn below_emit(s: Slot, kind: Output, value: Fields) {
    let value = unsafe { materialize(kind, value) };
    unsafe { frame::<Below<'_>>(s) }.output.insert(value);
}
#[repr(C)]
struct BelowHost {
    next: unsafe extern "C" fn(Slot) -> Fields,
    emit: unsafe extern "C" fn(Slot, Output, Fields),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::index_domains_below"]
    fn b_below(context: Slot, strict: u32, host: BelowHost);
}
pub(super) fn below(domains: &Domains, strict: bool) -> Domains {
    let mut c = Below {
        input: domains.iter(),
        output: Domains::new(),
    };
    unsafe {
        b_below(
            (&mut c as *mut Below<'_>).cast(),
            u32::from(strict),
            BelowHost {
                next: below_next,
                emit: below_emit,
            },
        )
    };
    c.output
}
#[repr(C)]
struct Bounds {
    lower: u64,
    upper: u64,
}
#[repr(C)]
struct Constant {
    present: u32,
    bits: u64,
}
struct Arithmetic<'a> {
    values: [&'a Value; 2],
    components: [usize; 2],
    inputs: [&'a Domains; 2],
    input: Option<btree_set::Iter<'a, IndexDomain>>,
    identities: [*mut ValueIdentity; 2],
    output: Domains,
    left: Domains,
    scratch: bool,
}
unsafe extern "C" fn select(s: Slot, side: u32) {
    let c = unsafe { frame::<Arithmetic<'_>>(s) };
    c.input = Some(c.inputs[side as usize].iter());
}
unsafe extern "C" fn next(s: Slot) -> Fields {
    unsafe { frame::<Arithmetic<'_>>(s) }
        .input
        .as_mut()
        .and_then(Iterator::next)
        .map(fields)
        .unwrap_or_default()
}
unsafe extern "C" fn emit(s: Slot, kind: Output, value: Fields) {
    let value = unsafe { materialize(kind, value) };
    let c = unsafe { frame::<Arithmetic<'_>>(s) };
    if c.scratch {
        c.left.insert(value);
    } else {
        c.output.insert(value);
    }
}
unsafe extern "C" fn same(s: Slot) -> u32 {
    let c = unsafe { frame::<Arithmetic<'_>>(s) };
    u32::from(c.values[0].same_component(c.components[0], c.values[1], c.components[1]))
}
unsafe extern "C" fn bounds(s: Slot, side: u32) -> Bounds {
    let c = unsafe { frame::<Arithmetic<'_>>(s) };
    let side = side as usize;
    let (lower, upper) = c.values[side].component_range(c.components[side]);
    Bounds { lower, upper }
}
unsafe extern "C" fn constant(s: Slot, side: u32) -> Constant {
    let c = unsafe { frame::<Arithmetic<'_>>(s) };
    let side = side as usize;
    match &c.values[side].constant {
        Some(v) => Constant {
            present: 1,
            bits: v[c.components[side]],
        },
        None => Constant {
            present: 0,
            bits: 0,
        },
    }
}
unsafe extern "C" fn nonzero(s: Slot, side: u32) -> u32 {
    let c = unsafe { frame::<Arithmetic<'_>>(s) };
    let side = side as usize;
    u32::from(c.values[side].component_nonzero(c.components[side]))
}
unsafe extern "C" fn shift_zero(s: Slot) -> u32 {
    let c = unsafe { frame::<Arithmetic<'_>>(s) };
    u32::from(
        c.values[1]
            .component_ranges(c.components[1])
            .shift_counts(Scalar::U32)
            .contains(0),
    )
}
unsafe extern "C" fn identity(s: Slot, side: u32) -> Borrowed {
    let c = unsafe { frame::<Arithmetic<'_>>(s) };
    let side = side as usize;
    let out = c.identities[side];
    unsafe { out.write(c.values[side].component_identity(c.components[side])) };
    out.cast()
}
unsafe extern "C" fn room(s: Slot, side: u32, id: Borrowed) -> u32 {
    let id = unsafe { *id.cast::<ValueIdentity>() };
    let c = unsafe { frame::<Arithmetic<'_>>(s) };
    u32::from(c.inputs[side as usize].contains(&IndexDomain::Room(id)))
}
unsafe extern "C" fn scratch(s: Slot, value: u32) {
    unsafe { frame::<Arithmetic<'_>>(s) }.scratch = value != 0;
}
unsafe extern "C" fn contains(s: Slot, kind: Output, value: Fields) -> u32 {
    let value = unsafe { materialize(kind, value) };
    u32::from(unsafe { frame::<Arithmetic<'_>>(s) }.left.contains(&value))
}
#[repr(C)]
struct ArithmeticHost {
    select: unsafe extern "C" fn(Slot, u32),
    next: unsafe extern "C" fn(Slot) -> Fields,
    emit: unsafe extern "C" fn(Slot, Output, Fields),
    same: unsafe extern "C" fn(Slot) -> u32,
    bounds: unsafe extern "C" fn(Slot, u32) -> Bounds,
    constant: unsafe extern "C" fn(Slot, u32) -> Constant,
    nonzero: unsafe extern "C" fn(Slot, u32) -> u32,
    shift_zero: unsafe extern "C" fn(Slot) -> u32,
    identity: unsafe extern "C" fn(Slot, u32) -> Borrowed,
    room: unsafe extern "C" fn(Slot, u32, Borrowed) -> u32,
    scratch: unsafe extern "C" fn(Slot, u32),
    contains: unsafe extern "C" fn(Slot, Output, Fields) -> u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::arithmetic_index_domains"]
    fn b_arithmetic(context: Slot, operation: u32, host: ArithmeticHost);
}
pub(super) fn arithmetic(op: &str, a: &Value, b: &Value, component: usize) -> Domains {
    let ai = component % a.regs.len();
    let bi = component % b.regs.len();
    let empty = Domains::new();
    let inputs = [
        a.proof.domains.get(ai).unwrap_or(&empty),
        b.proof.domains.get(bi).unwrap_or(&empty),
    ];
    let mut first = MaybeUninit::<ValueIdentity>::uninit();
    let mut second = MaybeUninit::<ValueIdentity>::uninit();
    let mut c = Arithmetic {
        values: [a, b],
        components: [ai, bi],
        inputs,
        input: None,
        identities: [first.as_mut_ptr(), second.as_mut_ptr()],
        output: Domains::new(),
        left: Domains::new(),
        scratch: false,
    };
    let operation = match op {
        "+" => 0,
        "-" => 1,
        "*" => 2,
        "/" => 3,
        "%" => 4,
        "&" => 5,
        "|" => 6,
        "^" => 7,
        "<<" => 8,
        ">>" => 9,
        "min" => 10,
        "max" => 11,
        _ => 12,
    };
    unsafe {
        b_arithmetic(
            (&mut c as *mut Arithmetic<'_>).cast(),
            operation,
            ArithmeticHost {
                select,
                next,
                emit,
                same,
                bounds,
                constant,
                nonzero,
                shift_zero,
                identity,
                room,
                scratch,
                contains,
            },
        )
    };
    c.output
}
