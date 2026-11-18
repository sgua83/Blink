//! 标量常量按实际类型与位宽求值；已知值直接成为既有 IR 常量。
use super::*;
use crate::builtin_types::{ScalarBinaryOp, ScalarEncoding};
use std::num::NonZeroU64;

impl Builder<'_> {
    pub(super) async fn module_constant(&mut self, key: String) -> Result<Value> {
        let world = self.world;
        let (ty, words, origin) = unsafe { world.call(evaluate(world, &key, &self.calls)) .await }?;
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
/// 使用现有表达式、调用与常量分析；不编译或运行宿主／设备初始化函数。
pub(crate) async fn constant_value(world: &World, key: &str) -> Result<(Ty, Vec<Bits>)> {
    let (ty, words, _) = unsafe { world.call(evaluate(world, key, &[])) .await }?;
    Ok((ty, words))
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
    let expected=match annotation {
        Some(ty)=>Some(unsafe { world.call(builder.expected_storage(ty,expression.span())).await }?),
        None=>None,
    };
    let result = unsafe { world.call(builder.expr_expected(expression, expected.as_ref())).await }?;
    let mut value = builder.value_result(result, expression.span())?;
    if let Some(expected) = expected {
        builder.require_same(&expected, &value.ty, expression.span())?;
    }
    if value.ty.contains_views()
        || value.constant.is_none()
        || builder.code[first..]
            .iter()
            .any(|inst| !matches!(&(inst).kind, InstKind::Constant(..)))
    {
        return definition.source.fail(
            expression.span(),
            "B114",
            "阶段派生参数必须可完全编译期求值；运行期或逐元素表达式放在 parallel 中",
        );
    }
    value.regs.sort_unstable();
    let operations = evaluation_operations(builder.code[..first].iter().filter(|inst|
        matches!(&inst.kind, InstKind::Constant(reg, ..) if value.regs.binary_search(reg).is_ok()))
        .chain(builder.code[first..].iter()));
    let origin = (!operations.is_empty()).then(|| Arc::new(GeneratedOrigin {
        reason: GenerationReason::StageConstantEvaluation,
        trigger: Some(GenerationAnchor::StageConstantEvaluation {
            expression: builder.source_at(expression.span(), SourceSyntax::Expression), operations,
        }),
    }));
    let root = Root::Literal(value.ty, value.constant.unwrap(), origin);
    Ok(if matches!(expression.kind(), ExprKind::Name(_)) {
        root.reference_literal(builder.source_at(expression.span(), SourceSyntax::Expression))
    } else { root })
}
fn evaluation_operations<'a>(code: impl Iterator<Item = &'a Inst>) -> Vec<OperationOrigin> {
    let mut operations: Vec<_> = code.filter(|inst| inst.generated_origin.is_some())
        .map(OperationOrigin::from_instruction).collect();
    operations.dedup_by_key(|origin| (
        origin.source_site.as_ref().map(Arc::as_ptr), origin.generated_origin.as_ref().map(Arc::as_ptr),
    ));
    operations
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
pub(super) async fn evaluate(world: &World, key: &str, calls: &[String]) -> Result<(Ty, Vec<Bits>, Option<Arc<GeneratedOrigin>>)> {
    let definition = &world.functions[key];
    if let Some(constant) = definition.constant.get() {
        if let Some(inputs) = &world.build_declarations {
            inputs.borrow_mut().insert(crate::build::Declaration::Constant(key.into()));
        }
        return Ok(constant.value.clone());
    }
    unsafe { world.call(world.evaluating_constants.evaluate(key, async || {
        let parent_inputs = world.build_declarations.as_ref().map(|inputs| inputs.take());
        let result = async {
            let instance = unsafe {
                world.call(crate::generics::instantiate(world, key, definition, Vec::new(), &[]))
            .await }?;
            let ty = unsafe {
                world.call(instance.bindings.type_name(world, &definition.function.result(),
                    (&definition.source, &definition.module, &definition.imports), definition.function.span()))
            .await }?;
            if ty.dataset || ty.tile || ty.view || ty.name == "void" || ty.is_never() {
                return definition.source.fail(
                    definition.function.span(),
                    "B234",
                    "const 需要普通值类型，不能持有资源或 void",
                );
            }
            let context = constant_context(key);
            let scope = LexicalScope::Function(instance.clone());
            let (source_definition, source_context) = definition.syntax_context(Some(key));
            let mut builder = Builder::from_checked(world, &scope, &context, 0,
                source_definition, source_context, BTreeSet::new());
            builder.in_function = true;
            builder.calls.extend_from_slice(calls);
            builder.calls.push(key.into());
            let body = definition.function.body().unwrap();
            let (returned, _) = unsafe { world.call(builder.function_body(&instance, &body)) .await }?;
            let value = builder.ordinary_return(returned, definition.function.span())?.unwrap();
            if value.ty.contains_views()
                || value.constant.is_none()
                || builder.code.iter().any(|inst| !matches!(&(inst).kind, InstKind::Constant(..)))
            {
                return definition.source.fail(
                    definition.function.span(),
                    "B234",
                    "const 初始化尚不能完全在编译期求值；不允许资源、运行期运算或控制流",
                );
            }
            let operations = evaluation_operations(builder.code.iter());
            let origin = (!operations.is_empty()).then(|| Arc::new(GeneratedOrigin {
                reason: GenerationReason::ModuleConstantEvaluation,
                trigger: Some(GenerationAnchor::ModuleConstantEvaluation {
                    definition: definition.source_origin().map(Arc::new), constant: Arc::from(key), operations,
                }),
            }));
            Ok((value.ty, value.constant.unwrap(), origin))
        }.await;
        let mut used = BTreeSet::new();
        if let (Some(inputs), Some(parent)) = (&world.build_declarations, parent_inputs) {
            used = inputs.replace(parent);
        }
        let value = result?;
        let constant = definition.constant.get_or_init(|| Arc::new(crate::frontend::EvaluatedConstant {
            value,
            inputs: used,
        }));
        if let Some(inputs) = &world.build_declarations {
            inputs.borrow_mut().insert(crate::build::Declaration::Constant(key.into()));
        }
        Ok(constant.value.clone())
    })) .await }.unwrap_or_else(|| definition.source.fail(
        definition.function.span(), "B160", format!("编译期值与类型递归依赖 {key}"),
    ))
}

impl World {
    pub(crate) async fn source_shape(
        &self, shape: &crate::syntax::Shape, bindings: &Arc<crate::generics::Bindings>,
        source: &Source, span: Span, module: &str, imports: &[String],
    ) -> Result<u32> {
        match shape.kind() {
            crate::syntax::ShapeKind::Literal(value) => Ok(value),
            crate::syntax::ShapeKind::Constant(name) => match bindings.values.get(name) {
                Some(value) => Ok(*value),
                None => unsafe { self.call(self.shape(&Some(Shape::Constant(name.into())), source, span, module, imports)) .await }.map(Option::unwrap),
            },
            crate::syntax::ShapeKind::Expression(expression) => {
                // Generated nodes can share a diagnostic span. Preserve the
                // original expression offset in the one actual active key.
                let key = format!("<shape>:{}:{module}:{:?}:{:?}:{:?}:{:?}",
                    source.path.display(), expression, expression.span(), bindings.types, bindings.values);
                unsafe { self.call(integer_expression(self, &key, source, module, imports, &Ty::scalar(Scalar::U32),
                    &expression, bindings,
                    ("B234", "形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用"))) .await }
            }
        }
    }

    pub(crate) async fn shape(
        &self, shape: &Option<Shape>, source: &Source, span: Span, module: &str, imports: &[String],
    ) -> Result<Option<u32>> {
        match shape {
            None => Ok(None),
            Some(Shape::Literal(n)) => Ok(Some(*n)),
            Some(Shape::Expression(expression)) => {
                let key=format!("<shape>:{}:{module}:{}",source.path.display(),crate::ast::expression_name(expression));
                // Semantic references keep their original module namespace.
                let bindings=Arc::new(crate::generics::Bindings::default());
                unsafe { self.call(integer_value(self,&key,source,module,imports,&Ty::scalar(Scalar::U32),expression.span,&bindings,
                    ("B234","形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用"),
                    async |builder| unsafe { self.call(builder.reference_expr(expression,None)) .await })) .await }.map(Some)
            },
            Some(Shape::Constant(name)) => {
                let key=self.resolve(name,module,imports,true).unique()
                    .filter(|key|self.functions[key].function.kind()==FunctionKind::Constant)
                    .ok_or_else(|| Diagnostic {
                        source:source.clone(),span,code:"B015",
                        message:format!("未知或不明确的编译期 const {name}"),
                    })?;
                let (ty,words)=unsafe { self.call(constant_value(self,&key)) .await }?;
                if ty!=Ty::scalar(Scalar::U32) {
                    return source.fail(span,"B015","形状或 const 实参必须为 u32 标量");
                }
                Ok(Some(words[0]as u32))
            },
        }
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
        let result=read(&mut builder).await?;
        let value=builder.value_result(result,span)?;
        builder.require_same(representation,&value.ty,span)?;
        if value.constant.is_none()||builder.code.iter().any(|instruction|!matches!(&instruction.kind,InstKind::Constant(..))){
            return source.fail(span,diagnostic.0,diagnostic.1);
        }
        Ok(value.constant.unwrap()[0]as u32)
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
    async fn reference_syntax(&mut self,expression:&crate::ast::Expr,expected:Option<&Ty>)->Result<Option<Value>> {
        use crate::ast::ExprKind as K;
        let span=expression.span;
        match &expression.kind {
            K::Number(number)=>self.number_expression(number,span),
            K::StaticU32(value)=>self.constant_expression(Scalar::U32,Bits::from(*value)),
            K::Bool(value)=>self.constant_expression(Scalar::Bool,Bits::from(*value)),
            K::Name(name)=>unsafe { self.world.call(self.name_expr(name,span)).await }.map(Some),
            K::Member(base,field)=>{
                let Some(value)=unsafe { self.world.call(self.reference_expr(base,None)).await }? else{return Ok(None);};
                self.member_value(value,field,span).map(Some)
            }
            K::Index(base,index)=>{
                let Some(value)=unsafe { self.world.call(self.reference_expr(base,None)).await }? else{return Ok(None);};
                if value.ty.indexed_element().is_none(){return self.fail(span,"B136","此类型不能按固定值下标读取");}
                unsafe { self.world.call(self.array_value_at(value,index.span,async |builder|unsafe { builder.world.call(builder.reference_expr(index,None)).await })).await }
            }
            K::Unary(op,value)=>{
                if op=="-"&&let K::Number(number)=&value.kind{return self.negative_number(number,span).map(Some);}
                let Some(value)=unsafe { self.world.call(self.reference_expr(value,None)).await }? else{return Ok(None);};
                self.unary_value(op,value,span).map(Some)
            }
            K::Binary(op,left,right)=>{
                let Some(left)=unsafe { self.world.call(self.reference_expr(left,None)).await }? else{return Ok(None);};
                if op=="&&"||op=="||"{
                    return unsafe { self.world.call(self.short_circuit_value(op,left,right.span,async |builder|
                        Ok(unsafe { builder.world.call(builder.reference_expr(right,None)).await }?.map(returns::Returned::value)),async |_,_|Ok(()),span)).await };
                }
                let Some(right)=unsafe { self.world.call(self.reference_expr(right,None)).await }? else{return Ok(None);};
                self.binary(op,left,right,span,None).map(Some)
            }
            K::Select(condition,yes,no)=>{
                let Some(condition)=unsafe { self.world.call(self.reference_expr(condition,None)).await }? else{return Ok(None);};
                self.require_bool(&condition,expression.span)?;
                let returned=unsafe { self.world.call(self.selection_after_condition(condition,
                    (yes.span,async |builder|Ok(unsafe { builder.world.call(builder.reference_expr(yes,expected)).await }?.map(returns::Returned::value))),
                    (no.span,async |builder|Ok(unsafe { builder.world.call(builder.reference_expr(no,expected)).await }?.map(returns::Returned::value))),
                    async |_,_|Ok(()),span)).await }?;
                self.ordinary_return(returned,span)
            }
            K::Construct(name,args)=>{
                let ty=unsafe { self.world.call(self.world.ty(name,self.scope.source(),span,self.scope.module(),self.scope.imports())).await }?;
                unsafe { self.world.call(self.record_values(ty,args.len(),span,async |builder,index,expected|unsafe { builder.world.call(builder.reference_expr(&args[index],expected)).await })).await }
            }
            K::Call(name,args)=>unsafe { self.world.call(self.reference_call(name,None,args,expected,span)).await },
            K::Specialize(name,types,args)=>unsafe { self.world.call(self.reference_call(name,Some(types),args,expected,span)).await },
            K::FunctionRef(..)=>self.fail(span,"B236","静态函数引用只能作为集合算法参数或 bind 的首参数，不能作为普通值"),
            K::Cooperate(..)=>self.fail(span,"B208","cooperate 调用只能作为协作根块的局部声明"),
            K::Target(..)=>self.fail(span,"B266","target 调用由 CPU 主流程发起，元素实例与协作成员不能选择发起位置"),
            K::Invoke(callee,args)=>{
                let result=unsafe { self.world.call(self.function_value_arguments(callee.span,span,
                    async |builder|unsafe { builder.world.call(builder.reference_expr(callee,None)).await },
                    async |builder,parameters|unsafe { builder.world.call(builder.reference_call_arguments(cooperative::CallParameters::CFunction(parameters),args,span)).await })).await }?;
                self.ordinary_return(result,span)
            },
        }
    }
    async fn reference_call(&mut self,name:&str,explicit:Option<&[GenericArgument]>,args:&[crate::ast::Expr],expected:Option<&Ty>,span:Span)->Result<Option<Value>> {
        if let Some((ty,index))=unsafe { self.world.call(self.variant_constructor_type(name,explicit,expected,span)).await }?{
            if ty.owns_resources(){return self.fail(span,"B224","拥有资源的记录由普通CPU应用函数组织");}
            return unsafe { self.world.call(self.variant_values(ty,index,args.len(),span,async |builder,payload|unsafe { builder.world.call(builder.reference_expr(&args[0],Some(payload))).await })).await };
        }
        if matches!(name,"Ok"|"Err"){
            let ty=match explicit {
                Some(arguments @ [GenericArgument::Type(_),GenericArgument::Type(_)])=>{
                    let mut name=TypeName::named("result");name.arguments=arguments.to_vec();
                    unsafe { self.world.call(self.expected_storage(&name,span)).await }?
                }
                Some(_)=>return self.fail(span,"B269","Ok／Err需要两个明确类型实参"),
                None=>match expected.filter(|ty|ty.result){
                    Some(ty)=>ty.clone(),
                    None=>return self.fail(span,"B269","Ok／Err需要已声明的result上下文或两个明确类型实参"),
                },
            };
            if ty.owns_resources(){return self.fail(span,"B224","拥有资源的记录由普通CPU应用函数组织");}
            return unsafe { self.world.call(self.variant_values(ty,usize::from(name=="Err"),args.len(),span,
                async |builder,payload|unsafe { builder.world.call(builder.reference_expr(&args[0],Some(payload))).await })).await };
        }
        if let Some(explicit)=explicit {
            if name=="unsafe_function" {
                return unsafe { self.world.call(self.function_from_address(explicit,args.len(),span,args.first().map(|arg|arg.span).unwrap_or(span),
                    async |builder|unsafe { builder.world.call(builder.reference_expr(&args[0],None)).await })).await };
            }
            if name=="unsafe_position" {
                let Some(place)=unsafe { self.world.call(self.external_position_value(explicit,args.len(),span,args.first().map(|arg|arg.span).unwrap_or(span),
                    async |builder|unsafe { builder.world.call(builder.reference_expr(&args[0],None)).await })).await }?else{return Ok(None);};
                return unsafe { self.world.call(self.read_place(&place,span,None)).await }.map(Some);
            }
            if matches!(name,"view"|"dataset"){return self.fail(span,"B234","形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用");}
            if matches!(name,"array"|"owned")||!self.world.resolve(name,self.scope.module(),self.scope.imports(),false).is_missing(){
                let mut name=TypeName::named(name);name.arguments=explicit.to_vec();
                let ty=unsafe { self.world.call(self.world.ty(&name,self.scope.source(),span,self.scope.module(),self.scope.imports())).await }?;
                return unsafe { self.world.call(self.record_values(ty,args.len(),span,async |builder,index,expected|unsafe { builder.world.call(builder.reference_expr(&args[index],expected)).await })).await };
            }
        }
        if explicit.is_none()&&(matches!(name,"array"|"owned")||!self.world.resolve(name,self.scope.module(),self.scope.imports(),false).is_missing()){
            let ty=unsafe { self.world.call(self.expected_storage(&TypeName::named(name),span)).await }?;
            if ty.owns_resources(){return self.fail(span,"B224","拥有资源的记录由普通CPU应用函数组织");}
        }
        let resolution=self.world.resolve(name,self.scope.module(),self.scope.imports(),true);
        match resolution {
            crate::frontend::NameResolution::Unique(key)=>{
                if self.world.functions[&key].function.params().len()!=args.len(){
                    return self.fail(span,"B111","函数实参数量与声明不匹配");
                }
                let declaration=self.world.functions[&key].clone();
                let Some(arguments)=unsafe { self.world.call(self.reference_call_arguments(cooperative::CallParameters::Declaration(&declaration),args,span)).await }?else{return Ok(None);};
                let instance=unsafe { self.world.call(self.specialized_arguments(key,&arguments,explicit,span)).await }?;
                let returned=unsafe { self.world.call(self.invoke_returned(instance,arguments,span,None,None)).await }?;
                return self.ordinary_return(returned,span);
            }
            crate::frontend::NameResolution::Ambiguous=>return self.fail(span,"B158",format!("函数名称{name}有多个可见声明，须使用模块限定名")),
            crate::frontend::NameResolution::Missing=>{}
        }
        if let Some(explicit)=explicit {
            if matches!(name,"size_of"|"align_of"){
                let([GenericArgument::Type(ty)],0)=(explicit,args.len())else{
                    return self.fail(span,"B153",format!("{name}<T>()需要一个明确类型且不接收值实参"));
                };
                let ty=unsafe { self.world.call(self.expected_storage(ty,span)).await }?;
                return self.constant_expression(Scalar::U64,Bits::from(if name=="size_of"{ty.size}else{ty.align}));
            }
            return self.fail(span,"B158",format!("未知或未导出的泛型函数{name}"));
        }
        if name=="c_function"{
            return unsafe { self.world.call(self.c_function_from(span,async |builder|{
                if args.len()!=1{return builder.fail(span,"B153","c_function需要一个明确的C ABI函数引用");}
                let(reference_name,types)=match &args[0].kind{
                    crate::ast::ExprKind::Name(name)=>(name.as_str(),None),
                    crate::ast::ExprKind::FunctionRef(name,types)=>(name.as_str(),Some(types.as_slice())),
                    _=>return builder.fail(args[0].span,"B153","c_function需要C ABI函数名及具体类型／const实参"),
                };
                unsafe { builder.world.call(callback::signature_named(builder.world,&builder.scope,reference_name,span,args[0].span,
                    async ||Ok(types.map(std::borrow::Cow::Borrowed)))).await }
            })).await }.map(Some);
        }
        if name=="println"{
            return unsafe { self.world.call(self.println_value(args.len(),span,args.first().map(|a|a.span).unwrap_or(span),
                async |builder|unsafe { builder.world.call(builder.reference_expr(&args[0],None)).await })).await };
        }
        if let Some(value)=unsafe { self.world.call(self.subgroup(name,args.len(),span,
            async |builder,index|unsafe { builder.world.call(builder.reference_expr(&args[index],None)).await },|index|args[index].span)).await }{return value;}
        if name=="unsafe_read"{
            if args.len()!=2{return self.fail(span,"B181","unsafe_read 需要 read 数据集与 u32 索引；调用者负责外部索引有效性");}
            let Some(view)=unsafe { self.world.call(self.reference_expr(&args[0],None)).await }?else{return Ok(None);};
            return unsafe { self.world.call(self.unsafe_read_view(view,span,args[1].span,async |builder|unsafe { builder.world.call(builder.reference_expr(&args[1],None)).await })).await };
        }
        if !self.world.resolve(name,self.scope.module(),self.scope.imports(),false).is_missing(){
            let ty=unsafe { self.world.call(self.world.ty(&TypeName::named(name),self.scope.source(),span,self.scope.module(),self.scope.imports())).await }?;
            return unsafe { self.world.call(self.record_values(ty,args.len(),span,async |builder,index,expected|unsafe { builder.world.call(builder.reference_expr(&args[index],expected)).await })).await };
        }
        if name=="index"{return self.fail(span,"B155","index() 只能在当前 program 的 parallel 域中使用");}
        if name=="ordinal"{return self.fail(span,"B168","ordinal只能用于元素游标");}
        unsafe { self.world.call(self.builtin_values(name,span,async |builder|unsafe { builder.world.call(builder.reference_arguments(args)).await })).await }
    }
    async fn reference_call_arguments(&mut self,parameters:cooperative::CallParameters<'_>,args:&[crate::ast::Expr],span:Span)->Result<Option<Vec<cooperative::Argument>>> {
        unsafe { self.world.call(self.call_arguments_from(parameters,args.len(),span,|index|args[index].span,async |builder,index|{
            let access=parameters.access(index);
            if matches!(access,Access::InOut|Access::Write|Access::Atomic)&&builder.reference_is_place(&args[index]){
                let Some(place)=unsafe { builder.world.call(builder.reference_place(&args[index])).await }?else{return Ok(None);};
                if access==Access::Atomic{
                    return builder.atomic_place_value(place,args[index].span).map(|place|place.map(cooperative::Argument::Atomic));
                }
                let place=unsafe { builder.world.call(builder.mutable_place_value(place,access,args[index].span)).await }?;
                return Ok(Some(cooperative::Argument::Reference(builder.borrow_owned(place))));
            }
            let expected=unsafe { builder.world.call(parameters.expected(builder.world,&builder.scope,index)).await }?;
            let value=unsafe { builder.world.call(builder.reference_expr(&args[index],expected.as_ref())).await }?;
            if !builder.flow.continues(){return Ok(None);}
            let result=value.map(returns::Returned::value);
            if matches!(access,Access::InOut|Access::Write){
                unsafe { builder.world.call(builder.mutable_returned_argument(result,access,args[index].span)).await }
            }else if access==Access::Atomic{
                // A value has no storage to lend, exactly as in the source place reader.
                builder.fail(args[index].span,"B138","表达式不是可赋值位置")
            }else{
                Ok(result.map(cooperative::Argument::resource))
            }
        })).await }
    }
    fn reference_is_place(&self,expression:&crate::ast::Expr)->bool {
        use crate::ast::ExprKind as K;
        match &expression.kind{
            K::Name(name)=>matches!(self.env.get(name),Some(Binding::AtomicBorrow(_)|Binding::ViewElement{..}|Binding::Owned(_)|Binding::OwnedValue(_))),
            K::Member(base,_)=>self.reference_is_place(base),
            K::Index(..)=>true,
            K::Specialize(name,..)if name=="unsafe_position"=>true,
            _=>false,
        }
    }
    async fn reference_place(&mut self,expression:&crate::ast::Expr)->Result<Option<Place>> {
        unsafe { self.world.call(self.at_source(expression.span,SourceSyntax::Place,async |builder|{
            use crate::ast::ExprKind as K;
            let span=expression.span;
            builder.view_uses.before(span);
            match &expression.kind {
                K::Name(name)=>builder.name_place(name,span).map(Some),
                K::Specialize(name,types,args)if name=="unsafe_position"=>
                    unsafe { builder.world.call(builder.external_position_value(types,args.len(),span,args.first().map(|a|a.span).unwrap_or(span),
                        async |builder|unsafe { builder.world.call(builder.reference_expr(&args[0],None)).await })).await },
                K::Member(base,field)=>{
                    let Some(place)=unsafe { builder.world.call(builder.reference_place(base)).await }?else{return Ok(None);};
                    builder.member_position(place,field,span)
                }
                K::Index(base,index)=>{
                    if builder.reference_is_place(base){
                        let Some(place)=unsafe { builder.world.call(builder.reference_place(base)).await }?else{return Ok(None);};
                        if place.ty().indexed_element().is_some(){
                            let Some(index_value)=unsafe { builder.world.call(builder.reference_expr(index,None)).await }?else{return Ok(None);};
                            return builder.array_index(place,index_value,index.span).map(Some);
                        }
                        let view=unsafe { builder.world.call(builder.read_place(&place,base.span,None)).await }?;
                        unsafe { builder.world.call(builder.view_value_at(view,index.span,async |builder|unsafe { builder.world.call(builder.reference_expr(index,None)).await })).await }
                    }else{
                        let Some(value)=unsafe { builder.world.call(builder.reference_expr(base,None)).await }?else{return Ok(None);};
                        if value.ty.array.is_some(){
                            let place=builder.materialize_array(value,None).place;
                            let Some(index_value)=unsafe { builder.world.call(builder.reference_expr(index,None)).await }?else{return Ok(None);};
                            builder.array_index(place,index_value,index.span).map(Some)
                        }else{
                            unsafe { builder.world.call(builder.view_value_at(value,index.span,async |builder|unsafe { builder.world.call(builder.reference_expr(index,None)).await })).await }
                        }
                    }
                }
                _=>builder.fail(span,"B138","表达式不是可赋值位置"),
            }
        })).await }
    }
    async fn reference_arguments(&mut self,args:&[crate::ast::Expr])->Result<Option<Vec<Value>>> {
        let mut values=Vec::new();
        for argument in args {
            let Some(value)=unsafe { self.world.call(self.reference_expr(argument,None)).await }? else { return Ok(None); };
            values.push(value);
        }
        Ok(Some(values))
    }
}
