//! Physical projections of the original numerical contract values.
//! B owns all wire tags, widths, field order and interval traversal.
use super::*;
use crate::module::codec::{Decode, Encode, ReadStatus, Reader, Result, Writer};
use std::{ffi::c_void, ptr};
#[repr(u32)]
#[derive(Clone, Copy, Default)]
enum IntegerKind {
    #[default]
    Add = 0,
    Subtract = 1,
    Multiply = 2,
    Divide = 3,
    Remainder = 4,
    And = 5,
    Or = 6,
    Xor = 7,
    ShiftLeft = 8,
    ShiftRight = 9,
}
#[repr(u32)]
#[derive(Clone, Copy, Default)]
enum ComparisonKind {
    #[default]
    Equal = 0,
    NotEqual = 1,
    Less = 2,
    Greater = 3,
    LessEqual = 4,
    GreaterEqual = 5,
}
#[repr(u32)]
#[derive(Clone, Copy, Default)]
enum FloatingKind {
    #[default]
    Cast = 0,
    Upper = 1,
}
#[repr(u32)]
#[derive(Clone, Copy, Default)]
enum ScalarTag {
    #[default]
    F32 = 0,
    U32 = 1,
    I32 = 2,
    Bool = 3,
    U64 = 4,
    I64 = 5,
    F64 = 6,
    U8 = 7,
    I8 = 8,
    U16 = 9,
    I16 = 10,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NumericIdentity {
    kind: u32,
    first: u64,
    second: u64,
    third: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NumericOperand {
    kind: u32,
    identity: NumericIdentity,
    scalar: ScalarTag,
    bits: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NumericSelection {
    affine: u32,
    operand: NumericOperand,
    base: NumericIdentity,
    scale_low: u64,
    scale_high: u64,
    offset_low: u64,
    offset_high: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NumericExpression {
    form: u32,
    operation: IntegerKind,
    scalar: ScalarTag,
    left: NumericOperand,
    right: NumericOperand,
    selection: NumericSelection,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NumericFloating {
    source: NumericIdentity,
    lower: u64,
    upper: u64,
    terms: [u64; 3],
    scalar: ScalarTag,
    form: FloatingKind,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NumericPredicate {
    operation: ComparisonKind,
    scalar: ScalarTag,
    left: NumericOperand,
    right: NumericOperand,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NumericInterval {
    lower: u64,
    upper: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Bytes {
    data: *const u8,
    count: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Span {
    data: *const c_void,
    count: usize,
    stride: usize,
}
#[repr(C)]
struct RangeWriter {
    append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    interval: unsafe extern "C" fn(*const c_void) -> NumericInterval,
}
trait NativeValue: Copy {
    type Frame: Default;
    fn into_frame(self) -> Self::Frame;
    fn from_frame(value: Self::Frame) -> Self;
}
impl NativeValue for Scalar {
    type Frame = ScalarTag;
    fn into_frame(self) -> ScalarTag {
        match self {
            Self::F32 => ScalarTag::F32,
            Self::U32 => ScalarTag::U32,
            Self::I32 => ScalarTag::I32,
            Self::Bool => ScalarTag::Bool,
            Self::U64 => ScalarTag::U64,
            Self::I64 => ScalarTag::I64,
            Self::F64 => ScalarTag::F64,
            Self::U8 => ScalarTag::U8,
            Self::I8 => ScalarTag::I8,
            Self::U16 => ScalarTag::U16,
            Self::I16 => ScalarTag::I16,
        }
    }
    fn from_frame(value: ScalarTag) -> Self {
        match value {
            ScalarTag::F32 => Self::F32,
            ScalarTag::U32 => Self::U32,
            ScalarTag::I32 => Self::I32,
            ScalarTag::Bool => Self::Bool,
            ScalarTag::U64 => Self::U64,
            ScalarTag::I64 => Self::I64,
            ScalarTag::F64 => Self::F64,
            ScalarTag::U8 => Self::U8,
            ScalarTag::I8 => Self::I8,
            ScalarTag::U16 => Self::U16,
            ScalarTag::I16 => Self::I16,
        }
    }
}
macro_rules! physical_frame {
    ($ty:ty, $frame:ty, $capture:ident, $materialize:ident, $reader:literal, $writer:literal) => {
        unsafe extern "C" {
            #[link_name = $reader]
            fn $capture(value: *const c_void) -> $frame;
            #[link_name = $writer]
            fn $materialize(value: $frame, output: *mut c_void);
        }

        impl NativeValue for $ty {
            type Frame = $frame;

            fn into_frame(self) -> Self::Frame {
                unsafe { $capture(ptr::from_ref(&self).cast()) }
            }

            fn from_frame(value: Self::Frame) -> Self {
                let mut output = std::mem::MaybeUninit::<Self>::uninit();
                unsafe {
                    $materialize(value, output.as_mut_ptr().cast());
                    output.assume_init()
                }
            }
        }
    };
}

physical_frame!(
    ValueIdentity, NumericIdentity, capture_identity, materialize_identity,
    "bootstrap.fact_key::capture_key_identity", "bootstrap.fact_key::materialize_key_identity"
);

impl NativeValue for Selector {
    type Frame = NumericSelection;

    fn into_frame(self) -> NumericSelection {
        match self {
            Self::Direct(operand) => NumericSelection {
                operand: operand.into_frame(),
                ..Default::default()
            },
            Self::Prefix(length) => {
                let (base, scale_low, offset_low) = match length {
                    Operand::Value(id) => (id.into_frame(), 1, 0),
                    Operand::Constant(_, bits) => (NumericIdentity::default(), 0, bits),
                };
                NumericSelection {
                    affine: 2,
                    base,
                    scale_low,
                    offset_low,
                    ..Default::default()
                }
            },
            Self::Affine { base, scale, offset } => NumericSelection {
                affine: 1,
                base: base.into_frame(),
                scale_low: scale as u64,
                scale_high: (scale >> 64) as u64,
                offset_low: offset as u64,
                offset_high: (offset >> 64) as u64,
                ..Default::default()
            },
        }
    }

    fn from_frame(value: NumericSelection) -> Self {
        if value.affine == 0 {
            return Self::Direct(Operand::from_frame(value.operand));
        }
        if value.affine == 2 {
            return Self::Prefix(if value.scale_low == 0 && value.scale_high == 0 {
                Operand::Constant(Scalar::U32, value.offset_low)
            } else {
                Operand::Value(ValueIdentity::from_frame(value.base))
            });
        }
        Self::Affine {
            base: ValueIdentity::from_frame(value.base),
            scale: ((u128::from(value.scale_high) << 64) | u128::from(value.scale_low)) as i128,
            offset: ((u128::from(value.offset_high) << 64) | u128::from(value.offset_low)) as i128,
        }
    }
}

physical_frame!(
    Operand, NumericOperand, capture_operand, materialize_operand,
    "bootstrap.fact_key::capture_key_operand", "bootstrap.fact_key::materialize_key_operand"
);

impl NativeValue for IntegerOperation {
    type Frame = IntegerKind;
    fn into_frame(self) -> IntegerKind {
        match self {
            Self::Add => IntegerKind::Add,
            Self::Subtract => IntegerKind::Subtract,
            Self::Multiply => IntegerKind::Multiply,
            Self::Divide => IntegerKind::Divide,
            Self::Remainder => IntegerKind::Remainder,
            Self::And => IntegerKind::And,
            Self::Or => IntegerKind::Or,
            Self::Xor => IntegerKind::Xor,
            Self::ShiftLeft => IntegerKind::ShiftLeft,
            Self::ShiftRight => IntegerKind::ShiftRight,
        }
    }
    fn from_frame(value: IntegerKind) -> Self {
        match value {
            IntegerKind::Add => Self::Add,
            IntegerKind::Subtract => Self::Subtract,
            IntegerKind::Multiply => Self::Multiply,
            IntegerKind::Divide => Self::Divide,
            IntegerKind::Remainder => Self::Remainder,
            IntegerKind::And => Self::And,
            IntegerKind::Or => Self::Or,
            IntegerKind::Xor => Self::Xor,
            IntegerKind::ShiftLeft => Self::ShiftLeft,
            IntegerKind::ShiftRight => Self::ShiftRight,
        }
    }
}
impl NativeValue for IntegerExpression {
    type Frame = NumericExpression;
    fn into_frame(self) -> NumericExpression {
        match self {
            Self::Binary(v0, v1, v2, v3) => {
                let mut value = NumericExpression::default();
                value.operation = v0.into_frame();
                value.scalar = v1.into_frame();
                value.left = v2.into_frame();
                value.right = v3.into_frame();
                value
            }
            Self::Selection(index) => NumericExpression {
                form: 1,
                selection: index.0.into_frame(),
                ..Default::default()
            },
        }
    }
    fn from_frame(value: NumericExpression) -> Self {
        if value.form == 1 { return Self::selection(Selector::from_frame(value.selection)); }
        Self::Binary(
            IntegerOperation::from_frame(value.operation),
            Scalar::from_frame(value.scalar),
            Operand::from_frame(value.left),
            Operand::from_frame(value.right),
        )
    }
}
impl NativeValue for Comparison {
    type Frame = ComparisonKind;
    fn into_frame(self) -> ComparisonKind {
        match self {
            Self::Equal => ComparisonKind::Equal,
            Self::NotEqual => ComparisonKind::NotEqual,
            Self::Less => ComparisonKind::Less,
            Self::Greater => ComparisonKind::Greater,
            Self::LessEqual => ComparisonKind::LessEqual,
            Self::GreaterEqual => ComparisonKind::GreaterEqual,
        }
    }
    fn from_frame(value: ComparisonKind) -> Self {
        match value {
            ComparisonKind::Equal => Self::Equal,
            ComparisonKind::NotEqual => Self::NotEqual,
            ComparisonKind::Less => Self::Less,
            ComparisonKind::Greater => Self::Greater,
            ComparisonKind::LessEqual => Self::LessEqual,
            ComparisonKind::GreaterEqual => Self::GreaterEqual,
        }
    }
}
impl NativeValue for FloatingForm {
    type Frame = FloatingKind;
    fn into_frame(self) -> FloatingKind {
        match self {
            Self::Cast => FloatingKind::Cast,
            Self::Upper => FloatingKind::Upper,
        }
    }
    fn from_frame(value: FloatingKind) -> Self {
        match value {
            FloatingKind::Cast => Self::Cast,
            FloatingKind::Upper => Self::Upper,
        }
    }
}
impl NativeValue for FloatingBound {
    type Frame = NumericFloating;
    fn into_frame(self) -> NumericFloating {
        NumericFloating {
            source: self.source.into_frame(),
            lower: self.lower,
            upper: self.upper,
            terms: self.terms,
            scalar: self.scalar.into_frame(),
            form: self.form.into_frame(),
        }
    }
    fn from_frame(value: NumericFloating) -> Self {
        Self {
            source: ValueIdentity::from_frame(value.source),
            lower: value.lower,
            upper: value.upper,
            terms: value.terms,
            scalar: Scalar::from_frame(value.scalar),
            form: FloatingForm::from_frame(value.form),
        }
    }
}
physical_frame!(
    ScalarPredicate, NumericPredicate, capture_predicate, materialize_predicate,
    "bootstrap.fact_key::capture_key_predicate", "bootstrap.fact_key::materialize_key_predicate"
);

unsafe extern "C" {
    #[link_name = "bootstrap.contract_codec::numeric_write_identity"]
    fn write_identity(
        value: NumericIdentity,
        output: *mut c_void,
        append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::numeric_read_identity"]
    fn read_identity(
        input: Bytes,
        position: *mut usize,
        output: *mut NumericIdentity,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_write_selection"]
    fn write_selection(
        value: NumericSelection,
        output: *mut c_void,
        append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::numeric_read_selection"]
    fn read_selection(input: Bytes, position: *mut usize, output: *mut NumericSelection) -> ReadStatus;

    #[link_name = "bootstrap.contract_codec::numeric_write_operand"]
    fn write_operand(
        value: NumericOperand,
        output: *mut c_void,
        append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::numeric_read_operand"]
    fn read_operand(input: Bytes, position: *mut usize, output: *mut NumericOperand) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_write_integer"]
    fn write_integer(
        value: IntegerKind,
        output: *mut c_void,
        append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::numeric_read_integer"]
    fn read_integer(input: Bytes, position: *mut usize, output: *mut IntegerKind) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_write_expression"]
    fn write_expression(
        value: NumericExpression,
        output: *mut c_void,
        append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::numeric_read_expression"]
    fn read_expression(
        input: Bytes,
        position: *mut usize,
        output: *mut NumericExpression,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_write_comparison"]
    fn write_comparison(
        value: ComparisonKind,
        output: *mut c_void,
        append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::numeric_read_comparison"]
    fn read_comparison(
        input: Bytes,
        position: *mut usize,
        output: *mut ComparisonKind,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_write_floating_form"]
    fn write_floating_form(
        value: FloatingKind,
        output: *mut c_void,
        append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::numeric_read_floating_form"]
    fn read_floating_form(
        input: Bytes,
        position: *mut usize,
        output: *mut FloatingKind,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_write_floating"]
    fn write_floating(
        value: NumericFloating,
        output: *mut c_void,
        append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::numeric_read_floating"]
    fn read_floating(
        input: Bytes,
        position: *mut usize,
        output: *mut NumericFloating,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_write_predicate"]
    fn write_predicate(
        value: NumericPredicate,
        output: *mut c_void,
        append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    );
    #[link_name = "bootstrap.contract_codec::numeric_read_predicate"]
    fn read_predicate(
        input: Bytes,
        position: *mut usize,
        output: *mut NumericPredicate,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_write_ranges"]
    fn write_ranges(values: Span, output: *mut c_void, host: RangeWriter);
    #[link_name = "bootstrap.contract_codec::numeric_read_ranges"]
    fn read_ranges(
        input: Bytes,
        position: *mut usize,
        output: *mut c_void,
        push: unsafe extern "C" fn(*mut c_void, u64, u64),
    ) -> ReadStatus;
}
unsafe extern "C" fn append(output: *mut c_void, data: *const u8, count: usize) {
    unsafe { &mut *output.cast::<Writer>() }
        .0
        .extend_from_slice(unsafe { std::slice::from_raw_parts(data, count) });
}
macro_rules! native_codec {
    ($ty:ty,$write:ident,$read:ident) => {
        impl crate::codec::contracts::Native for $ty{
            fn empty()->Self{Self::from_frame(Default::default())}
            unsafe fn write(&self,output:Slot){unsafe{$write((*self).into_frame(),output,append)}}
            unsafe fn read(context:Slot,output:&mut Self)->ReadStatus{
                let(bytes,position)=unsafe{crate::codec::contracts::input(context)};
                let mut value:<Self as NativeValue>::Frame=Default::default();
                let status=unsafe{$read(Bytes{data:bytes.data,count:bytes.count},position,&mut value)};
                if status==ReadStatus::Success{*output=Self::from_frame(value);}status
            }
        }
        impl Encode for $ty {
            fn encode(&self, writer: &mut Writer) {
                unsafe { $write((*self).into_frame(), ptr::from_mut(writer).cast(), append) }
            }
        }
        impl Decode for $ty {
            fn decode(reader: &mut Reader<'_>) -> Result<Self> {
                reader.with_native_input(|bytes, position| {
                    let mut output: <Self as NativeValue>::Frame = Default::default();
                    let status = unsafe {
                        $read(
                            Bytes {
                                data: bytes.as_ptr(),
                                count: bytes.len(),
                            },
                            ptr::from_mut(position),
                            ptr::from_mut(&mut output),
                        )
                    };
                    match status.message() {
                        Some(message) => Err(message.into()),
                        None => Ok(Self::from_frame(output)),
                    }
                })
            }
        }
    };
}
native_codec!(ValueIdentity, write_identity, read_identity);
native_codec!(Operand, write_operand, read_operand);
native_codec!(Selector, write_selection, read_selection);
native_codec!(IntegerOperation, write_integer, read_integer);
native_codec!(IntegerExpression, write_expression, read_expression);
native_codec!(Comparison, write_comparison, read_comparison);
native_codec!(FloatingForm, write_floating_form, read_floating_form);
native_codec!(FloatingBound, write_floating, read_floating);
native_codec!(ScalarPredicate, write_predicate, read_predicate);
unsafe extern "C" fn interval(value: *const c_void) -> NumericInterval {
    let value = unsafe { &*value.cast::<BitRange>() };
    NumericInterval {
        lower: value.lower,
        upper: value.upper,
    }
}
impl Encode for ScalarRanges {
    fn encode(&self, writer: &mut Writer) {
        unsafe {
            write_ranges(
                Span {
                    data: self.0.as_ptr().cast(),
                    count: self.0.len(),
                    stride: size_of::<BitRange>(),
                },
                ptr::from_mut(writer).cast(),
                RangeWriter { append, interval },
            )
        }
    }
}
impl Decode for ScalarRanges {
    fn decode(reader: &mut Reader<'_>) -> Result<Self> {
        reader.with_native_input(|bytes, position| {
            let mut output = Self::empty();
            let status = unsafe {
                read_ranges(
                    Bytes {
                        data: bytes.as_ptr(),
                        count: bytes.len(),
                    },
                    ptr::from_mut(position),
                    slot(&mut output.0),
                    range_sets::push_ends,
                )
            };
            match status.message() {
                Some(message) => Err(message.into()),
                None => Ok(output),
            }
        })
    }
}

type Slot = *mut c_void;
fn slot<T>(value: &mut T) -> Slot {
    ptr::from_mut(value).cast()
}
#[repr(C)]
struct NumericWriter {
    append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    expression: unsafe extern "C" fn(*const c_void) -> NumericExpression,
    floating: unsafe extern "C" fn(*const c_void) -> NumericFloating,
}
#[repr(C)]
struct NumericReader {
    expression: unsafe extern "C" fn(NumericExpression, Slot),
    floating: unsafe extern "C" fn(NumericFloating, Slot),
}
fn writer_host() -> NumericWriter {
    NumericWriter {
        append,
        expression: expression_fields,
        floating: floating_fields,
    }
}
fn reader_host() -> NumericReader {
    NumericReader {
        expression: materialize_expression,
        floating: materialize_floating,
    }
}
unsafe extern "C" fn materialize_expression(value: NumericExpression, output: Slot) {
    unsafe { output.cast::<IntegerExpression>().write(IntegerExpression::from_frame(value)) };
}
unsafe extern "C" fn materialize_floating(value: NumericFloating, output: Slot) {
    unsafe { output.cast::<FloatingBound>().write(FloatingBound::from_frame(value)) };
}
unsafe extern "C" fn expression_fields(source: *const c_void) -> NumericExpression {
    unsafe { *source.cast::<IntegerExpression>() }.into_frame()
}
unsafe extern "C" fn floating_fields(source: *const c_void) -> NumericFloating {
    unsafe { *source.cast::<FloatingBound>() }.into_frame()
}
unsafe extern "C" {
    #[link_name = "bootstrap.contract_codec::numeric_write_domain"]
    fn write_domain(source: *const c_void, output: Slot, host: NumericWriter);
    #[link_name = "bootstrap.contract_codec::numeric_read_domain"]
    fn read_domain(
        input: Bytes,
        position: *mut usize,
        output: Slot,
        host: NumericReader,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_write_fact"]
    fn write_fact(source: *const c_void, output: Slot, host: NumericWriter);
    #[link_name = "bootstrap.contract_codec::numeric_read_fact"]
    fn read_fact(
        input: Bytes,
        position: *mut usize,
        output: Slot,
        host: NumericReader,
    ) -> ReadStatus;
}
macro_rules! borrowed_codec {
    ($ty:ty,$empty:expr,$write:ident,$read:ident) => {
        impl crate::codec::contracts::Native for $ty{
            fn empty()->Self{$empty}
            unsafe fn write(&self,output:Slot){unsafe{$write(ptr::from_ref(self).cast(),output,writer_host())}}
            unsafe fn read(context:Slot,output:&mut Self)->ReadStatus{
                let(bytes,position)=unsafe{crate::codec::contracts::input(context)};
                let bytes=Bytes{data:bytes.data,count:bytes.count};
                unsafe { $read(bytes, position, slot(output), reader_host()) }
            }
        }
        impl Encode for $ty {
            fn encode(&self, writer: &mut Writer) {
                unsafe {
                    $write(
                        ptr::from_ref(self).cast(),
                        ptr::from_mut(writer).cast(),
                        writer_host(),
                    )
                }
            }
        }
        impl Decode for $ty {
            fn decode(reader: &mut Reader<'_>) -> Result<Self> {
                reader.with_native_input(|bytes, position| {
                    let bytes = Bytes {
                        data: bytes.as_ptr(),
                        count: bytes.len(),
                    };
                    let position = ptr::from_mut(position);
                    let mut output = $empty;
                    let status = unsafe {
                        $read(
                            bytes,
                            position,
                            slot(&mut output),
                            reader_host(),
                        )
                    };
                    match status.message() {
                        Some(message) => Err(message.into()),
                        None => Ok(output),
                    }
                })
            }
        }
    };
}
borrowed_codec!(IndexDomain, IndexDomain::Tile, write_domain, read_domain);
borrowed_codec!(
    Fact,
    Fact::NonZero(ValueIdentity::TileLength),
    write_fact,
    read_fact
);

pub(super) mod conditions;

// Physical construction for the original public contract wire.
impl crate::codec::contracts::Native for ScalarRanges {
    fn empty() -> Self {
        Self::empty()
    }
    unsafe fn write(&self, output: Slot) {
        unsafe {
            write_ranges(
                Span {
                    data: self.0.as_ptr().cast(),
                    count: self.0.len(),
                    stride: size_of::<BitRange>(),
                },
                output,
                RangeWriter { append, interval },
            )
        }
    }
    unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
        let (bytes, position) = unsafe { crate::codec::contracts::input(context) };
        unsafe {
            read_ranges(
                Bytes {
                    data: bytes.data,
                    count: bytes.count,
                },
                position,
                slot(&mut output.0),
                range_sets::push_ends,
            )
        }
    }
}
