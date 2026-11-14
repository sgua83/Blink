//! B owns the complete implication algorithm. This adapter lends the actual
//! interval slice without relying on Rust tuple layout or copying its table.
use super::{Bits, Scalar};
use std::ffi::c_void;

#[repr(C)]
struct BitRange {
    lower: Bits,
    upper: Bits,
}
#[repr(C)]
struct BitPattern {
    mask: Bits,
    value: Bits,
}
#[repr(C)]
struct RangeQuery {
    order_bias: Bits,
    range: BitRange,
    known: BitPattern,
    required: BitPattern,
}
unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::covers_range"]
    fn covers_range(
        query: &RangeQuery,
        intervals: *mut c_void,
        count: u64,
        interval: unsafe extern "C" fn(*mut c_void, u64) -> BitRange,
    ) -> u32;
}
unsafe extern "C" fn interval(source: *mut c_void, index: u64) -> BitRange {
    let values = unsafe { &*source.cast::<&[(Bits, Bits)]>() };
    let (lower, upper) = values[index as usize];
    BitRange { lower, upper }
}
pub(super) fn covers(
    scalar: Scalar,
    (lower, upper): (Bits, Bits),
    (mask, value): (Bits, Bits),
    intervals: &[(Bits, Bits)],
    (required_mask, required_value): (Bits, Bits),
) -> bool {
    let query = RangeQuery {
        order_bias: scalar.order_bias(),
        range: BitRange { lower, upper },
        known: BitPattern { mask, value },
        required: BitPattern { mask: required_mask, value: required_value },
    };
    unsafe {
        covers_range(
            &query,
            (&intervals as *const &[(Bits, Bits)]).cast_mut().cast(),
            intervals.len() as u64,
            interval,
        ) != 0
    }
}
