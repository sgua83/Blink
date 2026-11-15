//! Numerical values cross synchronously; the original Proof owns every fact.
use super::{Bits, KnownBits, Scalar, bitwise::Operation};
use std::ffi::c_void;

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
struct Range {
    lower: Bits,
    upper: Bits,
}
type Reader = unsafe extern "C" fn(*const c_void, u64) -> Range;
unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::known_cast"]
    fn cast_value(
        source_mask: Bits,
        source_sign: Bits,
        target_mask: Bits,
        value: Pattern,
    ) -> Pattern;
    #[link_name = "bootstrap.scalar_fold::known_combined"]
    fn combine(
        target_mask: Bits,
        am: Bits,
        sign_a: Bits,
        a: Pattern,
        bm: Bits,
        sign_b: Bits,
        b: Pattern,
    ) -> Pattern;
    #[link_name = "bootstrap.scalar_fold::known_common"]
    fn common_value(target_mask: Bits, a: Pattern, bm: Bits, sign_b: Bits, b: Pattern) -> Pattern;
    #[link_name = "bootstrap.scalar_fold::known_bitwise"]
    fn binary(operation: Operation, a: Pattern, b: Pattern) -> Pattern;
    #[link_name = "bootstrap.scalar_fold::known_ranges"]
    fn read_ranges(
        mask: Bits,
        bias: Bits,
        context: *const c_void,
        count: u64,
        read: Reader,
    ) -> Pattern;
    #[link_name = "bootstrap.scalar_fold::known_shifted"]
    fn shift(
        mask: Bits,
        bias: Bits,
        left: u32,
        value: Pattern,
        context: *const c_void,
        count: u64,
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
pub(super) fn cast(source: KnownBits, target: Scalar) -> KnownBits {
    value(target, unsafe {
        cast_value(
            source.scalar.bit_mask(),
            source.scalar.order_bias(),
            target.bit_mask(),
            source.into(),
        )
    })
}
pub(super) fn combined(a: KnownBits, b: KnownBits, target: Scalar) -> KnownBits {
    value(target, unsafe {
        combine(
            target.bit_mask(),
            a.scalar.bit_mask(),
            a.scalar.order_bias(),
            a.into(),
            b.scalar.bit_mask(),
            b.scalar.order_bias(),
            b.into(),
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
unsafe extern "C" fn interval(context: *const c_void, index: u64) -> Range {
    let ranges = unsafe { &*context.cast::<&[(Bits, Bits)]>() };
    let (lower, upper) = ranges[index as usize];
    Range { lower, upper }
}
pub(super) fn ranges(scalar: Scalar, ranges: &[(Bits, Bits)]) -> Option<KnownBits> {
    present(scalar, unsafe {
        read_ranges(
            scalar.bit_mask(),
            scalar.order_bias(),
            (&ranges as *const &[(Bits, Bits)]).cast(),
            ranges.len() as u64,
            interval,
        )
    })
}
pub(super) fn shifted(source: KnownBits, left: bool, counts: &[(Bits, Bits)]) -> Option<KnownBits> {
    present(source.scalar, unsafe {
        shift(
            source.scalar.bit_mask(),
            source.scalar.order_bias(),
            u32::from(left),
            source.into(),
            (&counts as *const &[(Bits, Bits)]).cast(),
            counts.len() as u64,
            interval,
        )
    })
}
