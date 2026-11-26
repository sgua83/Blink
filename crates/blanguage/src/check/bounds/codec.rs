//! Borrows numerical proof values and their existing codec buffers for B.
use super::*;
use crate::codec::contracts::{self, Address, Bytes, Native, Slot, address, append, slot};
use crate::module::codec::ReadStatus;

type Append = unsafe extern "C" fn(Slot, *const u8, usize);

macro_rules! numeric_codec {
    ($ty:ty, $empty:expr, $write:ident, $read:ident, $writer:literal, $reader:literal) => {
        unsafe extern "C" {
            #[link_name = $writer]
            fn $write(source: Address, output: Slot, append: Append);
            #[link_name = $reader]
            fn $read(input: Bytes, position: *mut usize, output: Slot) -> ReadStatus;
        }
        impl Native for $ty {
            fn empty() -> Self {
                $empty
            }
            unsafe fn write(&self, output: Slot) {
                unsafe { $write(address(self), output, append) }
            }
            unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
                let (bytes, position) = unsafe { contracts::input(context) };
                unsafe { $read(bytes, position, slot(output)) }
            }
        }
        contracts::codec!($ty);
    };
}

numeric_codec!(
    ValueIdentity,
    ValueIdentity::Register(0),
    write_identity, read_identity,
    "bootstrap.contract_codec::numeric_write_identity_key",
    "bootstrap.contract_codec::numeric_read_identity_key"
);
numeric_codec!(
    Operand,
    Operand::Value(ValueIdentity::Register(0)),
    write_operand, read_operand,
    "bootstrap.contract_codec::numeric_write_operand_key",
    "bootstrap.contract_codec::numeric_read_operand_key"
);
numeric_codec!(
    Selector,
    Selector::Direct(Operand::Value(ValueIdentity::Register(0))),
    write_selection, read_selection,
    "bootstrap.contract_codec::numeric_write_selection_key",
    "bootstrap.contract_codec::numeric_read_selection_key"
);
numeric_codec!(
    IntegerOperation,
    IntegerOperation::Add,
    write_integer, read_integer,
    "bootstrap.contract_codec::numeric_write_integer_key",
    "bootstrap.contract_codec::numeric_read_integer_key"
);
numeric_codec!(
    IntegerExpression,
    IntegerExpression::Binary(
        IntegerOperation::Add, Scalar::F32,
        Operand::Value(ValueIdentity::Register(0)), Operand::Value(ValueIdentity::Register(0)),
    ),
    write_expression, read_expression,
    "bootstrap.contract_codec::numeric_write_expression_key",
    "bootstrap.contract_codec::numeric_read_expression_key"
);
numeric_codec!(
    Comparison,
    Comparison::Equal,
    write_comparison, read_comparison,
    "bootstrap.contract_codec::numeric_write_comparison_key",
    "bootstrap.contract_codec::numeric_read_comparison_key"
);
numeric_codec!(
    FloatingForm,
    FloatingForm::Cast,
    write_floating_form, read_floating_form,
    "bootstrap.contract_codec::numeric_write_floating_form_key",
    "bootstrap.contract_codec::numeric_read_floating_form_key"
);
numeric_codec!(
    FloatingBound,
    FloatingBound {
        source: ValueIdentity::Register(0), lower: 0, upper: 0,
        terms: [0; 3], scalar: Scalar::F32, form: FloatingForm::Cast,
    },
    write_floating, read_floating,
    "bootstrap.contract_codec::numeric_write_floating_key",
    "bootstrap.contract_codec::numeric_read_floating_key"
);
numeric_codec!(
    ScalarPredicate,
    ScalarPredicate {
        op: Comparison::Equal, scalar: Scalar::F32,
        a: Operand::Value(ValueIdentity::Register(0)), b: Operand::Value(ValueIdentity::Register(0)),
    },
    write_predicate, read_predicate,
    "bootstrap.contract_codec::numeric_write_predicate_key",
    "bootstrap.contract_codec::numeric_read_predicate_key"
);
numeric_codec!(
    IndexDomain,
    IndexDomain::Tile,
    write_domain, read_domain,
    "bootstrap.contract_codec::numeric_write_domain",
    "bootstrap.contract_codec::numeric_read_domain"
);
numeric_codec!(
    Fact,
    Fact::NonZero(ValueIdentity::TileLength),
    write_fact, read_fact,
    "bootstrap.contract_codec::numeric_write_fact",
    "bootstrap.contract_codec::numeric_read_fact"
);

unsafe extern "C" {
    #[link_name = "bootstrap.contract_codec::numeric_write_ranges"]
    fn write_ranges(intervals: *const BitRange, count: usize, output: Slot, append: Append);
    #[link_name = "bootstrap.contract_codec::numeric_read_ranges"]
    fn read_ranges(input: Bytes, position: *mut usize, output: Slot) -> ReadStatus;
}

impl Native for ScalarRanges {
    fn empty() -> Self {
        Self::empty()
    }

    unsafe fn write(&self, output: Slot) {
        unsafe { write_ranges(self.0.as_ptr(), self.0.len(), output, append) }
    }

    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        let (bytes, position) = unsafe { contracts::input(context) };
        unsafe { read_ranges(bytes, position, slot(&mut output.0)) }
    }
}
contracts::codec!(ScalarRanges);

pub(super) mod conditions;
