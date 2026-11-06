//! 标量常量按实际类型与位宽求值；已知值直接成为既有 IR 常量。
use super::*;
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
    let mut builder = Builder::new(world, definition, program, 0, Some(&instance.name))?;
    builder.source_context = Arc::new(SourceContext { domain_path: domain_path.clone(), ..(*builder.source_context).clone() });
    builder.in_function = true;
    builder.roots(roots);
    // roots中的无关uniform装载不是当前表达式的计算，不要求它们为常量。
    let first = builder.code.len();
    let expected = annotation
        .map(|ty| builder.expected_storage(ty, expression.span))
        .transpose()?;
    let result = builder.expr_expected(expression, expected.as_ref())?;
    let mut value = builder.value_result(result, expression.span)?;
    if let Some(expected) = expected {
        builder.require_same(&expected, &value.ty, expression.span)?;
    }
    if value.ty.contains_views()
        || value.constant.is_none()
        || builder.code[first..]
            .iter()
            .any(|inst| !matches!(&(inst).kind, InstKind::Constant(..)))
    {
        return definition.source.fail(
            expression.span,
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
            expression: builder.source_at(expression.span, SourceSyntax::Expression), operations,
        }),
    }));
    let root = Root::Literal(value.ty, value.constant.unwrap(), origin);
    Ok(if matches!(&expression.kind, ExprKind::Name(_)) {
        root.reference_literal(builder.source_at(expression.span, SourceSyntax::Expression))
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
fn constant_context(name:&str)->Program {
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
            source_stages: 0,
            stages: Vec::new(),
        functions: std::sync::Arc::from([]),
            prefix_requirements: Vec::new(),
            prefix_guarantees: Vec::new(), element_requirements: Vec::new(), element_guarantees: Vec::new(), length_requirements: Vec::new(), imports: BTreeMap::new(),
        }
}
pub(super) fn evaluate(world: &World, key: &str, calls: &[String]) -> Result<(Ty, Vec<Bits>, Option<Arc<GeneratedOrigin>>)> {
    let definition = &world.functions[key];
    if !world.evaluating_constants.borrow_mut().insert(key.into()) {
        return definition.source.fail(
            definition.function.span,
            "B160",
            format!("编译期值与类型递归依赖 {key}"),
        );
    }
    let result = (|| {
        let definition = &world.functions[key];
        let ty = &definition.function.result;
        if ty.dataset || ty.tile || ty.view || ty.name == "void" || ty.is_never() {
            return definition.source.fail(
                definition.function.span,
                "B234",
                "const 需要普通值类型，不能持有资源或 void",
            );
        }
        let context=constant_context(key);
        let mut builder = Builder::new(world, definition, &context, 0, None)?;
        builder.in_function = true;
        builder.calls = calls.to_vec();
        let value = builder
            .inline_function(key.into(), Vec::new(), definition.function.span)?
            .unwrap();
        if value.ty.contains_views()
            || value.constant.is_none()
            || builder
                .code
                .iter()
                .any(|inst| !matches!(&(inst).kind, InstKind::Constant(..)))
        {
            return definition.source.fail(
                definition.function.span,
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
    world.evaluating_constants.borrow_mut().remove(key);
    if result.is_ok() && let Some(inputs) = &world.build_declarations {
        inputs.borrow_mut().insert(crate::build::Declaration::Constant(key.into()));
    }
    result
}
impl World {
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
                let key=format!("<shape>:{}:{module}:{}",source.path.display(),crate::function_types::expression_name(expression));
                Ok(Some(integer_expression(self,&key,source,module,imports,&Ty::scalar(Scalar::U32),expression,
                    ("B234","形状或const实参必须完全在编译期求值，不能包含运行期运算或副作用"))?))
            },
            Some(Shape::Constant(name)) => {
                let key = self
                    .resolve(name, module, imports, true)
                    .filter(|key| self.functions[key].function.kind == FunctionKind::Constant)
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
pub(super) fn unary(operation: UnaryOp, scalar: Scalar, bits: Bits) -> Bits {
    match operation {
        UnaryOp::FloatNegate => bits ^ (1 << (scalar.bit_width() - 1)),
        UnaryOp::SignedNegate | UnaryOp::UnsignedNegate => scalar.truncate(0u64.wrapping_sub(bits)),
        UnaryOp::LogicalNot => Bits::from(bits == 0),
        UnaryOp::BitNot => scalar.truncate(!bits),
        UnaryOp::Bitcast => bits,
    }
}
pub(super) fn cast(from: Scalar, into: Scalar, bits: Bits) -> Option<Bits> {
    if from == into {
        return Some(into.truncate(bits));
    }
    if into == Scalar::Bool {
        return Some(Bits::from(match from {
            Scalar::F32 => f32::from_bits(bits as u32) != 0.0,
            Scalar::F64 => f64::from_bits(bits) != 0.0,
            _ => bits != 0,
        }));
    }
    if into.is_float() {
        let value = match from {
            Scalar::F32 => f32::from_bits(bits as u32) as f64,
            Scalar::F64 => f64::from_bits(bits),
            _ if from.is_signed() => from.signed(bits) as f64,
            _ => bits as f64,
        };
        // Integer-to-f32 rounds directly to f32, avoiding a double rounding through f64.
        return Some(if into == Scalar::F64 { value.to_bits() } else {
            (match from {
                Scalar::F32 | Scalar::F64 => value as f32,
                _ if from.is_signed() => from.signed(bits) as f32,
                _ => bits as f32,
            }).to_bits() as Bits
        });
    }
    if from.is_float() {
        let value = if from == Scalar::F64 { f64::from_bits(bits) } else { f32::from_bits(bits as u32) as f64 };
        let maximum = (1u128 << (into.size() * 8 - u32::from(into.is_signed()))) as f64;
        let minimum = if into.is_signed() { -maximum } else { 0.0 };
        if !(minimum..maximum).contains(&value) {
            return None;
        }
        return Some(into.truncate(if into.is_signed() { value as i64 as Bits } else { value as Bits }));
    }
    Some(into.truncate(if from.is_signed() { from.signed(bits) as Bits } else { bits }))
}
pub(super) fn divide(scalar: Scalar, a: Bits, divisor: NonZeroU64, remainder: bool) -> Bits {
    let b = divisor.get();
    let bits = if scalar.is_signed() {
        if remainder {
            scalar.signed(a).wrapping_rem(scalar.signed(b)) as Bits
        } else {
            scalar.signed(a).wrapping_div(scalar.signed(b)) as Bits
        }
    } else if remainder {
        a % b
    } else {
        a / b
    };
    scalar.truncate(bits)
}
pub(super) fn binary(op: BinaryOp, scalar: Scalar, a: Bits, b: Bits) -> Option<Bits> {
    if scalar == Scalar::F64 {
        let (a,b) = (f64::from_bits(a),f64::from_bits(b));
        return Some(match op {
            BinaryOp::FAdd => (a+b).to_bits(), BinaryOp::FSub => (a-b).to_bits(),
            BinaryOp::FMul => (a*b).to_bits(), BinaryOp::FDiv => (a/b).to_bits(),
            BinaryOp::Compare(c) => Bits::from(match c {
                Comparison::Equal => a==b, Comparison::NotEqual => a!=b,
                Comparison::Less => a<b, Comparison::Greater => a>b,
                Comparison::LessEqual => a<=b, Comparison::GreaterEqual => a>=b,
            }),
            _ => return None,
        });
    }
    if scalar == Scalar::F32 {
        let (a, b) = (f32::from_bits(a as u32), f32::from_bits(b as u32));
        return Some(Bits::from(match op {
            BinaryOp::FAdd => (a + b).to_bits(),
            BinaryOp::FSub => (a - b).to_bits(),
            BinaryOp::FMul => (a * b).to_bits(),
            BinaryOp::FDiv => (a / b).to_bits(),
            BinaryOp::Compare(c) => return binary(BinaryOp::Compare(c), Scalar::F64, (a as f64).to_bits(), (b as f64).to_bits()),
            _ => return None,
        }));
    }
    let shift = (b & (Bits::from(scalar.size()) * 8 - 1)) as u32;
    let bits = match op {
        BinaryOp::IAdd => a.wrapping_add(b),
        BinaryOp::ISub => a.wrapping_sub(b),
        BinaryOp::IMul => a.wrapping_mul(b),
        BinaryOp::And => a & b,
        BinaryOp::Or => a | b,
        BinaryOp::Xor => a ^ b,
        BinaryOp::ShiftLeft => a.wrapping_shl(shift),
        BinaryOp::ShiftRightLogical => a.wrapping_shr(shift),
        BinaryOp::ShiftRightArithmetic => scalar.signed(a).wrapping_shr(shift) as Bits,
        BinaryOp::Compare(c) => {
            let ordering = if scalar.is_signed() {
                scalar.signed(a).cmp(&scalar.signed(b))
            } else {
                a.cmp(&b)
            };
            Bits::from(match c {
                Comparison::Equal => ordering.is_eq(),
                Comparison::NotEqual => !ordering.is_eq(),
                Comparison::Less => ordering.is_lt(),
                Comparison::Greater => ordering.is_gt(),
                Comparison::LessEqual => !ordering.is_gt(),
                Comparison::GreaterEqual => !ordering.is_lt(),
            })
        }
        BinaryOp::FAdd | BinaryOp::FSub | BinaryOp::FMul | BinaryOp::FDiv => return None,
    };
    Some(scalar.truncate(bits))
}

// 编号与形状共享原整数表达式、类型及常量求值所有者。
pub(crate) fn variant_discriminant(world:&World,identity:&str,source:&Source,module:&str,imports:&[String],representation:&Ty,expression:&Expr)->Result<u32> {
    integer_expression(world,&format!("<variant-tag>:{identity}"),source,module,imports,representation,expression,
        ("B268","变体编号必须是可完全编译期求值的底层整数"))
}
fn integer_expression(world:&World,key:&str,source:&Source,module:&str,imports:&[String],representation:&Ty,expression:&Expr,diagnostic:(&'static str,&'static str))->Result<u32> {
    if !world.evaluating_constants.borrow_mut().insert(key.into()) {
        return source.fail(expression.span,"B160",format!("编译期表达式与类型递归依赖 {key}"));
    }
    let result=(|| {
        let definition=Arc::new(Definition {
            syntax_origin:crate::frontend::DefinitionSyntax::Generated(None),
            source:source.clone(),module:module.into(),imports:imports.to_vec(),public:false,compiled:None,
            execution:crate::execution::Requirement::Invocation,
            function:Function {name:"<integer-expression>".into(),types:Vec::new(),values:Vec::new(),params:Vec::new(),
                result:representation.syntax(),kind:FunctionKind::Constant,target:Target::Compute,
                contracts:Vec::new(),c_abi:false,external:None,
                body:Some(vec![Stmt {span:expression.span,kind:StmtKind::Return(Some(expression.clone()))}]),span:expression.span},
        });
        let context=constant_context(&definition.function.name);
        let mut builder=Builder::new(world,&definition,&context,0,None)?;builder.in_function=true;
        let result=builder.expr(expression)?;let value=builder.value_result(result,expression.span)?;
        builder.require_same(representation,&value.ty,expression.span)?;
        if value.constant.is_none()||builder.code.iter().any(|instruction|!matches!(&(instruction).kind,InstKind::Constant(..))) {
            return source.fail(expression.span,diagnostic.0,diagnostic.1);
        }
        Ok(value.constant.unwrap()[0]as u32)
    })();
    world.evaluating_constants.borrow_mut().remove(key);result
}
