//! dataset 与块内原子共用位置、类型、索引证明及显式内存序。
use super::*;
impl Builder<'_> {
    pub(super) fn atomic_storage(&self, place: &Place) -> bool {
        match place.storage() {
            Place::AtomicParameter(_) => true,
            Place::Element { binding, .. } => self.view_access[binding] == Access::Atomic,
            Place::TileElement { tile, .. } => self.tiles[*tile].access == Access::Atomic,
            _ => false,
        }
    }
    pub(super) fn atomic_place(&mut self, target: &Expr) -> Result<Option<Place>> {
        let Some(place) = self.place(target)? else {
            return Ok(None);
        };
        if self.atomic_storage(&place) {
            Ok(Some(place))
        } else {
            self.fail(
                target.span,
                "B214",
                "原子借用需要显式 atomic dataset／tile 字段或已有原子参数",
            )
        }
    }
    fn atomic_location(
        &mut self,
        target: &Expr,
    ) -> Result<Option<(Option<(AtomicTarget, Reg, Option<Reg>, u32)>, Scalar)>> {
        let Some(place) = self.atomic_place(target)? else {
            return Ok(None);
        };
        let ty = place.ty();
        if ![Scalar::U32, Scalar::I32, Scalar::U64, Scalar::I64].into_iter().any(|s| *ty == Ty::scalar(s)) {
            return self.fail(target.span, "B215", "原子目标只支持 u32/i32/u64/i64 标量字段");
        }
        let scalar = ty.components[0].0;
        let (base, offset, word) = match &place {
            Place::Indexed {
                base,
                offset,
                words,
                ..
            } => (&**base, Some(*offset), words[0]),
            Place::Element { words, .. } | Place::TileElement { words, .. } => {
                (&place, None, words[0])
            }
            Place::AtomicParameter(_) => return Ok(Some((None, scalar))),
            _ => unreachable!(),
        };
        let location = match base {
            Place::AtomicParameter(_) => None,
            Place::Element { binding, index, .. } => {
                Some((AtomicTarget::Dataset(*binding), *index, offset, word))
            }
            Place::TileElement { tile, index, .. } => {
                Some((AtomicTarget::Tile(*tile), *index, offset, word))
            }
            _ => unreachable!(),
        };
        if scalar.size() == 8 && matches!(location, Some((AtomicTarget::Tile(_), ..))) {
            return self.fail(target.span, "B215", "64位tile原子尚未实现");
        }
        Ok(Some((location, scalar)))
    }
    fn atomic_order(&self, expr: &Expr) -> Result<MemoryOrder> {
        if let ExprKind::Name(name) = &expr.kind
            && let Some(order) = MemoryOrder::from_source(name)
        {
            return Ok(order);
        }
        self.fail(
            expr.span,
            "B216",
            "原子内存序需要显式 relaxed、acquire、release 或 acq_rel",
        )
    }
    fn atomic_operand(&mut self, expr: &Expr, scalar: Scalar) -> Result<Option<Reg>> {
        let Some(value) = self.expr(expr)? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(scalar), &value.ty, expr.span)?;
        Ok(Some(value.regs[0]))
    }
    fn atomic_effect(&mut self, atomic: Atomic) {
        if let AtomicTarget::Dataset(binding) = atomic.target {
            if atomic.op.reads() {
                self.reads.insert(binding);
            }
            if atomic.op.writes() {
                self.writes.insert(binding);
                let range = if atomic.offset.is_some() {
                    (0, self.program.buffers[binding].ty.size)
                } else {
                    (atomic.word, atomic.word + atomic.scalar.size())
                };
                self.memory_regions.entry(binding).or_default().1.insert(range);
            }
            self.cross_access.insert(binding);
        }
        self.code.push(Inst::Atomic(atomic));
    }
    pub(super) fn atomic_store(&mut self, args: &[Expr], span: Span) -> Result<()> {
        if args.len() != 3 {
            return self.fail(span, "B217", "atomic_store(目标, 值, 内存序) 需要三个参数");
        }
        let Some((location, scalar)) = self.atomic_location(&args[0])? else {
            return Ok(());
        };
        let Some(value) = self.atomic_operand(&args[1], scalar)? else {
            return Ok(());
        };
        let order = self.atomic_order(&args[2])?;
        if !matches!(order, MemoryOrder::Relaxed | MemoryOrder::Release) {
            return self.fail(
                args[2].span,
                "B216",
                "atomic_store 的内存序只能是 relaxed 或 release",
            );
        }
        if let Some((target, index, offset, word)) = location {
            self.atomic_effect(Atomic {
                result: None,
                scalar,
                target,
                index,
                word,
                offset,
                op: AtomicOp::Store(value),
                order,
            });
        }
        Ok(())
    }
    pub(super) fn atomic_value(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        let arity = match name {
            "atomic_load" => 2,
            "atomic_compare_exchange" => 5,
            _ => 3,
        };
        if args.len() != arity {
            return self.fail(
                span,
                "B217",
                format!(
                    "{name} 需要 {arity} 个参数，最后显式提供内存序；比较交换提供成功与失败两种序"
                ),
            );
        }
        let Some((location, scalar)) = self.atomic_location(&args[0])? else {
            return Ok(None);
        };
        let order_at = if name == "atomic_compare_exchange" {
            3
        } else {
            arity - 1
        };
        let order = self.atomic_order(&args[order_at])?;
        let op = if name == "atomic_load" {
            if !matches!(order, MemoryOrder::Relaxed | MemoryOrder::Acquire) {
                return self.fail(
                    args[order_at].span,
                    "B216",
                    "atomic_load 的内存序只能是 relaxed 或 acquire",
                );
            }
            AtomicOp::Load
        } else {
            let Some(value) = self.atomic_operand(&args[1], scalar)? else {
                return Ok(None);
            };
            match name {
                "atomic_add" => AtomicOp::Add(value),
                "atomic_sub" => AtomicOp::Sub(value),
                "atomic_exchange" => AtomicOp::Exchange(value),
                "atomic_min" => AtomicOp::Min(value),
                "atomic_max" => AtomicOp::Max(value),
                "atomic_and" => AtomicOp::And(value),
                "atomic_or" => AtomicOp::Or(value),
                "atomic_xor" => AtomicOp::Xor(value),
                "atomic_compare_exchange" => {
                    let Some(desired) = self.atomic_operand(&args[2], scalar)? else {
                        return Ok(None);
                    };
                    let failure = self.atomic_order(&args[4])?;
                    let permitted = match order {
                        MemoryOrder::Relaxed | MemoryOrder::Release => {
                            failure == MemoryOrder::Relaxed
                        }
                        MemoryOrder::Acquire | MemoryOrder::AcquireRelease => {
                            matches!(failure, MemoryOrder::Relaxed | MemoryOrder::Acquire)
                        }
                    };
                    if !permitted {
                        return self.fail(
                            args[4].span,
                            "B216",
                            "比较交换失败序不得带 release，也不得强于成功序",
                        );
                    }
                    AtomicOp::CompareExchange {
                        expected: value,
                        desired,
                        failure,
                    }
                }
                _ => return self.fail(span, "B158", format!("未知原子操作 {name}")),
            }
        };
        let result = self.reg(scalar);
        if let Some((target, index, offset, word)) = location {
            self.atomic_effect(Atomic {
                result: Some(result),
                scalar,
                target,
                index,
                word,
                offset,
                op,
                order,
            });
        }
        Ok(Some(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(scalar),
            regs: vec![result],
            uniform: false,
            constant: None,
        }))
    }
}
