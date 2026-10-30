//! x64 原子作用于原数据地址；读改写指令返回旧值，不经过宿主锁或回调。
use super::{operands::Operand, *};
impl Emitter<'_> {
    pub(super) fn atomic(&mut self, atomic: &Atomic) {
        let shared = matches!(atomic.target, AtomicTarget::Dataset(_));
        let wide = atomic.scalar.size() == 8;
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
            AtomicOp::Load => self.instruction(None, &[0x8b], wide, 0, at),
            AtomicOp::Store(value) => {
                // 对齐的32／64位MOV在x64是原子的；TSO同时满足release。
                self.load(value);
                self.instruction(None, &[0x89], wide, 0, at);
            }
            AtomicOp::Add(value) | AtomicOp::Sub(value) => {
                self.load(value);
                if matches!(atomic.op, AtomicOp::Sub(_)) {
                    self.instruction(None, &[0xf7], wide, 3, Operand::Register(0));
                }
                self.instruction(shared.then_some(0xf0), &[0x0f, 0xc1], wide, 0, at);
            }
            AtomicOp::Exchange(value) => {
                self.load(value);
                if shared {
                    self.instruction(None, &[0x87], wide, 0, at); // 内存XCHG自带LOCK。
                } else {
                    self.instruction(None, &[0x89], wide, 0, Operand::Register(2));
                    self.instruction(None, &[0x8b], wide, 0, at);
                    self.instruction(None, &[0x89], wide, 2, at);
                }
            }
            AtomicOp::CompareExchange {
                expected, desired, ..
            } => {
                self.load(desired);
                self.instruction(None, &[0x89], wide, 0, Operand::Register(2));
                self.load(expected);
                self.instruction(shared.then_some(0xf0), &[0x0f, 0xb1], wide, 2, at);
            }
            AtomicOp::And(value) | AtomicOp::Or(value) | AtomicOp::Xor(value)
            | AtomicOp::Min(value) | AtomicOp::Max(value) => {
                self.load(value);
                self.instruction(None, &[0x89], wide, 0, Operand::Register(2));
                self.instruction(None, &[0x8b], wide, 0, at);
                let begin = self.code.len();
                self.instruction(None, &[0x89], wide, 0, Operand::Register(1));
                let opcode = match atomic.op {
                    AtomicOp::And(_) => 0x21,
                    AtomicOp::Or(_) => 0x09,
                    AtomicOp::Xor(_) => 0x31,
                    _ => 0x39,
                };
                self.instruction(None, &[opcode], wide, 2, Operand::Register(1));
                if matches!(atomic.op, AtomicOp::Min(_) | AtomicOp::Max(_)) {
                    let condition = match (atomic.scalar.is_signed(), matches!(atomic.op, AtomicOp::Min(_))) {
                        (true, true) => 0x4f,
                        (true, false) => 0x4c,
                        (false, true) => 0x47,
                        (false, false) => 0x42,
                    };
                    self.instruction(None, &[0x0f, condition], wide, 1, Operand::Register(2));
                }
                if shared {
                    // CMPXCHG失败时RAX／EAX已是新的旧值，重算同一次共享RMW。
                    self.instruction(Some(0xf0), &[0x0f, 0xb1], wide, 1, at);
                    let again = self.jump(Some(0x85));
                    self.patch(again, begin);
                } else {
                    self.instruction(None, &[0x89], wide, 1, at);
                }
            }
        }
        if let Some(result) = atomic.result {
            self.store(result);
        }
    }
}
