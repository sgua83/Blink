//! 记录按声明字段构造；不对异构分量作统一数值强转，不产生运行时对象。
use super::*;
impl Builder<'_> {
    pub(super) fn dataset_field(&self, expr: &Expr) -> Result<(usize, u32, Ty)> {
        match &expr.kind {
            ExprKind::Name(name) => {
                let Some(Binding::Data(view)) = self.env.get(name) else {
                    return self.fail(expr.span, "B200", "需要命名 dataset 或其字段");
                };
                Ok((
                    view.binding,
                    0,
                    self.program.buffers[view.binding].ty.clone(),
                ))
            }
            ExprKind::Member(base, name) => {
                let (binding, word, ty) = self.dataset_field(base)?;
                let Some((offset, field)) = ty.field(name) else {
                    return self.fail(expr.span, "B141", "dataset 字段不存在");
                };
                Ok((binding, word + offset / 4, field.clone()))
            }
            _ => self.fail(expr.span, "B200", "需要命名 dataset 或其字段"),
        }
    }
    pub(super) fn record_expression(
        &mut self,
        name: &TypeName,
        args: &[Expr],
        span: Span,
    ) -> Result<Value> {
        let ty = self.world.ty(
            name,
            &self.definition.source,
            span,
            &self.definition.module,
            &self.definition.imports,
        )?;
        let mut values = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            let expected = ty
                .array
                .as_deref()
                .or_else(|| ty.fields.get(index).map(|(_, _, ty)| ty.as_ref()));
            values.push(self.expr_expected(arg, expected)?);
        }
        self.record_value(name, values, span)
    }
    pub(super) fn record_value(
        &mut self,
        name: &TypeName,
        values: Vec<Value>,
        span: Span,
    ) -> Result<Value> {
        let ty = self.world.ty(
            name,
            &self.definition.source,
            span,
            &self.definition.module,
            &self.definition.imports,
        )?;
        if ty.owns_resources() {
            return self.fail(span, "B224", "拥有资源的记录通过移动构造，不能复制为普通值");
        }
        if ty.result {
            return self.fail(span, "B269", "结果通过 Ok／Err 构造，不能直接构造原始存储");
        }
        if ty.enumeration {
            if values.len() != 1 {
                return self.fail(
                    span,
                    "B268",
                    "编号枚举构造需要一个同类型值或其声明的整数表示",
                );
            }
            let mut value = values.into_iter().next().unwrap();
            if value.ty != ty {
                self.require_same(&Ty::scalar(ty.components[0].0), &value.ty, span)?;
            }
            value.ty = ty;
            value.literal = false;
            return Ok(value);
        }
        if ty.matrix_shape().is_some() {
            return self.matrix_construct(ty, values, span);
        }
        if ty.array.is_some() && values.len() == 1 && values[0].ty == ty {
            return Ok(values.into_iter().next().unwrap());
        }
        if let Some(element) = &ty.array {
            let count = ty.values[0] as usize;
            let values = if values.len() == 1 {
                vec![values[0].clone(); count]
            } else {
                values
            };
            if values.len() != count {
                return self.fail(span, "B157", "array 构造需要全部元素，或一个填充值");
            }
            for value in &values {
                self.require_same(element, &value.ty, span)?;
            }
            let mut proof = bounds::Proof::default();
            let uniform = values.iter().all(|v| v.uniform);
            let mut regs = Vec::new();
            let mut constant = Some(Vec::new());
            for value in values {
                proof.append(&value);
                regs.extend(value.regs);
                constant = constant.zip(value.constant).map(|(mut a, b)| {
                    a.extend(b);
                    a
                });
            }
            return Ok(Value {
                proof,
                literal: false,
                ty,
                regs,
                uniform,
                constant,
            });
        }
        self.pack_record(ty, values, span)
    }
    pub(super) fn pack_record(&mut self, ty: Ty, values: Vec<Value>, span: Span) -> Result<Value> {
        if values.len() != ty.fields.len() {
            return self.fail(span, "B157", "记录构造需要按声明顺序提供每个字段");
        }
        let uniform = values.iter().all(|v| v.uniform);
        let mut regs = Vec::new();
        let mut proof = bounds::Proof::default();
        let mut constant = Some(Vec::new());
        for ((_, _, field), value) in ty.fields.iter().zip(values) {
            self.require_same(field, &value.ty, span)?;
            proof.append(&value);
            regs.extend(value.regs);
            constant = constant.zip(value.constant).map(|(mut a, b)| {
                a.extend(b);
                a
            });
        }
        Ok(Value {
            proof,
            literal: false,
            ty,
            regs,
            uniform,
            constant,
        })
    }
}
