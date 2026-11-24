//! Subgroup 值协作不提供任意地址写入；活动集合由当前 GPU 控制流决定。
use super::*;
use std::ffi::c_void;

#[derive(Clone,Copy)]
#[repr(C)]
struct SubgroupBuiltin {operation:SubgroupOp,arity:u32,rule:u32}
#[allow(dead_code)]#[repr(u32)]enum SubgroupFact {Shape,Numeric,Integer}
#[allow(dead_code)]#[derive(Clone,Copy,Default)]#[repr(u32)]enum SubgroupResult {#[default] U32,Bool,PredicateU32,PredicateBool,Operand,BadShape,BadBits}
#[derive(Clone,Copy,Default)]#[repr(C)]struct Cursor {phase:u32,result:SubgroupResult,index:u64}
#[derive(Clone,Copy,Default)]#[repr(C)]struct Facts {count:u64,components:u64,gpu:u32,present:u32}
#[allow(dead_code)]#[repr(u32)]enum Need {BadTarget,BadArity,Value,Argument,ArgumentType,Predicate,U32,Bool,Operand,Component,CompleteScalar,CompleteOperand,Empty,BadShape,BadBits}
#[repr(C)]struct Step {cursor:Cursor,need:Need}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::source_subgroup_name"]fn source_subgroup_name(bytes:*const u8,length:usize)->SubgroupBuiltin;
    #[link_name="bootstrap.operator_types::subgroup_step"]fn step(cursor:Cursor,builtin:SubgroupBuiltin,facts:Facts,ty:*const c_void,fact:unsafe extern "C" fn(*const c_void,SubgroupFact)->u32)->Step;
}
unsafe extern "C" fn subgroup_fact(ty:*const c_void,fact:SubgroupFact)->u32 {
    let ty=unsafe{&*ty.cast::<Ty>()};
    u32::from(match fact {
        SubgroupFact::Shape=>!ty.components.is_empty()&&ty.fields.is_empty()&&ty.view.is_none()&&ty.array.is_none(),
        SubgroupFact::Numeric=>ty.is_numeric(),
        SubgroupFact::Integer=>ty.components[0].0.is_integer(),
    })
}
impl Builder<'_> {
    pub(super) async fn subgroup(
        &mut self,name:&str,count:usize,span:Span,
        mut argument:impl std::ops::AsyncFnMut(&mut Self,usize)->Result<Option<Value>>,
        argument_span:impl Fn(usize)->Span,
    )->Option<Result<Option<Value>>> {
        let builtin=unsafe{source_subgroup_name(name.as_ptr(),name.len())};
        if builtin.rule==0{return None;}
        let world=self.world;
        Some(unsafe{world.call(async {
            let mut cursor=Cursor::default();let mut facts=Facts{count:count as u64,gpu:u32::from(self.program.target==Target::Gpu),..Default::default()};
            let mut value:Option<Value>=None;let mut other:Option<Value>=None;let mut fixed:Option<Ty>=None;let mut regs=Vec::new();
            let ty=loop {
                let operand=value.as_ref().map_or(std::ptr::null(),|v|std::ptr::from_ref(&v.ty));
                let next=step(cursor,builtin,facts,operand.cast(),subgroup_fact);cursor=next.cursor;
                match next.need {
                    Need::BadTarget=>return self.fail(span,"B195","subgroup 协作需要显式 on gpu，不能作为 CPU／通用 compute 语义"),
                    Need::BadArity=>return self.fail(span,"B196","subgroup 操作参数数量不符"),
                    Need::Value=>{value=argument(self,0).await?;facts.present=u32::from(value.is_some());},
                    Need::Argument=>{other=argument(self,1).await?;facts.present=u32::from(other.is_some());},
                    Need::ArgumentType=>self.require_same(&Ty::scalar(Scalar::U32),&other.as_ref().unwrap().ty,argument_span(1))?,
                    Need::Predicate=>self.require_bool(value.as_ref().unwrap(),span)?,
                    Need::U32=>{let ty=Ty::scalar(Scalar::U32);facts.components=ty.components.len() as u64;fixed=Some(ty);},
                    Need::Bool=>{let ty=Ty::scalar(Scalar::Bool);facts.components=ty.components.len() as u64;fixed=Some(ty);},
                    Need::Operand=>{facts.components=value.as_ref().unwrap().ty.components.len() as u64;},
                    Need::Component=>{
                        let ty=match cursor.result{SubgroupResult::Operand=>&value.as_ref().unwrap().ty,_=>fixed.as_ref().unwrap()};
                        let component=cursor.index as usize;let dst=self.reg(ty.components[component].0);
                        self.emit(InstKind::Subgroup{dst,op:builtin.operation,value:value.as_ref().map(|v|v.regs[component]),argument:other.as_ref().map(|v|v.regs[0])});regs.push(dst);
                    },
                    Need::CompleteScalar=>break fixed.take().unwrap(),
                    Need::CompleteOperand=>break value.take().unwrap().ty,
                    Need::Empty=>return Ok(None),
                    Need::BadShape=>return self.fail(span,"B197","subgroup 数值协作需要数值标量／向量，广播与 shuffle 也支持 bool；不能传记录或资源 view"),
                    Need::BadBits=>return self.fail(span,"B197","subgroup 按位协作只接受整数标量或向量"),
                }
            };
            Ok(Some(Value{proof:bounds::Proof::default(),literal:false,ty,regs,uniform:false,constant:None}))
        }).await})
    }
}
