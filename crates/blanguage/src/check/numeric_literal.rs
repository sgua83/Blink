//! The source owns all bytes; B owns the complete numeric decoding call.
//! Only the original semantic type and diagnostic are constructed here.
use crate::{Result, ast::{Source, Span}, ir::{Bits, Scalar, Ty}};

#[repr(u32)]
#[derive(Clone, Copy)]
enum Kind { F32, F64, U32, U64, I32, I64 }
impl Kind {
    fn scalar(self) -> Scalar {
        match self {
            Self::F32 => Scalar::F32, Self::F64 => Scalar::F64,
            Self::U32 => Scalar::U32, Self::U64 => Scalar::U64,
            Self::I32 => Scalar::I32, Self::I64 => Scalar::I64,
        }
    }
}
#[repr(u32)]
enum Error { None, Integer, Float, NonFinite, NegativeRange }
#[repr(C)]
struct Literal { bits: Bits, kind: Kind, error: Error }
unsafe extern "C" {
    #[link_name = "bootstrap.numeric_literal::decode"]
    fn decode(bytes: *const u8, length: u64, negative: u32) -> Literal;
}
pub(super) fn number(source: &Source, span: Span, text: &str, negative: bool) -> Result<(Ty, Bits)> {
    let value = unsafe { decode(text.as_ptr(), text.len() as u64, u32::from(negative)) };
    match value.error {
        Error::None => Ok((Ty::scalar(value.kind.scalar()), value.bits)),
        Error::Integer => source.fail(span, "B123", format!("非法 {} 常量 {text}", value.kind.scalar().name())),
        Error::Float => source.fail(span, "B121", format!("非法 {} 常量 {text}", value.kind.scalar().name())),
        Error::NonFinite => source.fail(span, "B122", format!("浮点常量超出有限 {} 范围", value.kind.scalar().name())),
        Error::NegativeRange => source.fail(span, "B180", "负整数字面量超出其有符号类型范围"),
    }
}
