//! Subgroup 值协作不提供任意地址写入；活动集合由当前 GPU 控制流决定。
use super::*;
use std::ffi::c_void;

#[repr(C)]
struct SubgroupBuiltin {
    operation: SubgroupOp,
    arity: u32,
    rule: u32,
}

#[allow(dead_code)]
#[repr(u32)]
enum SubgroupFact { Shape, Numeric, Integer }

#[allow(dead_code)]
#[repr(u32)]
enum SubgroupResult { U32, Bool, PredicateU32, PredicateBool, Operand, BadShape, BadBits }

unsafe extern "C" {
    #[link_name = "bootstrap.operator_types::source_subgroup"]
    fn source_subgroup(bytes: *const u8, length: usize) -> SubgroupBuiltin;
    #[link_name = "bootstrap.operator_types::subgroup_result"]
    fn subgroup_result(rule: u32, ty: *const c_void,
        fact: unsafe extern "C" fn(*const c_void, SubgroupFact) -> u32) -> SubgroupResult;
}

unsafe extern "C" fn subgroup_fact(ty: *const c_void, fact: SubgroupFact) -> u32 {
    let ty = unsafe { &*ty.cast::<Ty>() };
    u32::from(match fact {
        SubgroupFact::Shape => !ty.components.is_empty() && ty.fields.is_empty()
            && ty.view.is_none() && ty.array.is_none(),
        SubgroupFact::Numeric => ty.is_numeric(),
        SubgroupFact::Integer => ty.components[0].0.is_integer(),
    })
}

impl Builder<'_> {
    pub(super) async fn subgroup(
        &mut self,
        name: &str,
        count: usize,
        span: Span,
        argument: impl std::ops::AsyncFnMut(&mut Self,usize)->Result<Option<Value>>,
        argument_span: impl Fn(usize)->Span,
    ) -> Option<Result<Option<Value>>> {
        let suffix = name.strip_prefix("subgroup_")?;
        let builtin = unsafe { source_subgroup(suffix.as_ptr(), suffix.len()) };
        if builtin.rule == 0 { return None; }
        Some(unsafe { self.world.call(self.subgroup_value(builtin,count,span,argument,argument_span)).await })
    }

    async fn subgroup_value(
        &mut self,
        builtin: SubgroupBuiltin,
        count: usize,
        span: Span,
        mut lower: impl std::ops::AsyncFnMut(&mut Self,usize)->Result<Option<Value>>,
        argument_span: impl Fn(usize)->Span,
    ) -> Result<Option<Value>> {
        if self.program.target != Target::Gpu {
            return self.fail(
                span,
                "B195",
                "subgroup 协作需要显式 on gpu，不能作为 CPU／通用 compute 语义",
            );
        }
        if count != builtin.arity as usize {
            return self.fail(span, "B196", "subgroup 操作参数数量不符");
        }
        let value = if count>0 {
            let Some(value) = lower(self,0).await? else {
                return Ok(None);
            };
            Some(value)
        } else {
            None
        };
        let argument = if count>1 {
            let Some(value) = lower(self,1).await? else {
                return Ok(None);
            };
            Some(value)
        } else {
            None
        };
        if let Some(argument) = &argument {
            self.require_same(&Ty::scalar(Scalar::U32), &argument.ty, argument_span(1))?;
        }
        let operand = value.as_ref().map_or(std::ptr::null(), |value| std::ptr::from_ref(&value.ty));
        let result = unsafe { subgroup_result(builtin.rule, operand.cast(), subgroup_fact) };
        let ty = match result {
            SubgroupResult::U32 => Ty::scalar(Scalar::U32),
            SubgroupResult::Bool => Ty::scalar(Scalar::Bool),
            SubgroupResult::PredicateBool | SubgroupResult::PredicateU32 => {
                self.require_bool(value.as_ref().unwrap(), span)?;
                Ty::scalar(match result {
                    SubgroupResult::PredicateBool => Scalar::Bool,
                    _ => Scalar::U32,
                })
            }
            SubgroupResult::Operand => value.as_ref().unwrap().ty.clone(),
            SubgroupResult::BadShape => return self.fail(span, "B197",
                "subgroup 数值协作需要数值标量／向量，广播与 shuffle 也支持 bool；不能传记录或资源 view"),
            SubgroupResult::BadBits => return self.fail(span, "B197",
                "subgroup 按位协作只接受整数标量或向量"),
        };
        let mut regs = Vec::new();
        for (component, (scalar, _)) in ty.components.iter().enumerate() {
            let dst = self.reg(*scalar);
            self.emit(InstKind::Subgroup {
                dst,
                op: builtin.operation,
                value: value.as_ref().map(|v| v.regs[component]),
                argument: argument.as_ref().map(|v| v.regs[0]),
            });
            regs.push(dst);
        }
        Ok(Some(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs,
            uniform: false,
            constant: None,
        }))
    }
}
