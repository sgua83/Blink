//! 固定维度矩阵按行布局；算术静态降低到既有标量值 IR。
use super::*;

#[repr(u32)]
enum MatrixAction { Copy, Add, Subtract, Multiply, Divide, Negate }

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
    emit: unsafe extern "C" fn(*mut MatrixValues<'a, 'w>, MatrixStep),
    output: unsafe extern "C" fn(*mut MatrixValues<'a, 'w>, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.operator_types::expand_square_matrix"]
    fn expand_square_matrix(width: u32, inverse: u32, emitter: MatrixEmitter<'_, '_>);
    #[link_name = "bootstrap.operator_types::expand_matrix_binary"]
    fn expand_matrix_binary(plan: operators::Plan, left_count: u32, input_count: u32, emitter: MatrixEmitter<'_, '_>);
}

// The slots hold the actual typed values. Only the B expansion assigns a
// temporary position, moves its last use, or publishes a result component.
struct MatrixValues<'a, 'w> {
    builder: &'a mut Builder<'w>,
    slots: Vec<Option<Value>>,
    output: Vec<Value>,
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

    fn emit(&mut self, step: MatrixStep) -> Value {
        let left = self.operand(step.left);
        let (token, operation) = match step.action {
            MatrixAction::Copy => return left,
            MatrixAction::Negate => {
                let mut value = left;
                if let Some(bits) = &value.constant {
                    return self.builder.constant(Scalar::F32, bits[0] ^ 0x80000000);
                }
                let dst = self.builder.reg(Scalar::F32);
                self.builder.emit(InstKind::Unary {
                    dst,
                    op: UnaryOp::FloatNegate,
                    a: value.regs[0],
                });
                value.regs[0] = dst;
                value.proof = bounds::Proof::default();
                return value;
            }
            MatrixAction::Multiply => ("*", BinaryOp::FMul),
            MatrixAction::Add => ("+", BinaryOp::FAdd),
            MatrixAction::Subtract => ("-", BinaryOp::FSub),
            MatrixAction::Divide => ("/", BinaryOp::FDiv),
        };
        let right = self.operand(step.right);
        self.builder.lower_numeric_binary(token, Ty::scalar(Scalar::F32), left, right,
            Scalar::F32, false, Some(operation), false, None, None)
    }
}

unsafe extern "C" fn emit_matrix(context: *mut MatrixValues<'_, '_>, step: MatrixStep) {
    let values = unsafe { &mut *context };
    let destination = step.destination as usize;
    let value = values.emit(step);
    if destination >= values.slots.len() {
        values.slots.resize_with(destination + 1, || None);
    }
    values.slots[destination] = Some(value);
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

    pub(super) fn matrix_binary(&mut self, ty: Ty, plan: operators::Plan, a: Value, b: Value) -> Value {
        let a = if plan.cast_left != 0 { self.cast(a, Scalar::F32) } else { a };
        let b = if plan.cast_right != 0 { self.cast(b, Scalar::F32) } else { b };
        let left_count = a.regs.len() as u32;
        let input_count = left_count + b.regs.len() as u32;
        let mut slots = Vec::with_capacity(input_count as usize);
        for value in [a, b] {
            if value.regs.len() == 1 {
                slots.push(Some(value));
            } else {
                slots.extend((0..value.regs.len()).map(|index| Some(Self::matrix_component(&value, index))));
            }
        }
        let mut values = MatrixValues {
            builder: self,
            slots,
            output: Vec::with_capacity(ty.components.len()),
        };
        let emitter = MatrixEmitter { context: &mut values, emit: emit_matrix, output: output_matrix };
        unsafe { expand_matrix_binary(plan, left_count, input_count, emitter); }
        Self::matrix_result(ty, values.output)
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
        };
        let emitter = MatrixEmitter {
            context: &mut values,
            emit: emit_matrix,
            output: output_matrix,
        };
        unsafe { expand_square_matrix(rows as u32, u32::from(inverse), emitter); }
        if inverse {
            Ok(Self::matrix_result(value.ty, values.output))
        } else {
            Ok(values.output.pop().unwrap())
        }
    }
}
