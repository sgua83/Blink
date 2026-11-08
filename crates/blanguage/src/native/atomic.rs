//! x64 原子作用于原数据地址；读改写指令返回旧值，不经过宿主锁或回调。
use super::{operands::Operand, *};
impl Emitter<'_> {
    pub(super) fn atomic(&mut self, atomic: &Atomic) {
        let shared = !matches!(atomic.target, IndexedSource::Tile { .. });
        let wide = atomic.scalar.size() == 8;
        // 一个CPU范围独占完整tile；Data与实际CPU View均共享原地址。
        let at = if let Some(offset) = atomic.offset {
            self.indexed_address(&atomic.target, offset, atomic.word);
            Memory::new(15, 0)
        } else {
            match atomic.target {
                IndexedSource::Data { binding, index } => {
                    self.address(binding, index);
                    Memory::new(15, atomic.word)
                }
                IndexedSource::Tile { tile, index } => {
                    self.load(index);
                    self.bytes(&[0x4c, 0x8d, 0x3c, 0x84]); // r15 = rsp + index * 4
                    Memory::new(15, self.shared_stream(self.stage.shared_word(tile, atomic.word)))
                }
                IndexedSource::View { .. } | IndexedSource::Pointer { .. } => {
                    self.indexed_base(&atomic.target);
                    Memory::new(15, atomic.word)
                }
            }
        };
        if wide && let IndexedSource::Tile { tile, .. } = atomic.target {
            let delta = (self.shared_stream(self.stage.shared_word(tile, atomic.word + 4)) as i64
                - self.shared_stream(self.stage.shared_word(tile, atomic.word)) as i64) as u32;
            self.tile_atomic64(atomic, at, at.offset(delta));
            return;
        }
        let at: Operand = at.into();
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
    // CPU范围独占整个tile；低／高字保持原lane流，寄存器运算消费完整64位值。
    fn tile_atomic64(&mut self, atomic: &Atomic, low: Memory, high: Memory) {
        let read = |e: &mut Self| {
            e.instruction(None, &[0x8b], false, 0, low.into());
            e.instruction(None, &[0x8b], false, 1, high.into());
            e.bytes(&[0x48, 0xc1, 0xe1, 32, 0x48, 0x09, 0xc8]);
        };
        let write = |e: &mut Self, register: u8| {
            e.instruction(None, &[0x89], false, register, low.into());
            e.instruction(None, &[0xc1], true, 5, Operand::Register(register));
            e.bytes(&[32]);
            e.instruction(None, &[0x89], false, register, high.into());
        };
        let update = |e: &mut Self, value: Reg, opcode: u8, condition: Option<u8>| {
            e.load(value);
            e.instruction(None, &[0x89], true, 0, Operand::Register(2));
            read(e);
            e.instruction(None, &[0x89], true, 0, Operand::Register(1));
            e.instruction(None, &[opcode], true, 2, Operand::Register(1));
            if let Some(condition) = condition {
                e.instruction(None, &[0x0f, condition], true, 1, Operand::Register(2));
            }
            write(e, 1);
        };
        match atomic.op {
            AtomicOp::Load => read(self),
            AtomicOp::Store(value) => {
                self.load(value);
                write(self, 0);
            }
            AtomicOp::Exchange(value) => {
                self.load(value);
                self.instruction(None, &[0x89], true, 0, Operand::Register(2));
                read(self);
                write(self, 2);
            }
            AtomicOp::CompareExchange { expected, desired, .. } => {
                self.load(expected);
                self.instruction(None, &[0x89], true, 0, Operand::Register(2));
                read(self);
                self.instruction(None, &[0x39], true, 2, Operand::Register(0));
                let different = self.jump(Some(0x85));
                self.instruction(None, &[0x89], true, 0, Operand::Register(1));
                self.load(desired);
                write(self, 0);
                self.instruction(None, &[0x89], true, 1, Operand::Register(0));
                self.patch(different, self.code.len());
            }
            AtomicOp::Add(value) => update(self, value, 0x01, None),
            AtomicOp::Sub(value) => update(self, value, 0x29, None),
            AtomicOp::And(value) => update(self, value, 0x21, None),
            AtomicOp::Or(value) => update(self, value, 0x09, None),
            AtomicOp::Xor(value) => update(self, value, 0x31, None),
            AtomicOp::Min(value) => update(self, value, 0x39, Some(if atomic.scalar.is_signed() { 0x4f } else { 0x47 })),
            AtomicOp::Max(value) => update(self, value, 0x39, Some(if atomic.scalar.is_signed() { 0x4c } else { 0x42 })),
        }
        if let Some(result) = atomic.result { self.store(result); }
    }

}
