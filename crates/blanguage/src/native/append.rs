//! 单次有界接纳直接使用原计数和输出地址；不暴露尚未写完的票号。
use super::{operands::Operand, *};
impl Emitter<'_> {
    pub(super) fn append(&mut self, a: &Append) {
        self.instruction(
            None,
            &[0x8b],
            true,
            15,
            Operand::Memory {
                base: 12,
                offset: a.count.binding as u32 * 8,
            },
        );
        let counter = Operand::Memory {
            base: 15,
            offset: a.count.word * 4,
        };
        self.instruction(
            None,
            &[0x8b],
            false,
            1,
            Operand::Memory {
                base: 13,
                offset: 8 + a.target as u32 * 4,
            },
        );
        self.instruction(None, &[0x8b], false, 0, counter);
        let begin = self.code.len();
        self.bytes(&[0x39, 0xc8]); // old >= capacity 时不修改计数
        let full = self.jump(Some(0x83));
        self.bytes(&[0x8d, 0x50, 0x01]); // edx = old + 1；old < capacity 保证不回绕
        self.instruction(Some(0xf0), &[0x0f, 0xb1], false, 2, counter);
        let again = self.jump(Some(0x85));
        self.patch(again, begin);
        // EAX 是成功预留的原计数，直接形成唯一记录地址。
        self.instruction(
            None,
            &[0x8b],
            true,
            15,
            Operand::Memory {
                base: 12,
                offset: a.target as u32 * 8,
            },
        );
        self.offset_address(self.program.buffers[a.target].stride);
        for (word, value) in &a.values {
            self.load(*value);
            self.instruction(
                None,
                &[0x89],
                false,
                0,
                Operand::Memory {
                    base: 15,
                    offset: word * 4,
                },
            );
        }
        self.bytes(&[0xb8, 1, 0, 0, 0]);
        let done = self.jump(None);
        self.patch(full, self.code.len());
        self.bytes(&[0x31, 0xc0]);
        self.patch(done, self.code.len());
        self.store(a.accepted);
    }
}
