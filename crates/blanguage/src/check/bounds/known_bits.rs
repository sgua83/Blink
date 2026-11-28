//! Numerical values cross synchronously; the original Proof owns every fact.
use super::{Bits, Facts, IndexDomain, KnownBits, Scalar, ValueIdentity};
use crate::ir::scalar_ranges::{self, BitRange as Range};
use std::ffi::c_void;

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
struct KnownProjection {
    pattern: Pattern,
    present: u32,
}

type Reader = unsafe extern "C" fn(*const c_void, usize) -> Range;
unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::known_cast_path"]
    fn cast_values(
        source: *const Scalar,
        targets: *const Scalar,
        count: usize,
        first_required: usize,
        value: Pattern,
    ) -> Pattern;
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
    #[link_name = "bootstrap.common_facts::known_fact_values"]
    fn read_facts(sets: *const &Facts, count: usize, identity: &ValueIdentity, scalar: &Scalar) -> KnownProjection;
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
    present(target, unsafe {
        cast_values(
            &source.scalar,
            targets.as_ptr(),
            N,
            first_required,
            source.into(),
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
    let domains = unsafe { &mut *context.cast::<super::domain_store::Iter<'_>>() };
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
    domains: super::domain_store::Iter<'_>,
) -> Option<KnownBits> {
    read_ranges(scalar, ranges.as_ptr().cast(), ranges.len(), super::range_sets::read_record, domains)
}

pub(super) fn from_pairs(
    scalar: Scalar,
    ranges: &[(Bits, Bits)],
    domains: super::domain_store::Iter<'_>,
) -> Option<KnownBits> {
    read_ranges(scalar, std::ptr::from_ref(&ranges).cast(), ranges.len(), scalar_ranges::interval, domains)
}

fn read_ranges(
    scalar: Scalar,
    ranges: *const c_void,
    count: usize,
    read: Reader,
    mut domains: super::domain_store::Iter<'_>,
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


pub(super) fn from_facts(scalar: Scalar, identity: ValueIdentity, sets: &[&Facts]) -> Option<KnownBits> {
    let result = unsafe { read_facts(sets.as_ptr(), sets.len(), &identity, &scalar) };
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
