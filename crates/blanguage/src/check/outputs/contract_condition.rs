//! B traverses the original conditions; native queries borrow World and Ty.
//! Result nodes, types, bits and member indices keep their original native owners.
use super::*;
use std::{ffi::c_void, ptr};
type Address = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Bytes { data: *const u8, count: u64 }
impl Bytes {
    fn of(text: &str) -> Self { Self { data: text.as_ptr(), count: text.len() as u64 } }
    unsafe fn text<'a>(self) -> &'a str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.data, self.count as usize)) }
    }
}
#[repr(C)]
struct NativeResult { value: Address, ok: u32 }
#[repr(C)]
struct TypeResult { ty: Address, literal: u32, ok: u32 }
#[repr(C)]
struct Node { kind: u32, flag: u32, text: Bytes, left: Address, right: Address }
#[repr(C)]
struct Constant { ty: Address, bits: Address, count: u64 }
#[repr(C)]
struct TypeInfo { scalar: u32, width: u32, is_array: u32, is_variant: u32, components: u64, length: u64 }
#[repr(C)]
struct Lookup { key: Address, found: u32, constant: u32, is_public: u32 }
#[repr(C)]
struct Variant { found: u32, discriminant: u32 }
#[repr(C)]
struct Member { ty: Address, indices: Address, owner: Address, count: u64, ok: u32 }
#[repr(C)]
struct BinaryRule { ty: Address, kind: u32, scalar: u32, cast_left: u32, cast_right: u32, comparison: u32, operation: u32, ok: u32 }
#[repr(C)]
struct UnaryRule { operation: u32, ok: u32 }
#[repr(C)]
struct Term { value: Address, original: Address, ty: Address, literal: u32, ok: u32 }
#[repr(C)]
struct Host {
    node: unsafe extern "C" fn(Address) -> Node,
    constant: unsafe extern "C" fn(Address) -> Constant,
    broadcast_bit: unsafe extern "C" fn(Address, u64) -> u64,
    type_info: unsafe extern "C" fn(Address) -> TypeInfo,
    copy_node: unsafe extern "C" fn(Address) -> Address,
    copy_type: unsafe extern "C" fn(Address) -> Address,
    drop_node: unsafe extern "C" fn(Address),
    drop_type: unsafe extern "C" fn(Address),
    scalar_type: unsafe extern "C" fn(u32, u64) -> Address,
    bits: unsafe extern "C" fn(u64) -> Address,
    push_bit: unsafe extern "C" fn(Address, u64),
    drop_bits: unsafe extern "C" fn(Address),
    make_constant: unsafe extern "C" fn(Address, Address, Address) -> Address,
    make_member: unsafe extern "C" fn(Address, Bytes) -> Address,
    make_unary: unsafe extern "C" fn(Bytes, Address) -> Address,
    make_binary: unsafe extern "C" fn(Bytes, Address, Address) -> Address,
    parameter: unsafe extern "C" fn(Address, Bytes) -> u32,
    lookup: unsafe extern "C" fn(Address, Bytes) -> Lookup,
    evaluate: unsafe extern "C" fn(Address, Address) -> NativeResult,
    variant: unsafe extern "C" fn(Address, Bytes) -> Variant,
    member: unsafe extern "C" fn(Address, Address, Bytes) -> Member,
    drop_indices: unsafe extern "C" fn(Address),
    path_type: unsafe extern "C" fn(Address, Address) -> NativeResult,
    unary_type: unsafe extern "C" fn(Address, Bytes, Address) -> UnaryRule,
    binary_type: unsafe extern "C" fn(Address, Bytes, Address, u32, Address, u32) -> BinaryRule,
    same_type: unsafe extern "C" fn(Address, Address, Address) -> u32,
    divide: unsafe extern "C" fn(u32, u64, u64, u32) -> u64,
    fail: unsafe extern "C" fn(Address, u32, Bytes),
}
struct Context<'a> { source: &'a Source, span: Span, error: Option<Diagnostic> }
#[repr(C)]
struct BindingContext<'a> { common: Context<'a>, world: &'a World, definition: Contract<'a> }
#[repr(C)]
struct PathContext<'a> { common: Context<'a>, path: &'a dyn Fn(&OutputCondition) -> Result<Ty> }
impl Context<'_> {
    fn result<T>(&mut self, result: Result<T>) -> NativeResult {
        match result {
            Ok(value) => NativeResult { value: own(value), ok: 1 },
            Err(error) => { self.error = Some(error); NativeResult { value: ptr::null_mut(), ok: 0 } }
        }
    }
}
fn own<T>(value: T) -> Address { Box::into_raw(Box::new(value)).cast() }
unsafe fn borrowed<'a, T>(value: Address) -> &'a T { unsafe { &*value.cast() } }
unsafe fn take<T>(value: Address) -> T { unsafe { *Box::from_raw(value.cast::<T>()) } }
unsafe fn context<'a>(value: Address) -> &'a mut Context<'a> { unsafe { &mut *value.cast() } }
unsafe fn binding_context<'a>(value: Address) -> &'a mut BindingContext<'a> {
    unsafe { &mut *value.cast() }
}
unsafe fn path_context<'a>(value: Address) -> &'a mut PathContext<'a> {
    unsafe { &mut *value.cast() }
}
fn address<T>(value: &T) -> Address { ptr::from_ref(value).cast_mut().cast() }
const SCALARS: [Scalar; 11] = [Scalar::F32, Scalar::U32, Scalar::I32, Scalar::Bool,
    Scalar::U64, Scalar::I64, Scalar::F64, Scalar::U8, Scalar::I8, Scalar::U16, Scalar::I16];

unsafe extern "C" fn node(value: Address) -> Node {
    let mut result = Node { kind: 0, flag: 0, text: Bytes::of(""), left: ptr::null_mut(), right: ptr::null_mut() };
    match unsafe { borrowed::<OutputCondition>(value) } {
        OutputCondition::Name(text) => { result.kind = 0; result.text = Bytes::of(text); }
        OutputCondition::Number(text) => { result.kind = 1; result.text = Bytes::of(text); }
        OutputCondition::Bool(value) => { result.kind = 2; result.flag = u32::from(*value); }
        OutputCondition::StaticU32(value) => { result.kind = 3; result.flag = *value; }
        OutputCondition::Constant(value) => { result.kind = 4; result.left = address(value); }
        OutputCondition::Member(base, field) => { result.kind = 5; result.left = address(base.as_ref()); result.text = Bytes::of(field); }
        OutputCondition::Unary(op, value) => { result.kind = 6; result.left = address(value.as_ref()); result.text = Bytes::of(op); }
        OutputCondition::Binary(op, left, right) => { result.kind = 7; result.left = address(left.as_ref()); result.right = address(right.as_ref()); result.text = Bytes::of(op); }
    }
    result
}
unsafe extern "C" fn constant(value: Address) -> Constant {
    let value = unsafe { borrowed::<OutputConstant>(value) };
    Constant { ty: address(&value.ty), bits: value.bits.as_ptr().cast_mut().cast(), count: value.bits.len() as u64 }
}
unsafe extern "C" fn type_info(value: Address) -> TypeInfo {
    let ty = unsafe { borrowed::<Ty>(value) };
    let scalar = ty.components.first().map(|value| value.0);
    TypeInfo { scalar: scalar.map_or(u32::MAX, |scalar| scalar as u32), width: scalar.map_or(0, Scalar::bit_width),
        is_array: u32::from(ty.array.is_some()), is_variant: u32::from(ty.has_variants()),
        components: ty.components.len() as u64, length: ty.values.first().copied().unwrap_or(0) as u64 }
}
unsafe extern "C" fn broadcast_bit(value: Address, index: u64) -> u64 {
    let value = unsafe { borrowed::<OutputConstant>(value) };
    value.bits[index as usize % value.bits.len()]
}
unsafe extern "C" fn copy_node(value: Address) -> Address { own(unsafe { borrowed::<OutputCondition>(value) }.clone()) }
unsafe extern "C" fn copy_type(value: Address) -> Address { own(unsafe { borrowed::<Ty>(value) }.clone()) }
unsafe extern "C" fn drop_node(value: Address) { drop(unsafe { take::<OutputCondition>(value) }); }
unsafe extern "C" fn drop_type(value: Address) { drop(unsafe { take::<Ty>(value) }); }
unsafe extern "C" fn scalar_type(scalar: u32, count: u64) -> Address { own(Ty::vector(SCALARS[scalar as usize], count as usize)) }
unsafe extern "C" fn bits(count: u64) -> Address { own(Vec::<Bits>::with_capacity(count as usize)) }
unsafe extern "C" fn push_bit(value: Address, bits: u64) { unsafe { &mut *value.cast::<Vec<Bits>>() }.push(bits); }
unsafe extern "C" fn drop_bits(value: Address) { drop(unsafe { take::<Vec<Bits>>(value) }); }
// Consumes the original result containers. Origin only borrows the source
// constant until the newly owned result has its original GeneratedOrigin.
unsafe extern "C" fn make_constant(ty: Address, bits: Address, origin: Address) -> Address {
    let generated_origin = if origin.is_null() { None } else {
        let value = unsafe { borrowed::<OutputConstant>(origin) };
        value.generated_origin.clone()
    };
    own(OutputCondition::Constant(OutputConstant { ty: unsafe { take(ty) }, bits: unsafe { take(bits) }, generated_origin }))
}
unsafe extern "C" fn make_member(base: Address, field: Bytes) -> Address {
    own(OutputCondition::Member(unsafe { Box::from_raw(base.cast()) }, unsafe { field.text() }.into()))
}
unsafe extern "C" fn make_unary(op: Bytes, value: Address) -> Address {
    own(OutputCondition::Unary(unsafe { op.text() }.into(), unsafe { Box::from_raw(value.cast()) }))
}
unsafe extern "C" fn make_binary(op: Bytes, left: Address, right: Address) -> Address {
    own(OutputCondition::Binary(unsafe { op.text() }.into(), unsafe { Box::from_raw(left.cast()) }, unsafe { Box::from_raw(right.cast()) }))
}
unsafe extern "C" fn parameter(ctx: Address, name: Bytes) -> u32 {
    let definition = unsafe { &binding_context(ctx).definition };
    u32::from(definition.has_parameter(unsafe { name.text() }))
}
unsafe extern "C" fn lookup(ctx: Address, name: Bytes) -> Lookup {
    let ctx = unsafe { binding_context(ctx) };
    let (world, definition) = (ctx.world, &ctx.definition);
    let found = world.resolve(unsafe { name.text() }, definition.module, definition.imports, true).unique();
    if let Some((key, definition)) = found.as_ref().and_then(|key| world.functions.get_key_value(key)) {
        Lookup { key: address(key), found: 1, constant: u32::from(definition.function.kind() == FunctionKind::Constant), is_public: u32::from(definition.public) }
    } else { Lookup { key: ptr::null_mut(), found: 0, constant: 0, is_public: 0 } }
}
unsafe extern "C" fn evaluate(ctx: Address, key: Address) -> NativeResult {
    let ctx = unsafe { binding_context(ctx) };
    let world = ctx.world;
    let result = constant::evaluate(world, unsafe { borrowed::<String>(key) }, &[])
        .map(|(ty, bits, generated_origin)| OutputCondition::Constant(OutputConstant { ty, bits, generated_origin }));
    ctx.common.result(result)
}
unsafe extern "C" fn variant(ty: Address, field: Bytes) -> Variant {
    let ty = unsafe { borrowed::<Ty>(ty) };
    match ty.variants.iter().find(|variant| variant.name == unsafe { field.text() }) {
        Some(variant) => Variant { found: 1, discriminant: variant.discriminant },
        None => Variant { found: 0, discriminant: 0 },
    }
}
unsafe extern "C" fn member(ctx: Address, ty: Address, field: Bytes) -> Member {
    let ctx = unsafe { context(ctx) };
    match member_type(unsafe { borrowed(ty) }, unsafe { field.text() }, ctx.source, ctx.span) {
        Ok((ty, indices)) => {
            let data = indices.as_ptr().cast_mut().cast(); let count = indices.len() as u64;
            Member { ty: own(ty), indices: data, owner: own(indices), count, ok: 1 }
        }
        Err(error) => { ctx.error = Some(error); Member { ty: ptr::null_mut(), indices: ptr::null_mut(), owner: ptr::null_mut(), count: 0, ok: 0 } }
    }
}
unsafe extern "C" fn drop_indices(value: Address) { drop(unsafe { take::<Vec<usize>>(value) }); }
unsafe extern "C" fn path_type(ctx: Address, value: Address) -> NativeResult {
    let ctx = unsafe { path_context(ctx) };
    let result = (ctx.path)(unsafe { borrowed(value) }); ctx.common.result(result)
}
unsafe extern "C" fn unary_type(ctx: Address, op: Bytes, ty: Address) -> UnaryRule {
    let ctx = unsafe { context(ctx) };
    match operators::unary(unsafe { op.text() }, unsafe { borrowed(ty) }, ctx.source, ctx.span) {
        Ok(operation) => UnaryRule { operation: match operation {
            None => 0, Some(UnaryOp::FloatNegate) => 1, Some(UnaryOp::SignedNegate) => 2,
            Some(UnaryOp::UnsignedNegate) => 3, Some(UnaryOp::LogicalNot) => 4,
            Some(UnaryOp::BitNot) => 5, Some(UnaryOp::Bitcast) => 0,
        }, ok: 1 },
        Err(error) => { ctx.error = Some(error); UnaryRule { operation: 0, ok: 0 } }
    }
}
fn operation(op: Option<BinaryOp>) -> u32 {
    use BinaryOp::*; use Comparison as C;
    match op {
        None => 0, Some(FAdd) => 1, Some(FSub) => 2, Some(FMul) => 3, Some(FDiv) => 4,
        Some(IAdd) => 5, Some(ISub) => 6, Some(IMul) => 7,
        Some(Compare(C::Equal)) => 8, Some(Compare(C::NotEqual)) => 9,
        Some(Compare(C::Less)) => 10, Some(Compare(C::Greater)) => 11,
        Some(Compare(C::LessEqual)) => 12, Some(Compare(C::GreaterEqual)) => 13,
        Some(And) => 14, Some(Or) => 15, Some(Xor) => 16, Some(ShiftLeft) => 17,
        Some(ShiftRightLogical) => 18, Some(ShiftRightArithmetic) => 19,
    }
}
unsafe extern "C" fn binary_type(ctx: Address, op: Bytes, a: Address, a_literal: u32, b: Address, b_literal: u32) -> BinaryRule {
    let ctx = unsafe { context(ctx) };
    match operators::binary(unsafe { op.text() }, unsafe { borrowed(a) }, a_literal != 0,
        unsafe { borrowed(b) }, b_literal != 0, ctx.source, ctx.span) {
        Ok((ty, kind)) => {
            let mut result = BinaryRule { ty: own(ty), kind: 0, scalar: 0, cast_left: 0, cast_right: 0, comparison: 0, operation: 0, ok: 1 };
            match kind {
                operators::Binary::Numeric { operands, comparison, operation: op } => {
                    result.scalar = operands.scalar as u32; result.cast_left = u32::from(operands.cast_left);
                    result.cast_right = u32::from(operands.cast_right); result.comparison = u32::from(comparison); result.operation = operation(op);
                }
                operators::Binary::Identity => result.kind = 1,
                operators::Binary::Handle => result.kind = 2,
                operators::Binary::Matrix(kind) => { use matrix::Binary::*; match kind {
                    Elements(_) => result.kind = 3, Product { .. } => result.kind = 4,
                    MatrixScalar { cast, .. } => { result.kind = 5; result.cast_right = u32::from(cast); }
                    ScalarMatrix { cast, .. } => { result.kind = 6; result.cast_left = u32::from(cast); }
                    MatrixVector { .. } => result.kind = 7, VectorMatrix { .. } => result.kind = 8,
                } }
            }
            result
        }
        Err(error) => { ctx.error = Some(error); BinaryRule { ty: ptr::null_mut(), kind: 0, scalar: 0, cast_left: 0, cast_right: 0, comparison: 0, operation: 0, ok: 0 } }
    }
}
unsafe extern "C" fn same_type(ctx: Address, a: Address, b: Address) -> u32 {
    let ctx = unsafe { context(ctx) };
    match operators::same(unsafe { borrowed(a) }, unsafe { borrowed(b) }, ctx.source, ctx.span) {
        Ok(()) => 1, Err(error) => { ctx.error = Some(error); 0 }
    }
}
unsafe extern "C" fn divide(scalar: u32, a: u64, b: u64, remainder: u32) -> u64 {
    // B has applied the original B152/nonzero rule. Scalar owns signed bit
    // interpretation and truncation at the existing scalar_fold ABI.
    constant::divide(SCALARS[scalar as usize], a, unsafe { std::num::NonZeroU64::new_unchecked(b) }, remainder != 0)
}
unsafe extern "C" fn fail(ctx: Address, kind: u32, text: Bytes) {
    let ctx = unsafe { context(ctx) }; let text = unsafe { text.text() };
    let (code, message) = match kind {
        1 => ("B225", format!("输出条件没有编译期常量 {text}")),
        2 => ("B225", "公开输出条件的模块常量必须在接口中可见".into()),
        3 => ("B225", "输出条件使用声明的变体名称".into()),
        4 => ("B152", "整数除数的每个分量都必须是非零常量或在当前作用域已证明非零的值".into()),
        _ => {
            let scalar = SCALARS[((kind - 100) % 16) as usize];
            match (kind - 100) / 16 {
                1 => ("B123", format!("非法 {} 常量 {text}", scalar.name())),
                2 => ("B121", format!("非法 {} 常量 {text}", scalar.name())),
                3 => ("B122", format!("浮点常量超出有限 {} 范围", scalar.name())),
                _ => ("B180", "负整数字面量超出其有符号类型范围".into()),
            }
        }
    };
    ctx.error = Some(Diagnostic { source: ctx.source.clone(), span: ctx.span, code, message });
}
fn host() -> Host {
    Host { node, constant, broadcast_bit, type_info, copy_node, copy_type, drop_node, drop_type, scalar_type,
        bits, push_bit, drop_bits, make_constant, make_member, make_unary, make_binary,
        parameter, lookup, evaluate, variant, member, drop_indices, path_type, unary_type, binary_type, same_type, divide, fail }
}
unsafe extern "C" {
    #[link_name = "bootstrap.contract_condition::bind"]
    fn source_bind(ctx: Address, value: Address, is_public: u32, host: Host) -> NativeResult;
    #[link_name = "bootstrap.contract_condition::identity"]
    fn source_identity(ctx: Address, value: Address, host: Host) -> Term;
    #[link_name = "bootstrap.contract_condition::condition_type"]
    fn source_condition_type(ctx: Address, value: Address, host: Host) -> TypeResult;
}
pub(super) fn bind_constants(world: &World, definition: Contract<'_>, value: &OutputCondition, span: Span) -> Result<OutputCondition> {
    let mut ctx = BindingContext { common: Context { source: definition.source, span, error: None }, world, definition };
    let result = unsafe { source_bind(ptr::from_mut(&mut ctx).cast(), address(value), u32::from(definition.public), host()) };
    if result.ok == 0 { Err(ctx.common.error.unwrap()) } else { Ok(unsafe { take(result.value) }) }
}
pub(super) fn identity(value: &OutputCondition, path: &impl Fn(&OutputCondition) -> Result<Ty>, source: &Source, span: Span) -> Result<OutputCondition> {
    let mut ctx = PathContext { common: Context { source, span, error: None }, path };
    let result = unsafe { source_identity(ptr::from_mut(&mut ctx).cast(), address(value), host()) };
    if result.ok == 0 { Err(ctx.common.error.unwrap()) } else {
        if !result.ty.is_null() { unsafe { drop_type(result.ty) }; }
        Ok(unsafe { take(result.value) })
    }
}
pub(super) fn condition_type(value: &OutputCondition, path: &impl Fn(&OutputCondition) -> Result<Ty>, source: &Source, span: Span) -> Result<(Ty, bool)> {
    let mut ctx = PathContext { common: Context { source, span, error: None }, path };
    let result = unsafe { source_condition_type(ptr::from_mut(&mut ctx).cast(), address(value), host()) };
    if result.ok == 0 { Err(ctx.common.error.unwrap()) } else { Ok((unsafe { take(result.ty) }, result.literal != 0)) }
}
