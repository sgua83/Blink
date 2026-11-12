//! 标量常量按实际类型与位宽求值；已知值直接成为既有 IR 常量。
use super::*;
use crate::builtin_types::{ScalarBinaryOp, ScalarEncoding};
use std::num::NonZeroU64;

impl Builder<'_> {
    pub(super) fn module_constant(&mut self, key: String) -> Result<Value> {
        let (ty, words, origin) = evaluate(self.world, &key, &self.calls)?;
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
pub(crate) fn constant_value(world: &World, key: &str) -> Result<(Ty, Vec<Bits>)> {
    let (ty, words, _) = evaluate(world, key, &[])?;
    Ok((ty, words))
}
/// 阶段派生参数复用已有表达式及常量语义；没有宿主求值器或新增运行阶段。
pub(super) fn stage_expression(
    world: &World,
    instance: &crate::generics::Instance,
    roots: &BTreeMap<String, Root>,
    program: &Program,
    expression: &Expr,
    annotation: Option<&TypeName>,
    domain_path: &Arc<[DomainSite]>,
) -> Result<Root> {
    let definition = &instance.definition;
    let mut builder = Builder::new(world, instance, program, 0, Some(&instance.name))?;
    builder.source_context = Arc::new(SourceContext { domain_path: domain_path.clone(), ..(*builder.source_context).clone() });
    builder.in_function = true;
    builder.roots(roots);
    // roots中的无关uniform装载不是当前表达式的计算，不要求它们为常量。
    let first = builder.code.len();
    let expected = annotation
        .map(|ty| builder.expected_storage(ty, expression.span()))
        .transpose()?;
    let result = builder.expr_expected(expression, expected.as_ref())?;
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
pub(super) fn evaluate(world: &World, key: &str, calls: &[String]) -> Result<(Ty, Vec<Bits>, Option<Arc<GeneratedOrigin>>)> {
    let definition = &world.functions[key];
    if let Some(constant) = definition.constant.get() {
        if let Some(inputs) = &world.build_declarations {
            inputs.borrow_mut().insert(crate::build::Declaration::Constant(key.into()));
        }
        return Ok(constant.value.clone());
    }
    world.evaluating_constants.evaluate(key, || {
        let parent_inputs = world.build_declarations.as_ref().map(|inputs| inputs.take());
        let result = (|| {
            let instance = crate::generics::instantiate(world, key, definition, Vec::new(), &[])?;
            let ty=instance.bindings.type_name(world,&definition.function.result(),
                (&definition.source,&definition.module,&definition.imports),definition.function.span())?;
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
            let (returned, _) = builder.function_body(&instance, &body)?;
            let value = builder.ordinary_return(returned, definition.function.span())?.unwrap();
            if value.ty.contains_views()
                || value.constant.is_none()
                || builder
                    .code
                    .iter()
                    .any(|inst| !matches!(&(inst).kind, InstKind::Constant(..)))
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
        })();
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
    }).unwrap_or_else(|| definition.source.fail(
        definition.function.span(), "B160", format!("编译期值与类型递归依赖 {key}"),
    ))
}
impl World {

    pub(crate) fn source_shape(
        &self, shape: &crate::syntax::Shape, bindings: &Arc<crate::generics::Bindings>,
        source: &Source, span: Span, module: &str, imports: &[String],
    ) -> Result<u32> {
        match shape.kind() {
            crate::syntax::ShapeKind::Literal(value) => Ok(value),
            crate::syntax::ShapeKind::Constant(name) => match bindings.values.get(name) {
                Some(value) => Ok(*value),
                None => self.shape(&Some(Shape::Constant(name.into())), source, span, module, imports).map(Option::unwrap),
            },
            crate::syntax::ShapeKind::Expression(expression) => {
                // Generated nodes can share a diagnostic span. The source store's
                // actual expression offset distinguishes their active evaluations.
                let key = format!("<shape>:{}:{module}:{:?}:{:?}:{:?}:{:?}",
                    source.path.display(), expression, expression.span(), bindings.types, bindings.values);
                integer_expression(self, &key, source, module, imports, &Ty::scalar(Scalar::U32),
                    &expression, bindings,
                    ("B234", "形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用"))
            }
        }
    }

    pub(crate) fn shape(
        &self,
        shape: &Option<Shape>,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
    ) -> Result<Option<u32>> {
        match shape {
            None => Ok(None),
            Some(Shape::Literal(n)) => Ok(Some(*n)),
            Some(Shape::Expression(expression))=>{
                let key=format!("<shape>:{}:{module}:{}",source.path.display(),crate::ast::expression_name(expression));
                // This semantic reference has a module namespace, not a source
                // declaration's formal parameters. Source shapes use source_shape
                // with their actual instance bindings before reaching this boundary.
                let bindings=Arc::new(crate::generics::Bindings::default());
                integer_value(self,&key,source,module,imports,&Ty::scalar(Scalar::U32),expression.span,&bindings,
                    ("B234","形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用"),
                    |builder|builder.reference_expr(expression,None)).map(Some)
            },
            Some(Shape::Constant(name)) => {
                let key = self
                    .resolve(name, module, imports, true).unique()
                    .filter(|key| self.functions[key].function.kind() == FunctionKind::Constant)
                    .ok_or_else(|| Diagnostic {
                        source: source.clone(),
                        span,
                        code: "B015",
                        message: format!("未知或不明确的编译期 const {name}"),
                    })?;
                let (ty, words) = constant_value(self, &key)?;
                if ty != Ty::scalar(Scalar::U32) {
                    return source.fail(span, "B015", "形状或 const 实参必须为 u32 标量");
                }
                Ok(Some(words[0] as u32))
            }
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
pub(crate) fn variant_discriminant(world:&World,identity:&str,source:&Source,module:&str,imports:&[String],representation:&Ty,expression:&Expr,bindings:&Arc<crate::generics::Bindings>)->Result<u32> {
    integer_expression(world,&format!("<variant-tag>:{identity}"),source,module,imports,representation,expression,bindings,
        ("B268","变体编号必须是可完全编译期求值的底层整数"))
}
fn integer_expression(world:&World,key:&str,source:&Source,module:&str,imports:&[String],representation:&Ty,expression:&Expr,bindings:&Arc<crate::generics::Bindings>,diagnostic:(&'static str,&'static str))->Result<u32> {
    bindings.check_expression(expression,source)?;
    integer_value(world,key,source,module,imports,representation,expression.span(),bindings,diagnostic,
        |builder|builder.expr(expression))
}
fn integer_value(world:&World,key:&str,source:&Source,module:&str,imports:&[String],representation:&Ty,
    span:Span,bindings:&Arc<crate::generics::Bindings>,diagnostic:(&'static str,&'static str),
    read:impl FnOnce(&mut Builder<'_>)->Result<Option<Value>>)->Result<u32> {
    world.evaluating_constants.evaluate(key, || {
        let context=constant_context(key);
        let mut builder=Builder::expression(world,&context,source,module,imports,bindings.clone(),span);
        builder.in_function=true;
        let result=read(&mut builder)?;
        let value=builder.value_result(result,span)?;
        builder.require_same(representation,&value.ty,span)?;
        if value.constant.is_none()||builder.code.iter().any(|instruction|!matches!(&instruction.kind,InstKind::Constant(..))){
            return source.fail(span,diagnostic.0,diagnostic.1);
        }
        Ok(value.constant.unwrap()[0]as u32)
    }).unwrap_or_else(|| source.fail(span,"B160",format!("编译期表达式与类型递归依赖 {key}")))
}

// The runtime/SDK schema is kept as data. This reader borrows its actual nodes;
// all typing, numeric operations, construction and emitted code stay in Builder.
impl Builder<'_> {
    fn reference_expr(&mut self,expression:&crate::ast::Expr,expected:Option<&Ty>)->Result<Option<Value>> {
        let mut value=self.at_source(expression.span,SourceSyntax::Expression,|builder|
            builder.reference_syntax(expression,expected))?;
        if let Some(value)=&mut value{self.bound_value(value);}
        Ok(value)
    }
    fn reference_syntax(&mut self,expression:&crate::ast::Expr,expected:Option<&Ty>)->Result<Option<Value>> {
        use crate::ast::ExprKind as K;
        let span=expression.span;
        match &expression.kind {
            K::Number(number)=>self.number_expression(number,span),
            K::StaticU32(value)=>self.constant_expression(Scalar::U32,Bits::from(*value)),
            K::Bool(value)=>self.constant_expression(Scalar::Bool,Bits::from(*value)),
            K::Name(name)=>self.name_expr(name,span).map(Some),
            K::Member(base,field)=>{
                let Some(value)=self.reference_expr(base,None)? else{return Ok(None);};
                self.member_value(value,field,span).map(Some)
            }
            K::Index(base,index)=>{
                let Some(value)=self.reference_expr(base,None)? else{return Ok(None);};
                if value.ty.indexed_element().is_none(){return self.fail(span,"B136","此类型不能按固定值下标读取");}
                self.array_value_at(value,index.span,|builder|builder.reference_expr(index,None))
            }
            K::Unary(op,value)=>{
                if op=="-"&&let K::Number(number)=&value.kind{return self.negative_number(number,span).map(Some);}
                let Some(value)=self.reference_expr(value,None)? else{return Ok(None);};
                self.unary_value(op,value,span).map(Some)
            }
            K::Binary(op,left,right)=>{
                let Some(left)=self.reference_expr(left,None)? else{return Ok(None);};
                if op=="&&"||op=="||"{
                    return self.short_circuit_value(op,left,right.span,|builder|
                        Ok(builder.reference_expr(right,None)?.map(returns::Returned::value)),|_,_|Ok(()),span);
                }
                let Some(right)=self.reference_expr(right,None)? else{return Ok(None);};
                self.binary(op,left,right,span,None).map(Some)
            }
            K::Select(condition,yes,no)=>{
                let Some(condition)=self.reference_expr(condition,None)? else{return Ok(None);};
                self.require_bool(&condition,expression.span)?;
                let returned=self.selection_after_condition(condition,
                    (yes.span,|builder|Ok(builder.reference_expr(yes,expected)?.map(returns::Returned::value))),
                    (no.span,|builder|Ok(builder.reference_expr(no,expected)?.map(returns::Returned::value))),
                    |_,_|Ok(()),span)?;
                self.ordinary_return(returned,span)
            }
            K::Construct(name,args)=>{
                let ty=self.world.ty(name,self.scope.source(),span,self.scope.module(),self.scope.imports())?;
                self.record_values(ty,args.len(),span,|builder,index,expected|builder.reference_expr(&args[index],expected))
            }
            K::Call(name,args)=>self.reference_call(name,None,args,expected,span),
            K::Specialize(name,types,args)=>self.reference_call(name,Some(types),args,expected,span),
            K::FunctionRef(..)=>self.fail(span,"B236","静态函数引用只能作为集合算法参数或 bind 的首参数，不能作为普通值"),
            K::Cooperate(..)=>self.fail(span,"B208","cooperate 调用只能作为协作根块的局部声明"),
            K::Target(..)=>self.fail(span,"B266","target 调用由 CPU 主流程发起，元素实例与协作成员不能选择发起位置"),
            K::Invoke(callee,args)=>{
                let result=self.function_value_arguments(callee.span,span,
                    |builder|builder.reference_expr(callee,None),
                    |builder,parameters|builder.reference_call_arguments(cooperative::CallParameters::CFunction(parameters),args,span))?;
                self.ordinary_return(result,span)
            },
        }
    }
    fn reference_call(&mut self,name:&str,explicit:Option<&[GenericArgument]>,args:&[crate::ast::Expr],expected:Option<&Ty>,span:Span)->Result<Option<Value>> {
        if let Some((ty,index))=self.variant_constructor_type(name,explicit,expected,span)?{
            if ty.owns_resources(){return self.fail(span,"B224","拥有资源的记录由普通CPU应用函数组织");}
            return self.variant_values(ty,index,args.len(),span,|builder,payload|builder.reference_expr(&args[0],Some(payload)));
        }
        if matches!(name,"Ok"|"Err"){
            let ty=match explicit {
                Some(arguments @ [GenericArgument::Type(_),GenericArgument::Type(_)])=>{
                    let mut name=TypeName::named("result");name.arguments=arguments.to_vec();
                    self.expected_storage(&name,span)?
                }
                Some(_)=>return self.fail(span,"B269","Ok／Err需要两个明确类型实参"),
                None=>match expected.filter(|ty|ty.result){
                    Some(ty)=>ty.clone(),
                    None=>return self.fail(span,"B269","Ok／Err需要已声明的result上下文或两个明确类型实参"),
                },
            };
            if ty.owns_resources(){return self.fail(span,"B224","拥有资源的记录由普通CPU应用函数组织");}
            return self.variant_values(ty,usize::from(name=="Err"),args.len(),span,
                |builder,payload|builder.reference_expr(&args[0],Some(payload)));
        }
        if let Some(explicit)=explicit {
            if name=="unsafe_function" {
                return self.function_from_address(explicit,args.len(),span,args.first().map(|arg|arg.span).unwrap_or(span),
                    |builder|builder.reference_expr(&args[0],None));
            }
            if name=="unsafe_position" {
                let Some(place)=self.external_position_value(explicit,args.len(),span,args.first().map(|arg|arg.span).unwrap_or(span),
                    |builder|builder.reference_expr(&args[0],None))?else{return Ok(None);};
                return self.read_place(&place,span,None).map(Some);
            }
            if matches!(name,"view"|"dataset"){return self.fail(span,"B234","形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用");}
            if matches!(name,"array"|"owned")||!self.world.resolve(name,self.scope.module(),self.scope.imports(),false).is_missing(){
                let mut name=TypeName::named(name);name.arguments=explicit.to_vec();
                let ty=self.world.ty(&name,self.scope.source(),span,self.scope.module(),self.scope.imports())?;
                return self.record_values(ty,args.len(),span,|builder,index,expected|builder.reference_expr(&args[index],expected));
            }
        }
        if explicit.is_none()&&(matches!(name,"array"|"owned")||!self.world.resolve(name,self.scope.module(),self.scope.imports(),false).is_missing()){
            let ty=self.expected_storage(&TypeName::named(name),span)?;
            if ty.owns_resources(){return self.fail(span,"B224","拥有资源的记录由普通CPU应用函数组织");}
        }
        let resolution=self.world.resolve(name,self.scope.module(),self.scope.imports(),true);
        match resolution {
            crate::frontend::NameResolution::Unique(key)=>{
                if self.world.functions[&key].function.params().len()!=args.len(){
                    return self.fail(span,"B111","函数实参数量与声明不匹配");
                }
                let declaration=self.world.functions[&key].clone();
                let Some(arguments)=self.reference_call_arguments(cooperative::CallParameters::Declaration(&declaration),args,span)?else{return Ok(None);};
                let instance=self.specialized_arguments(key,&arguments,explicit,span)?;
                let returned=self.invoke_returned(instance,arguments,span,None,None)?;
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
                let ty=self.expected_storage(ty,span)?;
                return self.constant_expression(Scalar::U64,Bits::from(if name=="size_of"{ty.size}else{ty.align}));
            }
            return self.fail(span,"B158",format!("未知或未导出的泛型函数{name}"));
        }
        if name=="c_function"{
            return self.c_function_from(span,|builder|{
                if args.len()!=1{return builder.fail(span,"B153","c_function需要一个明确的C ABI函数引用");}
                let(reference_name,types)=match &args[0].kind{
                    crate::ast::ExprKind::Name(name)=>(name.as_str(),None),
                    crate::ast::ExprKind::FunctionRef(name,types)=>(name.as_str(),Some(types.as_slice())),
                    _=>return builder.fail(args[0].span,"B153","c_function需要C ABI函数名及具体类型／const实参"),
                };
                callback::signature_named(builder.world,&builder.scope,reference_name,span,args[0].span,
                    ||Ok(types.map(std::borrow::Cow::Borrowed)))
            }).map(Some);
        }
        if name=="println"{
            return self.println_value(args.len(),span,args.first().map(|a|a.span).unwrap_or(span),
                |builder|builder.reference_expr(&args[0],None));
        }
        if let Some(value)=self.subgroup(name,args.len(),span,
            |builder,index|builder.reference_expr(&args[index],None),|index|args[index].span){return value;}
        if name=="unsafe_read"{
            if args.len()!=2{return self.fail(span,"B181","unsafe_read 需要 read 数据集与 u32 索引；调用者负责外部索引有效性");}
            let Some(view)=self.reference_expr(&args[0],None)?else{return Ok(None);};
            return self.unsafe_read_view(view,span,args[1].span,|builder|builder.reference_expr(&args[1],None));
        }
        if !self.world.resolve(name,self.scope.module(),self.scope.imports(),false).is_missing(){
            let ty=self.world.ty(&TypeName::named(name),self.scope.source(),span,self.scope.module(),self.scope.imports())?;
            return self.record_values(ty,args.len(),span,|builder,index,expected|builder.reference_expr(&args[index],expected));
        }
        if name=="index"{return self.fail(span,"B155","index() 只能在当前 program 的 parallel 域中使用");}
        if name=="ordinal"{return self.fail(span,"B168","ordinal只能用于元素游标");}
        self.builtin_values(name,span,|builder|builder.reference_arguments(args))
    }
    fn reference_call_arguments(&mut self,parameters:cooperative::CallParameters<'_>,args:&[crate::ast::Expr],span:Span)->Result<Option<Vec<cooperative::Argument>>> {
        self.call_arguments_from(parameters,args.len(),span,|index|args[index].span,|builder,index|{
            let access=parameters.access(index);
            if matches!(access,Access::InOut|Access::Write|Access::Atomic)&&builder.reference_is_place(&args[index]){
                let Some(place)=builder.reference_place(&args[index])?else{return Ok(None);};
                if access==Access::Atomic{
                    return builder.atomic_place_value(place,args[index].span).map(|place|place.map(cooperative::Argument::Atomic));
                }
                let place=builder.mutable_place_value(place,access,args[index].span)?;
                return Ok(Some(cooperative::Argument::Reference(builder.borrow_owned(place))));
            }
            let expected=parameters.expected(builder.world,&builder.scope,index)?;
            let value=builder.reference_expr(&args[index],expected.as_ref())?;
            if !builder.flow.continues(){return Ok(None);}
            let result=value.map(returns::Returned::value);
            if matches!(access,Access::InOut|Access::Write){
                builder.mutable_returned_argument(result,access,args[index].span)
            }else if access==Access::Atomic{
                // A value has no storage to lend, exactly as in the source place reader.
                builder.fail(args[index].span,"B138","表达式不是可赋值位置")
            }else{
                Ok(result.map(cooperative::Argument::resource))
            }
        })
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
    fn reference_place(&mut self,expression:&crate::ast::Expr)->Result<Option<Place>> {
        self.at_source(expression.span,SourceSyntax::Place,|builder|{
            use crate::ast::ExprKind as K;
            let span=expression.span;
            builder.view_uses.before(span);
            match &expression.kind {
                K::Name(name)=>builder.name_place(name,span).map(Some),
                K::Specialize(name,types,args)if name=="unsafe_position"=>
                    builder.external_position_value(types,args.len(),span,args.first().map(|a|a.span).unwrap_or(span),
                        |builder|builder.reference_expr(&args[0],None)),
                K::Member(base,field)=>{
                    let Some(place)=builder.reference_place(base)?else{return Ok(None);};
                    builder.member_position(place,field,span)
                }
                K::Index(base,index)=>{
                    if builder.reference_is_place(base){
                        let Some(place)=builder.reference_place(base)?else{return Ok(None);};
                        if place.ty().indexed_element().is_some(){
                            let Some(index_value)=builder.reference_expr(index,None)?else{return Ok(None);};
                            return builder.array_index(place,index_value,index.span).map(Some);
                        }
                        let view=builder.read_place(&place,base.span,None)?;
                        builder.view_value_at(view,index.span,|builder|builder.reference_expr(index,None))
                    }else{
                        let Some(value)=builder.reference_expr(base,None)?else{return Ok(None);};
                        if value.ty.array.is_some(){
                            let place=builder.materialize_array(value,None).place;
                            let Some(index_value)=builder.reference_expr(index,None)?else{return Ok(None);};
                            builder.array_index(place,index_value,index.span).map(Some)
                        }else{
                            builder.view_value_at(value,index.span,|builder|builder.reference_expr(index,None))
                        }
                    }
                }
                _=>builder.fail(span,"B138","表达式不是可赋值位置"),
            }
        })
    }
    fn reference_arguments(&mut self,args:&[crate::ast::Expr])->Result<Option<Vec<Value>>> {
        args.iter().map(|argument|self.reference_expr(argument,None)).collect()
    }
}
