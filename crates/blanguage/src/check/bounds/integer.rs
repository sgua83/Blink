//! B owns integer interval values and complete set rules; native retains the actual containers.
use super::{Bits, Scalar, ScalarRanges};

#[derive(Clone, Copy)]
#[repr(u32)]
enum Operation {
    Minimum,
    Maximum,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    ShiftLeft,
    ShiftRight,
}
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
fn decoded(value: Pieces) -> ScalarRanges {
    let pieces = [value.first, value.second];
    ScalarRanges::from_sorted_intervals(
        pieces[..value.count as usize]
            .iter()
            .map(|v| (v.lower, v.upper)),
    )
}
unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::integer_range"]
    fn evaluate(operation: Operation, width: u32, signed: u32, left: Range, right: Range)
    -> Pieces;
    #[link_name = "bootstrap.scalar_fold::integer_range_unary"]
    fn evaluate_unary(operation: Unary, width: u32, signed: u32, value: Range) -> Pieces;
    #[link_name = "bootstrap.scalar_fold::integer_range_tile_sum"]
    fn evaluate_tile_sum(width: u32, signed: u32, values: Range, lanes: TileSumLanes) -> Pieces;
    #[link_name = "bootstrap.scalar_fold::integer_range_project"]
    fn project_values(width: u32, bias: Bits, lower: Wide, upper: Wide) -> Pieces;
}
pub(super) fn arithmetic(
    op: &str,
    scalar: Scalar,
    left: (Bits, Bits),
    right: (Bits, Bits),
) -> ScalarRanges {
    let operation = match op {
        "min" => Operation::Minimum,
        "max" => Operation::Maximum,
        "+" => Operation::Add,
        "-" => Operation::Subtract,
        "*" => Operation::Multiply,
        "/" => Operation::Divide,
        "%" => Operation::Remainder,
        "<<" => Operation::ShiftLeft,
        ">>" => Operation::ShiftRight,
        _ => return ScalarRanges::interval(0, scalar.bit_mask()),
    };
    decoded(unsafe {
        evaluate(
            operation,
            scalar.bit_width(),
            u32::from(scalar.is_signed()),
            left.into(),
            right.into(),
        )
    })
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
