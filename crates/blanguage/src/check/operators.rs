//! 运算规则由B统一决定；这里借原Ty、形成原结果与诊断。
use super::*;
use std::ffi::c_void;

pub(super) use crate::codec::types::SourceMath as MathSignature;
#[repr(C)]
struct MathArgument { scalar:u32, flags:u32 }
#[repr(C)]
struct MathShape { status:u32, scalar:u32, width:usize }
#[repr(C)]
struct MathQueries {
    context:*const c_void,
    numeric:unsafe extern "C" fn(*const c_void,usize)->u32,
    argument:unsafe extern "C" fn(*const c_void,usize)->MathArgument,
    width:unsafe extern "C" fn(*const c_void,usize)->usize,
}
struct MathContext<'a> { values:&'a [Value] }
unsafe extern "C" fn math_numeric(context:*const c_void,index:usize)->u32 {
    let context=unsafe { &*context.cast::<MathContext<'_>>() };
    u32::from(context.values[index].ty.is_numeric())
}
unsafe extern "C" fn math_argument(context:*const c_void,index:usize)->MathArgument {
    let value=&unsafe { &*context.cast::<MathContext<'_>>() }.values[index];
    MathArgument { scalar:value.ty.components[0].0 as u32,
        flags:u32::from(!value.ty.fields.is_empty()) | (u32::from(value.ty.array.is_some())<<1) }
}
unsafe extern "C" fn math_width(context:*const c_void,index:usize)->usize {
    unsafe { &*context.cast::<MathContext<'_>>() }.values[index].regs.len()
}
impl MathContext<'_> {
    fn queries(&self)->MathQueries {
        MathQueries { context:std::ptr::from_ref(self).cast(),numeric:math_numeric,argument:math_argument,width:math_width }
    }
}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::integer_math"]
    fn source_integer_math(extremum:u32,count:usize,queries:MathQueries)->u32;
    #[link_name="bootstrap.operator_types::floating_math"]
    fn source_floating_math(count:usize,queries:MathQueries)->MathShape;
}
pub(super) fn integer_math(extremum:u32,values:&[Value])->u32 {
    let context=MathContext { values };
    unsafe { source_integer_math(extremum,values.len(),context.queries()) }
}
pub(super) fn floating_math(values:&[Value])->Option<(Scalar,usize)> {
    let context=MathContext { values };
    let shape=unsafe { source_floating_math(values.len(),context.queries()) };
    (shape.status==0).then(||(SCALARS[shape.scalar as usize],shape.width))
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Operand { components: u64, rows: u64, columns: u64, scalar: u32, flags: u32 }
#[repr(u32)]
#[derive(Clone, Copy)]
enum Status {
    Valid=0, Mismatch=124, NotNumeric=146, MixedNumeric=148, VectorShape=149,
    BooleanOperator=150, IntegerOperator=151, UnaryOperator=177, BinaryOperator=178,
    EnumOperator=268, VariantOperator=269,
    CFunctionOperator=1471, AddressOperator=1472, HandleOperator=1473, AggregateOperator=1474,
    MatrixRight=2371, MatrixLeft=2372, MatrixShape=2373,
}
#[repr(u32)]
#[derive(Clone, Copy)]
pub(super) enum Kind { Numeric, Identity, Handle, Elements, Product, MatrixScalar, ScalarMatrix, MatrixVector, VectorMatrix }
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Plan {
    status: Status, pub(super) kind: Kind, scalar: u32,
    pub(super) cast_left: u32, pub(super) cast_right: u32, comparison: u32, operation: u32,
    rows: u64, inner: u64, columns: u64, count: u64,
}
#[repr(C)]
struct Queries {
    context: *const c_void,
    same: unsafe extern "C" fn(*const c_void)->u32,
    builtin: unsafe extern "C" fn(*const c_void,u32,u32,u64)->u32,
    numeric: unsafe extern "C" fn(*const c_void,u32)->u32,
    views: unsafe extern "C" fn(*const c_void,u32)->u32,
}
struct Context<'a> { types: [&'a Ty;2] }
const SCALARS: [Scalar;11] = [
    Scalar::F32,Scalar::U32,Scalar::I32,Scalar::Bool,Scalar::U64,Scalar::I64,
    Scalar::F64,Scalar::U8,Scalar::I8,Scalar::U16,Scalar::I16,
];
impl Operand {
    fn of(ty:&Ty)->Self {
        let matrix=ty.matrix_shape();
        Self {
            components:ty.components.len() as u64, rows:matrix.map_or(0,|shape|shape.0 as u64),
            columns:matrix.map_or(0,|shape|shape.1 as u64),
            scalar:ty.components.first().map_or(u32::MAX,|value|value.0 as u32),
            flags:u32::from(ty.is_c_function()) | (u32::from(ty.is_address())<<1)
                | (u32::from(ty.is_handle())<<2) | (u32::from(ty.has_variants())<<3)
                | (u32::from(ty.enumeration)<<4) | (u32::from(ty.array.is_some())<<5)
                | (u32::from(!ty.fields.is_empty())<<6) | (u32::from(matrix.is_some())<<7),
        }
    }
}
unsafe extern "C" fn same_type(context:*const c_void)->u32 {
    let context=unsafe { &*context.cast::<Context<'_>>() };
    u32::from(context.types[0]==context.types[1])
}
unsafe extern "C" fn builtin_type(context:*const c_void,side:u32,scalar:u32,count:u64)->u32 {
    let context=unsafe { &*context.cast::<Context<'_>>() };
    u32::from(context.types[side as usize]==&Ty::vector(SCALARS[scalar as usize],count as usize))
}
unsafe extern "C" fn numeric_type(context:*const c_void,side:u32)->u32 {
    let context=unsafe { &*context.cast::<Context<'_>>() };
    u32::from(context.types[side as usize].is_numeric())
}
unsafe extern "C" fn view_type(context:*const c_void,side:u32)->u32 {
    let context=unsafe { &*context.cast::<Context<'_>>() };
    u32::from(context.types[side as usize].contains_views())
}
impl Context<'_> {
    fn queries(&self)->Queries {
        Queries {context:std::ptr::from_ref(self).cast(),same:same_type,builtin:builtin_type,numeric:numeric_type,views:view_type}
    }
}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::numeric"]
    fn source_numeric(a:Operand,a_literal:u32,b:Operand,b_literal:u32)->Plan;
    #[link_name="bootstrap.operator_types::binary"]
    fn source_binary(op:*const u8,length:u64,a:Operand,a_literal:u32,b:Operand,b_literal:u32,queries:Queries)->Plan;
    #[link_name="bootstrap.operator_types::unary"]
    fn source_unary(op:*const u8,length:u64,value:Operand,queries:Queries)->Plan;
}
fn finish<T>(plan:Plan,source:&Source,span:Span,op:&str,mismatch:impl FnOnce()->String,apply:impl FnOnce(Plan)->T)->Result<T> {
    let (code,message)=match plan.status {
        Status::Valid=>return Ok(apply(plan)),
        Status::Mismatch=>return source.fail(span,"B124",mismatch()),
        Status::NotNumeric=>("B146","数值一元运算需要数值类型"),
        Status::MixedNumeric=>("B148","不同数值类型必须显式转换"),
        Status::VectorShape=>("B149","向量维度不匹配"),
        Status::BooleanOperator=>("B150","bool 只支持逻辑或相等运算"),
        Status::IntegerOperator=>("B151","位运算与取余需要整数"),
        Status::UnaryOperator=>("B177","当前类型不支持该一元运算"),
        Status::BinaryOperator=>return source.fail(span,"B178",format!("未知二元运算 {op}")),
        Status::EnumOperator=>("B268","编号枚举只直接比较同类型值；数值运算先显式取得声明的整数表示"),
        Status::VariantOperator=>("B269","结果值的运算先通过 match 或 try 取得对应载荷"),
        Status::CFunctionOperator=>("B147","C函数地址只比较同签名的完整身份，不参与数值运算或次序比较"),
        Status::AddressOperator=>("B147","address只比较完整地址身份；数值运算须显式取得address_bits"),
        Status::HandleOperator=>("B147","不透明身份只支持完整身份的相等或不等比较"),
        Status::AggregateOperator=>("B147","结构体和资源视图不能作为数值进行运算"),
        Status::MatrixRight=>("B237","矩阵运算需要相容矩阵、f32 标量或匹配列数的 float 向量"),
        Status::MatrixLeft=>("B237","左侧需要 f32 标量或匹配行数的 float 向量，且只支持乘法"),
        Status::MatrixShape=>("B237","矩阵维度或运算不相容；乘法使用行列内积"),
    };
    source.fail(span,code,message)
}
pub(super) fn same(a:&Ty,b:&Ty,source:&Source,span:Span)->Result<()> {
    if a==b {Ok(())} else {source.fail(span,"B124",format!("类型不匹配：需要 {}，实际为 {}",a.name,b.name))}
}
pub(super) struct Numeric {pub scalar:Scalar,pub cast_left:bool,pub cast_right:bool}
fn numeric_result(plan:Plan)->Numeric {
    Numeric {scalar:SCALARS[plan.scalar as usize],cast_left:plan.cast_left!=0,cast_right:plan.cast_right!=0}
}
pub(super) fn numeric(a:&Ty,a_literal:bool,b:&Ty,b_literal:bool,source:&Source,span:Span)->Result<Numeric> {
    let plan=unsafe {source_numeric(Operand::of(a),u32::from(a_literal),Operand::of(b),u32::from(b_literal))};
    finish(plan,source,span,"",||format!("类型不匹配：需要 {}，实际为 {}",a.name,b.name),numeric_result)
}
pub(super) enum Binary {
    Identity,Handle,Matrix(Plan),Numeric {operands:Numeric,comparison:bool,operation:Option<BinaryOp>},
}
pub(super) fn binary(op:&str,a:&Ty,a_literal:bool,b:&Ty,b_literal:bool,source:&Source,span:Span)->Result<(Ty,Binary)> {
    let context=Context {types:[a,b]};
    let plan=unsafe {source_binary(op.as_ptr(),op.len() as u64,Operand::of(a),u32::from(a_literal),Operand::of(b),u32::from(b_literal),context.queries())};
    finish(plan,source,span,op,||format!("类型不匹配：需要 {}，实际为 {}",a.name,b.name),|plan| {
        let count=plan.count as usize;let rows=plan.rows as usize;let columns=plan.columns as usize;
        match plan.kind {
            Kind::Identity=>(Ty::scalar(Scalar::Bool),Binary::Identity),
            Kind::Handle=>(Ty::scalar(Scalar::Bool),Binary::Handle),
            Kind::Elements|Kind::MatrixScalar=>(a.clone(),Binary::Matrix(plan)),
            Kind::ScalarMatrix=>(b.clone(),Binary::Matrix(plan)),
            Kind::Product=>(Ty::matrix(rows,columns),Binary::Matrix(plan)),
            Kind::MatrixVector=>(Ty::vector(Scalar::F32,rows),Binary::Matrix(plan)),
            Kind::VectorMatrix=>(Ty::vector(Scalar::F32,columns),Binary::Matrix(plan)),
            Kind::Numeric=>{
                use BinaryOp::*;use Comparison as C;
                let operations=[None,Some(FAdd),Some(FSub),Some(FMul),Some(FDiv),Some(IAdd),Some(ISub),Some(IMul),
                    Some(Compare(C::Equal)),Some(Compare(C::NotEqual)),Some(Compare(C::Less)),Some(Compare(C::Greater)),
                    Some(Compare(C::LessEqual)),Some(Compare(C::GreaterEqual)),Some(And),Some(Or),Some(Xor),
                    Some(ShiftLeft),Some(ShiftRightLogical),Some(ShiftRightArithmetic)];
                let operands=numeric_result(plan);let comparison=plan.comparison!=0;
                (Ty::vector(if comparison {Scalar::Bool} else {operands.scalar},count),
                    Binary::Numeric {operands,comparison,operation:operations[plan.operation as usize]})
            },
        }
    })
}
pub(super) fn unary(op:&str,ty:&Ty,source:&Source,span:Span)->Result<Option<UnaryOp>> {
    let context=Context {types:[ty,ty]};
    let plan=unsafe {source_unary(op.as_ptr(),op.len() as u64,Operand::of(ty),context.queries())};
    finish(plan,source,span,op,||format!("类型不匹配：需要 bool，实际为 {}",ty.name),|plan| {
        [None,Some(UnaryOp::FloatNegate),Some(UnaryOp::SignedNegate),Some(UnaryOp::UnsignedNegate),
            Some(UnaryOp::LogicalNot),Some(UnaryOp::BitNot)][plan.operation as usize]
    })
}



#[repr(u32)]
#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) enum GeometryBuiltin { Unknown,Dot,Length,Normalize,Cross }
#[repr(u32)]
enum GeometryStatus { Valid,Arity,Same,DotType,LengthType,CrossType }
#[repr(C)]
struct GeometryPlan { status:GeometryStatus, side:u32, width:usize, result_width:usize }
#[repr(C)]
struct GeometryQueries {
    values:MathQueries,
    same:unsafe extern "C" fn(*const c_void)->u32,
    float3:unsafe extern "C" fn(*const c_void,usize)->u32,
}
unsafe extern "C" fn geometry_same(context:*const c_void)->u32 {
    let values=unsafe { &*context.cast::<MathContext<'_>>() }.values;
    u32::from(values[0].ty==values[1].ty)
}
unsafe extern "C" fn geometry_float3(context:*const c_void,side:usize)->u32 {
    let values=unsafe { &*context.cast::<MathContext<'_>>() }.values;
    u32::from(values[side].ty==Ty::vector(Scalar::F32,3))
}
#[repr(u32)]
enum GeometryAction { Copy,Add,Subtract,Multiply,Divide,Sqrt }
#[repr(C)]
#[derive(Clone,Copy)]
struct GeometryOperand { slot:u32, component:usize, projected:u32, take:u32 }
#[repr(C)]
struct GeometryStep { action:GeometryAction, left:GeometryOperand, right:GeometryOperand, destination:u32, width:usize }
#[repr(C)]
struct GeometryEmitter<'a,'w> {
    context:*mut GeometryValues<'a,'w>,
    emit:unsafe extern "C" fn(*mut GeometryValues<'a,'w>,GeometryStep),
    output:unsafe extern "C" fn(*mut GeometryValues<'a,'w>,u32,u32),
}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::source_geometry"]
    fn source_geometry(bytes:*const u8,length:usize)->GeometryBuiltin;
    #[link_name="bootstrap.operator_types::geometry_plan"]
    fn source_geometry_plan(builtin:GeometryBuiltin,count:usize,queries:GeometryQueries)->GeometryPlan;
    #[link_name="bootstrap.operator_types::expand_geometry"]
    fn expand_geometry(builtin:GeometryBuiltin,width:usize,emitter:GeometryEmitter<'_,'_>);
}
impl GeometryBuiltin {
    pub(super) fn recognize(name:&str)->Self { unsafe { source_geometry(name.as_ptr(),name.len()) } }
}
// These five positions hold only the actual input, product and accumulator values.
// B decides every read, final move and emitted component during this synchronous call.
struct GeometryValues<'a,'w> {
    builder:&'a mut Builder<'w>, slots:[Option<Value>;5], output:Option<Value>,
}
impl GeometryValues<'_,'_> {
    fn binary(&mut self,token:&str,operation:BinaryOp,left:Value,right:GeometryOperand,width:usize)->Value {
        let right=self.operand(right);
        self.builder.lower_numeric_binary(token,Ty::vector(Scalar::F32,width),left,right,
            Scalar::F32,false,Some(operation),false,None,None)
    }
    fn operand(&mut self,operand:GeometryOperand)->Value {
        let slot=&mut self.slots[operand.slot as usize];
        if operand.projected!=0 {
            let input=slot.as_ref().unwrap();
            let value=Value { proof:bounds::Proof::default(),literal:false,ty:Ty::scalar(Scalar::F32),
                regs:vec![input.regs[operand.component]],uniform:input.uniform,constant:None };
            if operand.take!=0 { *slot=None; }
            value
        } else if operand.take!=0 { slot.take().unwrap() }
        else { slot.as_ref().unwrap().clone() }
    }
}
unsafe extern "C" fn emit_geometry(context:*mut GeometryValues<'_,'_>,step:GeometryStep) {
    let values=unsafe { &mut *context };
    let left=values.operand(step.left);
    let value=match step.action {
        GeometryAction::Copy=>left,
        GeometryAction::Sqrt=>values.builder.lower_math(MathOp::Sqrt,vec![left],Scalar::F32,1),
        GeometryAction::Add=>values.binary("+",BinaryOp::FAdd,left,step.right,step.width),
        GeometryAction::Subtract=>values.binary("-",BinaryOp::FSub,left,step.right,step.width),
        GeometryAction::Multiply=>values.binary("*",BinaryOp::FMul,left,step.right,step.width),
        GeometryAction::Divide=>values.binary("/",BinaryOp::FDiv,left,step.right,step.width),
    };
    values.slots[step.destination as usize]=Some(value);
}
unsafe extern "C" fn output_geometry(context:*mut GeometryValues<'_,'_>,slot:u32,components:u32) {
    let values=unsafe { &mut *context };
    let value=values.slots[slot as usize].take().unwrap();
    if components!=0 { values.output.as_mut().unwrap().regs.push(value.regs[0]); }
    else { values.output=Some(value); }
}
impl Builder<'_> {
    pub(super) fn geometry_value(&mut self,builtin:GeometryBuiltin,values:Vec<Value>,span:Span)->Result<Value> {
        let context=MathContext { values:&values };
        let plan=unsafe { source_geometry_plan(builtin,values.len(),GeometryQueries {
            values:context.queries(),same:geometry_same,float3:geometry_float3,
        }) };
        match plan.status {
            GeometryStatus::Valid=>{},
            GeometryStatus::Arity=>return self.fail(span,"B153",match builtin {
                GeometryBuiltin::Dot=>"dot 需要两个向量",
                GeometryBuiltin::Cross=>"cross 需要两个 float3",
                _=>"length／normalize 需要一个向量",
            }),
            GeometryStatus::Same=>return self.fail(span,"B124",format!("类型不匹配：需要 {}，实际为 {}",values[0].ty.name,values[1].ty.name)),
            GeometryStatus::DotType=>return self.fail(span,"B154","dot 需要 float 向量"),
            GeometryStatus::LengthType=>return self.fail(span,"B154","需要 float 向量"),
            GeometryStatus::CrossType=>return self.fail(span,"B124",format!("类型不匹配：需要 {}，实际为 {}",Ty::vector(Scalar::F32,3).name,values[plan.side as usize].ty.name)),
        }
        let output=(builtin==GeometryBuiltin::Cross).then(||Value {
            proof:bounds::Proof::default(),literal:false,ty:Ty::vector(Scalar::F32,plan.result_width),
            regs:Vec::with_capacity(plan.result_width),uniform:values.iter().all(|v|v.uniform),constant:None,
        });
        let mut input=values.into_iter();
        let mut context=GeometryValues { builder:self,slots:[input.next(),input.next(),None,None,None],output };
        unsafe { expand_geometry(builtin,plan.width,GeometryEmitter {
            context:&mut context,emit:emit_geometry,output:output_geometry,
        }) };
        Ok(context.output.unwrap())
    }
}
