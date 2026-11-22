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
        self.builder.lower_numeric_binary(token,Ty::vector(Scalar::F32,width),&left,&right,
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


#[repr(u32)]
#[derive(Clone,Copy)]
pub(super) enum ConstructEntry { Value,Matrix }
#[repr(u32)]
enum ConstructFact { CFunction,Address,Handle,Variants,Enumeration,Array,Matrix,Views,Fields,Width }
#[repr(u32)]
#[derive(Clone,Copy)]
enum ConstructStatus { Valid,CFunctionTarget,CFunctionInput,AddressTarget,AddressInput,HandleTarget,HandleInput,Variants,Enumeration,Array,Matrix,Views,MatrixInput,Components,MatrixComponents }
#[repr(u32)]
#[derive(Clone,Copy,PartialEq,Eq)]
enum ConstructKind { Zero,Identity,Enumeration,Numeric,Diagonal,Matrix }
#[repr(C)]
#[derive(Clone,Copy)]
struct ConstructPlan { status:ConstructStatus,kind:ConstructKind,scalar:u32,splat:u32,width:usize,rows:usize,columns:usize }
#[repr(C)]
struct ConstructQueries {
    context:*const c_void,
    fact:unsafe extern "C" fn(*const c_void,usize,ConstructFact)->usize,
    same:unsafe extern "C" fn(*const c_void,usize)->u32,
    scalar_identity:unsafe extern "C" fn(*const c_void)->u32,
}
struct ConstructTypes<'a> { target:&'a Ty,values:&'a [Value] }
unsafe extern "C" fn construct_fact(context:*const c_void,index:usize,fact:ConstructFact)->usize {
    let value=&unsafe { &*context.cast::<ConstructTypes<'_>>() }.values[index];
    let ty=&value.ty;
    match fact {
        ConstructFact::CFunction=>usize::from(ty.is_c_function()),
        ConstructFact::Address=>usize::from(ty.is_address()),
        ConstructFact::Handle=>usize::from(ty.is_handle()),
        ConstructFact::Variants=>usize::from(ty.has_variants()),
        ConstructFact::Enumeration=>usize::from(ty.enumeration),
        ConstructFact::Array=>usize::from(ty.array.is_some()),
        ConstructFact::Matrix=>usize::from(ty.matrix_shape().is_some()),
        ConstructFact::Views=>usize::from(ty.contains_views()),
        ConstructFact::Fields=>usize::from(!ty.fields.is_empty()),
        ConstructFact::Width=>value.regs.len(),
    }
}
unsafe extern "C" fn construct_same(context:*const c_void,index:usize)->u32 {
    let context=unsafe { &*context.cast::<ConstructTypes<'_>>() };
    u32::from(context.target==&context.values[index].ty)
}
unsafe extern "C" fn construct_scalar_identity(context:*const c_void)->u32 {
    let context=unsafe { &*context.cast::<ConstructTypes<'_>>() };
    u32::from(context.target==&Ty::scalar(context.values[0].ty.components[0].0))
}
#[repr(u32)]
enum ConstructArgument { Append,ReplaceProof,Matrix,Diagonal }
#[repr(u32)]
enum ConstructComponent { Project,Diagonal,Zero }
#[repr(C)]
struct ConstructEmitter<'a,'w> {
    context:*mut ConstructValues<'a,'w>,
    argument:unsafe extern "C" fn(*mut ConstructValues<'a,'w>,u32,ConstructArgument)->usize,
    component:unsafe extern "C" fn(*mut ConstructValues<'a,'w>,ConstructComponent,usize,u32),
    splat:unsafe extern "C" fn(*mut ConstructValues<'a,'w>,usize),
}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::construct_plan"]
    fn source_construct_plan(entry:ConstructEntry,target:Operand,count:usize,queries:ConstructQueries)->ConstructPlan;
    #[link_name="bootstrap.operator_types::expand_construct"]
    fn expand_construct(plan:ConstructPlan,count:usize,emitter:ConstructEmitter<'_,'_>);
}
struct ConstructValues<'a,'w> {
    builder:&'a mut Builder<'w>,input:std::vec::IntoIter<Value>,current:Option<Value>,zero:Option<Value>,output:Value,
}
unsafe extern "C" fn construct_argument(context:*mut ConstructValues<'_,'_>,scalar:u32,mode:ConstructArgument)->usize {
    let values=unsafe { &mut *context };
    let input=values.input.next().unwrap();
    let value=values.builder.cast(input,SCALARS[scalar as usize]);
    let width=value.regs.len();
    match mode {
        ConstructArgument::Append=>Builder::append_value(&mut values.output,value,false),
        ConstructArgument::ReplaceProof=>Builder::append_value(&mut values.output,value,true),
        ConstructArgument::Matrix=>values.current=Some(value),
        ConstructArgument::Diagonal=>{
            values.current=Some(value);values.zero=Some(values.builder.constant(Scalar::F32,0));
        },
    }
    width
}
unsafe extern "C" fn construct_component(context:*mut ConstructValues<'_,'_>,kind:ConstructComponent,index:usize,last:u32) {
    let values=unsafe { &mut *context };
    let value=match kind {
        ConstructComponent::Project=>{
            let value=Builder::matrix_component(values.current.as_ref().unwrap(),index);
            if last!=0 { values.current=None; }
            value
        },
        ConstructComponent::Diagonal=>if last!=0 {values.current.take().unwrap()}else{values.current.as_ref().unwrap().clone()},
        ConstructComponent::Zero=>if last!=0 {values.zero.take().unwrap()}else{values.zero.as_ref().unwrap().clone()},
    };
    Builder::append_value(&mut values.output,value,false);
}
unsafe extern "C" fn construct_splat(context:*mut ConstructValues<'_,'_>,width:usize) {
    let output=&mut unsafe { &mut *context }.output;
    output.proof.splat(width);output.regs.resize(width,output.regs[0]);
    if let Some(bits)=&mut output.constant { bits.resize(width,bits[0]); }
}
impl Builder<'_> {
    pub(super) fn construct_value(&mut self,entry:ConstructEntry,ty:Ty,values:Vec<Value>,span:Span)->Result<Value> {
        let types=ConstructTypes { target:&ty,values:&values };
        let plan=unsafe { source_construct_plan(entry,Operand::of(&ty),values.len(),ConstructQueries {
            context:std::ptr::from_ref(&types).cast(),fact:construct_fact,same:construct_same,scalar_identity:construct_scalar_identity,
        }) };
        let error=match plan.status {
            ConstructStatus::Valid=>None,
            ConstructStatus::CFunctionTarget=>Some(("B154","cfn构造只接受空值或同签名函数值；原生地址解释须显式unsafe_function")),
            ConstructStatus::CFunctionInput=>Some(("B154","C函数地址不能转换为数值或其他身份；地址解释须显式function_address")),
            ConstructStatus::AddressTarget=>Some(("B154","address构造只接受空值或同型地址；整数解释须显式unsafe_address")),
            ConstructStatus::AddressInput=>Some(("B154","address不能隐式转换为数值；位模式须显式address_bits")),
            ConstructStatus::HandleTarget=>Some(("B154","handle构造只接受空值或同类型身份；外部身份解释须显式unsafe_handle")),
            ConstructStatus::HandleInput=>Some(("B154","不透明handle不能转换为数值或公开其载荷")),
            ConstructStatus::Variants=>Some(("B269","结果值须先通过 match 或 try 取得对应载荷")),
            ConstructStatus::Enumeration=>Some(("B268","枚举值只直接取回其声明的整数表示；其他数值转换在该整数上显式进行")),
            ConstructStatus::Array=>Some(("B238","数组不能转换成数值标量／向量；先取得数组元素")),
            ConstructStatus::Matrix=>Some(("B237","矩阵不能转换成数值标量／向量；先取得行字段")),
            ConstructStatus::Views=>Some(("B185","view 不能转换成数值或重解释设备地址")),
            ConstructStatus::MatrixInput=>Some(("B237","矩阵构造接收数值分量或相同类型矩阵")),
            ConstructStatus::Components=>Some(("B157","类型构造的分量数不匹配")),
            ConstructStatus::MatrixComponents=>Some(("B157","矩阵构造需要全部行优先分量，或一个对角标量")),
        };
        if let Some((code,message))=error { return self.fail(span,code,message); }
        let uniform=plan.kind!=ConstructKind::Numeric || values.iter().all(|value|value.uniform);
        let count=values.len();let mut input=values.into_iter();
        match plan.kind {
            ConstructKind::Zero=>return Ok(self.constant_value(ty,vec![0;plan.width],None)),
            ConstructKind::Identity=>{let mut value=input.next().unwrap();value.literal=false;return Ok(value);},
            ConstructKind::Enumeration=>return Ok(self.cast(input.next().unwrap(),SCALARS[plan.scalar as usize])),
            ConstructKind::Numeric|ConstructKind::Diagonal|ConstructKind::Matrix=>{},
        }
        let output=Value { proof:bounds::Proof::default(),literal:false,ty,regs:Vec::with_capacity(plan.width),
            uniform,constant:Some(Vec::with_capacity(plan.width)) };
        let mut context=ConstructValues { builder:self,input,current:None,zero:None,output };
        unsafe { expand_construct(plan,count,ConstructEmitter {
            context:&mut context,argument:construct_argument,component:construct_component,splat:construct_splat,
        }) };
        Ok(context.output)
    }
}


#[repr(C)]
#[derive(Clone,Copy)]
pub(super) struct BitcastSignature { present:u32,source:u32,target:u32,maximum:usize }
#[repr(u32)]
enum BitcastStatus { Valid,Arity,Type }
#[repr(C)]
struct BitcastPlan { status:BitcastStatus,source:u32,target:u32,width:usize }
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::source_bitcast"]
    fn source_bitcast(bytes:*const u8,length:usize)->BitcastSignature;
    #[link_name="bootstrap.operator_types::bitcast_plan"]
    fn source_bitcast_plan(signature:BitcastSignature,count:usize,context:*const c_void,
        width:unsafe extern "C" fn(*const c_void)->usize,
        builtin:unsafe extern "C" fn(*const c_void,u32,usize)->u32)->BitcastPlan;
}
impl BitcastSignature {
    pub(super) fn recognize(name:&str)->Self { unsafe { source_bitcast(name.as_ptr(),name.len()) } }
    pub(super) fn is_present(self)->bool { self.present!=0 }
}
unsafe extern "C" fn bitcast_width(context:*const c_void)->usize {
    unsafe { &*context.cast::<MathContext<'_>>() }.values[0].regs.len()
}
unsafe extern "C" fn bitcast_builtin(context:*const c_void,scalar:u32,width:usize)->u32 {
    let value=&unsafe { &*context.cast::<MathContext<'_>>() }.values[0];
    u32::from(value.ty==Ty::vector(SCALARS[scalar as usize],width))
}
#[repr(u32)]
enum ConversionMode { Numeric,Bitcast }
#[repr(u32)]
enum ConversionSource { Type,Register }
#[repr(u32)]
enum ConversionAction { DiscardConstants,CastProof,BitcastProof,ConstantValue,CastOrigin,BitcastOrigin,Reuse,Cast,Bitcast,ConstantBitcast,BeginConstants }
#[repr(u32)]
enum ConversionOutcome { Registers,Constant }
#[repr(C)]
struct ConversionInput {
    mode:ConversionMode,target:crate::builtin_types::ScalarEncoding,types:usize,registers:usize,
    constants:*const Bits,constant_count:usize,constant_present:u32,
}
#[repr(C)]
struct ConversionEmitter<'a,'w> {
    context:*mut ConversionValues<'a,'w>,
    source:unsafe extern "C" fn(*const ConversionValues<'a,'w>,usize,ConversionSource)->crate::builtin_types::ScalarEncoding,
    constant:unsafe extern "C" fn(*mut ConversionValues<'a,'w>,Bits),
    action:unsafe extern "C" fn(*mut ConversionValues<'a,'w>,ConversionAction,usize)->usize,
}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::convert_value"]
    fn source_convert_value(input:ConversionInput,emitter:ConversionEmitter<'_,'_>)->ConversionOutcome;
}
struct ConversionValues<'a,'w> {
    builder:&'a mut Builder<'w>,input:Value,target:Scalar,ty:Option<Ty>,proof:Option<bounds::Proof>,
    constant:Option<Vec<Bits>>,registers:Vec<Reg>,first:usize,output:Option<Value>,
}
impl ConversionValues<'_,'_> {
    fn constant_value(&mut self,bits:Vec<Bits>)->usize {
        self.first=self.builder.code.len();
        self.output=Some(self.builder.constant_value(self.ty.take().unwrap(),bits,None));
        self.builder.code.len()-self.first
    }
}
unsafe extern "C" fn conversion_source(context:*const ConversionValues<'_,'_>,index:usize,source:ConversionSource)->crate::builtin_types::ScalarEncoding {
    let values=unsafe { &*context };
    match source {
        ConversionSource::Type=>values.input.ty.components[index].0,
        ConversionSource::Register=>values.builder.registers[values.input.regs[index]],
    }.into()
}
unsafe extern "C" fn conversion_constant(context:*mut ConversionValues<'_,'_>,bits:Bits) {
    unsafe { &mut *context }.constant.as_mut().unwrap().push(bits);
}
unsafe extern "C" fn conversion_action(context:*mut ConversionValues<'_,'_>,action:ConversionAction,index:usize)->usize {
    let values=unsafe { &mut *context };
    match action {
        ConversionAction::DiscardConstants=>values.constant=None,
        ConversionAction::CastProof=>values.proof=Some(bounds::Proof::cast(&values.input,values.target)),
        ConversionAction::BitcastProof=>values.proof=Some(bounds::Proof::bitcast(&values.input,values.target)),
        ConversionAction::ConstantValue=>{
            let bits=values.constant.take().unwrap();
            let count=values.constant_value(bits);
            values.output.as_mut().unwrap().proof=values.proof.take().unwrap();
            return count;
        },
        ConversionAction::ConstantBitcast=>{
            let bits=values.input.constant.take().unwrap();
            return values.constant_value(bits);
        },
        ConversionAction::BeginConstants=>values.constant=Some(Vec::with_capacity(values.input.constant.as_ref().unwrap().len())),
        ConversionAction::CastOrigin|ConversionAction::BitcastOrigin=>{
            let inst=&mut values.builder.code[values.first+index];
            let result_bits=values.output.as_ref().unwrap().constant.as_ref().unwrap()[index];
            let source=values.input.ty.components[index].0;
            let (reason,trigger)=match action {
                ConversionAction::CastOrigin=>(GenerationReason::CastConstantFold,GenerationAnchor::CastConstantFold {
                    expression:inst.source_site.clone(),source_scalar:source,target_scalar:values.target,
                    input_bits:values.input.constant.as_ref().unwrap()[index],result_bits,
                    component_index:index,component_count:values.input.regs.len(),
                }),
                _=>(GenerationReason::BitcastConstantFold,GenerationAnchor::BitcastConstantFold {
                    expression:inst.source_site.clone(),source_scalar:source,target_scalar:values.target,
                    bits:result_bits,component_index:index,component_count:values.input.regs.len(),
                }),
            };
            inst.generated_origin=Some(Arc::new(GeneratedOrigin { reason,trigger:Some(trigger) }));
        },
        ConversionAction::Reuse=>values.registers.push(values.input.regs[index]),
        ConversionAction::Cast=>{
            let src=values.input.regs[index];let dst=values.builder.reg(values.target);
            values.builder.emit(InstKind::Cast { dst,src });values.registers.push(dst);
        },
        ConversionAction::Bitcast=>{
            let a=values.input.regs[index];let dst=values.builder.reg(values.target);
            values.builder.emit(InstKind::Unary { dst,op:UnaryOp::Bitcast,a });values.registers.push(dst);
        },
    }
    0
}
impl Builder<'_> {
    fn convert_value(&mut self,mode:ConversionMode,input:Value,target:Scalar)->Value {
        let constants=input.constant.as_deref();
        let fact=ConversionInput {
            mode,target:target.into(),types:input.ty.components.len(),registers:input.regs.len(),
            constants:constants.map_or(std::ptr::null(),|bits|bits.as_ptr()),
            constant_count:constants.map_or(0,|bits|bits.len()),constant_present:u32::from(constants.is_some()),
        };
        let ty=Some(Ty::vector(target,input.regs.len()));
        let mut context=ConversionValues { builder:self,input,target,ty,proof:None,constant:None,registers:Vec::new(),first:0,output:None };
        let outcome=unsafe { source_convert_value(fact,ConversionEmitter {
            context:&mut context,source:conversion_source,constant:conversion_constant,action:conversion_action,
        }) };
        match outcome {
            ConversionOutcome::Constant=>context.output.unwrap(),
            ConversionOutcome::Registers=>Value {
                proof:context.proof.unwrap(),literal:false,ty:context.ty.unwrap(),regs:context.registers,
                uniform:context.input.uniform,constant:context.constant,
            },
        }
    }
    pub(super) fn cast_value(&mut self,value:Value,target:Scalar)->Value { self.convert_value(ConversionMode::Numeric,value,target) }
    pub(super) fn bitcast_value(&mut self,name:&str,signature:BitcastSignature,values:Vec<Value>,span:Span)->Result<Value> {
        let context=MathContext { values:&values };
        let plan=unsafe { source_bitcast_plan(signature,values.len(),std::ptr::from_ref(&context).cast(),bitcast_width,bitcast_builtin) };
        match plan.status {
            BitcastStatus::Valid=>{},
            BitcastStatus::Arity=>return self.fail(span,"B153",format!("{name} 需要一个参数")),
            BitcastStatus::Type=>return self.fail(span,"B154",format!("{name} 需要对应位宽的数值标量；32位也接受向量")),
        }
        Ok(self.convert_value(ConversionMode::Bitcast,values.into_iter().next().unwrap(),SCALARS[plan.target as usize]))
    }
}


#[repr(u32)]
#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) enum MathPath { Floating,Minimum,Maximum }
#[repr(C)]
struct MathPreparation { status:u32,path:MathPath,pair:u32 }
#[repr(u32)]
enum MathAction { IntegerProof,IntegerConstant,KnownOrigin,CompareMinimum,CompareMaximum,Select,FloatBegin,FloatInput,FloatEnd,FloatingProof }
#[repr(C)]
struct MathValueFacts { width:usize,literal:u32,uniform:u32 }
#[repr(C)]
struct MathStep { action:MathAction,component:usize,left:usize,right:usize }
#[repr(C)]
struct MathEmitter<'a,'w> {
    context:*mut MathValues<'a,'w>,
    value:unsafe extern "C" fn(*const MathValues<'a,'w>,usize)->MathValueFacts,
    action:unsafe extern "C" fn(*mut MathValues<'a,'w>,MathStep)->usize,
}
#[repr(C)]
struct MathOutcome { constant:u32,literal:u32,uniform:u32 }
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::prepare_math"]
    fn prepare_math(arity:usize,extremum:u32,count:usize,queries:MathQueries)->MathPreparation;
    #[link_name="bootstrap.operator_types::math_shape"]
    fn source_math_shape(path:MathPath,count:usize,queries:MathQueries)->MathShape;
    #[link_name="bootstrap.operator_types::expand_math"]
    fn expand_math(path:MathPath,count:usize,width:usize,emitter:MathEmitter<'_,'_>)->MathOutcome;
}
struct MathValues<'a,'w> {
    builder:&'a mut Builder<'w>,name:&'a str,args:Vec<Value>,operation:Option<MathOp>,scalar:Scalar,width:usize,
    ty:Option<Ty>,proof:Option<bounds::Proof>,known:Option<Vec<(Bits,IntegerKnownValueBasis)>>,
    comparison:Option<Value>,registers:Vec<Reg>,current:Vec<Reg>,pending:Option<Reg>,first:usize,output:Option<Value>,
}
unsafe extern "C" fn math_value_facts(context:*const MathValues<'_,'_>,index:usize)->MathValueFacts {
    let value=&unsafe { &*context }.args[index];
    MathValueFacts { width:value.regs.len(),literal:u32::from(value.literal),uniform:u32::from(value.uniform) }
}
unsafe extern "C" fn math_action(context:*mut MathValues<'_,'_>,step:MathStep)->usize {
    let values=unsafe { &mut *context };
    match step.action {
        MathAction::IntegerProof=>{
            let proof=bounds::arithmetic(values.builder,values.name,&values.args[0],&values.args[1]);
            values.known=proof.known_integer_value(values.ty.as_ref().unwrap());values.proof=Some(proof);
            return usize::from(values.known.is_some());
        },
        MathAction::IntegerConstant=>{
            values.first=values.builder.code.len();
            let bits=values.known.as_ref().unwrap().iter().map(|(bits,_)|*bits).collect();
            let mut value=values.builder.constant_value(values.ty.take().unwrap(),bits,None);
            value.proof=values.proof.take().unwrap();values.output=Some(value);
            return values.builder.code.len()-values.first;
        },
        MathAction::KnownOrigin=>{
            let (result_bits,basis)=values.known.as_ref().unwrap()[step.component];
            let a=&values.args[0];let b=&values.args[1];let ai=step.left;let bi=step.right;
            let inst=&mut values.builder.code[values.first+step.component];
            inst.generated_origin=Some(Arc::new(GeneratedOrigin {
                reason:GenerationReason::MinmaxKnownValue,
                trigger:Some(GenerationAnchor::MinmaxKnownValue {
                    expression:inst.source_site.clone(),operation:values.name.into(),scalar:values.scalar,
                    left_ranges:a.component_range_intervals(ai),right_ranges:b.component_range_intervals(bi),
                    left_constant_present:a.constant.is_some(),right_constant_present:b.constant.is_some(),
                    same_value:a.same_component(ai,b,bi),result_bits,basis,
                    left_component_index:ai,left_component_count:a.regs.len(),right_component_index:bi,right_component_count:b.regs.len(),
                    component_index:step.component,component_count:values.width,
                }),
            }));
        },
        MathAction::CompareMinimum|MathAction::CompareMaximum=>{
            let (name,comparison)=match step.action {MathAction::CompareMinimum=>("<",Comparison::Less),_=>(">",Comparison::Greater)};
            values.comparison=Some(values.builder.lower_numeric_binary(name,Ty::vector(Scalar::Bool,values.width),
                &values.args[0],&values.args[1],values.scalar,true,Some(BinaryOp::Compare(comparison)),false,None,None));
        },
        MathAction::Select=>{
            let dst=values.builder.reg(values.scalar);
            values.builder.emit(InstKind::Select { dst,condition:values.comparison.as_ref().unwrap().regs[step.component],
                a:values.args[0].regs[step.left],b:values.args[1].regs[step.right] });values.registers.push(dst);
        },
        MathAction::FloatBegin=>{
            values.pending=Some(values.builder.reg(values.scalar));values.current=Vec::with_capacity(values.args.len());
        },
        MathAction::FloatInput=>values.current.push(values.args[step.left].regs[step.right]),
        MathAction::FloatEnd=>{
            let dst=values.pending.take().unwrap();
            values.builder.emit(InstKind::Math { dst,op:values.operation.unwrap(),args:std::mem::take(&mut values.current) });
            values.registers.push(dst);
        },
        MathAction::FloatingProof=>values.proof=Some(bounds::math(values.operation.unwrap(),&values.args)),
    }
    0
}
impl Builder<'_> {
    pub(super) fn math_value(&mut self,name:&str,signature:MathSignature,mut args:Vec<Value>,span:Span)->Result<Value> {
        let context=MathContext { values:&args };
        let preparation=unsafe { prepare_math(signature.arity as usize,signature.extremum,args.len(),context.queries()) };
        if preparation.status!=0 { return self.fail(span,"B153",format!("{name} 需要 {} 个参数",signature.arity)); }
        if preparation.pair!=0 {
            let b=args.pop().unwrap();let a=args.pop().unwrap();
            let (a,b)=self.numeric_operands(a,b,span)?;args.push(a);args.push(b);
        }
        let context=MathContext { values:&args };
        let shape=unsafe { source_math_shape(preparation.path,args.len(),context.queries()) };
        if shape.status!=0 { return self.fail(span,"B154",format!("{name} 需要相同浮点类型；f32 向量须维度相容")); }
        let operation=if preparation.path==MathPath::Floating {
            Some(signature.operation().ok_or_else(||Diagnostic {source:self.scope.source().clone(),span,code:"B179",message:format!("未知数学运算 {name}")})?)
        } else {None};
        Ok(self.expand_math_value(name,preparation.path,operation,args,SCALARS[shape.scalar as usize],shape.width))
    }
    pub(super) fn floating_math_value(&mut self,operation:MathOp,args:Vec<Value>,scalar:Scalar,width:usize)->Value {
        self.expand_math_value("",MathPath::Floating,Some(operation),args,scalar,width)
    }
    fn expand_math_value(&mut self,name:&str,path:MathPath,operation:Option<MathOp>,args:Vec<Value>,scalar:Scalar,width:usize)->Value {
        let count=args.len();
        let mut values=MathValues { builder:self,name,args,operation,scalar,width,ty:Some(Ty::vector(scalar,width)),proof:None,known:None,
            comparison:None,registers:Vec::new(),current:Vec::new(),pending:None,first:0,output:None };
        let outcome=unsafe { expand_math(path,count,width,MathEmitter {context:&mut values,value:math_value_facts,action:math_action}) };
        if outcome.constant!=0 {
            let mut value=values.output.unwrap();value.literal=outcome.literal!=0;value.uniform=outcome.uniform!=0;value
        } else { Value {proof:values.proof.unwrap(),literal:outcome.literal!=0,ty:values.ty.unwrap(),regs:values.registers,
            uniform:outcome.uniform!=0,constant:None} }
    }
}
