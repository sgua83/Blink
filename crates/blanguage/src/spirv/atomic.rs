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
        let (pointer, scope, memory) = if let Some(offset) = atomic.offset {
            let (source, scope, memory) = match atomic.target {
                AtomicTarget::Dataset(binding) => (
                    IndexedSource::Data {
                        binding,
                        index: atomic.index,
                    },
                    1,
                    0x40,
                ),
                AtomicTarget::Tile(tile) => (
                    IndexedSource::Tile {
                        tile,
                        index: atomic.index,
                    },
                    2,
                    0x100,
                ),
            };
            (
                self.indexed_address(&source, offset, atomic.word),
                scope,
                memory,
            )
        } else {
            match atomic.target {
                AtomicTarget::Dataset(binding) => {
                    (self.address(binding, atomic.index, atomic.word), 1, 0x40)
                }
                AtomicTarget::Tile(tile) => (
                    self.tile_word_address(
                        self.stage.shared_word(tile, atomic.word),
                        self.value(atomic.index),
                    ),
                    2,
                    0x100,
                ),
            }
        };
        let scope = self.constant(Scalar::U32, scope);
        let semantics = self.atomic_semantics(atomic.order, memory);
        let u = self.types[&Scalar::U32];
        let opcode = match atomic.op {
            AtomicOp::Load => 227,
            AtomicOp::Store(_) => 228,
            AtomicOp::Exchange(_) => 229,
            AtomicOp::CompareExchange { .. } => 230,
            AtomicOp::Add(_) => 234,
            AtomicOp::Sub(_) => 235,
            AtomicOp::Min(value) => {
                if self.stage.registers[value] == Scalar::I32 {
                    236
                } else {
                    237
                }
            }
            AtomicOp::Max(value) => {
                if self.stage.registers[value] == Scalar::I32 {
                    238
                } else {
                    239
                }
            }
            AtomicOp::And(_) => 240,
            AtomicOp::Or(_) => 241,
            AtomicOp::Xor(_) => 242,
        };
        if let AtomicOp::Store(value) = atomic.op {
            let value = self.to_word(value);
            self.emit(opcode, &[pointer, scope, semantics, value]);
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
                operands.push(self.to_word(desired));
                operands.push(self.to_word(expected));
            }
            ref op => {
                operands.extend(op.operands().iter().map(|r| self.to_word(*r)));
            }
        };
        self.emit(opcode, &operands);
        if let Some(dst) = atomic.result {
            self.from_word(dst, result);
        }
    }
}
