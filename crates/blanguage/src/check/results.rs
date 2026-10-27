//! 结果使用一个判别字和重叠载荷；分支与传播沿原返回域和资源所有者。
use super::*;

impl Builder<'_> {
    pub(super) fn expr_expected(
        &mut self,
        e: &Expr,
        expected: Option<&Ty>,
    ) -> Result<Option<Value>> {
        match &e.kind {
            ExprKind::Call(name, args) if matches!(name.as_str(), "Ok" | "Err") => {
                let Some(ty) = expected.filter(|ty| ty.result) else {
                    return self.fail(
                        e.span,
                        "B269",
                        "Ok／Err 需要已声明的 result<T,E> 上下文，或显式给出两个类型实参",
                    );
                };
                self.result_constructor(ty.clone(), name == "Ok", args, e.span)
            }
            ExprKind::Select(c, yes, no) => self.selection_expected(c, yes, no, e.span, expected),
            _ => self.expr(e),
        }
    }
    pub(super) fn result_constructor(
        &mut self,
        ty: Ty,
        success: bool,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        let payload = ty.arguments[usize::from(!success)].clone();
        let value = if payload.name == "void" {
            if !args.is_empty() {
                return self.fail(span, "B269", "void 结果分支不接受载荷");
            }
            None
        } else {
            if args.len() != 1 {
                return self.fail(span, "B269", "结果分支需要一个匹配类型的载荷");
            }
            let Some(value) = self.expr_expected(&args[0], Some(&payload))? else {
                return Ok(None);
            };
            self.require_same(&payload, &value.ty, span)?;
            Some(value)
        };
        Ok(Some(self.result_value(ty, success, value)))
    }
    fn result_word(&mut self, value: &Value, index: usize, scalar: Scalar) -> Value {
        let source = value.ty.components[index].0;
        let mut word = Value {
            ty: Ty::scalar(source),
            regs: vec![value.regs[index]],
            proof: value.projected_proof(&[index]),
            uniform: value.uniform,
            literal: false,
            constant: value.constant.as_ref().map(|bits| vec![bits[index]]),
        };
        if source == Scalar::F32 || scalar == Scalar::F32 {
            if source != scalar {
                let reg = self.reg(scalar);
                self.code.push(Inst::Unary {
                    dst: reg,
                    op: UnaryOp::Bitcast,
                    a: word.regs[0],
                });
                word.ty = Ty::scalar(scalar);
                word.regs = vec![reg];
                word.proof = bounds::Proof::default();
            }
            word
        } else {
            self.cast(word, scalar)
        }
    }
    pub(super) fn result_value(&mut self, ty: Ty, success: bool, payload: Option<Value>) -> Value {
        let mut value = self.constant_value(ty.clone(), vec![0; ty.components.len()]);
        let tag = self.constant(Scalar::Bool, u32::from(!success));
        value.regs[0] = tag.regs[0];
        value
            .proof
            .assign(&[0], value.regs.len(), bounds::Proof::capture(&tag));
        if let Some(payload) = payload {
            let offset = ir::align_up(4, ty.align);
            for (index, (_, position)) in payload.ty.components.iter().enumerate() {
                let component = ty
                    .components
                    .iter()
                    .position(|(_, at)| *at == offset + position)
                    .unwrap();
                let word = self.result_word(&payload, index, Scalar::U32);
                value.regs[component] = word.regs[0];
                value.proof.assign(
                    &[component],
                    value.regs.len(),
                    bounds::Proof::capture(&word),
                );
                if let (Some(bits), Some(source)) = (&mut value.constant, &word.constant) {
                    bits[component] = source[0];
                } else {
                    value.constant = None;
                }
            }
            value.uniform = payload.uniform;
        }
        if let Some(bits) = &mut value.constant {
            bits[0] = u32::from(!success);
        }
        value.literal = false;
        value
    }
    pub(super) fn result_payload(&mut self, value: &Value, success: bool) -> Option<Value> {
        let ty = value.ty.arguments[usize::from(!success)].clone();
        if ty.name == "void" {
            return None;
        }
        let offset = ir::align_up(4, value.ty.align);
        let words: Vec<_> = ty
            .components
            .iter()
            .map(|(_, at)| {
                value
                    .ty
                    .components
                    .iter()
                    .position(|(_, word)| *word == offset + at)
                    .unwrap()
            })
            .collect();
        let mut proof = bounds::Proof::default();
        let mut regs = Vec::new();
        let mut constant = Some(Vec::new());
        for (index, ((scalar, _), word)) in ty.components.iter().zip(words).enumerate() {
            let fresh = self.result_word(value, word, *scalar);
            regs.push(fresh.regs[0]);
            proof.assign(
                &[index],
                ty.components.len(),
                bounds::Proof::capture(&fresh),
            );
            if let (Some(bits), Some(source)) = (&mut constant, fresh.constant) {
                bits.push(source[0]);
            } else {
                constant = None;
            }
        }
        Some(Value {
            ty,
            regs,
            proof,
            literal: false,
            uniform: value.uniform,
            constant,
        })
    }
    pub(super) fn result_condition(&mut self, value: &Value, span: Span) -> Result<Value> {
        let tag = self.result_word(value, 0, Scalar::U32);
        let zero = self.constant(Scalar::U32, 0);
        self.binary("==", tag, zero, span)
    }
    fn result_arm(
        &mut self,
        value: &Value,
        success: bool,
        binding: &Option<String>,
        body: &[Stmt],
        span: Span,
    ) -> Result<()> {
        let payload = self.result_payload(value, success);
        if let Some(name) = binding.as_ref().filter(|name| name.as_str() != "_") {
            let payload = payload.unwrap();
            self.bind_local_value(name, payload, span)?;
            self.block(body)?;
            self.env.remove(name);
            Ok(())
        } else {
            self.block(body)
        }
    }
    pub(super) fn result_match(
        &mut self,
        expr: &Expr,
        yes_name: &Option<String>,
        yes: &[Stmt],
        no_name: &Option<String>,
        no: &[Stmt],
        span: Span,
    ) -> Result<()> {
        self.match_returned(expr, yes_name, yes, no_name, no, span)
    }
    pub(super) fn check_result_patterns(
        &self,
        ty: &Ty,
        yes_name: &Option<String>,
        no_name: &Option<String>,
        span: Span,
    ) -> Result<()> {
        if !ty.result {
            return self.fail(span, "B269", "match 需要 result<T,E> 值");
        }
        for (success, binding) in [(true, yes_name), (false, no_name)] {
            let unit = ty.arguments[usize::from(!success)].name == "void";
            if binding.is_none() && !unit {
                return self.fail(span, "B269", "有载荷分支需要名称或 _ 模式");
            }
            if unit && binding.as_ref().is_some_and(|name| name != "_") {
                return self.fail(span, "B269", "void 分支没有可绑定的载荷");
            }
        }
        Ok(())
    }
    pub(super) fn match_value(
        &mut self,
        value: Value,
        yes_name: &Option<String>,
        yes: &[Stmt],
        no_name: &Option<String>,
        no: &[Stmt],
        span: Span,
    ) -> Result<()> {
        self.check_result_patterns(&value.ty, yes_name, no_name, span)?;
        let condition = self.result_condition(&value, span)?;
        self.conditional_blocks(
            condition,
            |b| b.result_arm(&value, true, yes_name, yes, span),
            |b| b.result_arm(&value, false, no_name, no, span),
        )
    }

    pub(super) fn explicit_result(
        &mut self,
        name: &str,
        explicit: &[GenericArgument],
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        let mut ty = TypeName::named("result");
        ty.arguments = explicit.to_vec();
        let ty = self.world.ty(
            &ty,
            &self.definition.source,
            span,
            &self.definition.module,
            &self.definition.imports,
        )?;
        self.result_constructor(ty, name == "Ok", args, span)
    }
}
