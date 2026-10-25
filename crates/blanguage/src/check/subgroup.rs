//! Subgroup 值协作不提供任意地址写入；活动集合由当前 GPU 控制流决定。
use super::*;
impl Builder<'_> {
    pub(super) fn subgroup(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Result<Value>> {
        let op = match name {
            "subgroup_lane" => SubgroupOp::Lane,
            "subgroup_size" => SubgroupOp::Size,
            "subgroup_id" => SubgroupOp::Id,
            "subgroup_elect" => SubgroupOp::Elect,
            "subgroup_any" => SubgroupOp::Any,
            "subgroup_all" => SubgroupOp::All,
            "subgroup_count" => SubgroupOp::Count,
            "subgroup_prefix_count" => SubgroupOp::PrefixCount,
            "subgroup_broadcast_first" => SubgroupOp::BroadcastFirst,
            "subgroup_sum" => SubgroupOp::Sum,
            "subgroup_exclusive_sum" => SubgroupOp::ExclusiveSum,
            "subgroup_min" => SubgroupOp::Min,
            "subgroup_max" => SubgroupOp::Max,
            "subgroup_bit_and" => SubgroupOp::BitAnd,
            "subgroup_bit_or" => SubgroupOp::BitOr,
            "subgroup_bit_xor" => SubgroupOp::BitXor,
            "subgroup_exclusive_bit_and" => SubgroupOp::ExclusiveBitAnd,
            "subgroup_exclusive_bit_or" => SubgroupOp::ExclusiveBitOr,
            "subgroup_exclusive_bit_xor" => SubgroupOp::ExclusiveBitXor,
            _ => return None,
        };
        Some(self.subgroup_value(op, args, span))
    }
    fn subgroup_value(&mut self, op: SubgroupOp, args: &[Expr], span: Span) -> Result<Value> {
        if self.program.target != Target::Gpu {
            return self.fail(
                span,
                "B195",
                "subgroup 协作需要显式 on gpu，不能作为 CPU／通用 compute 语义",
            );
        }
        let nullary = matches!(
            op,
            SubgroupOp::Lane | SubgroupOp::Size | SubgroupOp::Id | SubgroupOp::Elect
        );
        if args.len() != usize::from(!nullary) {
            return self.fail(span, "B196", "subgroup 操作参数数量不符");
        }
        let value = args.first().map(|e| self.expr(e)).transpose()?;
        let ty = match op {
            SubgroupOp::Elect => Ty::scalar(Scalar::Bool),
            SubgroupOp::Lane | SubgroupOp::Size | SubgroupOp::Id => Ty::scalar(Scalar::U32),
            SubgroupOp::Any | SubgroupOp::All | SubgroupOp::Count | SubgroupOp::PrefixCount => {
                self.require_bool(value.as_ref().unwrap(), span)?;
                Ty::scalar(if matches!(op, SubgroupOp::Any | SubgroupOp::All) {
                    Scalar::Bool
                } else {
                    Scalar::U32
                })
            }
            SubgroupOp::Sum
            | SubgroupOp::ExclusiveSum
            | SubgroupOp::Min
            | SubgroupOp::Max
            | SubgroupOp::BitAnd
            | SubgroupOp::BitOr
            | SubgroupOp::BitXor
            | SubgroupOp::ExclusiveBitAnd
            | SubgroupOp::ExclusiveBitOr
            | SubgroupOp::ExclusiveBitXor
            | SubgroupOp::BroadcastFirst => {
                let ty = &value.as_ref().unwrap().ty;
                if ty.components.is_empty()
                    || !ty.fields.is_empty()
                    || ty.view.is_some()
                    || ty.array.is_some()
                    || (op != SubgroupOp::BroadcastFirst && !ty.is_numeric())
                {
                    return self.fail(span, "B197", "subgroup 数值协作需要数值标量／向量，广播也支持 bool；不能传记录或资源 view");
                }
                if matches!(
                    op,
                    SubgroupOp::BitAnd
                        | SubgroupOp::BitOr
                        | SubgroupOp::BitXor
                        | SubgroupOp::ExclusiveBitAnd
                        | SubgroupOp::ExclusiveBitOr
                        | SubgroupOp::ExclusiveBitXor
                ) && !matches!(ty.components[0].0, Scalar::U32 | Scalar::I32)
                {
                    return self.fail(span, "B197", "subgroup 按位协作只接受 u32/i32 标量或向量");
                }
                ty.clone()
            }
        };
        let mut regs = Vec::new();
        for (component, (scalar, _)) in ty.components.iter().enumerate() {
            let dst = self.reg(*scalar);
            self.code.push(Inst::Subgroup {
                dst,
                op,
                value: value.as_ref().map(|v| v.regs[component]),
            });
            regs.push(dst);
        }
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs,
            uniform: false,
            constant: None,
        })
    }
}
