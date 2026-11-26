//! B owns integer interval values and complete set rules; native retains the actual containers.
use super::{
    Bits, Fact, Facts, IndexDomain, IntegerExpression, IntegerOperation, IntegerQuery, Operand,
    Scalar, ScalarRanges, ValueIdentity,
};
use std::{collections::btree_set::Iter, ffi::c_void};

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Unary {
    BitNot,
    Negate,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Range {
    lower: Bits,
    upper: Bits,
}
impl From<(Bits, Bits)> for Range {
    fn from((lower, upper): (Bits, Bits)) -> Self {
        Self { lower, upper }
    }
}
#[repr(C)]
struct Wide {
    low: Bits,
    high: Bits,
}
impl From<i128> for Wide {
    fn from(value: i128) -> Self {
        let bits = value as u128;
        Self {
            low: bits as Bits,
            high: (bits >> 64) as Bits,
        }
    }
}
#[repr(C)]
struct Pieces {
    first: Range,
    second: Range,
    count: u32,
}
#[repr(C)]
struct TileSumLanes {
    minimum: u32,
    maximum: u32,
    has_nonzero: u32,
    nonzero: u32,
}
#[derive(Default)]
#[repr(C)]
struct TileLane {
    present: u32,
    lower: u32,
    upper: u32,
}
#[derive(Default)]
#[repr(C)]
struct TileResidue {
    period: Bits,
    first: u32,
    last: u32,
}
#[repr(C)]
struct TileNonzeroBound {
    present: u32,
    value: u32,
}

struct TileFacts<'a> {
    facts: &'a Facts,
    query: IntegerQuery<'a>,
    lanes: Iter<'a, Fact>,
    residues: Iter<'a, Fact>,
    global: ValueIdentity,
    lane: ValueIdentity,
    width: u32,
}

fn remainder(fact: &Fact) -> Option<(ValueIdentity, ValueIdentity, Bits)> {
    let Fact::Index(result, IndexDomain::Computed(IntegerExpression::Binary(
        IntegerOperation::Remainder, Scalar::U32, Operand::Value(source),
        Operand::Constant(Scalar::U32, period),
    ))) = *fact else { return None };
    Some((result, source, period))
}

unsafe extern "C" fn next_lane(context: *mut c_void) -> TileLane {
    let input = unsafe { &mut *context.cast::<TileFacts<'_>>() };
    for (result, source, period) in input.lanes.by_ref().filter_map(remainder) {
        if period != Bits::from(input.width) || input.query.root(source) != input.global {
            continue;
        }
        let (lower, upper) = input.query.bounds(result, Scalar::U32);
        input.lane = input.query.root(result);
        input.residues = input.facts.iter();
        return TileLane { present: 1, lower: lower as u32, upper: upper as u32 };
    }
    TileLane::default()
}

unsafe extern "C" fn next_residue(context: *mut c_void) -> TileResidue {
    let input = unsafe { &mut *context.cast::<TileFacts<'_>>() };
    for (result, source, period) in input.residues.by_ref().filter_map(remainder) {
        if input.query.root(source) == input.lane {
            let (first, last) = input.query.bounds(result, Scalar::U32);
            return TileResidue { period, first: first as u32, last: last as u32 };
        }
    }
    // A verified remainder has a nonzero divisor. Zero denotes the end of the facts.
    TileResidue::default()
}

fn decoded(value: Pieces) -> ScalarRanges {
    let pieces = [value.first, value.second];
    ScalarRanges::from_sorted_intervals(
        pieces[..value.count as usize]
            .iter()
            .map(|v| (v.lower, v.upper)),
    )
}
unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::integer_range_unary"]
    fn evaluate_unary(operation: Unary, width: u32, signed: u32, value: Range) -> Pieces;
    #[link_name = "bootstrap.scalar_fold::tile_nonzero_bound"]
    fn nonzero_bound(
        context: *mut c_void, width: u32, maximum: u32,
        next_lane: unsafe extern "C" fn(*mut c_void) -> TileLane,
        next_residue: unsafe extern "C" fn(*mut c_void) -> TileResidue,
    ) -> TileNonzeroBound;
    #[link_name = "bootstrap.scalar_fold::integer_range_tile_sum"]
    fn evaluate_tile_sum(width: u32, signed: u32, values: Range, lanes: TileSumLanes) -> Pieces;
    #[link_name = "bootstrap.scalar_fold::integer_range_project"]
    fn project_values(width: u32, bias: Bits, lower: Wide, upper: Wide) -> Pieces;
}
pub(super) fn unary(operation: Unary, scalar: Scalar, value: (Bits, Bits)) -> ScalarRanges {
    decoded(unsafe {
        evaluate_unary(
            operation,
            scalar.bit_width(),
            u32::from(scalar.is_signed()),
            value.into(),
        )
    })
}
pub(super) fn tile_nonzero_count<'a>(
    facts: &'a Facts, query: IntegerQuery<'a>, index: ValueIdentity, width: u32, maximum: u32,
) -> Option<u32> {
    let global = query.root(index);
    let mut input = TileFacts {
        facts,
        query,
        lanes: facts.iter(),
        residues: facts.iter(),
        global,
        lane: global,
        width,
    };
    let bound = unsafe {
        nonzero_bound(std::ptr::from_mut(&mut input).cast(), width, maximum, next_lane, next_residue)
    };
    (bound.present != 0).then_some(bound.value)
}

pub(super) fn tile_sum(
    scalar: Scalar, values: (Bits, Bits), (minimum, maximum): (u32, u32), nonzero: Option<u32>,
) -> ScalarRanges {
    let lanes = TileSumLanes {
        minimum,
        maximum,
        has_nonzero: u32::from(nonzero.is_some()),
        nonzero: nonzero.unwrap_or_default(),
    };
    decoded(unsafe {
        evaluate_tile_sum(scalar.bit_width(), u32::from(scalar.is_signed()), values.into(), lanes)
    })
}

pub(super) fn project((lower, upper): (i128, i128), bias: Bits, width: u32) -> ScalarRanges {
    decoded(unsafe { project_values(width, bias, lower.into(), upper.into()) })
}
