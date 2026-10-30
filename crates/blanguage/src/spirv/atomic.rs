//! 原子指令按实际地址区分 Device 存储与 Workgroup 共享作用域。
use super::*;
impl Emitter<'_> {
    fn atomic_semantics(&mut self, order: MemoryOrder, memory: u32) -> u32 {
        let semantics = match order {
            MemoryOrder::Relaxed => 0,
            MemoryOrder::Acquire => memory | 2,
            MemoryOrder::Release => memory | 4,
            MemoryOrder::AcquireRelease => memory | 8,
        };
        self.constant(Scalar::U32, semantics)
    }
    pub(super) fn atomic(&mut self, atomic: &Atomic) {
        let unit = Self::raw_scalar(atomic.scalar);
        let (mut pointer, scope, memory) = match atomic.target {
            AtomicTarget::Dataset(binding) => {
                let at = if let Some(offset) = atomic.offset {
                    self.indexed_address(&IndexedSource::Data { binding, index: atomic.index }, offset, atomic.word)
                } else {
                    self.address(binding, atomic.index, atomic.word)
                };
                (at, 1, 0x40)
            }
            AtomicTarget::Tile(tile) => (
                self.tile_word_address(
                    self.stage.shared_word(tile, atomic.word),
                    self.value(atomic.index),
                    atomic.offset.map(|r| self.value(r)),
                ).0,
                2,
                0x100,
            ),
        };
        if unit == Scalar::U64 {
            let typed = self.id();
            self.emit(124, &[self.physical_pointers[&unit], typed, pointer]);
            pointer = typed;
        }
        let scope = self.constant(Scalar::U32, scope);
        let semantics = self.atomic_semantics(atomic.order, memory);
        let u = self.types[&unit];
        let opcode = match atomic.op {
            AtomicOp::Load => 227,
            AtomicOp::Store(_) | AtomicOp::Exchange(_) => 229,
            AtomicOp::CompareExchange { .. } => 230,
            AtomicOp::Add(_) => 234,
            AtomicOp::Sub(_) => 235,
            AtomicOp::Min(_) => if atomic.scalar.is_signed() { 236 } else { 237 },
            AtomicOp::Max(_) => if atomic.scalar.is_signed() { 238 } else { 239 },
            AtomicOp::And(_) => 240,
            AtomicOp::Or(_) => 241,
            AtomicOp::Xor(_) => 242,
        };
        if let AtomicOp::Store(value) = atomic.op && unit == Scalar::U32 {
            let value = self.to_words(value, unit)[0];
            self.emit(228, &[pointer, scope, semantics, value]);
            return;
        }
        let result = self.id();
        let mut operands = vec![u, result, pointer, scope, semantics];
        match atomic.op {
            AtomicOp::CompareExchange {
                expected,
                desired,
                failure,
            } => {
                operands.push(self.atomic_semantics(failure, memory));
                operands.push(self.to_words(desired, unit)[0]);
                operands.push(self.to_words(expected, unit)[0]);
            }
            ref op => {
                operands.extend(op.operands().iter().map(|r| self.to_words(*r, unit)[0]));
            }
        };
        self.emit(opcode, &operands);
        if let Some(dst) = atomic.result {
            self.from_words(dst, &[result], unit);
        }
    }
}
