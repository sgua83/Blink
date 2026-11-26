//! Numerical values cross synchronously; the original Proof owns every fact.
use super::{Bits, Fact, Facts, IndexDomain, KnownBits, Scalar, ValueIdentity};
use crate::builtin_types::ScalarEncoding;
use crate::ir::scalar_ranges::{self, BitRange as Range};
use std::{collections::btree_set, ffi::c_void, slice};

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Operation {
    And,
    Or,
    Xor,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Pattern {
    mask: Bits,
    value: Bits,
}
impl From<KnownBits> for Pattern {
    fn from(value: KnownBits) -> Self {
        Self {
            mask: value.mask,
            value: value.value,
        }
    }
}
#[repr(C)]
struct KnownDomain {
    source_mask: Bits,
    source_sign: Bits,
    pattern: Pattern,
    present: u32,
}

impl KnownDomain {
    const END: Self = Self {
        source_mask: 0,
        source_sign: 0,
        pattern: Pattern { mask: 0, value: 0 },
        present: 0,
    };

    fn new(scalar: Scalar, mask: Bits, value: Bits) -> Self {
        Self {
            source_mask: scalar.bit_mask(),
            source_sign: scalar.order_bias(),
            pattern: Pattern { mask, value },
            present: 1,
        }
    }
}

#[repr(C)]
struct KnownBounds {
    range: Range,
    zero: u32,
}

#[repr(C)]
struct KnownProjection {
    pattern: Pattern,
    present: u32,
}

type Reader = unsafe extern "C" fn(*const c_void, usize) -> Range;
unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::known_cast_path"]
    fn cast_values(
        source: ScalarEncoding,
        targets: *const ScalarEncoding,
        count: usize,
        first_required: usize,
        value: Pattern,
    ) -> Pattern;
    #[link_name = "bootstrap.scalar_fold::known_common"]
    fn common_value(target_mask: Bits, a: Pattern, bm: Bits, sign_b: Bits, b: Pattern) -> Pattern;
    #[link_name = "bootstrap.scalar_fold::known_bitwise"]
    fn binary(operation: Operation, a: Pattern, b: Pattern) -> Pattern;
    #[link_name = "bootstrap.scalar_fold::known_proof"]
    fn read_proof(
        mask: Bits,
        bias: Bits,
        ranges: *const c_void,
        count: usize,
        read: Reader,
        domains: *mut c_void,
        next_domain: unsafe extern "C" fn(*mut c_void) -> KnownDomain,
    ) -> Pattern;
    #[link_name = "bootstrap.scalar_fold::known_facts"]
    fn read_facts(
        mask: Bits,
        bias: Bits,
        context: *mut c_void,
        count: u64,
        read_bound: unsafe extern "C" fn(*const c_void, u64) -> KnownBounds,
        next_domain: unsafe extern "C" fn(*mut c_void) -> KnownDomain,
    ) -> KnownProjection;
    #[link_name = "bootstrap.scalar_fold::known_shifted"]
    fn shift(
        mask: Bits,
        bias: Bits,
        left: u32,
        value: Pattern,
        context: *const c_void,
        count: usize,
        read: Reader,
    ) -> Pattern;
}
fn value(scalar: Scalar, result: Pattern) -> KnownBits {
    KnownBits {
        scalar,
        mask: result.mask,
        value: result.value,
    }
}
fn present(scalar: Scalar, result: Pattern) -> Option<KnownBits> {
    (result.mask != 0).then(|| value(scalar, result))
}
pub(super) fn cast_path<const N: usize>(
    source: KnownBits,
    targets: [Scalar; N],
    first_required: usize,
) -> Option<KnownBits> {
    let target = targets[N - 1];
    let encodings = targets.map(ScalarEncoding::from);
    present(target, unsafe {
        cast_values(
            source.scalar.into(),
            encodings.as_ptr(),
            N,
            first_required,
            source.into(),
        )
    })
}

pub(super) fn common(a: KnownBits, b: KnownBits) -> Option<KnownBits> {
    present(a.scalar, unsafe {
        common_value(
            a.scalar.bit_mask(),
            a.into(),
            b.scalar.bit_mask(),
            b.scalar.order_bias(),
            b.into(),
        )
    })
}
pub(super) fn bitwise(
    scalar: Scalar,
    operation: Operation,
    a: Option<KnownBits>,
    b: Option<KnownBits>,
) -> Option<KnownBits> {
    let unknown = Pattern { mask: 0, value: 0 };
    present(scalar, unsafe {
        binary(
            operation,
            a.map_or(unknown, Pattern::from),
            b.map_or(unknown, Pattern::from),
        )
    })
}
unsafe extern "C" fn next_domain(context: *mut c_void) -> KnownDomain {
    let domains = unsafe { &mut *context.cast::<btree_set::Iter<'_, IndexDomain>>() };
    for domain in domains {
        if let IndexDomain::KnownBits(scalar, mask, value) = *domain {
            return KnownDomain::new(scalar, mask, value);
        }
    }
    KnownDomain::END
}

pub(super) fn from_proof(
    scalar: Scalar,
    ranges: &[Range],
    domains: btree_set::Iter<'_, IndexDomain>,
) -> Option<KnownBits> {
    read_ranges(scalar, ranges.as_ptr().cast(), ranges.len(), super::range_sets::read_record, domains)
}

pub(super) fn from_pairs(
    scalar: Scalar,
    ranges: &[(Bits, Bits)],
    domains: btree_set::Iter<'_, IndexDomain>,
) -> Option<KnownBits> {
    read_ranges(scalar, std::ptr::from_ref(&ranges).cast(), ranges.len(), scalar_ranges::interval, domains)
}

fn read_ranges(
    scalar: Scalar,
    ranges: *const c_void,
    count: usize,
    read: Reader,
    mut domains: btree_set::Iter<'_, IndexDomain>,
) -> Option<KnownBits> {
    present(scalar, unsafe {
        read_proof(
            scalar.bit_mask(),
            scalar.order_bias(),
            ranges,
            count,
            read,
            std::ptr::from_mut(&mut domains).cast(),
            next_domain,
        )
    })
}

struct FactInput<'a> {
    sets: &'a [&'a Facts],
    groups: slice::Iter<'a, &'a Facts>,
    domains: Option<btree_set::Range<'a, Fact>>,
    identity: ValueIdentity,
    scalar: Scalar,
}

unsafe extern "C" fn fact_bounds(context: *const c_void, index: u64) -> KnownBounds {
    let input = unsafe { &*context.cast::<FactInput<'_>>() };
    let facts = input.sets[index as usize];
    let identity = input.identity;
    let scalar = input.scalar;
    let mask = scalar.bit_mask();
    let lower = match facts
        .range(Fact::Minimum(identity, scalar, 0)..=Fact::Minimum(identity, scalar, mask))
        .next_back()
    {
        Some(Fact::Minimum(_, _, bound)) => *bound,
        _ => 0,
    };
    let upper = match facts
        .range(Fact::Maximum(identity, scalar, 0)..=Fact::Maximum(identity, scalar, mask))
        .next()
    {
        Some(Fact::Maximum(_, _, bound)) => *bound,
        _ => mask,
    };
    KnownBounds {
        range: Range { lower, upper },
        zero: u32::from(facts.contains(&Fact::Zero(identity))),
    }
}

unsafe extern "C" fn next_fact_domain(context: *mut c_void) -> KnownDomain {
    let input = unsafe { &mut *context.cast::<FactInput<'_>>() };
    let identity = input.identity;
    let scalar = input.scalar;
    loop {
        match input.domains.as_mut().and_then(Iterator::next) {
            Some(Fact::Index(_, IndexDomain::KnownBits(source, mask, value))) => {
                return KnownDomain::new(*source, *mask, *value);
            }
            Some(_) => continue,
            None => {}
        }
        let Some(facts) = input.groups.next() else {
            return KnownDomain::END;
        };
        input.domains = Some(facts.range(
            Fact::Index(identity, IndexDomain::KnownBits(scalar, 0, 0))
                ..=Fact::Index(identity, IndexDomain::KnownBits(scalar, Bits::MAX, Bits::MAX)),
        ));
    }
}

pub(super) fn from_facts(scalar: Scalar, identity: ValueIdentity, sets: &[&Facts]) -> Option<KnownBits> {
    let mut input = FactInput {
        sets,
        groups: sets.iter(),
        domains: None,
        identity,
        scalar,
    };
    let result = unsafe {
        read_facts(
            scalar.bit_mask(),
            scalar.order_bias(),
            std::ptr::from_mut(&mut input).cast(),
            sets.len() as u64,
            fact_bounds,
            next_fact_domain,
        )
    };
    (result.present != 0).then(|| value(scalar, result.pattern))
}

pub(super) fn shifted(source: KnownBits, left: bool, counts: &[Range]) -> Option<KnownBits> {
    present(source.scalar, unsafe {
        shift(
            source.scalar.bit_mask(),
            source.scalar.order_bias(),
            u32::from(left),
            source.into(),
            counts.as_ptr().cast(),
            counts.len(),
            super::range_sets::read_record,
        )
    })
}
