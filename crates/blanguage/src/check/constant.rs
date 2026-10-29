//! 整数常量按实际标量位宽求值；已证明的值直接成为既有 IR 常量。
use super::*;
use std::num::NonZeroU64;

impl Builder<'_> {
    pub(super) fn module_constant(&mut self, key: String) -> Result<Value> {
        let (ty, words) = evaluate(self.world, &key, &self.calls)?;
        Ok(self.constant_value(ty, words))
    }
    pub(super) fn literal(&mut self, scalar: Scalar, bits: Bits) -> Value {
        let mut value = self.constant(scalar, bits);
        value.literal = true;
        value
    }
    pub(super) fn constant_value(&mut self, ty: Ty, bits: Vec<Bits>) -> Value {
        let regs = ty
            .components
            .iter()
            .zip(&bits)
            .map(|((s, _), b)| self.constant(*s, *b).regs[0])
            .collect();
        Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs,
            uniform: true,
            constant: Some(bits),
        }
    }
}
/// 使用现有表达式、调用与常量分析；不编译或运行宿主／设备初始化函数。
pub(super) fn constant_value(world: &World, key: &str) -> Result<(Ty, Vec<Bits>)> {
    evaluate(world, key, &[])
}
/// 阶段派生参数复用已有表达式及常量语义；没有宿主求值器或新增运行阶段。
pub(super) fn stage_expression(
    world: &World,
    definition: &Arc<Definition>,
    roots: &BTreeMap<String, Root>,
    program: &Program,
    expression: &Expr,
) -> Result<Root> {
    let mut builder = Builder::new(world, definition, program, 0);
    builder.in_function = true;
    builder.roots(roots);
    // roots中的无关uniform装载不是当前表达式的计算，不要求它们为常量。
    let first = builder.code.len();
    let result = builder.expr(expression)?;
    let value = builder.value_result(result, expression.span)?;
    if value.ty.contains_views()
        || value.constant.is_none()
        || builder.code[first..]
            .iter()
            .any(|inst| !matches!(inst, Inst::Constant(..)))
    {
        return definition.source.fail(
            expression.span,
            "B114",
            "阶段派生参数必须可完全编译期求值；运行期或逐元素表达式放在 parallel 中",
        );
    }
    Ok(Root::Literal(value.ty, value.constant.unwrap()))
}
fn evaluate(world: &World, key: &str, calls: &[String]) -> Result<(Ty, Vec<Bits>)> {
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
        let context = Program {
            name: key.into(),
            target: Target::Compute,
            buffers: Vec::new(),
            accelerations: Vec::new(),
            textures: Vec::new(),
            uniforms: Vec::new(),
            uniform_bytes: 0,
            source_stages: 0,
            stages: Vec::new(),
            prefix_requirements: Vec::new(),
            prefix_guarantees: Vec::new(),
        };
        let mut builder = Builder::new(world, definition, &context, 0);
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
                .any(|inst| !matches!(inst, Inst::Constant(..)))
        {
            return definition.source.fail(
                definition.function.span,
                "B234",
                "const 初始化尚不能完全在编译期求值；不允许资源、运行期运算或控制流",
            );
        }
        Ok((value.ty, value.constant.unwrap()))
    })();
    world.evaluating_constants.borrow_mut().remove(key);
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
pub(super) fn cast(from: Scalar, into: Scalar, bits: Bits) -> Option<Bits> {
    if from == into {
        return Some(into.truncate(bits));
    }
    if into == Scalar::Bool {
        return Some(Bits::from(if from == Scalar::F32 {
            f32::from_bits(bits as u32) != 0.0
        } else {
            bits != 0
        }));
    }
    if into == Scalar::F32 {
        let value = if from.is_signed() { from.signed(bits) as f32 } else { bits as f32 };
        return Some(Bits::from(value.to_bits()));
    }
    if from == Scalar::F32 {
        let value = f32::from_bits(bits as u32);
        let maximum = (1u128 << (into.size() * 8 - u32::from(into.is_signed()))) as f32;
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
    if scalar == Scalar::F32 {
        return None;
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

// 编号表达式沿原常量求值，不建立另一份宿主表达式解释器。
pub(crate) fn variant_discriminant(world: &World, identity: &str, source: &Source, module: &str, imports: &[String], representation: &Ty, expression: &Expr) -> Result<u32> {
    let key=format!("<variant-tag>:{identity}");
    if !world.evaluating_constants.borrow_mut().insert(key.clone()) {
        return source.fail(expression.span,"B160",format!("编译期变体编号与类型递归依赖 {identity}"));
    }
    let result=(|| {
    let definition = Arc::new(Definition {
        source: source.clone(), module: module.into(), imports: imports.to_vec(), public: false, compiled: None,
        execution: crate::execution::Requirement::Invocation,
        function: Function { name: "<variant-number>".into(), types: Vec::new(), values: Vec::new(), params: Vec::new(),
            result: representation.syntax(), kind: FunctionKind::Constant, target: Target::Compute,
            contracts: Vec::new(), external: None, body: Some(vec![Stmt { span: expression.span, kind: StmtKind::Return(Some(expression.clone())) }]), span: expression.span },
    });
    let context = Program { name: definition.function.name.clone(), target: Target::Compute,
        buffers: Vec::new(), accelerations: Vec::new(), textures: Vec::new(), uniforms: Vec::new(), uniform_bytes: 0,
        source_stages: 0, stages: Vec::new(), prefix_requirements: Vec::new(), prefix_guarantees: Vec::new() };
    let mut builder = Builder::new(world, &definition, &context, 0);
    builder.in_function = true;
    let result = builder.expr(expression)?;
    let value = builder.value_result(result, expression.span)?;
    builder.require_same(representation, &value.ty, expression.span)?;
    if value.constant.is_none() || builder.code.iter().any(|inst| !matches!(inst, Inst::Constant(..))) {
        return source.fail(expression.span, "B268", "变体编号必须是可完全编译期求值的底层整数");
    }
    Ok(value.constant.unwrap()[0] as u32)
    })();
    world.evaluating_constants.borrow_mut().remove(&key);
    result
}
