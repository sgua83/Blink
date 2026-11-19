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

#[repr(u32)]
enum MatrixAction { Copy, Multiply, Add, Subtract, Divide, Negate }

#[derive(Clone, Copy)]
#[repr(C)]
struct MatrixOperand {
    slot: u32,
    take: u32,
}

#[repr(C)]
struct MatrixStep {
    action: MatrixAction,
    left: MatrixOperand,
    right: MatrixOperand,
    destination: u32,
}

#[repr(C)]
struct MatrixEmitter<'a, 'w> {
    context: *mut MatrixValues<'a, 'w>,
    emit: unsafe extern "C" fn(*mut MatrixValues<'a, 'w>, MatrixStep) -> u32,
    output: unsafe extern "C" fn(*mut MatrixValues<'a, 'w>, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.operator_types::expand_square_matrix"]
    fn expand_square_matrix(width: u32, inverse: u32, emitter: MatrixEmitter<'_, '_>) -> u32;
}

// The slots hold the actual typed values. Only the B expansion assigns a
// temporary position, moves its last use, or publishes a result component.
struct MatrixValues<'a, 'w> {
    builder: &'a mut Builder<'w>,
    slots: Vec<Option<Value>>,
    output: Vec<Value>,
    failure: Option<Diagnostic>,
    span: Span,
}

impl MatrixValues<'_, '_> {
    fn operand(&mut self, operand: MatrixOperand) -> Value {
        let slot = &mut self.slots[operand.slot as usize];
        if operand.take != 0 {
            slot.take().unwrap()
        } else {
            slot.as_ref().unwrap().clone()
        }
    }

    fn emit(&mut self, step: MatrixStep) -> Result<Value> {
        let left = self.operand(step.left);
        let operation = match step.action {
            MatrixAction::Copy => return Ok(left),
            MatrixAction::Negate => {
                let mut value = left;
                if let Some(bits) = &value.constant {
                    return Ok(self.builder.constant(Scalar::F32, bits[0] ^ 0x80000000));
                }
                let dst = self.builder.reg(Scalar::F32);
                self.builder.emit(InstKind::Unary {
                    dst,
                    op: UnaryOp::FloatNegate,
                    a: value.regs[0],
                });
                value.regs[0] = dst;
                value.proof = bounds::Proof::default();
                return Ok(value);
            }
            MatrixAction::Multiply => "*",
            MatrixAction::Add => "+",
            MatrixAction::Subtract => "-",
            MatrixAction::Divide => "/",
        };
        let right = self.operand(step.right);
        self.builder.binary(operation, left, right, self.span, None)
    }
}

unsafe extern "C" fn emit_matrix(context: *mut MatrixValues<'_, '_>, step: MatrixStep) -> u32 {
    let values = unsafe { &mut *context };
    let destination = step.destination as usize;
    match values.emit(step) {
        Ok(value) => {
            if destination >= values.slots.len() {
                values.slots.resize_with(destination + 1, || None);
            }
            values.slots[destination] = Some(value);
            1
        }
        Err(diagnostic) => {
            values.failure = Some(diagnostic);
            0
        }
    }
}

unsafe extern "C" fn output_matrix(context: *mut MatrixValues<'_, '_>, slot: u32) {
    let values = unsafe { &mut *context };
    values.output.push(values.slots[slot as usize].take().unwrap());
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
        let mut result = self.binary("*", a[0].clone(), b[0].clone(), span,None)?;
        for (a, b) in a.iter().zip(b).skip(1) {
            let product = self.binary("*", a.clone(), b.clone(), span,None)?;
            result = self.binary("+", result, product, span,None)?;
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
                        Self::matrix_component(&b, i), span,None)?);
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
                    values.push(self.binary(op, Self::matrix_component(&a, i), b.clone(), span,None)?);
                }
                Ok(Self::matrix_result(a.ty, values))
            }
            Binary::ScalarMatrix { count, cast } => {
                let a = if cast { self.cast(a, Scalar::F32) } else { a };
                let mut values = Vec::new();
                for i in 0..count {
                    values.push(self.binary(op, a.clone(), Self::matrix_component(&b, i), span,None)?);
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
        let components = (0..rows * columns).map(|at| Self::matrix_component(&value, at));
        if name == "transpose" {
            let components: Vec<_> = components.collect();
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

        let inverse = name != "determinant";
        let mut values = MatrixValues {
            builder: self,
            slots: components.map(Some).collect(),
            output: Vec::with_capacity(if inverse { rows * columns } else { 1 }),
            failure: None,
            span,
        };
        let emitter = MatrixEmitter {
            context: &mut values,
            emit: emit_matrix,
            output: output_matrix,
        };
        if unsafe { expand_square_matrix(rows as u32, u32::from(inverse), emitter) } == 0 {
            return Err(values.failure.unwrap());
        }
        if inverse {
            Ok(Self::matrix_result(value.ty, values.output))
        } else {
            Ok(values.output.pop().unwrap())
        }
    }
}
