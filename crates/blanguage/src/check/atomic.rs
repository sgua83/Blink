//! dataset 与块内原子共用位置、类型、索引证明及显式内存序。
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum AtomicSourceKind {
    Unknown, Load, Store, Add, Sub, Exchange, CompareExchange, Min, Max, And, Or, Xor,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct AtomicBuiltin {
    kind: AtomicSourceKind,
    access: Access,
    arity: u32,
    order_index: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.operator_types::source_atomic"]
    fn source_atomic(bytes: *const u8, length: usize, statement: u32) -> AtomicBuiltin;
    #[link_name = "bootstrap.operator_types::atomic_order_allowed"]
    fn atomic_order_allowed(kind: AtomicSourceKind, order: u32, failure: u32) -> u32;
}

impl AtomicBuiltin {
    pub(super) fn from_source(name: &str, statement: bool) -> Option<Self> {
        let suffix = name.strip_prefix("atomic_")?;
        let builtin = unsafe { source_atomic(suffix.as_ptr(), suffix.len(), u32::from(statement)) };
        (builtin.kind != AtomicSourceKind::Unknown).then_some(builtin)
    }
    pub(super) fn is_store(self) -> bool { self.kind == AtomicSourceKind::Store }
    fn permits(self, order: MemoryOrder, failure: MemoryOrder) -> bool {
        unsafe { atomic_order_allowed(self.kind, order as u32, failure as u32) != 0 }
    }
}

impl Builder<'_> {
    pub(super) fn atomic_storage(&self, place: &Place) -> bool {
        match place.storage() {
            Place::AtomicParameter(_)
            | Place::Reference { access: Access::Atomic, .. }
            | Place::ApplicationElement { access: Access::Atomic, .. } => true,
            Place::Element { binding, .. } => self.view_access[binding] == Access::Atomic,
            Place::TileElement { tile, .. } => self.tiles[*tile].access == Access::Atomic,
            _ => false,
        }
    }
    pub(super) async fn atomic_place(&mut self, target: &Expr) -> Result<Option<Place>> {
        let Some(place) = unsafe { self.world.call(self.place(target)).await }? else {
            return Ok(None);
        };
        self.atomic_place_value(place,target.span())
    }
    pub(super) fn atomic_place_value(&mut self,place:Place,span:Span)->Result<Option<Place>> {
        if self.atomic_storage(&place) {
            Ok(Some(place))
        } else {
            self.fail(
                span,
                "B214",
                "原子借用需要显式 atomic dataset／tile 字段或已有原子参数",
            )
        }
    }
    async fn atomic_location(
        &mut self,
        target: &Expr,
        access: Access,
    ) -> Result<Option<(Place, Option<(IndexedSource, Option<Reg>, u32)>, Scalar)>> {
        let Some(place) = unsafe { self.world.call(self.atomic_place(target)).await }? else {
            return Ok(None);
        };
        let ty = place.ty();
        if ![Scalar::U32, Scalar::I32, Scalar::U64, Scalar::I64].into_iter().any(|s| *ty == Ty::scalar(s)) {
            return self.fail(target.span(), "B215", "原子目标只支持 u32/i32/u64/i64 标量字段");
        }
        let scalar = ty.components[0].0;
        let write=access.writes();
        unsafe { self.world.call(self.require_place_access(&place,access,target.span())).await }?;
        self.dataset_effect(&place,access,true,target.span())?;
        let (base, offset, word) = match &place {
            Place::Indexed { base, offset, words, .. } => (&**base, Some(*offset), words[0]),
            Place::Element { words, .. } | Place::TileElement { words, .. }
            | Place::Reference { words, .. } | Place::ApplicationElement { words, .. } => (&place, None, words[0]),
            Place::AtomicParameter(_) => return Ok(Some((place, None, scalar))),
            _ => unreachable!(),
        };
        let location = match base {
            Place::AtomicParameter(_) => None,
            Place::Element { binding, index, .. } => Some((IndexedSource::Data { binding: *binding, index: *index }, offset, word)),
            Place::TileElement { tile, index, .. } => Some((IndexedSource::Tile { tile: *tile, index: *index }, offset, word)),
            Place::Reference { .. } | Place::ApplicationElement { .. } if offset.is_some() => {
                let (source, offset, word) = self.array_memory(base, offset.unwrap(), word, scalar, write, target.span())?;
                Some((source, Some(offset), word))
            }
            Place::Reference { address, .. } => {
                let (address, word) = address.component(word);
                let index = self.constant(Scalar::U32, 0).regs[0];
                Some((IndexedSource::View { address, index, stride: ElementStride::Constant(0) }, None, word))
            }
            Place::ApplicationElement { handle, identity, index, stride, .. } => {
                let (address, index, stride) = self.application_memory(*handle, *identity, *index, *stride, write, None, [(scalar, word)], target.span())?;
                Some((IndexedSource::View { address, index, stride: ElementStride::Constant(u64::from(stride)) }, None, word))
            }
            _ => unreachable!(),
        };
        Ok(Some((place, location, scalar)))
    }
    fn atomic_order(&self, expr: &Expr) -> Result<MemoryOrder> {
        if let ExprKind::Name(name) = expr.kind()
            && let Some(order) = MemoryOrder::from_source(name)
        {
            return Ok(order);
        }
        self.fail(
            expr.span(),
            "B216",
            "原子内存序需要显式 relaxed、acquire、release 或 acq_rel",
        )
    }
    async fn atomic_operand(&mut self, expr: &Expr, scalar: Scalar) -> Result<Option<Value>> {
        let Some(value) = unsafe { self.world.call(self.expr(expr)).await }? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(scalar), &value.ty, expr.span())?;
        Ok(Some(value))
    }
    fn atomic_effect(&mut self, place: &Place, atomic: Atomic, stored: Option<&Value>) {
        if atomic.op.writes() {
            let output = stored.map(|value| self.path_proof(&value.ty, bounds::Proof::capture(value))
                .element_facts(|binding| bounds::Proof::default().with_domains(Domains::from([IndexDomain::Data(binding)]))));
            self.data_access(place, Access::Write, output.as_ref());
        }
        if let IndexedSource::Data { binding, .. } = atomic.target {
            if atomic.op.reads() {
                self.reads.insert(binding);
            }
            if atomic.op.writes() {
                self.writes.insert(binding);
            }
            self.cross_access.insert(binding);
        }
        self.emit(InstKind::Atomic(atomic));
    }
    pub(super) async fn atomic_operation(
        &mut self,
        builtin: AtomicBuiltin,
        name: &str,
        args: &Expressions,
        span: Span,
    ) -> Result<Option<Value>> {
        let arity = builtin.arity as usize;
        if args.len() != arity {
            if builtin.is_store() {
                return self.fail(span, "B217", "atomic_store(目标, 值, 内存序) 需要三个参数");
            }
            return self.fail(span, "B217", format!(
                "{name} 需要 {arity} 个参数，最后显式提供内存序；比较交换提供成功与失败两种序"
            ));
        }

        let Some((place, location, scalar)) = unsafe {
            self.world.call(self.atomic_location(&args.at(0), builtin.access)).await
        }? else {
            return Ok(None);
        };
        let order_expr = args.at(builtin.order_index as usize);
        let (order, op, stored) = if builtin.is_store() {
            let Some(value) = unsafe {
                self.world.call(self.atomic_operand(&args.at(1), scalar)).await
            }? else {
                return Ok(None);
            };
            let order = self.atomic_order(&order_expr)?;
            if !builtin.permits(order, MemoryOrder::Relaxed) {
                return self.fail(order_expr.span(), "B216", "atomic_store 的内存序只能是 relaxed 或 release");
            }
            (order, AtomicOp::Store(value.regs[0]), Some(value))
        } else {
            let order = self.atomic_order(&order_expr)?;
            let (op, stored) = if builtin.kind == AtomicSourceKind::Load {
                if !builtin.permits(order, MemoryOrder::Relaxed) {
                    return self.fail(order_expr.span(), "B216", "atomic_load 的内存序只能是 relaxed 或 acquire");
                }
                (AtomicOp::Load, None)
            } else {
                let Some(value) = unsafe {
                    self.world.call(self.atomic_operand(&args.at(1), scalar)).await
                }? else {
                    return Ok(None);
                };
                let operand = value.regs[0];
                match builtin.kind {
                    AtomicSourceKind::Add => (AtomicOp::Add(operand), None),
                    AtomicSourceKind::Sub => (AtomicOp::Sub(operand), None),
                    AtomicSourceKind::Exchange => (AtomicOp::Exchange(operand), Some(value)),
                    AtomicSourceKind::Min => (AtomicOp::Min(operand), Some(value)),
                    AtomicSourceKind::Max => (AtomicOp::Max(operand), Some(value)),
                    AtomicSourceKind::And => (AtomicOp::And(operand), None),
                    AtomicSourceKind::Or => (AtomicOp::Or(operand), None),
                    AtomicSourceKind::Xor => (AtomicOp::Xor(operand), None),
                    AtomicSourceKind::CompareExchange => {
                        let Some(desired) = unsafe {
                            self.world.call(self.atomic_operand(&args.at(2), scalar)).await
                        }? else {
                            return Ok(None);
                        };
                        let failure_expr = args.at(4);
                        let failure = self.atomic_order(&failure_expr)?;
                        if !builtin.permits(order, failure) {
                            return self.fail(failure_expr.span(), "B216", "比较交换失败序不得带 release，也不得强于成功序");
                        }
                        (AtomicOp::CompareExchange {
                            expected: operand,
                            desired: desired.regs[0],
                            failure,
                        }, Some(desired))
                    }
                    _ => return self.fail(span, "B158", format!("未知原子操作 {name}")),
                }
            };
            (order, op, stored)
        };

        let result = (!builtin.is_store()).then(|| self.reg(scalar));
        if let Some((target, offset, word)) = location {
            self.atomic_effect(&place, Atomic {
                result, scalar, target, word, offset, op, order,
            }, stored.as_ref());
        }
        Ok(result.map(|result| Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(scalar),
            regs: vec![result],
            uniform: false,
            constant: None,
        }))
    }
}
