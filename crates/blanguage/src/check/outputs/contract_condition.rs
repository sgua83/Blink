//! B traverses the original conditions; World children finish outside native stepping.
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
#[derive(Clone, Copy)]
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
    variant: unsafe extern "C" fn(Address, Bytes) -> Variant,
    member: unsafe extern "C" fn(Address, Address, Bytes) -> Member,
    drop_indices: unsafe extern "C" fn(Address),
    unary_type: unsafe extern "C" fn(Address, Bytes, Address) -> UnaryRule,
    binary_type: unsafe extern "C" fn(Address, Bytes, Address, u32, Address, u32) -> BinaryRule,
    same_type: unsafe extern "C" fn(Address, Address, Address) -> u32,
    divide: unsafe extern "C" fn(u32, u64, u64, u32) -> u64,
    fail: unsafe extern "C" fn(Address, u32, Bytes),
    take_bound: unsafe extern "C" fn(Address, u32) -> Address,
    put_bound: unsafe extern "C" fn(Address, u32, Address),
    term: unsafe extern "C" fn(Address, u32) -> Term,
    take_term: unsafe extern "C" fn(Address, u32) -> Term,
    put_term: unsafe extern "C" fn(Address, u32, Term),
    term_type: unsafe extern "C" fn(Address, u32) -> Address,
    typed: unsafe extern "C" fn(Address, u32) -> TypeResult,
    put_type: unsafe extern "C" fn(Address, u32, TypeResult),
    clear_type: unsafe extern "C" fn(Address, u32),
}
struct Context<'a> { source: &'a Source, span: Span, error: Option<Diagnostic> }
#[repr(C)]
struct BindingContext<'a> {
    common: Context<'a>, world: &'a World, definition: ConditionScope<'a>,
    nodes: [Option<Box<OutputCondition>>; 2],
}
// These are the original owned term/type locals. Native descriptors only borrow
// them; take/put move the same allocation at the actual B consumption point.
struct OwnedTerm(Term);
impl OwnedTerm {
    fn raw(self) -> Term {
        let value=self.0;
        std::mem::forget(self);
        value
    }
    fn condition(mut self) -> OutputCondition {
        let value=unsafe { take(self.0.value) };
        self.0.value=ptr::null_mut();
        value
    }
}
impl Drop for OwnedTerm {
    fn drop(&mut self) {
        if !self.0.value.is_null() { unsafe { drop_node(self.0.value) }; }
        if !self.0.ty.is_null() { unsafe { drop_type(self.0.ty) }; }
    }
}
pub(super) enum Layout<'a> {
    Borrowed(&'a Ty),
    Owned(Box<Ty>),
}
impl Layout<'_> {
    pub(super) fn ty(&self) -> &Ty {
        match self {
            Self::Borrowed(ty) => ty,
            Self::Owned(ty) => ty,
        }
    }
}
struct ConditionType<'a> { layout: Layout<'a>, literal: bool }
impl ConditionType<'_> {
    fn describe(&self) -> TypeResult {
        TypeResult { ty: address(self.layout.ty()), literal: u32::from(self.literal), ok: 1 }
    }
}
#[repr(C)]
struct QueryContext<'a> {
    common: Context<'a>,
    terms: [Option<OwnedTerm>; 2],
    types: [Option<ConditionType<'a>>; 2],
}
impl<'a> QueryContext<'a> {
    fn new(source: &'a Source, span: Span) -> Self {
        Self { common: Context { source, span, error: None }, terms: [None,None], types: [None,None] }
    }
}
fn own<T>(value: T) -> Address { Box::into_raw(Box::new(value)).cast() }
unsafe fn borrowed<'a, T>(value: Address) -> &'a T { unsafe { &*value.cast() } }
unsafe fn take<T>(value: Address) -> T { unsafe { *Box::from_raw(value.cast::<T>()) } }
unsafe fn context<'a>(value: Address) -> &'a mut Context<'a> { unsafe { &mut *value.cast() } }
unsafe fn binding_context<'a>(value: Address) -> &'a mut BindingContext<'a> {
    unsafe { &mut *value.cast() }
}
unsafe fn query_context<'a>(value: Address) -> &'a mut QueryContext<'a> {
    unsafe { &mut *value.cast() }
}
unsafe extern "C" fn take_bound(ctx: Address, index: u32) -> Address {
    Box::into_raw(unsafe { binding_context(ctx) }.nodes[index as usize].take().unwrap()).cast()
}
unsafe extern "C" fn put_bound(ctx: Address, index: u32, value: Address) {
    unsafe { binding_context(ctx) }.nodes[index as usize]=Some(unsafe { Box::from_raw(value.cast()) });
}
unsafe extern "C" fn term(ctx: Address, index: u32) -> Term {
    unsafe { query_context(ctx) }.terms[index as usize].as_ref().unwrap().0
}
unsafe extern "C" fn take_term(ctx: Address, index: u32) -> Term {
    unsafe { query_context(ctx) }.terms[index as usize].take().unwrap().raw()
}
unsafe extern "C" fn put_term(ctx: Address, index: u32, value: Term) {
    unsafe { query_context(ctx) }.terms[index as usize]=Some(OwnedTerm(value));
}
unsafe extern "C" fn term_type(ctx: Address, index: u32) -> Address {
    let ctx=unsafe { query_context(ctx) };
    let term=ctx.terms[index as usize].as_ref().unwrap();
    if !term.0.ty.is_null() { term.0.ty }
    else { address(ctx.types[index as usize].as_ref().unwrap().layout.ty()) }
}
unsafe extern "C" fn typed(ctx: Address, index: u32) -> TypeResult {
    unsafe { query_context(ctx) }.types[index as usize].as_ref().unwrap().describe()
}
unsafe extern "C" fn put_type(ctx: Address, index: u32, value: TypeResult) {
    unsafe { query_context(ctx) }.types[index as usize]=Some(ConditionType {
        layout: Layout::Owned(unsafe { Box::from_raw(value.ty.cast()) }), literal: value.literal!=0,
    });
}
unsafe extern "C" fn clear_type(ctx: Address, index: u32) {
    unsafe { query_context(ctx) }.types[index as usize]=None;
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
                operators::Binary::Matrix(plan) => {
                    result.kind = plan.kind as u32;
                    result.cast_left = plan.cast_left;
                    result.cast_right = plan.cast_right;
                }
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
        parameter, lookup, variant, member, drop_indices, unary_type, binary_type, same_type, divide, fail,
        take_bound, put_bound, term, take_term, put_term, term_type, typed, put_type, clear_type }
}
#[repr(C)]
struct BindingCursor { source: Address, request: Address, stage: u32, is_public: u32 }
#[repr(C)]
struct QueryCursor { source: Address, request: Address, stage: u32, slot: u32 }
#[repr(u32)]
enum BindingNeed { Done, Child, Constant }
#[repr(u32)]
enum IdentityNeed { Done, Child, Type }
#[repr(u32)]
enum TypeNeed { Done, Child, Path }
unsafe extern "C" {
    #[link_name = "bootstrap.contract_condition::input_names"]
    fn b_input_names(condition: Address, context: Address,
        inspect: unsafe extern "C" fn(Address) -> Node,
        emit: unsafe extern "C" fn(Address, Bytes));
    #[link_name = "bootstrap.source_bind::positional_index"]
    fn b_positional_index(bytes: *const u8, length: u64) -> u64;
    #[link_name="bootstrap.contract_condition::bind_start"]
    fn bind_start(value: Address, is_public: u32) -> BindingCursor;
    #[link_name="bootstrap.contract_condition::bind_step"]
    fn bind_step(state: *mut BindingCursor, ctx: Address, host: Host) -> BindingNeed;
    #[link_name="bootstrap.contract_condition::identity_start"]
    fn identity_start(value: Address) -> QueryCursor;
    #[link_name="bootstrap.contract_condition::identity_step"]
    fn identity_step(state: *mut QueryCursor, ctx: Address, host: Host) -> IdentityNeed;
    #[link_name="bootstrap.contract_condition::type_start"]
    fn type_start(value: Address) -> QueryCursor;
    #[link_name="bootstrap.contract_condition::type_step"]
    fn type_step(state: *mut QueryCursor, ctx: Address, host: Host) -> TypeNeed;
}
pub(super) fn add_input_names(condition: &OutputCondition, names: &mut BTreeSet<String>) {
    unsafe extern "C" fn emit(context: Address, name: Bytes) {
        let names=unsafe { &mut *context.cast::<BTreeSet<String>>() };
        names.insert(unsafe { name.text() }.to_owned());
    }
    unsafe { b_input_names(address(condition),ptr::from_mut(names).cast(),node,emit) };
}

pub(super) fn positional_index(name: &str) -> usize {
    let index = unsafe { b_positional_index(name.as_ptr(), name.len() as u64) };
    usize::try_from(index).unwrap_or(usize::MAX)
}

async fn bind_node(world: &World, definition: ConditionScope<'_>, value: &OutputCondition, span: Span) -> Result<Box<OutputCondition>> {
    let mut ctx=BindingContext {
        common: Context { source: definition.source, span, error: None }, world, definition,
        nodes: [None,None],
    };
    let mut cursor=unsafe { bind_start(address(value),u32::from(definition.public)) };
    loop {
        let need=unsafe { bind_step(&mut cursor,ptr::from_mut(&mut ctx).cast(),host()) };
        if let Some(error)=ctx.common.error.take() { return Err(error); }
        match need {
            BindingNeed::Done => return Ok(ctx.nodes[0].take().unwrap()),
            BindingNeed::Child => {
                let input=unsafe { borrowed(cursor.request) };
                ctx.nodes[0]=Some(unsafe { world.call(bind_node(world,definition,input,span)).await }?);
            }
            BindingNeed::Constant => {
                let key=unsafe { borrowed::<String>(cursor.request) };
                let constant=unsafe { world.call(constant::evaluate(world,key,&[])).await }?;
                let (ty,bits,generated_origin)=constant.value.clone();
                ctx.nodes[0]=Some(Box::new(OutputCondition::Constant(OutputConstant { ty,bits,generated_origin })));
            }
        }
    }
}
pub(super) async fn bind_constants(world: &World, definition: ConditionScope<'_>, value: &OutputCondition, span: Span) -> Result<OutputCondition> {
    Ok(*unsafe { world.call(bind_node(world,definition,value,span)).await }?)
}
async fn identity_term<'a>(
    world: &World, value: &OutputCondition,
    path: &impl std::ops::AsyncFn(&OutputCondition) -> Result<Cow<'a, Ty>>,
    source: &'a Source, span: Span,
) -> Result<OwnedTerm> {
    let mut ctx=QueryContext::new(source,span);
    let mut cursor=unsafe { identity_start(address(value)) };
    loop {
        let need=unsafe { identity_step(&mut cursor,ptr::from_mut(&mut ctx).cast(),host()) };
        if let Some(error)=ctx.common.error.take() { return Err(error); }
        match need {
            IdentityNeed::Done => return Ok(ctx.terms[0].take().unwrap()),
            IdentityNeed::Child => {
                let input=unsafe { borrowed(cursor.request) };
                ctx.terms[cursor.slot as usize]=Some(unsafe { world.call(identity_term(world,input,path,source,span)).await }?);
            }
            IdentityNeed::Type => {
                let input=unsafe { borrowed(cursor.request) };
                ctx.types[cursor.slot as usize]=Some(unsafe { world.call(type_value(world,input,path,source,span)).await }?);
            }
        }
    }
}
pub(super) async fn identity<'a>(
    world: &World, value: &OutputCondition,
    path: &impl std::ops::AsyncFn(&OutputCondition) -> Result<Cow<'a, Ty>>,
    source: &'a Source, span: Span,
) -> Result<OutputCondition> {
    Ok(unsafe { world.call(identity_term(world,value,path,source,span)).await }?.condition())
}
async fn type_value<'a>(
    world: &World, value: &OutputCondition,
    path: &impl std::ops::AsyncFn(&OutputCondition) -> Result<Cow<'a, Ty>>,
    source: &'a Source, span: Span,
) -> Result<ConditionType<'a>> {
    let mut ctx=QueryContext::new(source,span);
    let mut cursor=unsafe { type_start(address(value)) };
    loop {
        let need=unsafe { type_step(&mut cursor,ptr::from_mut(&mut ctx).cast(),host()) };
        if let Some(error)=ctx.common.error.take() { return Err(error); }
        match need {
            TypeNeed::Done => return Ok(ctx.types[0].take().unwrap()),
            TypeNeed::Child => {
                let input=unsafe { borrowed(cursor.request) };
                ctx.types[cursor.slot as usize]=Some(unsafe { world.call(type_value(world,input,path,source,span)).await }?);
            }
            TypeNeed::Path => {
                let input=unsafe { borrowed(cursor.request) };
                let ty=unsafe { world.call(path(input)).await }?;
                let layout = match ty {
                    Cow::Borrowed(ty) => Layout::Borrowed(ty),
                    Cow::Owned(ty) => Layout::Owned(Box::new(ty)),
                };
                ctx.types[cursor.slot as usize]=Some(ConditionType { layout, literal:false });
            }
        }
    }
}
pub(super) async fn condition_type<'a>(
    world: &World, value: &OutputCondition,
    path: &impl std::ops::AsyncFn(&OutputCondition) -> Result<Cow<'a, Ty>>,
    source: &'a Source, span: Span,
) -> Result<Layout<'a>> {
    Ok(unsafe { world.call(type_value(world,value,path,source,span)).await }?.layout)
}
