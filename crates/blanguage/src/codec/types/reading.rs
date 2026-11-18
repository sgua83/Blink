//! Actual typed construction frames for the B contract reader.
use super::super::{Reader, Result};
use super::Bytes;
use crate::{ast, ir};
use std::{ffi::c_void, ptr};
type Slot = *mut c_void;
fn slot<T>(value: &mut T) -> Slot {
    ptr::from_mut(value).cast()
}
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadStatus {
    Success = 0,
    Overflow = 1,
    Truncated = 2,
    Boolean = 3,
    Scalar = 4,
    Access = 5,
    Target = 6,
    Shape = 7,
    Argument = 8,
    Expression = 9,
    Predicate = 10,
    Condition = 11,
    RustError = 12,
    Identity = 13,
    Operand = 14,
    IntegerOperation = 15,
    IntegerExpression = 16,
    Comparison = 17,
    FloatingForm = 18,
    IndexDomain = 19,
    Fact = 20,
    ConditionVersion = 21,
    ConditionNode = 22,
    ContractAddressRoot = 23,
    ContractInitializationSource = 24,
    ContractElementFacts = 25,
    ContractDataLayout = 26,
    ContractReturned = 27,
    ContractTaskProjection = 28,
    ContractExclusion = 29,
    ContractViewAccess = 30,
    ContractResidencyPart = 31,
    ContractResidencyOrigin = 32,
    ContractResidency = 33,
    ContractResidencyOutputKind = 34,
    ContractTextureDimension = 35,
    ContractTextureFormat = 36,
    ContractElementFact = 37,
    ContractCallParameterKind = 38,
    ContractContractKind = 39,
    ContractRelationOp = 40,
    ContractOwnership = 41,
    ContractAddressIdentity = 42,
    ContractBorrowSource = 43,
    ResourceSortOrder = 44,
    ResourceCollectiveOperator = 45,
    ResourceOperation = 46,
    ResourceImage = 47,
    ResourceLinkedImage = 48,
    ResourceModuleTarget = 49,
    ResourceNativeSymbol = 50,
    ResourceRangeResult = 51,
    ResourceContinuationCode = 52,
    ResourceCpuStageCode = 53,
}
impl ReadStatus {
    pub(crate) fn message(self) -> Option<&'static str> {
        match self {
            ReadStatus::Success | ReadStatus::RustError => None,
            ReadStatus::ResourceImage => Some("模块枚举编码错误：Image"),
            ReadStatus::ResourceLinkedImage => Some("模块枚举编码错误：Image<TemplateProgram, usize>"),
            ReadStatus::ResourceModuleTarget => Some("模块枚举编码错误：native::ModuleTarget"),
            ReadStatus::ResourceNativeSymbol => Some("模块枚举编码错误：native::NativeSymbol"),
            ReadStatus::ResourceRangeResult => Some("模块枚举编码错误：native::RangeResult"),
            ReadStatus::ResourceContinuationCode => Some("模块枚举编码错误：native::ContinuationCode<C>"),
            ReadStatus::ResourceCpuStageCode => Some("模块枚举编码错误：native::StageCode<C>"),
            ReadStatus::ResourceSortOrder => Some("模块枚举编码错误：SortOrder"),
            ReadStatus::ResourceCollectiveOperator => Some("模块枚举编码错误：CollectiveOperator"),
            ReadStatus::ResourceOperation => Some("模块枚举编码错误：Operation<ReductionLayout>"),
            ReadStatus::Overflow => Some("模块接口长度溢出"),
            ReadStatus::Truncated => Some("模块接口被截断"),
            ReadStatus::Boolean => Some("模块布尔值编码错误"),
            ReadStatus::Scalar => Some("模块枚举编码错误：ir::Scalar"),
            ReadStatus::Access => Some("模块枚举编码错误：ast::Access"),
            ReadStatus::Target => Some("模块枚举编码错误：ast::Target"),
            ReadStatus::Shape => Some("模块枚举编码错误：ast::Shape"),
            ReadStatus::Argument => Some("模块枚举编码错误：ast::GenericArgument"),
            ReadStatus::Expression => Some("模块枚举编码错误：ast::ExprKind"),
            ReadStatus::Predicate => Some("模块枚举编码错误：ast::ContractPredicate"),
            ReadStatus::Condition => Some("模块枚举编码错误：ast::OutputCondition"),
            ReadStatus::Identity => Some("模块枚举编码错误：ValueIdentity"),
            ReadStatus::Operand => Some("模块枚举编码错误：Operand"),
            ReadStatus::IntegerOperation => Some("模块枚举编码错误：IntegerOperation"),
            ReadStatus::IntegerExpression => Some("模块枚举编码错误：IntegerExpression"),
            ReadStatus::Comparison => Some("模块枚举编码错误：Comparison"),
            ReadStatus::FloatingForm => Some("模块枚举编码错误：FloatingForm"),
            ReadStatus::IndexDomain => Some("模块枚举编码错误：IndexDomain"),
            ReadStatus::Fact => Some("模块枚举编码错误：Fact"),
            ReadStatus::ConditionVersion => Some("模块分量条件合同格式不兼容，需要用同一编译器重新编译"),
            ReadStatus::ConditionNode => Some("模块枚举编码错误：EncodedClause<F>"),
            ReadStatus::ContractBorrowSource => Some("模块枚举编码错误：Source"),
            ReadStatus::ContractAddressIdentity => Some("模块地址数值关系无效"),
            ReadStatus::ContractOwnership => Some("invalid ownership tag"),
            ReadStatus::ContractRelationOp => Some("模块枚举编码错误：Op"),
            ReadStatus::ContractContractKind => Some("模块枚举编码错误：ast::ContractKind"),
            ReadStatus::ContractCallParameterKind => Some("模块枚举编码错误：ir::CallParameterKind"),
            ReadStatus::ContractElementFact => Some("模块枚举编码错误：ElementFact"),
            ReadStatus::ContractTextureFormat => Some("模块枚举编码错误：crate::ast::TextureFormat"),
            ReadStatus::ContractTextureDimension => Some("模块枚举编码错误：crate::ast::TextureDimension"),
            ReadStatus::ContractResidencyOutputKind => Some("模块枚举编码错误：OutputKind"),
            ReadStatus::ContractResidency => Some("模块枚举编码错误：Residency"),
            ReadStatus::ContractResidencyOrigin => Some("模块枚举编码错误：Origin"),
            ReadStatus::ContractResidencyPart => Some("模块枚举编码错误：Part"),
            ReadStatus::ContractViewAccess => Some("模块枚举编码错误：Access"),
            ReadStatus::ContractExclusion => Some("模块枚举编码错误：Exclusion"),
            ReadStatus::ContractTaskProjection => Some("模块枚举编码错误：TaskProjection"),
            ReadStatus::ContractReturned => Some("模块枚举编码错误：Returned"),
            ReadStatus::ContractDataLayout => Some("模块枚举编码错误：DataLayout"),
            ReadStatus::ContractElementFacts => Some("模块枚举编码错误：ElementFacts"),
            ReadStatus::ContractInitializationSource => Some("模块枚举编码错误：InitializationSource"),
            ReadStatus::ContractAddressRoot => Some("模块枚举编码错误：Root"),
        }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScalarTag {
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
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessTag {
    Value = 0,
    Read = 1,
    Write = 2,
    InOut = 3,
    Atomic = 4,
}
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetTag {
    Compute = 0,
    Cpu = 1,
    Gpu = 2,
}
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArgumentTag {
    Type = 0,
    Value = 1,
}
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExpressionTag {
    Number = 0,
    StaticU32 = 1,
    Bool = 2,
    Name = 3,
    Member = 4,
    Index = 5,
    Call = 6,
    Invoke = 7,
    Construct = 8,
    Specialize = 9,
    FunctionRef = 10,
    Cooperate = 11,
    Target = 12,
    Unary = 13,
    Binary = 14,
    Select = 15,
}
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConditionTag {
    Name = 0,
    Number = 1,
    Bool = 2,
    Member = 3,
    Unary = 4,
    Binary = 5,
    StaticU32 = 6,
    Constant = 7,
}
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum PredicateTag {
    Prefix = 0,
    Nonzero = 1,
    Nonempty = 2,
    Contiguous = 3,
    Memory = 4,
    Index = 5,
    Range = 6,
    SameLength = 7,
    Initialized = 8,
    Cleanup = 9,
}
pod!(TypeFields {
    name: Slot,
    arguments: Slot,
    values: Slot,
    size: Slot,
    alignment: Slot,
    components: Slot,
    fields: Slot,
    view: Slot,
    array: Slot,
    enumeration: Slot,
    result: Slot,
    variants: Slot,
    dataset: Slot,
    cleanup: Slot,
    function: Slot
});
pod!(NameFields {
    name: Slot,
    canonical: Slot,
    arguments: Slot,
    dataset: Slot,
    view: Slot,
    tile: Slot,
    count: Slot,
    contracts: Slot
});
pod!(FunctionFields {
    result: Slot,
    parameters: Slot,
    contracts: Slot
});
pod!(ConstantFields {
    ty: Slot,
    bits: Slot
});
pod!(ExpressionFields {
    text: Slot,
    a: Slot,
    b: Slot,
    c: Slot,
    arguments: Slot,
    types: Slot,
    ty: Slot,
    target: Slot,
    literal: Slot
});
pod!(ConditionFields {
    text: Slot,
    a: Slot,
    b: Slot,
    literal: Slot,
    constant: Slot
});
pod!(PredicateFields {
    first: Slot,
    second: Slot,
    third: Slot,
    condition: Slot,
    intervals: Slot,
    access: Slot
});
#[repr(C)]
struct ReadHost {
    context: Slot,
    type_fields: unsafe extern "C" fn(Slot) -> TypeFields,
    name_fields: unsafe extern "C" fn(Slot) -> NameFields,
    function_fields: unsafe extern "C" fn(Slot) -> FunctionFields,
    constant_fields: unsafe extern "C" fn(Slot) -> ConstantFields,
    text: unsafe extern "C" fn(Slot, Slot, Bytes) -> ReadStatus,
    string_option: unsafe extern "C" fn(Slot, Slot, Bytes) -> ReadStatus,
    push_u32: unsafe extern "C" fn(Slot, u32),
    push_u64: unsafe extern "C" fn(Slot, u64),
    push_component: unsafe extern "C" fn(Slot, ScalarTag, u32),
    push_type: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    push_field: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    push_variant: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    push_parameter: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    push_interval: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    push_argument: unsafe extern "C" fn(Slot, Slot, ArgumentTag) -> ReadStatus,
    push_expression: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    push_predicate: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    type_box: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    type_name_box: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    function_box: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    shape_option: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    predicates_option: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    arguments_option: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    shape_expression: unsafe extern "C" fn(Slot, Slot) -> ReadStatus,
    shape_literal: unsafe extern "C" fn(Slot, u32),
    shape_name: unsafe extern "C" fn(Slot, Slot, Bytes) -> ReadStatus,
    expression: unsafe extern "C" fn(Slot, Slot, ExpressionTag) -> ReadStatus,
    condition: unsafe extern "C" fn(Slot, Slot, ConditionTag) -> ReadStatus,
    predicate: unsafe extern "C" fn(Slot, Slot, PredicateTag) -> ReadStatus,
    access: unsafe extern "C" fn(Slot, AccessTag),
    target: unsafe extern "C" fn(Slot, TargetTag),
    optional_access: unsafe extern "C" fn(Slot, AccessTag),
    expression_span: unsafe extern "C" fn(Slot, u64, u64),
}

unsafe extern "C" {
#[link_name="bootstrap.contract_codec::contract_read_ast_condition"]fn read_condition(input:Bytes,position:*mut usize,output:Slot,host:ReadHost)->ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_type"]
    fn read_type(input: Bytes, position: *mut usize, output: Slot, host: ReadHost) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_type_name"]
    fn read_type_name(
        input: Bytes,
        position: *mut usize,
        output: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_function"]
    fn read_function(
        input: Bytes,
        position: *mut usize,
        output: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_shape"]
    fn read_shape(input: Bytes, position: *mut usize, output: Slot, host: ReadHost) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_expression"]
    fn read_expression(
        input: Bytes,
        position: *mut usize,
        output: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_predicate"]
    fn read_predicate(
        input: Bytes,
        position: *mut usize,
        output: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_arguments"]
    fn read_arguments(
        input: Bytes,
        position: *mut usize,
        output: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_predicates"]
    fn read_predicates(
        input: Bytes,
        position: *mut usize,
        output: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_named_type"]
    fn read_named_type(
        input: Bytes,
        position: *mut usize,
        name: Slot,
        word: Slot,
        ty: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_parameter"]
    fn read_parameter(
        input: Bytes,
        position: *mut usize,
        access: Slot,
        ty: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_interval"]
    fn read_interval(
        input: Bytes,
        position: *mut usize,
        lower: Slot,
        upper: Slot,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_expression_payload"]
    fn read_expression_payload(
        kind: ExpressionTag,
        fields: ExpressionFields,
        input: Bytes,
        position: *mut usize,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_condition_payload"]
    fn read_condition_payload(
        kind: ConditionTag,
        fields: ConditionFields,
        input: Bytes,
        position: *mut usize,
        host: ReadHost,
    ) -> ReadStatus;
    #[link_name = "bootstrap.contract_codec::read_predicate_payload"]
    fn read_predicate_payload(
        kind: PredicateTag,
        fields: PredicateFields,
        input: Bytes,
        position: *mut usize,
        host: ReadHost,
    ) -> ReadStatus;
}

struct Context {
    input: Bytes,
    position: *mut usize,
    result: Result<()>,
}
type Read = unsafe extern "C" fn(Bytes, *mut usize, Slot, ReadHost) -> ReadStatus;
unsafe fn parse(context: Slot, output: Slot, read: Read) -> ReadStatus {
    // No Rust reference to Context stays live while native callbacks update
    // its original Result; the copied input is only a borrowed byte view.
    let (input, position) = {
        let c = unsafe { &*context.cast::<Context>() };
        (c.input, c.position)
    };
    unsafe { read(input, position, output, host(context)) }
}
pub(crate) fn with_context<T>(
    reader: &mut Reader<'_>,
    read: impl FnOnce(Slot) -> (ReadStatus, T),
) -> Result<T> {
    let mut context = Context {
        input: Bytes {
            data: reader.bytes.as_ptr(),
            count: reader.bytes.len(),
        },
        position: ptr::from_mut(&mut reader.position),
        result: Ok(()),
    };
    let (status, value) = read(slot(&mut context));
    if let Some(message) = status.message() {
        context.result = Err(message.into());
    }
    context.result.map(|()| value)
}
pub(crate) unsafe fn input(context: Slot) -> (Bytes, *mut usize) {
    let c = unsafe { &*context.cast::<Context>() };
    (c.input, c.position)
}
pub(crate) unsafe fn fail(context: Slot, error: Box<dyn std::error::Error>) -> ReadStatus {
    unsafe { &mut *context.cast::<Context>() }.result = Err(error);
    ReadStatus::RustError
}
pub(in crate::codec) fn decode(reader: &mut Reader<'_>) -> Result<ir::Ty> {
    with_context(reader, |context| {
        let mut value = empty_type();
        let status = unsafe { parse(context, slot(&mut value), read_type) };
        (status, value)
    })
}
pub(crate) unsafe fn native_type(context: Slot, output: &mut ir::Ty) -> ReadStatus {
    unsafe { parse(context, slot(output), read_type) }
}
pub(crate) unsafe fn native_predicate(
    context: Slot,
    output: &mut ast::ContractPredicate,
) -> ReadStatus {
    unsafe { parse(context, slot(output), read_predicate) }
}
pub(crate) unsafe fn native_condition(
    context: Slot,
    output: &mut ast::OutputCondition,
) -> ReadStatus {
    unsafe { parse(context, slot(output), read_condition) }
}
pub(crate) unsafe extern "C" fn native_text(
    context: Slot,
    output: Slot,
    bytes: Bytes,
) -> ReadStatus {
    unsafe { text(context, output, bytes) }
}
pub(crate) fn empty_type() -> ir::Ty {
    ir::Ty {
        name: String::new(),
        arguments: Vec::new(),
        values: Vec::new(),
        size: 0,
        align: 0,
        components: Vec::new(),
        fields: Vec::new(),
        view: None,
        array: None,
        enumeration: false,
        result: false,
        variants: Vec::new(),
        dataset: None,
        cleanup: None,
        function: None,
    }
}
fn empty_name() -> ast::TypeName {
    ast::TypeName::named("")
}
fn empty_expression() -> ast::Expr {
    ast::Expr {
        kind: ast::ExprKind::StaticU32(0),
        span: ast::Span::default(),
    }
}
fn empty_condition() -> ast::OutputCondition {
    ast::OutputCondition::Bool(false)
}
fn empty_predicate() -> ast::ContractPredicate {
    ast::ContractPredicate::Nonzero(String::new())
}
fn access_value(value: AccessTag) -> ast::Access {
    match value {
        AccessTag::Value => ast::Access::Value,
        AccessTag::Read => ast::Access::Read,
        AccessTag::Write => ast::Access::Write,
        AccessTag::InOut => ast::Access::InOut,
        AccessTag::Atomic => ast::Access::Atomic,
    }
}
fn target_value(value: TargetTag) -> ast::Target {
    match value {
        TargetTag::Compute => ast::Target::Compute,
        TargetTag::Cpu => ast::Target::Cpu,
        TargetTag::Gpu => ast::Target::Gpu,
    }
}
unsafe extern "C" fn type_fields(output: Slot) -> TypeFields {
    let value = unsafe { &mut *output.cast::<ir::Ty>() };
    TypeFields {
        name: slot(&mut value.name),
        arguments: slot(&mut value.arguments),
        values: slot(&mut value.values),
        size: slot(&mut value.size),
        alignment: slot(&mut value.align),
        components: slot(&mut value.components),
        fields: slot(&mut value.fields),
        view: slot(&mut value.view),
        array: slot(&mut value.array),
        enumeration: slot(&mut value.enumeration),
        result: slot(&mut value.result),
        variants: slot(&mut value.variants),
        dataset: slot(&mut value.dataset),
        cleanup: slot(&mut value.cleanup),
        function: slot(&mut value.function),
    }
}
unsafe extern "C" fn name_fields(output: Slot) -> NameFields {
    let value = unsafe { &mut *output.cast::<ast::TypeName>() };
    NameFields {
        name: slot(&mut value.name),
        canonical: slot(&mut value.canonical),
        arguments: slot(&mut value.arguments),
        dataset: slot(&mut value.dataset),
        view: slot(&mut value.view),
        tile: slot(&mut value.tile),
        count: slot(&mut value.count),
        contracts: slot(&mut value.contracts),
    }
}
unsafe extern "C" fn function_fields(output: Slot) -> FunctionFields {
    let value = unsafe { &mut *output.cast::<ir::CFunctionType>() };
    FunctionFields {
        result: slot(&mut value.result),
        parameters: slot(&mut value.parameters),
        contracts: slot(&mut value.contracts),
    }
}
unsafe extern "C" fn constant_fields(output: Slot) -> ConstantFields {
    let value = unsafe { &mut *output.cast::<ast::OutputConstant>() };
    ConstantFields {
        ty: slot(&mut value.ty),
        bits: slot(&mut value.bits),
    }
}
unsafe fn make_text(context: Slot, bytes: Bytes) -> std::result::Result<String, ReadStatus> {
    let bytes = unsafe { std::slice::from_raw_parts(bytes.data, bytes.count) };
    match std::str::from_utf8(bytes) {
        Ok(value) => Ok(value.to_owned()),
        Err(error) => {
            unsafe { &mut *context.cast::<Context>() }.result = Err(error.into());
            Err(ReadStatus::RustError)
        }
    }
}
unsafe extern "C" fn text(context: Slot, output: Slot, bytes: Bytes) -> ReadStatus {
    match unsafe { make_text(context, bytes) } {
        Ok(value) => {
            unsafe { *output.cast::<String>() = value };
            ReadStatus::Success
        }
        Err(status) => status,
    }
}
unsafe extern "C" fn string_option(context: Slot, output: Slot, bytes: Bytes) -> ReadStatus {
    match unsafe { make_text(context, bytes) } {
        Ok(value) => {
            unsafe { *output.cast::<Option<String>>() = Some(value) };
            ReadStatus::Success
        }
        Err(status) => status,
    }
}
unsafe extern "C" fn push_u32(output: Slot, value: u32) {
    unsafe { &mut *output.cast::<Vec<u32>>() }.push(value);
}
unsafe extern "C" fn push_u64(output: Slot, value: u64) {
    unsafe { &mut *output.cast::<Vec<u64>>() }.push(value);
}
unsafe extern "C" fn push_component(output: Slot, value: ScalarTag, word: u32) {
    let scalar = match value {
        ScalarTag::F32 => ir::Scalar::F32,
        ScalarTag::U32 => ir::Scalar::U32,
        ScalarTag::I32 => ir::Scalar::I32,
        ScalarTag::Bool => ir::Scalar::Bool,
        ScalarTag::U64 => ir::Scalar::U64,
        ScalarTag::I64 => ir::Scalar::I64,
        ScalarTag::F64 => ir::Scalar::F64,
        ScalarTag::U8 => ir::Scalar::U8,
        ScalarTag::I8 => ir::Scalar::I8,
        ScalarTag::U16 => ir::Scalar::U16,
        ScalarTag::I16 => ir::Scalar::I16,
    };
    unsafe { &mut *output.cast::<Vec<(ir::Scalar, u32)>>() }.push((scalar, word));
}
unsafe extern "C" fn access(output: Slot, value: AccessTag) {
    unsafe { *output.cast::<ast::Access>() = access_value(value) };
}
unsafe extern "C" fn target(output: Slot, value: TargetTag) {
    unsafe { *output.cast::<ast::Target>() = target_value(value) };
}
unsafe extern "C" fn optional_access(output: Slot, value: AccessTag) {
    unsafe { *output.cast::<Option<ast::Access>>() = Some(access_value(value)) };
}
unsafe extern "C" fn expression_span(output: Slot, start: u64, end: u64) {
    unsafe { &mut *output.cast::<ast::Expr>() }.span = ast::Span {
        start: start as usize,
        end: end as usize,
    };
}

unsafe extern "C" fn push_type(context: Slot, output: Slot) -> ReadStatus {
    let mut value = empty_type();
    let status = unsafe { parse(context, slot(&mut value), read_type) };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<Vec<ir::Ty>>() }.push(value);
    }
    status
}
unsafe fn named_type(
    context: Slot,
    name: &mut String,
    word: &mut u32,
    ty: &mut ir::Ty,
) -> ReadStatus {
    let (input, position) = {
        let c = unsafe { &*context.cast::<Context>() };
        (c.input, c.position)
    };
    unsafe {
        read_named_type(
            input,
            position,
            slot(name),
            slot(word),
            slot(ty),
            host(context),
        )
    }
}
unsafe extern "C" fn push_field(context: Slot, output: Slot) -> ReadStatus {
    let mut name = String::new();
    let mut word = 0;
    let mut ty = empty_type();
    let status = unsafe { named_type(context, &mut name, &mut word, &mut ty) };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<Vec<(String, u32, Box<ir::Ty>)>>() }.push((
            name,
            word,
            Box::new(ty),
        ));
    }
    status
}
unsafe extern "C" fn push_variant(context: Slot, output: Slot) -> ReadStatus {
    let mut name = String::new();
    let mut discriminant = 0;
    let mut ty = empty_type();
    let status = unsafe { named_type(context, &mut name, &mut discriminant, &mut ty) };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<Vec<ir::Variant>>() }.push(ir::Variant {
            name,
            discriminant,
            ty: Box::new(ty),
        });
    }
    status
}
unsafe extern "C" fn push_parameter(context: Slot, output: Slot) -> ReadStatus {
    let mut access = ast::Access::Value;
    let mut ty = empty_name();
    let (input, position) = {
        let c = unsafe { &*context.cast::<Context>() };
        (c.input, c.position)
    };
    let status = unsafe {
        read_parameter(
            input,
            position,
            slot(&mut access),
            slot(&mut ty),
            host(context),
        )
    };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<Vec<(ast::Access, ast::TypeName)>>() }.push((access, ty));
    }
    status
}
unsafe extern "C" fn push_interval(context: Slot, output: Slot) -> ReadStatus {
    let mut lower = empty_condition();
    let mut upper = empty_condition();
    let (input, position) = {
        let c = unsafe { &*context.cast::<Context>() };
        (c.input, c.position)
    };
    let status = unsafe {
        read_interval(
            input,
            position,
            slot(&mut lower),
            slot(&mut upper),
            host(context),
        )
    };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<Vec<(ast::OutputCondition, ast::OutputCondition)>>() }
            .push((lower, upper));
    }
    status
}
unsafe extern "C" fn push_argument(context: Slot, output: Slot, kind: ArgumentTag) -> ReadStatus {
    match kind {
        ArgumentTag::Type => {
            let mut value = empty_name();
            let status = unsafe { parse(context, slot(&mut value), read_type_name) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<Vec<ast::GenericArgument>>() }
                    .push(ast::GenericArgument::Type(value));
            }
            status
        }
        ArgumentTag::Value => {
            let mut value = ast::Shape::Literal(0);
            let status = unsafe { parse(context, slot(&mut value), read_shape) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<Vec<ast::GenericArgument>>() }
                    .push(ast::GenericArgument::Value(value));
            }
            status
        }
    }
}
unsafe extern "C" fn push_expression(context: Slot, output: Slot) -> ReadStatus {
    let mut value = empty_expression();
    let status = unsafe { parse(context, slot(&mut value), read_expression) };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<Vec<ast::Expr>>() }.push(value);
    }
    status
}
unsafe extern "C" fn push_predicate(context: Slot, output: Slot) -> ReadStatus {
    let mut value = empty_predicate();
    let status = unsafe { parse(context, slot(&mut value), read_predicate) };
    if status == ReadStatus::Success {
        unsafe { &mut *output.cast::<Vec<ast::ContractPredicate>>() }.push(value);
    }
    status
}
unsafe extern "C" fn type_box(context: Slot, output: Slot) -> ReadStatus {
    let mut value = empty_type();
    let status = unsafe { parse(context, slot(&mut value), read_type) };
    if status == ReadStatus::Success {
        unsafe { *output.cast::<Option<Box<ir::Ty>>>() = Some(Box::new(value)) };
    }
    status
}
unsafe extern "C" fn type_name_box(context: Slot, output: Slot) -> ReadStatus {
    let mut value = empty_name();
    let status = unsafe { parse(context, slot(&mut value), read_type_name) };
    if status == ReadStatus::Success {
        unsafe { *output.cast::<Option<Box<ast::TypeName>>>() = Some(Box::new(value)) };
    }
    status
}
unsafe extern "C" fn function_box(context: Slot, output: Slot) -> ReadStatus {
    let mut value = ir::CFunctionType {
        result: empty_name(),
        parameters: Vec::new(),
        contracts: Vec::new(),
    };
    let status = unsafe { parse(context, slot(&mut value), read_function) };
    if status == ReadStatus::Success {
        unsafe { *output.cast::<Option<Box<ir::CFunctionType>>>() = Some(Box::new(value)) };
    }
    status
}
unsafe extern "C" fn shape_option(context: Slot, output: Slot) -> ReadStatus {
    let mut value = ast::Shape::Literal(0);
    let status = unsafe { parse(context, slot(&mut value), read_shape) };
    if status == ReadStatus::Success {
        unsafe { *output.cast::<Option<ast::Shape>>() = Some(value) };
    }
    status
}
unsafe extern "C" fn predicates_option(context: Slot, output: Slot) -> ReadStatus {
    let mut value = Vec::<ast::ContractPredicate>::new();
    let status = unsafe { parse(context, slot(&mut value), read_predicates) };
    if status == ReadStatus::Success {
        unsafe {
            *output.cast::<Option<Box<Vec<ast::ContractPredicate>>>>() = Some(Box::new(value))
        };
    }
    status
}
unsafe extern "C" fn arguments_option(context: Slot, output: Slot) -> ReadStatus {
    let mut value = Vec::<ast::GenericArgument>::new();
    let status = unsafe { parse(context, slot(&mut value), read_arguments) };
    if status == ReadStatus::Success {
        unsafe { *output.cast::<Option<Vec<ast::GenericArgument>>>() = Some(value) };
    }
    status
}
unsafe extern "C" fn shape_expression(context: Slot, output: Slot) -> ReadStatus {
    let mut value = empty_expression();
    let status = unsafe { parse(context, slot(&mut value), read_expression) };
    if status == ReadStatus::Success {
        unsafe { *output.cast::<ast::Shape>() = ast::Shape::Expression(Box::new(value)) };
    }
    status
}
unsafe extern "C" fn shape_literal(output: Slot, value: u32) {
    unsafe { *output.cast::<ast::Shape>() = ast::Shape::Literal(value) };
}
unsafe extern "C" fn shape_name(context: Slot, output: Slot, bytes: Bytes) -> ReadStatus {
    match unsafe { make_text(context, bytes) } {
        Ok(value) => {
            unsafe { *output.cast::<ast::Shape>() = ast::Shape::Constant(value) };
            ReadStatus::Success
        }
        Err(status) => status,
    }
}

unsafe fn expr_payload(context: Slot, kind: ExpressionTag, fields: ExpressionFields) -> ReadStatus {
    let (input, position) = {
        let c = unsafe { &*context.cast::<Context>() };
        (c.input, c.position)
    };
    unsafe { read_expression_payload(kind, fields, input, position, host(context)) }
}
unsafe extern "C" fn expression(context: Slot, output: Slot, kind: ExpressionTag) -> ReadStatus {
    use ast::ExprKind as K;
    match kind {
        ExpressionTag::Number => {
            let mut text = String::new();
            let fields = ExpressionFields {
                text: slot(&mut text),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::Number(text);
            }
            status
        }
        ExpressionTag::StaticU32 => {
            let mut literal = 0u32;
            let fields = ExpressionFields {
                literal: slot(&mut literal),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::StaticU32(literal);
            }
            status
        }
        ExpressionTag::Bool => {
            let mut literal = false;
            let fields = ExpressionFields {
                literal: slot(&mut literal),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::Bool(literal);
            }
            status
        }
        ExpressionTag::Name => {
            let mut text = String::new();
            let fields = ExpressionFields {
                text: slot(&mut text),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::Name(text);
            }
            status
        }
        ExpressionTag::Member => {
            let mut a = empty_expression();
            let mut text = String::new();
            let fields = ExpressionFields {
                a: slot(&mut a),
                text: slot(&mut text),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::Member(Box::new(a), text);
            }
            status
        }
        ExpressionTag::Index => {
            let mut a = empty_expression();
            let mut b = empty_expression();
            let fields = ExpressionFields {
                a: slot(&mut a),
                b: slot(&mut b),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind =
                    K::Index(Box::new(a), Box::new(b));
            }
            status
        }
        ExpressionTag::Call => {
            let mut text = String::new();
            let mut arguments = Vec::<ast::Expr>::new();
            let fields = ExpressionFields {
                text: slot(&mut text),
                arguments: slot(&mut arguments),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::Call(text, arguments);
            }
            status
        }
        ExpressionTag::Invoke => {
            let mut a = empty_expression();
            let mut arguments = Vec::<ast::Expr>::new();
            let fields = ExpressionFields {
                a: slot(&mut a),
                arguments: slot(&mut arguments),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind =
                    K::Invoke(Box::new(a), arguments);
            }
            status
        }
        ExpressionTag::Construct => {
            let mut ty = empty_name();
            let mut arguments = Vec::<ast::Expr>::new();
            let fields = ExpressionFields {
                ty: slot(&mut ty),
                arguments: slot(&mut arguments),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::Construct(ty, arguments);
            }
            status
        }
        ExpressionTag::Specialize => {
            let mut text = String::new();
            let mut types = Vec::<ast::GenericArgument>::new();
            let mut arguments = Vec::<ast::Expr>::new();
            let fields = ExpressionFields {
                text: slot(&mut text),
                types: slot(&mut types),
                arguments: slot(&mut arguments),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind =
                    K::Specialize(text, types, arguments);
            }
            status
        }
        ExpressionTag::FunctionRef => {
            let mut text = String::new();
            let mut types = Vec::<ast::GenericArgument>::new();
            let fields = ExpressionFields {
                text: slot(&mut text),
                types: slot(&mut types),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::FunctionRef(text, types);
            }
            status
        }
        ExpressionTag::Cooperate => {
            let mut text = String::new();
            let mut types = None::<Vec<ast::GenericArgument>>;
            let mut arguments = Vec::<ast::Expr>::new();
            let fields = ExpressionFields {
                text: slot(&mut text),
                types: slot(&mut types),
                arguments: slot(&mut arguments),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind =
                    K::Cooperate(text, types, arguments);
            }
            status
        }
        ExpressionTag::Target => {
            let mut target = ast::Target::Compute;
            let mut a = empty_expression();
            let fields = ExpressionFields {
                target: slot(&mut target),
                a: slot(&mut a),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::Target(target, Box::new(a));
            }
            status
        }
        ExpressionTag::Unary => {
            let mut text = String::new();
            let mut a = empty_expression();
            let fields = ExpressionFields {
                text: slot(&mut text),
                a: slot(&mut a),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind = K::Unary(text, Box::new(a));
            }
            status
        }
        ExpressionTag::Binary => {
            let mut text = String::new();
            let mut a = empty_expression();
            let mut b = empty_expression();
            let fields = ExpressionFields {
                text: slot(&mut text),
                a: slot(&mut a),
                b: slot(&mut b),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind =
                    K::Binary(text, Box::new(a), Box::new(b));
            }
            status
        }
        ExpressionTag::Select => {
            let mut a = empty_expression();
            let mut b = empty_expression();
            let mut c = empty_expression();
            let fields = ExpressionFields {
                a: slot(&mut a),
                b: slot(&mut b),
                c: slot(&mut c),
                ..ExpressionFields::default()
            };
            let status = unsafe { expr_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { &mut *output.cast::<ast::Expr>() }.kind =
                    K::Select(Box::new(a), Box::new(b), Box::new(c));
            }
            status
        }
    }
}

unsafe fn condition_payload(
    context: Slot,
    kind: ConditionTag,
    fields: ConditionFields,
) -> ReadStatus {
    let (input, position) = {
        let c = unsafe { &*context.cast::<Context>() };
        (c.input, c.position)
    };
    unsafe { read_condition_payload(kind, fields, input, position, host(context)) }
}
unsafe extern "C" fn condition(context: Slot, output: Slot, kind: ConditionTag) -> ReadStatus {
    use ast::OutputCondition as K;
    match kind {
        ConditionTag::Name => {
            let mut text = String::new();
            let fields = ConditionFields {
                text: slot(&mut text),
                ..ConditionFields::default()
            };
            let status = unsafe { condition_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::OutputCondition>() = K::Name(text) };
            }
            status
        }
        ConditionTag::Number => {
            let mut text = String::new();
            let fields = ConditionFields {
                text: slot(&mut text),
                ..ConditionFields::default()
            };
            let status = unsafe { condition_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::OutputCondition>() = K::Number(text) };
            }
            status
        }
        ConditionTag::Bool => {
            let mut literal = false;
            let fields = ConditionFields {
                literal: slot(&mut literal),
                ..ConditionFields::default()
            };
            let status = unsafe { condition_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::OutputCondition>() = K::Bool(literal) };
            }
            status
        }
        ConditionTag::Member => {
            let mut a = empty_condition();
            let mut text = String::new();
            let fields = ConditionFields {
                a: slot(&mut a),
                text: slot(&mut text),
                ..ConditionFields::default()
            };
            let status = unsafe { condition_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::OutputCondition>() = K::Member(Box::new(a), text) };
            }
            status
        }
        ConditionTag::Unary => {
            let mut text = String::new();
            let mut a = empty_condition();
            let fields = ConditionFields {
                text: slot(&mut text),
                a: slot(&mut a),
                ..ConditionFields::default()
            };
            let status = unsafe { condition_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::OutputCondition>() = K::Unary(text, Box::new(a)) };
            }
            status
        }
        ConditionTag::Binary => {
            let mut text = String::new();
            let mut a = empty_condition();
            let mut b = empty_condition();
            let fields = ConditionFields {
                text: slot(&mut text),
                a: slot(&mut a),
                b: slot(&mut b),
                ..ConditionFields::default()
            };
            let status = unsafe { condition_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<ast::OutputCondition>() =
                        K::Binary(text, Box::new(a), Box::new(b))
                };
            }
            status
        }
        ConditionTag::StaticU32 => {
            let mut literal = 0u32;
            let fields = ConditionFields {
                literal: slot(&mut literal),
                ..ConditionFields::default()
            };
            let status = unsafe { condition_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::OutputCondition>() = K::StaticU32(literal) };
            }
            status
        }
        ConditionTag::Constant => {
            let mut constant = ast::OutputConstant::new(empty_type(), Vec::new());
            let fields = ConditionFields {
                constant: slot(&mut constant),
                ..ConditionFields::default()
            };
            let status = unsafe { condition_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::OutputCondition>() = K::Constant(constant) };
            }
            status
        }
    }
}

unsafe fn predicate_payload(
    context: Slot,
    kind: PredicateTag,
    fields: PredicateFields,
) -> ReadStatus {
    let (input, position) = {
        let c = unsafe { &*context.cast::<Context>() };
        (c.input, c.position)
    };
    unsafe { read_predicate_payload(kind, fields, input, position, host(context)) }
}
unsafe extern "C" fn predicate(context: Slot, output: Slot, kind: PredicateTag) -> ReadStatus {
    use ast::ContractPredicate as K;
    match kind {
        PredicateTag::Prefix => {
            let mut first = String::new();
            let mut second = String::new();
            let mut third = String::new();
            let fields = PredicateFields {
                first: slot(&mut first),
                second: slot(&mut second),
                third: slot(&mut third),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<ast::ContractPredicate>() = K::Prefix {
                        data: first,
                        count: second,
                        field: third,
                    }
                };
            }
            status
        }
        PredicateTag::Nonzero => {
            let mut first = String::new();
            let fields = PredicateFields {
                first: slot(&mut first),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::ContractPredicate>() = K::Nonzero(first) };
            }
            status
        }
        PredicateTag::Nonempty => {
            let mut first = String::new();
            let fields = PredicateFields {
                first: slot(&mut first),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::ContractPredicate>() = K::Nonempty(first) };
            }
            status
        }
        PredicateTag::Contiguous => {
            let mut first = String::new();
            let fields = PredicateFields {
                first: slot(&mut first),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::ContractPredicate>() = K::Contiguous(first) };
            }
            status
        }
        PredicateTag::Memory => {
            let mut access = None::<ast::Access>;
            let fields = PredicateFields {
                access: slot(&mut access),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe { *output.cast::<ast::ContractPredicate>() = K::Memory(access) };
            }
            status
        }
        PredicateTag::Index => {
            let mut first = String::new();
            let mut second = String::new();
            let fields = PredicateFields {
                first: slot(&mut first),
                second: slot(&mut second),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<ast::ContractPredicate>() = K::Index {
                        value: first,
                        resource: second,
                    }
                };
            }
            status
        }
        PredicateTag::Range => {
            let mut first = String::new();
            let mut intervals = Vec::<(ast::OutputCondition, ast::OutputCondition)>::new();
            let fields = PredicateFields {
                first: slot(&mut first),
                intervals: slot(&mut intervals),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<ast::ContractPredicate>() = K::Range {
                        value: first,
                        intervals,
                    }
                };
            }
            status
        }
        PredicateTag::SameLength => {
            let mut first = String::new();
            let mut second = String::new();
            let fields = PredicateFields {
                first: slot(&mut first),
                second: slot(&mut second),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<ast::ContractPredicate>() = K::SameLength {
                        left: first,
                        right: second,
                    }
                };
            }
            status
        }
        PredicateTag::Initialized => {
            let mut first = String::new();
            let mut condition = empty_condition();
            let fields = PredicateFields {
                first: slot(&mut first),
                condition: slot(&mut condition),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<ast::ContractPredicate>() = K::Initialized {
                        value: first,
                        condition,
                    }
                };
            }
            status
        }
        PredicateTag::Cleanup => {
            let mut first = String::new();
            let mut condition = empty_condition();
            let fields = PredicateFields {
                first: slot(&mut first),
                condition: slot(&mut condition),
                ..PredicateFields::default()
            };
            let status = unsafe { predicate_payload(context, kind, fields) };
            if status == ReadStatus::Success {
                unsafe {
                    *output.cast::<ast::ContractPredicate>() = K::Cleanup {
                        value: first,
                        condition,
                    }
                };
            }
            status
        }
    }
}

fn host(context: Slot) -> ReadHost {
    ReadHost {
        context,
        type_fields,
        name_fields,
        function_fields,
        constant_fields,
        text,
        string_option,
        push_u32,
        push_u64,
        push_component,
        push_type,
        push_field,
        push_variant,
        push_parameter,
        push_interval,
        push_argument,
        push_expression,
        push_predicate,
        type_box,
        type_name_box,
        function_box,
        shape_option,
        predicates_option,
        arguments_option,
        shape_expression,
        shape_literal,
        shape_name,
        expression,
        condition,
        predicate,
        access,
        target,
        optional_access,
        expression_span,
    }
}
