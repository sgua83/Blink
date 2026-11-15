//! B evaluates the original ordered integer interval extrema; the original
//! ScalarRanges and Proof containers continue to own the result.
use super::{Bits, Scalar};
#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Operation { And, Or, Xor }
#[repr(C)]
struct Range { lower: Bits, upper: Bits }
unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::integer_bitwise_range"]
    fn evaluate(operation: Operation, width: u32, order_bias: Bits, left: Range, right: Range) -> Range;
}
pub(super) fn range((al, ah): (Bits, Bits), (bl, bh): (Bits, Bits), scalar: Scalar,
    operation: Operation) -> (Bits, Bits)
{
    let result = unsafe { evaluate(operation, scalar.bit_width(), scalar.order_bias(),
        Range { lower: al, upper: ah }, Range { lower: bl, upper: bh }) };
    (result.lower, result.upper)
}
