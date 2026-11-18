//! 标量常量按实际类型与位宽求值；已知值直接成为既有 IR 常量。
use super::*;
use crate::builtin_types::{ScalarBinaryOp, ScalarEncoding};
use std::num::NonZeroU64;
mod model;

impl Builder<'_> {
    pub(super) async fn module_constant(&mut self, key: String) -> Result<Value> {
        let world = self.world;
        let constant = unsafe { world.call(evaluate(world, &key, &self.calls)) .await }?;
        let (ty, words, origin) = constant.value.clone();
        Ok(self.evaluated_constant(ty, words, origin))
    }
    pub(super) fn evaluated_constant(&mut self, ty: Ty, words: Vec<Bits>, origin: Option<Arc<GeneratedOrigin>>) -> Value {
        let first = self.code.len();
        let value = self.constant_value(ty, words,None);
        for instruction in &mut self.code[first..] { instruction.generated_origin = origin.clone(); }
        value
    }
    pub(super) fn literal(&mut self, scalar: Scalar, bits: Bits) -> Value {
        let mut value = self.constant(scalar, bits);
        value.literal = true;
        value
    }
    pub(crate) fn constant_value(&mut self, ty: Ty, bits: Vec<Bits>, generated_origin: Option<Arc<GeneratedOrigin>>) -> Value {
        let regs = ty
            .components
            .iter()
            .zip(&bits)
            .map(|((s, _), b)| self.constant_with_origin(*s, *b, generated_origin.clone()).regs[0])
            .collect();
        Value {
            proof: bounds::Proof::default().with_external_borrows(&ty),
            literal: false,
            ty,
            regs,
            uniform: true,
            constant: Some(bits),
        }
    }
}
/// 阶段派生参数复用已有表达式及常量语义；没有宿主求值器或新增运行阶段。
pub(super) async fn stage_expression(
    world: &World,
    instance: &crate::generics::Instance,
    roots: &BTreeMap<String, Root>,
    program: &Program,
    expression: &Expr,
    annotation: Option<&TypeName>,
    domain_path: &Arc<[DomainSite]>,
) -> Result<Root> {
    let definition = &instance.definition;
    let mut builder = unsafe { world.call(Builder::new(world, instance, program, 0, Some(&instance.name))).await }?;
    builder.source_context = Arc::new(SourceContext { domain_path: domain_path.clone(), ..(*builder.source_context).clone() });
    builder.in_function = true;
    builder.roots(roots);
    // roots中的无关uniform装载不是当前表达式的计算，不要求它们为常量。
    let first = builder.code.len();
    let mut cursor=model::ValueCursor::start(annotation.is_some(),true,matches!(expression.kind(),ExprKind::Name(_)));
    let mut expected:Option<Ty>=None;
    let mut value:Option<Value>=None;
    let mut operations=Vec::new();
    let mut facts=0;
    loop {
        match cursor.step(facts,&builder.code[first..]) {
            model::ValueNeed::Expected=>{
                expected=Some(unsafe{world.call(builder.expected_storage(annotation.unwrap_unchecked(),expression.span())).await}?);
            }
            model::ValueNeed::Read=>{
                let result=unsafe{world.call(builder.expr_expected(expression,expected.as_ref())).await}?;
                value=Some(builder.value_result(result,expression.span())?);
            }
            model::ValueNeed::Validate=>{
                let value=unsafe{value.as_ref().unwrap_unchecked()};
                if let Some(expected)=&expected {
                    builder.require_same(expected,&value.ty,expression.span())?;
                }
                facts=u32::from(value.ty.contains_views())|(u32::from(value.constant.is_some())<<1);
            }
            model::ValueNeed::Reject=>return definition.source.fail(expression.span(),"B114",
                "阶段派生参数必须可完全编译期求值；运行期或逐元素表达式放在 parallel 中"),
            model::ValueNeed::Sources=>{
                let value=unsafe{value.as_mut().unwrap_unchecked()};
                value.regs.sort_unstable();
                operations=model::source_operations(&builder.code,first,&value.regs);
            }
            need @ (model::ValueNeed::Literal|model::ValueNeed::Reference)=>{
                let value=unsafe{value.take().unwrap_unchecked()};
                let origin=(!operations.is_empty()).then(||Arc::new(GeneratedOrigin {
                    reason:GenerationReason::StageConstantEvaluation,
                    trigger:Some(GenerationAnchor::StageConstantEvaluation {
                        expression:builder.source_at(expression.span(),SourceSyntax::Expression),operations,
                    }),
                }));
                let root=Root::Literal(value.ty,value.constant.unwrap(),origin);
                return Ok(if matches!(need,model::ValueNeed::Reference){
                    root.reference_literal(builder.source_at(expression.span(),SourceSyntax::Expression))
                }else{root});
            }
        }
    }
}

pub(super) fn constant_context(name:&str)->Program {
Program {
            name: name.into(),
            source_origin: None,
            generated_origin: None,
            target: Target::Compute,
            buffers: Vec::new(),
            accelerations: Vec::new(),
            textures: Vec::new(),
            uniforms: Vec::new(),
            uniform_bytes: 0,
            native_frame: None,
            resources: None,
            source_stages: 0,
            stages: Vec::new(),
        functions: std::sync::Arc::from([]),
            prefix_requirements: Vec::new(),
            prefix_guarantees: Vec::new(), element_requirements: Vec::new(), element_guarantees: Vec::new(), length_requirements: Vec::new(), imports: BTreeMap::new(),
        }
}
pub(crate) use model::evaluate;

impl World {
    pub(crate) async fn source_shape(
        &self,shape:&crate::syntax::Shape,bindings:&Arc<crate::generics::Bindings>,
        source:&Source,span:Span,module:&str,imports:&[String],
    )->Result<u32>{
        model::source_shape(self,shape,bindings,source,span,module,imports).await
    }
    pub(crate) async fn shape(
        &self,shape:&Option<Shape>,source:&Source,span:Span,module:&str,imports:&[String],
    )->Result<Option<u32>>{
        model::shape(self,shape,source,span,module,imports).await
    }
}

#[repr(u32)]
enum FoldUnary {
    Negate,
    LogicalNot,
    BitNot,
    FloatNegate,
}

#[repr(C)]
struct FoldedCast {
    bits: Bits,
    present: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::cast"]
    fn fold_cast(source: ScalarEncoding, target: ScalarEncoding, bits: Bits) -> FoldedCast;
    #[link_name = "bootstrap.scalar_fold::unary"]
    fn fold_unary(operation: FoldUnary, width: u32, bits: Bits) -> Bits;
    #[link_name = "bootstrap.scalar_fold::binary"]
    fn fold_integer_binary(operation: ScalarBinaryOp, width: u32, signed: u32, left: Bits, right: Bits) -> Bits;
    #[link_name = "bootstrap.scalar_fold::float_binary"]
    fn fold_float_binary(operation: ScalarBinaryOp, width: u32, left: Bits, right: Bits) -> Bits;
    #[link_name = "bootstrap.scalar_fold::divide_unsigned"]
    fn divide_unsigned(left: u64, right: u64, remainder: u32) -> u64;
    #[link_name = "bootstrap.scalar_fold::divide_signed"]
    fn divide_signed(left: i64, right: i64, remainder: u32) -> i64;
}

pub(super) fn unary(operation: UnaryOp, scalar: Scalar, bits: Bits) -> Bits {
    let operation = match operation {
        UnaryOp::FloatNegate => FoldUnary::FloatNegate,
        UnaryOp::Bitcast => return bits,
        UnaryOp::SignedNegate | UnaryOp::UnsignedNegate => FoldUnary::Negate,
        UnaryOp::LogicalNot => FoldUnary::LogicalNot,
        UnaryOp::BitNot => FoldUnary::BitNot,
    };
    unsafe { fold_unary(operation, scalar.bit_width(), bits) }
}
pub(super) fn cast(from: Scalar, into: Scalar, bits: Bits) -> Option<Bits> {
    if from == into {
        return Some(into.truncate(bits));
    }
    let result = unsafe { fold_cast(from.into(), into.into(), bits) };
    (result.present != 0).then_some(result.bits)
}
pub(super) fn divide(scalar: Scalar, a: Bits, divisor: NonZeroU64, remainder: bool) -> Bits {
    let b = divisor.get();
    let remainder = u32::from(remainder);
    // The original nonzero divisor has the bit width of this scalar.
    let bits = if scalar.is_signed() {
        unsafe { divide_signed(scalar.signed(a), scalar.signed(b), remainder) as Bits }
    } else {
        unsafe { divide_unsigned(a, b, remainder) }
    };
    scalar.truncate(bits)
}
pub(super) fn binary(op: BinaryOp, scalar: Scalar, a: Bits, b: Bits) -> Option<Bits> {
    let floating = scalar.is_float();
    match (op, floating) {
        (BinaryOp::FAdd | BinaryOp::FSub | BinaryOp::FMul | BinaryOp::FDiv, false)
        | (BinaryOp::IAdd | BinaryOp::ISub | BinaryOp::IMul | BinaryOp::And | BinaryOp::Or
            | BinaryOp::Xor | BinaryOp::ShiftLeft | BinaryOp::ShiftRightLogical
            | BinaryOp::ShiftRightArithmetic, true) => return None,
        _ => {}
    }
    let operation = ScalarBinaryOp::from(op);
    let width = scalar.bit_width();
    Some(if floating {
        unsafe { fold_float_binary(operation, width, a, b) }
    } else {
        unsafe { fold_integer_binary(operation, width, u32::from(scalar.is_signed()), a, b) }
    })
}

// 编号与形状共享原整数表达式、类型及常量求值所有者。
pub(crate) async fn variant_discriminant(world:&World,identity:&str,source:&Source,module:&str,imports:&[String],representation:&Ty,expression:&Expr,bindings:&Arc<crate::generics::Bindings>)->Result<u32> {
    unsafe { world.call(integer_expression(world,&format!("<variant-tag>:{identity}"),source,module,imports,representation,expression,bindings,
        ("B268","变体编号必须是可完全编译期求值的底层整数"))).await }
}
async fn integer_expression(world:&World,key:&str,source:&Source,module:&str,imports:&[String],representation:&Ty,expression:&Expr,bindings:&Arc<crate::generics::Bindings>,diagnostic:(&'static str,&'static str))->Result<u32> {
    bindings.check_expression(expression,source)?;
    unsafe { world.call(integer_value(world,key,source,module,imports,representation,expression.span(),bindings,diagnostic,
        async |builder| unsafe { world.call(builder.expr(expression)) .await })) .await }
}
async fn integer_value(world:&World,key:&str,source:&Source,module:&str,imports:&[String],representation:&Ty,
    span:Span,bindings:&Arc<crate::generics::Bindings>,diagnostic:(&'static str,&'static str),
    read:impl std::ops::AsyncFnOnce(&mut Builder<'_>)->Result<Option<Value>>)->Result<u32> {
    unsafe { world.call(world.evaluating_constants.evaluate(key, async || {
        let context=constant_context(key);
        let mut builder=Builder::expression(world,&context,source,module,imports,bindings.clone(),span);
        builder.in_function=true;
        let mut cursor=model::IntegerCursor::start();
        let mut read=Some(read);
        let mut value:Option<Value>=None;
        let mut facts=0;
        loop{
            match cursor.step(facts,&builder.code){
                model::IntegerNeed::Read=>{
                    let result=unsafe{read.take().unwrap_unchecked()}(&mut builder).await?;
                    value=Some(builder.value_result(result,span)?);
                }
                model::IntegerNeed::Validate=>{
                    let value=unsafe{value.as_ref().unwrap_unchecked()};
                    builder.require_same(representation,&value.ty,span)?;
                    facts=u32::from(value.constant.is_some())<<1;
                }
                model::IntegerNeed::Reject=>return source.fail(span,diagnostic.0,diagnostic.1),
                model::IntegerNeed::Finish=>return Ok(unsafe{value.take().unwrap_unchecked()}.constant.unwrap()[0]as u32),
            }
        }
    })) .await }.unwrap_or_else(|| source.fail(span,"B160",format!("编译期表达式与类型递归依赖 {key}")))
}

// The runtime/SDK schema is kept as data. This reader borrows its actual nodes;
// all typing, numeric operations, construction and emitted code stay in Builder.
impl Builder<'_> {
    async fn reference_expr(&mut self,expression:&crate::ast::Expr,expected:Option<&Ty>)->Result<Option<Value>> {
        let mut value=unsafe { self.world.call(self.at_source(expression.span,SourceSyntax::Expression,async |builder|
            unsafe { builder.world.call(builder.reference_syntax(expression,expected)).await })).await }?;
        if let Some(value)=&mut value{self.bound_value(value);}
        Ok(value)
    }
    async fn reference_call_arguments(&mut self,parameters:cooperative::CallParameters<'_>,args:&[crate::ast::Expr],span:Span)->Result<Option<Vec<cooperative::Argument>>> {
        unsafe{self.world.call(call_arguments::build(self,parameters,args.len(),span,|index|args[index].span,
            async |builder,index,_parameter|builder.reference_argument(parameters,&args[index],index).await)).await}
    }
}
