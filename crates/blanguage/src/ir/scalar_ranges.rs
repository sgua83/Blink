//! Synchronous access to the intervals held by the original SDK containers.
use super::{Bits, Scalar};
use std::{borrow::Cow, ffi::c_void, ptr};

pub(crate) type Pair = (Bits, Bits);
type Read = unsafe extern "C" fn(*const c_void, usize) -> BitRange;
type Write = unsafe extern "C" fn(*mut c_void, usize, BitRange);
type Push = unsafe extern "C" fn(*mut c_void, BitRange);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct BitRange {
    pub(crate) lower: Bits,
    pub(crate) upper: Bits,
}

impl From<Pair> for BitRange {
    fn from((lower, upper): Pair) -> Self {
        Self { lower, upper }
    }
}

impl From<BitRange> for Pair {
    fn from(range: BitRange) -> Self {
        (range.lower, range.upper)
    }
}

#[repr(C)]
pub(crate) struct BitPattern {
    mask: Bits,
    value: Bits,
}

impl From<Pair> for BitPattern {
    fn from((mask, value): Pair) -> Self {
        Self { mask, value }
    }
}

#[repr(C)]
struct RangeQuery {
    order_bias: Bits,
    range: BitRange,
    known: BitPattern,
    required: BitPattern,
}

#[repr(C)]
pub(crate) struct RangeInput {
    values: *const c_void,
    count: usize,
    get: Read,
}

impl RangeInput {
    pub(crate) fn borrowed(values: &&[Pair]) -> Self {
        Self { values: ptr::from_ref(values).cast(), count: values.len(), get: interval }
    }

    #[cfg(feature = "compiler")]
    pub(crate) fn records(values: &[BitRange], get: Read) -> Self {
        Self { values: values.as_ptr().cast(), count: values.len(), get }
    }
}

unsafe extern "C" {
    #[link_name = "codec.scalar_ranges::covers_range"]
    fn covers_range(query: &RangeQuery, intervals: *const c_void, count: usize, get: Read) -> u32;
    #[link_name = "codec.scalar_ranges::element_ranges_within"]
    fn element_ranges_within(actual: RangeInput, allowed: RangeInput, order_bias: Bits,
        known: BitPattern, required: BitPattern) -> u32;
    #[cfg(feature = "compiler")]
    #[link_name = "codec.scalar_ranges::intersect_ranges"]
    pub(crate) fn intersect_ranges(left: RangeInput, right: RangeInput, output: *mut c_void, push: Push) -> usize;
    #[link_name = "codec.scalar_ranges::intersect_element_ranges"]
    fn intersect_element_ranges(left: RangeInput, right: RangeInput, write_left: Write,
        write_right: Write, left_bits: BitPattern, right_bits: BitPattern, output: *mut c_void,
        push: Push) -> BitPattern;
}

pub(crate) unsafe extern "C" fn interval(source: *const c_void, index: usize) -> BitRange {
    let ranges = unsafe { &*source.cast::<&[Pair]>() };
    ranges[index].into()
}

unsafe extern "C" fn read_left(source: *const c_void, index: usize) -> BitRange {
    let ranges = unsafe { &*source.cast::<&mut [Pair]>() };
    ranges[index].into()
}

unsafe extern "C" fn write_left(source: *mut c_void, index: usize, value: BitRange) {
    let ranges = unsafe { &mut *source.cast::<&mut [Pair]>() };
    ranges[index] = value.into();
}

unsafe extern "C" fn read_right(source: *const c_void, index: usize) -> BitRange {
    let ranges = unsafe { &*source.cast::<Cow<'_, [Pair]>>() };
    ranges[index].into()
}

unsafe extern "C" fn write_right(source: *mut c_void, index: usize, value: BitRange) {
    let ranges = unsafe { &mut *source.cast::<Cow<'_, [Pair]>>() };
    ranges.to_mut()[index] = value.into();
}

pub(crate) unsafe extern "C" fn push(source: *mut c_void, value: BitRange) {
    unsafe { &mut *source.cast::<Vec<Pair>>() }.push(value.into());
}

pub(super) fn covers(scalar: Scalar, range: Pair, known: Pair, intervals: &[Pair], required: Pair) -> bool {
    let query = RangeQuery { order_bias: scalar.order_bias(), range: range.into(),
        known: known.into(), required: required.into() };
    let input = RangeInput::borrowed(&intervals);
    unsafe { covers_range(&query, input.values, input.count, input.get) != 0 }
}

pub(super) fn within(scalar: Scalar, actual: &[Pair], allowed: &[Pair], known: Pair, required: Pair) -> bool {
    unsafe {
        element_ranges_within(RangeInput::borrowed(&actual), RangeInput::borrowed(&allowed),
            scalar.order_bias(), known.into(), required.into()) != 0
    }
}

pub(super) fn intersect(mut left: &mut [Pair], right: &[Pair], left_bits: Pair, right_bits: Pair) -> (Vec<Pair>, Pair) {
    let mut right = Cow::Borrowed(right);
    let mut output = Vec::new();
    let lhs = RangeInput { count: left.len(), values: ptr::from_mut(&mut left).cast(), get: read_left };
    let rhs = RangeInput { count: right.len(), values: ptr::from_mut(&mut right).cast(), get: read_right };
    let bits = unsafe {
        intersect_element_ranges(lhs, rhs, write_left, write_right, left_bits.into(), right_bits.into(),
            ptr::from_mut(&mut output).cast(), push)
    };
    (output, (bits.mask, bits.value))
}
