//! Physical projections of the original numerical contract values.
//! B owns all wire tags, widths, field order and interval traversal.
use super::*;
use crate::module::codec::{Decode, Encode, ReadStatus, Reader, Result, Writer};
use std::{ffi::c_void, ptr};
#[repr(u32)]
#[derive(Clone, Copy, Default)]
enum IdentityKind {
    #[default]
    Register = 0,
    DataLength = 1,
    TileLength = 2,
    ParameterContent = 3,
    ResourceParameter = 4,
    ParameterStorage = 5,
    ElementComponent = 6,
    ElementParent = 7,
    ElementIndex = 8,
    DatasetContent = 9,
    MemoryValue = 10,
    LoopValue = 11,
    TaskResult = 12,
    TaskCompletion = 13,
    TaskWorkCompletion = 14,
    TaskControl = 15,
    TaskReturnSource = 16,
}
#[repr(u32)]
#[derive(Clone, Copy, Default)]
enum OperandKind {
    #[default]
    Value = 0,
    Constant = 1,
}
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
enum DomainKind {
    #[default]
    Data = 0,
    Fixed = 1,
    Tile = 2,
    DataLength = 3,
    TileLength = 4,
    View = 5,
    ViewLength = 6,
    ApplicationData = 7,
    ApplicationLength = 8,
    Upper = 9,
    Lower = 10,
    Room = 11,
    Sum = 12,
    Offset = 13,
    Shifted = 14,
    Computed = 15,
    CommonRange = 16,
    KnownBits = 17,
    Floating = 18,
}
#[repr(u32)]
#[derive(Clone, Copy, Default)]
enum FactKind {
    #[default]
    Equal = 0,
    Index = 1,
    Minimum = 2,
    Maximum = 3,
    NonZero = 4,
    Boolean = 5,
    Below = 6,
    Outside = 7,
    Zero = 8,
    NotEqual = 9,
    Predicate = 10,
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
    kind: IdentityKind,
    first: u64,
    second: u64,
    third: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NumericOperand {
    kind: OperandKind,
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
impl NativeValue for ValueIdentity {
    type Frame = NumericIdentity;
    fn into_frame(self) -> NumericIdentity {
        match self {
            Self::Register(v0) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::Register;
                value.first = v0 as u64;
                value
            }
            Self::DataLength(v0) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::DataLength;
                value.first = v0 as u64;
                value
            }
            Self::TileLength => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::TileLength;
                value
            }
            Self::ParameterContent(v0, v1) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::ParameterContent;
                value.first = v0 as u64;
                value.second = v1 as u64;
                value
            }
            Self::ResourceParameter(v0, v1) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::ResourceParameter;
                value.first = v0 as u64;
                value.second = v1 as u64;
                value
            }
            Self::ParameterStorage(v0) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::ParameterStorage;
                value.first = v0 as u64;
                value
            }
            Self::ElementComponent(v0) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::ElementComponent;
                value.first = v0 as u64;
                value
            }
            Self::ElementParent(v0, v1) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::ElementParent;
                value.first = v0 as u64;
                value.second = v1 as u64;
                value
            }
            Self::DatasetContent(v0, v1, v2) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::DatasetContent;
                value.first = v0 as u64;
                value.second = v1 as u64;
                value.third = v2 as u64;
                value
            }
            Self::MemoryValue(v0, v1) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::MemoryValue;
                value.first = v0 as u64;
                value.second = v1 as u64;
                value
            }
            Self::ElementIndex(v0) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::ElementIndex;
                value.first = v0 as u64;
                value
            }
            Self::LoopValue(v0, v1) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::LoopValue;
                value.first = v0 as u64;
                value.second = v1 as u64;
                value
            }
            Self::TaskResult(v0) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::TaskResult;
                value.first = v0 as u64;
                value
            }
            Self::TaskCompletion(v0, v1) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::TaskCompletion;
                value.first = v0 as u64;
                value.second = v1 as u64;
                value
            }
            Self::TaskControl(v0) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::TaskControl;
                value.first = v0 as u64;
                value
            }
            Self::TaskReturnSource(v0) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::TaskReturnSource;
                value.first = v0 as u64;
                value
            }
            Self::TaskWorkCompletion(v0, v1) => {
                let mut value = NumericIdentity::default();
                value.kind = IdentityKind::TaskWorkCompletion;
                value.first = v0 as u64;
                value.second = v1 as u64;
                value
            }
        }
    }
    fn from_frame(value: NumericIdentity) -> Self {
        match value.kind {
            IdentityKind::Register => Self::Register(value.first as usize),
            IdentityKind::DataLength => Self::DataLength(value.first as usize),
            IdentityKind::TileLength => Self::TileLength,
            IdentityKind::ParameterContent => {
                Self::ParameterContent(value.first as usize, value.second as usize)
            }
            IdentityKind::ResourceParameter => {
                Self::ResourceParameter(value.first as usize, value.second as usize)
            }
            IdentityKind::ParameterStorage => Self::ParameterStorage(value.first as usize),
            IdentityKind::ElementComponent => Self::ElementComponent(value.first as usize),
            IdentityKind::ElementParent => {
                Self::ElementParent(value.first as usize, value.second as usize)
            }
            IdentityKind::DatasetContent => Self::DatasetContent(
                value.first as usize,
                value.second as usize,
                value.third as usize,
            ),
            IdentityKind::MemoryValue => {
                Self::MemoryValue(value.first as usize, value.second as usize)
            }
            IdentityKind::ElementIndex => Self::ElementIndex(value.first as usize),
            IdentityKind::LoopValue => Self::LoopValue(value.first as usize, value.second as usize),
            IdentityKind::TaskResult => Self::TaskResult(value.first as usize),
            IdentityKind::TaskCompletion => {
                Self::TaskCompletion(value.first as usize, value.second as usize)
            }
            IdentityKind::TaskControl => Self::TaskControl(value.first as usize),
            IdentityKind::TaskReturnSource => Self::TaskReturnSource(value.first as usize),
            IdentityKind::TaskWorkCompletion => {
                Self::TaskWorkCompletion(value.first as u32, value.second as u32)
            }
        }
    }
}
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

impl NativeValue for Operand {
    type Frame = NumericOperand;
    fn into_frame(self) -> NumericOperand {
        match self {
            Self::Value(v0) => {
                let mut value = NumericOperand::default();
                value.kind = OperandKind::Value;
                value.identity = v0.into_frame();
                value
            }
            Self::Constant(v0, v1) => {
                let mut value = NumericOperand::default();
                value.kind = OperandKind::Constant;
                value.scalar = v0.into_frame();
                value.bits = v1;
                value
            }
        }
    }
    fn from_frame(value: NumericOperand) -> Self {
        match value.kind {
            OperandKind::Value => Self::Value(ValueIdentity::from_frame(value.identity)),
            OperandKind::Constant => Self::Constant(Scalar::from_frame(value.scalar), value.bits),
        }
    }
}
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
impl NativeValue for ScalarPredicate {
    type Frame = NumericPredicate;
    fn into_frame(self) -> NumericPredicate {
        NumericPredicate {
            operation: self.op.into_frame(),
            scalar: self.scalar.into_frame(),
            left: self.a.into_frame(),
            right: self.b.into_frame(),
        }
    }
    fn from_frame(value: NumericPredicate) -> Self {
        Self {
            op: Comparison::from_frame(value.operation),
            scalar: Scalar::from_frame(value.scalar),
            a: Operand::from_frame(value.left),
            b: Operand::from_frame(value.right),
        }
    }
}
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
#[derive(Clone, Copy, Default)]
struct DomainFields {
    kind: DomainKind,
    first: *const c_void,
    second: *const c_void,
    third: *const c_void,
    ordinal: u64,
    distance: u32,
    scalars: [ScalarTag; 4],
    mask: u64,
    bits: u64,
    expression: *const c_void,
    floating: *const c_void,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FactFields {
    kind: FactKind,
    first: *const c_void,
    second: *const c_void,
    domain: *const c_void,
    scalar: ScalarTag,
    bits: u64,
    truth: u32,
    predicate: *const c_void,
}
#[repr(C)]
struct NumericWriter {
    append: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    identity: unsafe extern "C" fn(*const c_void) -> NumericIdentity,
    expression: unsafe extern "C" fn(*const c_void) -> NumericExpression,
    floating: unsafe extern "C" fn(*const c_void) -> NumericFloating,
    predicate: unsafe extern "C" fn(*const c_void) -> NumericPredicate,
    domain: unsafe extern "C" fn(*const c_void) -> DomainFields,
    fact: unsafe extern "C" fn(*const c_void) -> FactFields,
}
#[repr(C)]
struct NumericReader {
    context: Slot,
    domain: unsafe extern "C" fn(Slot, Slot, DomainKind) -> ReadStatus,
    fact: unsafe extern "C" fn(Slot, Slot, FactKind) -> ReadStatus,
}
struct ReadContext {
    input: Bytes,
    position: *mut usize,
}
fn writer_host() -> NumericWriter {
    NumericWriter {
        append,
        identity: identity_fields,
        expression: expression_fields,
        floating: floating_fields,
        predicate: predicate_fields,
        domain: domain_fields,
        fact: fact_fields,
    }
}
fn reader_host(context: Slot) -> NumericReader {
    NumericReader {
        context,
        domain: construct_domain,
        fact: construct_fact,
    }
}
unsafe fn input(context: Slot) -> (Bytes, *mut usize) {
    let context = unsafe { &*context.cast::<ReadContext>() };
    (context.input, context.position)
}
unsafe extern "C" fn identity_fields(source: *const c_void) -> NumericIdentity {
    unsafe { *source.cast::<ValueIdentity>() }.into_frame()
}
unsafe extern "C" fn expression_fields(source: *const c_void) -> NumericExpression {
    unsafe { *source.cast::<IntegerExpression>() }.into_frame()
}
unsafe extern "C" fn floating_fields(source: *const c_void) -> NumericFloating {
    unsafe { *source.cast::<FloatingBound>() }.into_frame()
}
unsafe extern "C" fn predicate_fields(source: *const c_void) -> NumericPredicate {
    unsafe { *source.cast::<ScalarPredicate>() }.into_frame()
}
unsafe extern "C" fn domain_fields(source: *const c_void) -> DomainFields {
    let source = unsafe { &*source.cast::<IndexDomain>() };
    match source {
        IndexDomain::Data(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Data;
            fields.ordinal = (*v0) as u64;
            fields
        }
        IndexDomain::Fixed(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Fixed;
            fields.distance = (*v0);
            fields
        }
        IndexDomain::Tile => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Tile;
            fields
        }
        IndexDomain::DataLength(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::DataLength;
            fields.ordinal = (*v0) as u64;
            fields
        }
        IndexDomain::TileLength => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::TileLength;
            fields
        }
        IndexDomain::View(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::View;
            fields.first = ptr::from_ref(&v0[0]).cast();
            fields.second = ptr::from_ref(&v0[1]).cast();
            fields.third = ptr::from_ref(&v0[2]).cast();
            fields
        }
        IndexDomain::ViewLength(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::ViewLength;
            fields.first = ptr::from_ref(&v0[0]).cast();
            fields.second = ptr::from_ref(&v0[1]).cast();
            fields.third = ptr::from_ref(&v0[2]).cast();
            fields
        }
        IndexDomain::ApplicationData(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::ApplicationData;
            fields.first = ptr::from_ref(v0).cast();
            fields
        }
        IndexDomain::ApplicationLength(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::ApplicationLength;
            fields.first = ptr::from_ref(v0).cast();
            fields
        }
        IndexDomain::Upper(v0, v1) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Upper;
            fields.first = ptr::from_ref(v0).cast();
            fields.distance = (*v1);
            fields
        }
        IndexDomain::Room(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Room;
            fields.first = ptr::from_ref(v0).cast();
            fields
        }
        IndexDomain::Sum(v0, v1) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Sum;
            fields.first = ptr::from_ref(v0).cast();
            fields.second = ptr::from_ref(v1).cast();
            fields
        }
        IndexDomain::Shifted(v0, v1, v2) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Shifted;
            fields.first = ptr::from_ref(v0).cast();
            fields.second = ptr::from_ref(v1).cast();
            fields.distance = (*v2);
            fields
        }
        IndexDomain::Offset(v0, v1) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Offset;
            fields.first = ptr::from_ref(v0).cast();
            fields.distance = (*v1);
            fields
        }
        IndexDomain::Computed(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Computed;
            fields.expression = ptr::from_ref(v0).cast();
            fields
        }
        IndexDomain::Lower(v0, v1) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Lower;
            fields.first = ptr::from_ref(v0).cast();
            fields.distance = (*v1);
            fields
        }
        IndexDomain::CommonRange(v0, v1, v2, v3, v4) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::CommonRange;
            fields.first = ptr::from_ref(v0).cast();
            fields.scalars[0] = (*v1).into_frame();
            fields.scalars[1] = (*v2).into_frame();
            fields.scalars[2] = (*v3).into_frame();
            fields.scalars[3] = (*v4).into_frame();
            fields
        }
        IndexDomain::KnownBits(v0, v1, v2) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::KnownBits;
            fields.scalars[0] = (*v0).into_frame();
            fields.mask = (*v1);
            fields.bits = (*v2);
            fields
        }
        IndexDomain::Floating(v0) => {
            let mut fields = DomainFields::default();
            fields.kind = DomainKind::Floating;
            fields.floating = ptr::from_ref(v0).cast();
            fields
        }
    }
}
unsafe extern "C" fn fact_fields(source: *const c_void) -> FactFields {
    let source = unsafe { &*source.cast::<Fact>() };
    match source {
        Fact::Equal(v0, v1) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::Equal;
            fields.first = ptr::from_ref(v0).cast();
            fields.second = ptr::from_ref(v1).cast();
            fields
        }
        Fact::Index(v0, v1) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::Index;
            fields.first = ptr::from_ref(v0).cast();
            fields.domain = ptr::from_ref(v1).cast();
            fields
        }
        Fact::Minimum(v0, v1, v2) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::Minimum;
            fields.first = ptr::from_ref(v0).cast();
            fields.scalar = (*v1).into_frame();
            fields.bits = (*v2);
            fields
        }
        Fact::Maximum(v0, v1, v2) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::Maximum;
            fields.first = ptr::from_ref(v0).cast();
            fields.scalar = (*v1).into_frame();
            fields.bits = (*v2);
            fields
        }
        Fact::NonZero(v0) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::NonZero;
            fields.first = ptr::from_ref(v0).cast();
            fields
        }
        Fact::Boolean(v0, v1) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::Boolean;
            fields.first = ptr::from_ref(v0).cast();
            fields.truth = u32::from((*v1));
            fields
        }
        Fact::Below(v0, v1, v2, v3) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::Below;
            fields.first = ptr::from_ref(v0).cast();
            fields.second = ptr::from_ref(v1).cast();
            fields.truth = u32::from((*v2));
            fields.scalar = (*v3).into_frame();
            fields
        }
        Fact::Outside(v0, v1) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::Outside;
            fields.first = ptr::from_ref(v0).cast();
            fields.domain = ptr::from_ref(v1).cast();
            fields
        }
        Fact::Zero(v0) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::Zero;
            fields.first = ptr::from_ref(v0).cast();
            fields
        }
        Fact::NotEqual(v0, v1) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::NotEqual;
            fields.first = ptr::from_ref(v0).cast();
            fields.second = ptr::from_ref(v1).cast();
            fields
        }
        Fact::Predicate(v0, v1) => {
            let mut fields = FactFields::default();
            fields.kind = FactKind::Predicate;
            fields.predicate = ptr::from_ref(v0).cast();
            fields.truth = u32::from((*v1));
            fields
        }
    }
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
                let bytes=Bytes{data:bytes.data,count:bytes.count};let mut frame=ReadContext{input:bytes,position};
                unsafe{$read(bytes,position,slot(output),reader_host(slot(&mut frame)))}
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
                    let mut context = ReadContext {
                        input: bytes,
                        position,
                    };
                    let mut output = $empty;
                    let status = unsafe {
                        $read(
                            bytes,
                            position,
                            slot(&mut output),
                            reader_host(slot(&mut context)),
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

unsafe extern "C" {
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_ordinal"]
    fn read_payload_ordinal(input: Bytes, position: *mut usize, field0: *mut u64) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_count"]
    fn read_payload_count(input: Bytes, position: *mut usize, field0: *mut u32) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_view_identities"]
    fn read_payload_view_identities(
        input: Bytes,
        position: *mut usize,
        field0: *mut [NumericIdentity; 3],
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_identity_distance"]
    fn read_payload_identity_distance(
        input: Bytes,
        position: *mut usize,
        field0: *mut NumericIdentity,
        field1: *mut u32,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_identity_pair"]
    fn read_payload_identity_pair(
        input: Bytes,
        position: *mut usize,
        field0: *mut NumericIdentity,
        field1: *mut NumericIdentity,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_shifted_identities"]
    fn read_payload_shifted_identities(
        input: Bytes,
        position: *mut usize,
        field0: *mut NumericIdentity,
        field1: *mut NumericIdentity,
        field2: *mut u32,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_common_range"]
    fn read_payload_common_range(
        input: Bytes,
        position: *mut usize,
        field0: *mut NumericIdentity,
        field1: *mut ScalarTag,
        field2: *mut ScalarTag,
        field3: *mut ScalarTag,
        field4: *mut ScalarTag,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_known_bits"]
    fn read_payload_known_bits(
        input: Bytes,
        position: *mut usize,
        field0: *mut ScalarTag,
        field1: *mut u64,
        field2: *mut u64,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_identity_domain"]
    fn read_payload_identity_domain(
        input: Bytes,
        position: *mut usize,
        field0: *mut NumericIdentity,
        field1: Slot,
        host: NumericReader,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_identity_scalar_bits"]
    fn read_payload_identity_scalar_bits(
        input: Bytes,
        position: *mut usize,
        field0: *mut NumericIdentity,
        field1: *mut ScalarTag,
        field2: *mut u64,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_identity_truth"]
    fn read_payload_identity_truth(
        input: Bytes,
        position: *mut usize,
        field0: *mut NumericIdentity,
        field1: *mut u32,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_ordered_identities"]
    fn read_payload_ordered_identities(
        input: Bytes,
        position: *mut usize,
        field0: *mut NumericIdentity,
        field1: *mut NumericIdentity,
        field2: *mut u32,
        field3: *mut ScalarTag,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::numeric_read_payload_predicate_truth"]
    fn read_payload_predicate_truth(
        input: Bytes,
        position: *mut usize,
        field0: *mut NumericPredicate,
        field1: *mut u32,
    ) -> ReadStatus;
}
unsafe extern "C" fn construct_domain(context: Slot, output: Slot, kind: DomainKind) -> ReadStatus {
    let (bytes, position) = unsafe { input(context) };
    match kind {
        DomainKind::Data => {
            let mut v0: u64 = Default::default();
            let status = unsafe { read_payload_ordinal(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<IndexDomain>() = IndexDomain::Data(v0 as usize) };
            }
            status
        }
        DomainKind::Fixed => {
            let mut v0: u32 = Default::default();
            let status = unsafe { read_payload_count(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<IndexDomain>() = IndexDomain::Fixed(v0) };
            }
            status
        }
        DomainKind::Tile => {
            unsafe { *output.cast::<IndexDomain>() = IndexDomain::Tile };
            ReadStatus::Success
        }
        DomainKind::DataLength => {
            let mut v0: u64 = Default::default();
            let status = unsafe { read_payload_ordinal(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<IndexDomain>() = IndexDomain::DataLength(v0 as usize) };
            }
            status
        }
        DomainKind::TileLength => {
            unsafe { *output.cast::<IndexDomain>() = IndexDomain::TileLength };
            ReadStatus::Success
        }
        DomainKind::View => {
            let mut v0: [NumericIdentity; 3] = Default::default();
            let status =
                unsafe { read_payload_view_identities(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::View(v0.map(NativeValue::from_frame))
                };
            }
            status
        }
        DomainKind::ViewLength => {
            let mut v0: [NumericIdentity; 3] = Default::default();
            let status =
                unsafe { read_payload_view_identities(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::ViewLength(v0.map(NativeValue::from_frame))
                };
            }
            status
        }
        DomainKind::ApplicationData => {
            let mut v0: NumericIdentity = Default::default();
            let status = unsafe { read_identity(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::ApplicationData(ValueIdentity::from_frame(v0))
                };
            }
            status
        }
        DomainKind::ApplicationLength => {
            let mut v0: NumericIdentity = Default::default();
            let status = unsafe { read_identity(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::ApplicationLength(ValueIdentity::from_frame(v0))
                };
            }
            status
        }
        DomainKind::Upper => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: u32 = Default::default();
            let status = unsafe {
                read_payload_identity_distance(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::Upper(ValueIdentity::from_frame(v0), v1)
                };
            }
            status
        }
        DomainKind::Room => {
            let mut v0: NumericIdentity = Default::default();
            let status = unsafe { read_identity(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() = IndexDomain::Room(ValueIdentity::from_frame(v0))
                };
            }
            status
        }
        DomainKind::Sum => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: NumericIdentity = Default::default();
            let status = unsafe {
                read_payload_identity_pair(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() = IndexDomain::Sum(
                        ValueIdentity::from_frame(v0),
                        ValueIdentity::from_frame(v1),
                    )
                };
            }
            status
        }
        DomainKind::Shifted => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: NumericIdentity = Default::default();
            let mut v2: u32 = Default::default();
            let status = unsafe {
                read_payload_shifted_identities(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                    ptr::from_mut(&mut v2),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() = IndexDomain::Shifted(
                        ValueIdentity::from_frame(v0),
                        ValueIdentity::from_frame(v1),
                        v2,
                    )
                };
            }
            status
        }
        DomainKind::Offset => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: u32 = Default::default();
            let status = unsafe {
                read_payload_identity_distance(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::Offset(ValueIdentity::from_frame(v0), v1)
                };
            }
            status
        }
        DomainKind::Computed => {
            let mut v0: NumericExpression = Default::default();
            let status = unsafe { read_expression(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::Computed(IntegerExpression::from_frame(v0))
                };
            }
            status
        }
        DomainKind::Lower => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: u32 = Default::default();
            let status = unsafe {
                read_payload_identity_distance(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::Lower(ValueIdentity::from_frame(v0), v1)
                };
            }
            status
        }
        DomainKind::CommonRange => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: ScalarTag = Default::default();
            let mut v2: ScalarTag = Default::default();
            let mut v3: ScalarTag = Default::default();
            let mut v4: ScalarTag = Default::default();
            let status = unsafe {
                read_payload_common_range(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                    ptr::from_mut(&mut v2),
                    ptr::from_mut(&mut v3),
                    ptr::from_mut(&mut v4),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() = IndexDomain::CommonRange(
                        ValueIdentity::from_frame(v0),
                        Scalar::from_frame(v1),
                        Scalar::from_frame(v2),
                        Scalar::from_frame(v3),
                        Scalar::from_frame(v4),
                    )
                };
            }
            status
        }
        DomainKind::KnownBits => {
            let mut v0: ScalarTag = Default::default();
            let mut v1: u64 = Default::default();
            let mut v2: u64 = Default::default();
            let status = unsafe {
                read_payload_known_bits(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                    ptr::from_mut(&mut v2),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::KnownBits(Scalar::from_frame(v0), v1, v2)
                };
            }
            status
        }
        DomainKind::Floating => {
            let mut v0: NumericFloating = Default::default();
            let status = unsafe { read_floating(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<IndexDomain>() =
                        IndexDomain::Floating(FloatingBound::from_frame(v0))
                };
            }
            status
        }
    }
}
unsafe extern "C" fn construct_fact(context: Slot, output: Slot, kind: FactKind) -> ReadStatus {
    let (bytes, position) = unsafe { input(context) };
    match kind {
        FactKind::Equal => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: NumericIdentity = Default::default();
            let status = unsafe {
                read_payload_identity_pair(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<Fact>() =
                        Fact::Equal(ValueIdentity::from_frame(v0), ValueIdentity::from_frame(v1))
                };
            }
            status
        }
        FactKind::Index => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: IndexDomain = IndexDomain::Tile;
            let status = unsafe {
                read_payload_identity_domain(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    slot(&mut v1),
                    reader_host(context),
                )
            };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<Fact>() = Fact::Index(ValueIdentity::from_frame(v0), v1) };
            }
            status
        }
        FactKind::Minimum => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: ScalarTag = Default::default();
            let mut v2: u64 = Default::default();
            let status = unsafe {
                read_payload_identity_scalar_bits(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                    ptr::from_mut(&mut v2),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<Fact>() =
                        Fact::Minimum(ValueIdentity::from_frame(v0), Scalar::from_frame(v1), v2)
                };
            }
            status
        }
        FactKind::Maximum => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: ScalarTag = Default::default();
            let mut v2: u64 = Default::default();
            let status = unsafe {
                read_payload_identity_scalar_bits(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                    ptr::from_mut(&mut v2),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<Fact>() =
                        Fact::Maximum(ValueIdentity::from_frame(v0), Scalar::from_frame(v1), v2)
                };
            }
            status
        }
        FactKind::NonZero => {
            let mut v0: NumericIdentity = Default::default();
            let status = unsafe { read_identity(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<Fact>() = Fact::NonZero(ValueIdentity::from_frame(v0)) };
            }
            status
        }
        FactKind::Boolean => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: u32 = Default::default();
            let status = unsafe {
                read_payload_identity_truth(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<Fact>() = Fact::Boolean(ValueIdentity::from_frame(v0), (v1 != 0))
                };
            }
            status
        }
        FactKind::Below => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: NumericIdentity = Default::default();
            let mut v2: u32 = Default::default();
            let mut v3: ScalarTag = Default::default();
            let status = unsafe {
                read_payload_ordered_identities(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                    ptr::from_mut(&mut v2),
                    ptr::from_mut(&mut v3),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<Fact>() = Fact::Below(
                        ValueIdentity::from_frame(v0),
                        ValueIdentity::from_frame(v1),
                        (v2 != 0),
                        Scalar::from_frame(v3),
                    )
                };
            }
            status
        }
        FactKind::Outside => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: IndexDomain = IndexDomain::Tile;
            let status = unsafe {
                read_payload_identity_domain(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    slot(&mut v1),
                    reader_host(context),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<Fact>() = Fact::Outside(ValueIdentity::from_frame(v0), v1)
                };
            }
            status
        }
        FactKind::Zero => {
            let mut v0: NumericIdentity = Default::default();
            let status = unsafe { read_identity(bytes, position, ptr::from_mut(&mut v0)) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<Fact>() = Fact::Zero(ValueIdentity::from_frame(v0)) };
            }
            status
        }
        FactKind::NotEqual => {
            let mut v0: NumericIdentity = Default::default();
            let mut v1: NumericIdentity = Default::default();
            let status = unsafe {
                read_payload_identity_pair(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<Fact>() =
                        Fact::NotEqual(ValueIdentity::from_frame(v0), ValueIdentity::from_frame(v1))
                };
            }
            status
        }
        FactKind::Predicate => {
            let mut v0: NumericPredicate = Default::default();
            let mut v1: u32 = Default::default();
            let status = unsafe {
                read_payload_predicate_truth(
                    bytes,
                    position,
                    ptr::from_mut(&mut v0),
                    ptr::from_mut(&mut v1),
                )
            };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<Fact>() =
                        Fact::Predicate(ScalarPredicate::from_frame(v0), (v1 != 0))
                };
            }
            status
        }
    }
}


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
