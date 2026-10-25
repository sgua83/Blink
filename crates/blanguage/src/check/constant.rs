//! 整数常量使用源语言的模 32 位语义；已证明的值直接成为既有 IR 常量。
use super::*;
use std::num::NonZeroU32;

impl Builder<'_> {
    pub(super) fn module_constant(&mut self, key: String) -> Result<Value> {
        let (ty, words) = evaluate(self.world, &key, &self.calls)?;
        Ok(self.constant_value(ty, words))
    }
    pub(super) fn literal(&mut self, scalar: Scalar, bits: u32) -> Value {
        let mut value = self.constant(scalar, bits);
        value.literal = true;
        value
    }
    pub(super) fn constant_value(&mut self, ty: Ty, bits: Vec<u32>) -> Value {
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
pub(super) fn constant_value(world: &World, key: &str) -> Result<(Ty, Vec<u32>)> {
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
    let value = builder.expr(expression)?;
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
fn evaluate(world: &World, key: &str, calls: &[String]) -> Result<(Ty, Vec<u32>)> {
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
        if ty.dataset || ty.tile || ty.view || ty.name == "void" {
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
                Ok(Some(words[0]))
            }
        }
    }
}
pub(super) fn divide(scalar: Scalar, a: u32, divisor: NonZeroU32, remainder: bool) -> u32 {
    let b = divisor.get();
    if scalar == Scalar::I32 {
        if remainder {
            (a as i32).wrapping_rem(b as i32) as u32
        } else {
            (a as i32).wrapping_div(b as i32) as u32
        }
    } else if remainder {
        a % b
    } else {
        a / b
    }
}
pub(super) fn binary(op: BinaryOp, scalar: Scalar, a: u32, b: u32) -> Option<u32> {
    if scalar == Scalar::F32 {
        return None;
    }
    Some(match op {
        BinaryOp::IAdd => a.wrapping_add(b),
        BinaryOp::ISub => a.wrapping_sub(b),
        BinaryOp::IMul => a.wrapping_mul(b),
        BinaryOp::And => a & b,
        BinaryOp::Or => a | b,
        BinaryOp::Xor => a ^ b,
        BinaryOp::ShiftLeft => a.wrapping_shl(b),
        BinaryOp::ShiftRightLogical => a.wrapping_shr(b),
        BinaryOp::ShiftRightArithmetic => (a as i32).wrapping_shr(b) as u32,
        BinaryOp::Compare(c) => {
            let ordering = if scalar == Scalar::I32 {
                (a as i32).cmp(&(b as i32))
            } else {
                a.cmp(&b)
            };
            u32::from(match c {
                Comparison::Equal => ordering.is_eq(),
                Comparison::NotEqual => !ordering.is_eq(),
                Comparison::Less => ordering.is_lt(),
                Comparison::Greater => ordering.is_gt(),
                Comparison::LessEqual => !ordering.is_gt(),
                Comparison::GreaterEqual => !ordering.is_lt(),
            })
        }
        BinaryOp::FAdd | BinaryOp::FSub | BinaryOp::FMul | BinaryOp::FDiv => return None,
    })
}
