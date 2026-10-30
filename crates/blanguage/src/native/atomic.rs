//! x64 原子作用于原数据地址；读改写指令返回旧值，不经过宿主锁或回调。
use super::{operands::Operand, *};
impl Emitter<'_> {
    pub(super) fn atomic(&mut self, atomic: &Atomic) {
        let shared = matches!(atomic.target, AtomicTarget::Dataset(_));
        // 一个CPU范围独占完整tile，lane由原生循环执行，无跨线程原子性成本。
        let at = if let Some(offset) = atomic.offset {
            let source = match atomic.target {
                AtomicTarget::Dataset(binding) => IndexedSource::Data {
                    binding,
                    index: atomic.index,
                },
                AtomicTarget::Tile(tile) => IndexedSource::Tile {
                    tile,
                    index: atomic.index,
                },
            };
            self.indexed_address(&source, offset, atomic.word);
            Operand::memory(15, 0)
        } else {
            match atomic.target {
                AtomicTarget::Dataset(binding) => {
                    self.address(binding, atomic.index);
                    Operand::memory(15, atomic.word)
                }
                AtomicTarget::Tile(tile) => {
                    self.load(atomic.index);
                    self.bytes(&[0x4c, 0x8d, 0x3c, 0x84]); // r15 = rsp + index * 4
                    Operand::memory(
                        15,
                        self.tile_storage_base()
                            + self
                                .stage
                                .word_offset(self.stage.shared_word(tile, atomic.word))
                                as u32,
                    )
                }
            }
        };
        match atomic.op {
            AtomicOp::Load => self.instruction(None, &[0x8b], false, 0, at),
            AtomicOp::Store(value) => {
                // 对齐的 32 位 MOV 在 x64 是原子的；TSO 同时满足 release。
                self.load(value);
                self.instruction(None, &[0x89], false, 0, at);
            }
            AtomicOp::Add(value) | AtomicOp::Sub(value) => {
                self.load(value);
                if matches!(atomic.op, AtomicOp::Sub(_)) {
                    self.bytes(&[0xf7, 0xd8]);
                }
                self.instruction(shared.then_some(0xf0), &[0x0f, 0xc1], false, 0, at);
            }
            AtomicOp::Exchange(value) => {
                self.load(value);
                if shared {
                    self.instruction(None, &[0x87], false, 0, at); // 内存XCHG自带LOCK。
                } else {
                    self.bytes(&[0x89, 0xc2]);
                    self.instruction(None, &[0x8b], false, 0, at);
                    self.instruction(None, &[0x89], false, 2, at);
                }
            }
            AtomicOp::CompareExchange {
                expected, desired, ..
            } => {
                self.load(desired);
                self.bytes(&[0x89, 0xc2]);
                self.load(expected);
                self.instruction(shared.then_some(0xf0), &[0x0f, 0xb1], false, 2, at);
            }
            AtomicOp::And(value) => self.atomic_modify(value, at, &[0x21, 0xd1], shared),
            AtomicOp::Or(value) => self.atomic_modify(value, at, &[0x09, 0xd1], shared),
            AtomicOp::Xor(value) => self.atomic_modify(value, at, &[0x31, 0xd1], shared),
            AtomicOp::Min(value) | AtomicOp::Max(value) => {
                let signed = self.stage.registers[value] == Scalar::I32;
                let min = matches!(atomic.op, AtomicOp::Min(_));
                let condition = match (signed, min) {
                    (true, true) => 0x4f,
                    (true, false) => 0x4c,
                    (false, true) => 0x47,
                    (false, false) => 0x42,
                };
                self.atomic_modify(value, at, &[0x39, 0xd1, 0x0f, condition, 0xca], shared);
            }
        }
        if let Some(result) = atomic.result {
            self.store(result);
        }
    }
    fn atomic_modify(&mut self, value: Reg, at: Operand, combine: &[u8], shared: bool) {
        self.load(value);
        self.bytes(&[0x89, 0xc2]);
        self.instruction(None, &[0x8b], false, 0, at);
        let begin = self.code.len();
        self.bytes(&[0x89, 0xc1]);
        self.bytes(combine);
        if shared {
            // CMPXCHG失败时EAX已是新的旧值；重算同一次共享RMW。
            self.instruction(Some(0xf0), &[0x0f, 0xb1], false, 1, at);
            let again = self.jump(Some(0x85));
            self.patch(again, begin);
        } else {
            self.instruction(None, &[0x89], false, 1, at);
        }
    }
}
