//! 固定维度矩阵按行布局；算术静态降低到既有标量值 IR。
use super::*;

pub(super) enum Binary {
    Elements(usize),
    Product { rows: usize, inner: usize, columns: usize },
    MatrixScalar { count: usize, cast: bool },
    ScalarMatrix { count: usize, cast: bool },
    MatrixVector { rows: usize, columns: usize },
    VectorMatrix { rows: usize, columns: usize },
}
pub(super) fn binary_type(op: &str, a: &Ty, a_literal: bool, b: &Ty, b_literal: bool,
    source: &Source, span: Span) -> Result<(Ty, Binary)> {
    match (a.matrix_shape(), b.matrix_shape()) {
        (Some((rows, columns)), Some(other))
            if matches!(op, "+" | "-") && other == (rows, columns) =>
                Ok((a.clone(), Binary::Elements(rows * columns))),
        (Some((rows, inner)), Some((other, columns))) if op == "*" && inner == other =>
            Ok((Ty::matrix(rows, columns), Binary::Product { rows, inner, columns })),
        (Some((rows, columns)), None) => {
            let cast = b_literal && b.components.len() == 1 && b.is_numeric();
            let scalar = b == &Ty::scalar(Scalar::F32) || cast;
            if scalar && matches!(op, "*" | "/") {
                return Ok((a.clone(), Binary::MatrixScalar { count: rows * columns, cast }));
            }
            if op == "*" && b == &Ty::vector(Scalar::F32, columns) {
                return Ok((Ty::vector(Scalar::F32, rows), Binary::MatrixVector { rows, columns }));
            }
            source.fail(span, "B237", "矩阵运算需要相容矩阵、f32 标量或匹配列数的 float 向量")
        }
        (None, Some((rows, columns))) => {
            let cast = a_literal && a.components.len() == 1 && a.is_numeric();
            let scalar = a == &Ty::scalar(Scalar::F32) || cast;
            if op == "*" && scalar {
                return Ok((b.clone(), Binary::ScalarMatrix { count: rows * columns, cast }));
            }
            if op == "*" && a == &Ty::vector(Scalar::F32, rows) {
                return Ok((Ty::vector(Scalar::F32, columns), Binary::VectorMatrix { rows, columns }));
            }
            source.fail(span, "B237", "左侧需要 f32 标量或匹配行数的 float 向量，且只支持乘法")
        }
        _ => source.fail(span, "B237", "矩阵维度或运算不相容；乘法使用行列内积"),
    }
}


impl Builder<'_> {
    fn matrix_component(value: &Value, at: usize) -> Value {
        Value {
            proof: value.projected_proof(&[at]),
            literal: false,
            ty: Ty::scalar(Scalar::F32),
            regs: vec![value.regs[at]],
            uniform: value.uniform,
            constant: value.constant.as_ref().map(|v| vec![v[at]]),
        }
    }
    fn matrix_result(ty: Ty, values: Vec<Value>) -> Value {
        let uniform = values.iter().all(|v| v.uniform);
        let mut proof = bounds::Proof::default();
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
        Value {
            proof,
            literal: false,
            ty,
            regs,
            uniform,
            constant,
        }
    }
    pub(super) fn matrix_construct(
        &mut self,
        ty: Ty,
        mut values: Vec<Value>,
        span: Span,
    ) -> Result<Value> {
        let (rows, columns) = ty.matrix_shape().unwrap();
        if values.len() == 1 && values[0].ty == ty {
            let mut value = values.pop().unwrap();
            value.literal = false;
            return Ok(value);
        }
        for value in &values {
            if !value.ty.fields.is_empty() || value.ty.contains_views() || value.ty.array.is_some()
            {
                return self.fail(span, "B237", "矩阵构造接收数值分量或相同类型矩阵");
            }
        }
        if values.len() == 1 && values[0].regs.len() == 1 {
            let diagonal = self.cast(values.pop().unwrap(), Scalar::F32);
            let zero = self.constant(Scalar::F32, 0);
            return Ok(Self::matrix_result(
                ty,
                (0..rows)
                    .flat_map(|r| {
                        let diagonal = &diagonal;
                        let zero = &zero;
                        (0..columns).map(move |c| {
                            if r == c {
                                diagonal.clone()
                            } else {
                                zero.clone()
                            }
                        })
                    })
                    .collect(),
            ));
        }
        let mut components = Vec::new();
        for value in values {
            let value = self.cast(value, Scalar::F32);
            components.extend((0..value.regs.len()).map(|at| Self::matrix_component(&value, at)));
        }
        if components.len() != rows * columns {
            return self.fail(span, "B157", "矩阵构造需要全部行优先分量，或一个对角标量");
        }
        Ok(Self::matrix_result(ty, components))
    }
    fn matrix_dot(&mut self, a: &[Value], b: &[Value], span: Span) -> Result<Value> {
        let mut result = self.binary("*", a[0].clone(), b[0].clone(), span)?;
        for (a, b) in a.iter().zip(b).skip(1) {
            let product = self.binary("*", a.clone(), b.clone(), span)?;
            result = self.binary("+", result, product, span)?;
        }
        Ok(result)
    }
    pub(super) fn matrix_binary(
        &mut self, op: &str, a: Value, b: Value, kind: Binary, span: Span,
    ) -> Result<Value> {
        match kind {
            Binary::Elements(count) => {
                let mut values = Vec::new();
                for i in 0..count {
                    values.push(self.binary(op, Self::matrix_component(&a, i),
                        Self::matrix_component(&b, i), span)?);
                }
                Ok(Self::matrix_result(a.ty, values))
            }
            Binary::Product { rows, inner, columns } => {
                let mut values = Vec::new();
                for r in 0..rows {
                    for c in 0..columns {
                        let lhs: Vec<_> = (0..inner)
                            .map(|k| Self::matrix_component(&a, r * inner + k)).collect();
                        let rhs: Vec<_> = (0..inner)
                            .map(|k| Self::matrix_component(&b, k * columns + c)).collect();
                        values.push(self.matrix_dot(&lhs, &rhs, span)?);
                    }
                }
                Ok(Self::matrix_result(Ty::matrix(rows, columns), values))
            }
            Binary::MatrixScalar { count, cast } => {
                let b = if cast { self.cast(b, Scalar::F32) } else { b };
                let mut values = Vec::new();
                for i in 0..count {
                    values.push(self.binary(op, Self::matrix_component(&a, i), b.clone(), span)?);
                }
                Ok(Self::matrix_result(a.ty, values))
            }
            Binary::ScalarMatrix { count, cast } => {
                let a = if cast { self.cast(a, Scalar::F32) } else { a };
                let mut values = Vec::new();
                for i in 0..count {
                    values.push(self.binary(op, a.clone(), Self::matrix_component(&b, i), span)?);
                }
                Ok(Self::matrix_result(b.ty, values))
            }
            Binary::MatrixVector { rows, columns } => {
                let rhs: Vec<_> = (0..columns).map(|i| Self::matrix_component(&b, i)).collect();
                let mut values = Vec::new();
                for r in 0..rows {
                    let lhs: Vec<_> = (0..columns)
                        .map(|c| Self::matrix_component(&a, r * columns + c)).collect();
                    values.push(self.matrix_dot(&lhs, &rhs, span)?);
                }
                Ok(Self::matrix_result(Ty::vector(Scalar::F32, rows), values))
            }
            Binary::VectorMatrix { rows, columns } => {
                let lhs: Vec<_> = (0..rows).map(|i| Self::matrix_component(&a, i)).collect();
                let mut values = Vec::new();
                for c in 0..columns {
                    let rhs: Vec<_> = (0..rows)
                        .map(|r| Self::matrix_component(&b, r * columns + c)).collect();
                    values.push(self.matrix_dot(&lhs, &rhs, span)?);
                }
                Ok(Self::matrix_result(Ty::vector(Scalar::F32, columns), values))
            }
        }
    }
    fn matrix_determinant(&mut self, values: &[Value], size: usize, span: Span) -> Result<Value> {
        if size == 1 {
            return Ok(values[0].clone());
        }
        let mut terms = Vec::new();
        for c in 0..size {
            let minor: Vec<_> = (1..size)
                .flat_map(|r| {
                    (0..size)
                        .filter(move |k| *k != c)
                        .map(move |k| values[r * size + k].clone())
                })
                .collect();
            let determinant = self.matrix_determinant(&minor, size - 1, span)?;
            terms.push(self.binary("*", values[c].clone(), determinant, span)?);
        }
        let mut result = terms[0].clone();
        for (c, term) in terms.into_iter().enumerate().skip(1) {
            result = self.binary(if c % 2 == 0 { "+" } else { "-" }, result, term, span)?;
        }
        Ok(result)
    }
    pub(super) fn matrix_intrinsic(
        &mut self,
        name: &str,
        mut args: Vec<Value>,
        span: Span,
    ) -> Result<Value> {
        if args.len() != 1 {
            return self.fail(span, "B153", "矩阵内建需要一个矩阵参数");
        }
        let value = args.pop().unwrap();
        let Some((rows, columns)) = value.ty.matrix_shape() else {
            return self.fail(span, "B237", "矩阵内建需要 floatRxC 矩阵");
        };
        let components: Vec<_> = (0..rows * columns)
            .map(|at| Self::matrix_component(&value, at))
            .collect();
        if name == "transpose" {
            return Ok(Self::matrix_result(
                Ty::matrix(columns, rows),
                (0..columns)
                    .flat_map(|c| {
                        let components = &components;
                        (0..rows).map(move |r| components[r * columns + c].clone())
                    })
                    .collect(),
            ));
        }
        if rows != columns {
            return self.fail(span, "B237", "行列式和逆矩阵需要方阵");
        }
        if name == "determinant" {
            return self.matrix_determinant(&components, rows, span);
        }
        let mut cofactors = Vec::new();
        for r in 0..rows {
            for c in 0..columns {
                let minor: Vec<_> = (0..rows)
                    .filter(|i| *i != r)
                    .flat_map(|i| {
                        let components = &components;
                        (0..columns)
                            .filter(move |j| *j != c)
                            .map(move |j| components[i * columns + j].clone())
                    })
                    .collect();
                let mut cofactor = self.matrix_determinant(&minor, rows - 1, span)?;
                if (r + c) % 2 != 0 {
                    if let Some(bits) = &cofactor.constant {
                        cofactor = self.constant(Scalar::F32, bits[0] ^ 0x80000000);
                    } else {
                        let dst = self.reg(Scalar::F32);
                        self.emit(InstKind::Unary {
                            dst,
                            op: UnaryOp::FloatNegate,
                            a: cofactor.regs[0],
                        });
                        cofactor.regs = vec![dst];
                        cofactor.proof = bounds::Proof::default();
                    }
                }
                cofactors.push(cofactor);
            }
        }
        let determinant = self.matrix_dot(&components[..columns], &cofactors[..columns], span)?;
        let mut inverse = Vec::new();
        for r in 0..rows {
            for c in 0..columns {
                inverse.push(self.binary(
                    "/",
                    cofactors[c * columns + r].clone(),
                    determinant.clone(),
                    span,
                )?);
            }
        }
        Ok(Self::matrix_result(value.ty, inverse))
    }
}
