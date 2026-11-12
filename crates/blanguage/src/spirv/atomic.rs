//! 原子指令按实际地址区分 Device 存储与 Workgroup 共享作用域。
use super::*;
impl Emitter<'_> {
    pub(super) fn emit_atomic(&mut self, input: AtomicEmission) {
        let at = self.body.len();
        self.body.reserve(9);
        unsafe {
            let words = encode_atomic(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + words as usize);
        }
    }

    fn atomic_semantics(&mut self, order: MemoryOrder, memory: u32) -> u32 {
        let semantics = match order {
            MemoryOrder::Relaxed => 0,
            MemoryOrder::Acquire => memory | 2,
            MemoryOrder::Release => memory | 4,
            MemoryOrder::AcquireRelease => memory | 8,
        };
        self.constant(Scalar::U32, semantics)
    }
    pub(super) fn atomic(&mut self, atomic: &Atomic) -> Result<(), &'static str> {
        let unit = Self::raw_scalar(atomic.scalar);
        let (pointer, scope, memory) = match atomic.target {
            IndexedSource::Data { binding, index } => {
                let at = if let Some(offset) = atomic.offset {
                    self.indexed_address(unit, &atomic.target, offset, atomic.word)
                } else {
                    self.address(unit, binding, index, atomic.word)
                };
                (at, 1, 0x40)
            }
            IndexedSource::Tile { tile, index } => (
                self.tile_word_address(
                    self.stage.shared_word(tile, atomic.word),
                    self.value(index),
                    atomic.offset.map(|r| self.value(r)),
                ).0,
                2,
                0x100,
            ),
            IndexedSource::View { .. } | IndexedSource::Pointer { .. } => return Err("GPU原子不能使用CPU原生地址".into()),
        };
        let scope = self.constant(Scalar::U32, scope);
        let semantics = self.atomic_semantics(atomic.order, memory);
        let u = self.types[&unit];
        let result = if matches!(atomic.op, AtomicOp::Store(_)) && unit == Scalar::U32 {
            0
        } else {
            self.id()
        };
        let (operation, operand, comparison) = match atomic.op {
            AtomicOp::Load => (AtomicOperation::Load, None, None),
            AtomicOp::Store(value) => (AtomicOperation::Store, Some(value), None),
            AtomicOp::Add(value) => (AtomicOperation::Add, Some(value), None),
            AtomicOp::Sub(value) => (AtomicOperation::Sub, Some(value), None),
            AtomicOp::Exchange(value) => (AtomicOperation::Exchange, Some(value), None),
            AtomicOp::CompareExchange { expected, desired, failure } => (
                AtomicOperation::CompareExchange, Some(desired), Some((expected, failure)),
            ),
            AtomicOp::Min(value) => (AtomicOperation::Min, Some(value), None),
            AtomicOp::Max(value) => (AtomicOperation::Max, Some(value), None),
            AtomicOp::And(value) => (AtomicOperation::And, Some(value), None),
            AtomicOp::Or(value) => (AtomicOperation::Or, Some(value), None),
            AtomicOp::Xor(value) => (AtomicOperation::Xor, Some(value), None),
        };
        let failure = comparison.map_or(0, |(_, order)| self.atomic_semantics(order, memory));
        let value = operand.map_or(0, |reg| self.raw_bits(reg));
        let compare = comparison.map_or(0, |(reg, _)| self.raw_bits(reg));
        self.emit_atomic(AtomicEmission {
            operation, scalar: atomic.scalar.into(), result_type: u, result,
            pointer, scope, semantics, value, compare, failure,
        });
        if let Some(dst) = atomic.result {
            self.from_words(dst, &[result], unit);
        }
        Ok(())
    }
}
