//! 固定维度矩阵按行布局；算术静态降低到既有标量值 IR。
use super::*;

#[repr(u32)]
#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) enum MatrixBuiltin { Unknown,Transpose,Determinant,Inverse }
#[repr(u32)]
#[derive(Clone,Copy)]
enum MatrixIntrinsicStatus { Valid,Arity,Matrix,Square }
#[repr(C)]
struct MatrixIntrinsicInput { present:u32, rows:u32, columns:u32 }
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::source_matrix_intrinsic"]
    fn source_matrix_intrinsic(bytes:*const u8,length:usize)->MatrixBuiltin;
}
impl MatrixBuiltin {
    pub(super) fn recognize(name:&str)->Self { unsafe { source_matrix_intrinsic(name.as_ptr(),name.len()) } }
}
unsafe extern "C" fn inspect_intrinsic_matrix(context:*const MatrixValues<'_, '_>)->MatrixIntrinsicInput {
    match unsafe { &*context }.inputs.get(0).ty.matrix_shape() {
        Some((rows,columns))=>MatrixIntrinsicInput { present:1,rows:rows as u32,columns:columns as u32 },
        None=>MatrixIntrinsicInput { present:0,rows:0,columns:0 },
    }
}
unsafe extern "C" fn output_transpose(context:*mut MatrixValues<'_, '_>,slot:u32) {
    let values=unsafe { &mut *context };
    let component=Builder::matrix_component(values.current.as_ref().unwrap(),slot as usize);
    Builder::append_value(values.result.as_mut().unwrap(),component,false);
}

#[repr(u32)]
enum MatrixAction { Copy, Add, Subtract, Multiply, Divide, Negate, Constant }
#[repr(C)]
struct MatrixConstant { present:u32, bits:u32 }

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
    bits: u32,
}

#[repr(C)]
struct MatrixEmitter<'a, 'w> {
    context: *mut MatrixValues<'a, 'w>,
    constant: unsafe extern "C" fn(*const MatrixValues<'a, 'w>,u32)->MatrixConstant,
    emit: unsafe extern "C" fn(*mut MatrixValues<'a, 'w>, MatrixStep),
    output: unsafe extern "C" fn(*mut MatrixValues<'a, 'w>, u32),
}

#[repr(u32)]
enum MatrixControl { Cast,Select,Whole,Part,Release,OriginalType,TransposeBegin,ResultBegin,ResultPart,Scalar,Width,Reserve }
#[repr(C)]
struct MatrixCommand { need:MatrixControl,first:u64,second:u64 }
#[repr(C)]
struct MatrixHost<'a,'w> {
    emitter:MatrixEmitter<'a,'w>,
    action:unsafe extern "C" fn(*mut MatrixValues<'a,'w>,MatrixCommand)->u64,
    inspect:unsafe extern "C" fn(*const MatrixValues<'a,'w>)->MatrixIntrinsicInput,
    transpose:unsafe extern "C" fn(*mut MatrixValues<'a,'w>,u32),
}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::matrix_binary_value"]
    fn matrix_binary_value(plan:operators::Plan,host:MatrixHost<'_,'_>);
    #[link_name="bootstrap.operator_types::matrix_intrinsic_value"]
    fn matrix_intrinsic_value(builtin:MatrixBuiltin,count:u64,host:MatrixHost<'_,'_>)->MatrixIntrinsicStatus;
}

enum MatrixInputs { Binary([Option<Value>;2]),Intrinsic(Vec<Value>) }
impl MatrixInputs {
    fn get(&self,index:usize)->&Value {
        match self {Self::Binary(values)=>values[index].as_ref().unwrap(),Self::Intrinsic(values)=>&values[index]}
    }
    fn take(&mut self,index:usize)->Value {
        match self {Self::Binary(values)=>values[index].take().unwrap(),Self::Intrinsic(values)=>values.pop().unwrap()}
    }
    fn put(&mut self,index:usize,value:Value) {
        match self {Self::Binary(values)=>values[index]=Some(value),Self::Intrinsic(values)=>values.push(value)}
    }
}

// The slots hold the actual typed values. Only the B expansion assigns a
// temporary position, moves its last use, or publishes a result component.
struct MatrixValues<'a, 'w> {
    builder: &'a mut Builder<'w>,
    inputs: MatrixInputs,
    current: Option<Value>,
    ty: Option<Ty>,
    slots: Vec<Option<Value>>,
    output: Vec<Value>,
    parts: Option<std::vec::IntoIter<Value>>,
    result: Option<Value>,
}

impl<'a,'w> MatrixValues<'a,'w> {
    fn new(builder:&'a mut Builder<'w>,inputs:MatrixInputs,ty:Option<Ty>)->Self {
        let output=Vec::with_capacity(ty.as_ref().map_or(0,|ty|ty.components.len()));
        Self {builder,inputs,current:None,ty,slots:Vec::new(),output,parts:None,result:None}
    }
    fn host(&mut self)->MatrixHost<'a,'w> {
        MatrixHost {emitter:MatrixEmitter {context:self,constant:matrix_constant,emit:emit_matrix,output:output_matrix},
            action:matrix_action,inspect:inspect_intrinsic_matrix,transpose:output_transpose}
    }
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
            MatrixAction::Constant => return self.builder.constant(Scalar::F32,u64::from(step.bits)),
            MatrixAction::Negate => {
                let mut value = left;
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
        self.builder.lower_numeric_binary(token, Ty::scalar(Scalar::F32), &left, &right,
            Scalar::F32, false, Some(operation), false, None, None)
    }
}

unsafe extern "C" fn matrix_constant(context:*const MatrixValues<'_,'_>,slot:u32)->MatrixConstant {
    match &unsafe { &*context }.slots[slot as usize].as_ref().unwrap().constant {
        Some(bits)=>MatrixConstant {present:1,bits:bits[0] as u32},
        None=>MatrixConstant {present:0,bits:0},
    }
}

unsafe extern "C" fn matrix_action(context:*mut MatrixValues<'_,'_>,command:MatrixCommand)->u64 {
    let values=unsafe { &mut *context };
    let first=command.first as usize;
    match command.need {
        MatrixControl::Cast=>{
            let value=values.inputs.take(first);
            let value=values.builder.cast(value,Scalar::F32);
            values.inputs.put(first,value);
        }
        MatrixControl::Select=>values.current=Some(values.inputs.take(first)),
        MatrixControl::Whole=>values.slots.push(values.current.take()),
        MatrixControl::Part=>values.slots.push(Some(Builder::matrix_component(values.current.as_ref().unwrap(),first))),
        MatrixControl::Release=>{values.current.take();}
        MatrixControl::OriginalType=>values.ty=Some(values.current.take().unwrap().ty),
        MatrixControl::TransposeBegin=>{
            let ty=Ty::matrix(first,command.second as usize);
            let regs=Vec::with_capacity(ty.components.len());
            values.result=Some(Value {proof:bounds::Proof::default(),literal:false,ty,regs,uniform:true,constant:Some(Vec::new())});
        }
        MatrixControl::ResultBegin=>{
            values.result=Some(Value {proof:bounds::Proof::default(),literal:false,ty:values.ty.take().unwrap(),
                regs:Vec::new(),uniform:true,constant:Some(Vec::new())});
            let output=std::mem::take(&mut values.output);
            let count=output.len();
            values.parts=Some(output.into_iter());
            return count as u64;
        }
        MatrixControl::ResultPart=>{
            let value=values.parts.as_mut().unwrap().next().unwrap();
            Builder::append_value(values.result.as_mut().unwrap(),value,false);
        }
        MatrixControl::Scalar=>values.result=values.output.pop(),
        MatrixControl::Width=>return values.inputs.get(first).regs.len() as u64,
        MatrixControl::Reserve=>{
            values.slots.reserve_exact(first);
            values.output.reserve_exact(command.second as usize);
        }
    }
    0
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
    pub(super) fn matrix_component(value: &Value, at: usize) -> Value {
        Value {
            proof: value.projected_proof(&[at]),
            literal: false,
            ty: Ty::scalar(Scalar::F32),
            regs: vec![value.regs[at]],
            uniform: value.uniform,
            constant: value.constant.as_ref().map(|v| vec![v[at]]),
        }
    }
    pub(super) fn append_value(result:&mut Value,value:Value,replace_proof:bool) {
        result.uniform &= value.uniform;
        if replace_proof { result.proof=value.proof; } else { result.proof.append(&value); }
        result.regs.extend(value.regs);
        result.constant=result.constant.take().zip(value.constant).map(|(mut a,b)|{a.extend(b);a});
    }
    pub(super) fn matrix_binary(&mut self, ty: Ty, plan: operators::Plan, a: Value, b: Value) -> Value {
        let mut values=MatrixValues::new(self,MatrixInputs::Binary([Some(a),Some(b)]),Some(ty));
        unsafe { matrix_binary_value(plan,values.host()); }
        values.result.take().unwrap()
    }

    pub(super) fn matrix_intrinsic(
        &mut self,
        builtin:MatrixBuiltin,
        args:Vec<Value>,
        span:Span,
    )->Result<Value> {
        let count=args.len() as u64;
        let mut values=MatrixValues::new(self,MatrixInputs::Intrinsic(args),None);
        match unsafe { matrix_intrinsic_value(builtin,count,values.host()) } {
            MatrixIntrinsicStatus::Valid=>{},
            MatrixIntrinsicStatus::Arity=>return values.builder.fail(span,"B153","矩阵内建需要一个矩阵参数"),
            MatrixIntrinsicStatus::Matrix=>return values.builder.fail(span,"B237","矩阵内建需要 floatRxC 矩阵"),
            MatrixIntrinsicStatus::Square=>return values.builder.fail(span,"B237","行列式和逆矩阵需要方阵"),
        }
        Ok(values.result.take().unwrap())
    }
}
