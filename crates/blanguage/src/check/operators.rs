//! 运算规则由B统一决定；这里借原Ty、形成原结果与诊断。
use super::*;
use std::ffi::c_void;

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
enum Kind { Numeric, Identity, Handle, Elements, Product, MatrixScalar, ScalarMatrix, MatrixVector, VectorMatrix }
#[repr(C)]
#[derive(Clone, Copy)]
struct Plan {
    status: Status, kind: Kind, scalar: u32, cast_left: u32, cast_right: u32, comparison: u32, operation: u32,
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
    Identity,Handle,Matrix(matrix::Binary),Numeric {operands:Numeric,comparison:bool,operation:Option<BinaryOp>},
}
pub(super) fn binary(op:&str,a:&Ty,a_literal:bool,b:&Ty,b_literal:bool,source:&Source,span:Span)->Result<(Ty,Binary)> {
    let context=Context {types:[a,b]};
    let plan=unsafe {source_binary(op.as_ptr(),op.len() as u64,Operand::of(a),u32::from(a_literal),Operand::of(b),u32::from(b_literal),context.queries())};
    finish(plan,source,span,op,||format!("类型不匹配：需要 {}，实际为 {}",a.name,b.name),|plan| {
        use matrix::Binary as Matrix;
        let count=plan.count as usize;let rows=plan.rows as usize;let inner=plan.inner as usize;let columns=plan.columns as usize;
        match plan.kind {
            Kind::Identity=>(Ty::scalar(Scalar::Bool),Binary::Identity),
            Kind::Handle=>(Ty::scalar(Scalar::Bool),Binary::Handle),
            Kind::Elements=>(a.clone(),Binary::Matrix(Matrix::Elements(count))),
            Kind::Product=>(Ty::matrix(rows,columns),Binary::Matrix(Matrix::Product {rows,inner,columns})),
            Kind::MatrixScalar=>(a.clone(),Binary::Matrix(Matrix::MatrixScalar {count,cast:plan.cast_right!=0})),
            Kind::ScalarMatrix=>(b.clone(),Binary::Matrix(Matrix::ScalarMatrix {count,cast:plan.cast_left!=0})),
            Kind::MatrixVector=>(Ty::vector(Scalar::F32,rows),Binary::Matrix(Matrix::MatrixVector {rows,columns})),
            Kind::VectorMatrix=>(Ty::vector(Scalar::F32,columns),Binary::Matrix(Matrix::VectorMatrix {rows,columns})),
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

