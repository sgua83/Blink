//! x64 原子作用于原数据地址；读改写指令返回旧值，不经过宿主锁或回调。
use super::{operands::Operand, *};
impl Emitter<'_> {
    pub(super) fn atomic(&mut self, atomic: &Atomic) {
        let binding = match atomic.target {
            AtomicTarget::Dataset(binding) => binding,
            AtomicTarget::Tile(tile) => {
                self.tile_atomic(atomic, tile);
                return;
            }
        };
        let at = if let Some(offset) = atomic.offset {
            self.indexed_address(
                &IndexedSource::Data {
                    binding,
                    index: atomic.index,
                },
                offset,
                atomic.word,
            );
            Operand::memory(15, 0)
        } else {
            self.address(binding, atomic.index);
            Operand::memory(15, atomic.word)
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
                self.instruction(Some(0xf0), &[0x0f, 0xc1], false, 0, at);
            }
            AtomicOp::Exchange(value) => {
                self.load(value);
                // 内存 XCHG 自带 locked 语义。
                self.instruction(None, &[0x87], false, 0, at);
            }
            AtomicOp::CompareExchange {
                expected, desired, ..
            } => {
                self.load(desired);
                self.bytes(&[0x89, 0xc2]);
                self.load(expected);
                self.instruction(Some(0xf0), &[0x0f, 0xb1], false, 2, at);
            }
            AtomicOp::And(value) => self.atomic_modify(value, at, &[0x21, 0xd1]),
            AtomicOp::Or(value) => self.atomic_modify(value, at, &[0x09, 0xd1]),
            AtomicOp::Xor(value) => self.atomic_modify(value, at, &[0x31, 0xd1]),
            AtomicOp::Min(value) | AtomicOp::Max(value) => {
                let signed = self.stage.registers[value] == Scalar::I32;
                let min = matches!(atomic.op, AtomicOp::Min(_));
                let condition = match (signed, min) {
                    (true, true) => 0x4f,
                    (true, false) => 0x4c,
                    (false, true) => 0x47,
                    (false, false) => 0x42,
                };
                self.atomic_modify(value, at, &[0x39, 0xd1, 0x0f, condition, 0xca]);
            }
        }
        if let Some(result) = atomic.result {
            self.store(result);
        }
    }
    fn tile_atomic(&mut self, atomic: &Atomic, tile: usize) {
        // 一个 CPU 范围独占完整 tile，所有 lane 由原生阶段循环执行。
        // 共享 word 在线程私有帧内，不需要 LOCK 或宿主线程屏障。
        let at = if let Some(offset) = atomic.offset {
            self.indexed_address(
                &IndexedSource::Tile {
                    tile,
                    index: atomic.index,
                },
                offset,
                atomic.word,
            );
            Operand::memory(15, 0)
        } else {
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
        };
        match atomic.op {
            AtomicOp::Load => self.instruction(None, &[0x8b], false, 0, at),
            AtomicOp::Store(value) => {
                self.load(value);
                self.instruction(None, &[0x89], false, 0, at);
            }
            AtomicOp::CompareExchange {
                expected, desired, ..
            } => {
                self.load(desired);
                self.bytes(&[0x89, 0xc2]);
                self.load(expected);
                self.bytes(&[0x89, 0xc1]);
                self.instruction(None, &[0x8b], false, 0, at);
                self.bytes(&[0x39, 0xc8, 0x0f, 0x45, 0xd0]); // EAX != expected 时保留旧值
                self.instruction(None, &[0x89], false, 2, at);
            }
            AtomicOp::Add(value) => self.tile_modify(value, at, &[0x01, 0xd1]),
            AtomicOp::Sub(value) => self.tile_modify(value, at, &[0x29, 0xd1]),
            AtomicOp::Exchange(value) => self.tile_modify(value, at, &[0x89, 0xd1]),
            AtomicOp::And(value) => self.tile_modify(value, at, &[0x21, 0xd1]),
            AtomicOp::Or(value) => self.tile_modify(value, at, &[0x09, 0xd1]),
            AtomicOp::Xor(value) => self.tile_modify(value, at, &[0x31, 0xd1]),
            AtomicOp::Min(value) | AtomicOp::Max(value) => {
                let signed = self.stage.registers[value] == Scalar::I32;
                let min = matches!(atomic.op, AtomicOp::Min(_));
                let condition = match (signed, min) {
                    (true, true) => 0x4f,
                    (true, false) => 0x4c,
                    (false, true) => 0x47,
                    (false, false) => 0x42,
                };
                self.tile_modify(value, at, &[0x39, 0xd1, 0x0f, condition, 0xca]);
            }
        }
        if let Some(result) = atomic.result {
            self.store(result);
        }
    }
    fn tile_modify(&mut self, value: Reg, at: Operand, combine: &[u8]) {
        self.load(value);
        self.bytes(&[0x89, 0xc2]);
        self.instruction(None, &[0x8b], false, 0, at);
        self.bytes(&[0x89, 0xc1]); // EAX 交付旧值，ECX 计算待写新值
        self.bytes(combine);
        self.instruction(None, &[0x89], false, 1, at);
    }
    fn atomic_modify(&mut self, value: Reg, at: Operand, combine: &[u8]) {
        self.load(value);
        self.bytes(&[0x89, 0xc2]);
        self.instruction(None, &[0x8b], false, 0, at);
        let begin = self.code.len();
        self.bytes(&[0x89, 0xc1]);
        self.bytes(combine);
        // CMPXCHG 失败时 EAX 已是新的旧值；重新计算同一原子 RMW。
        self.instruction(Some(0xf0), &[0x0f, 0xb1], false, 1, at);
        let again = self.jump(Some(0x85));
        self.patch(again, begin);
    }
}
