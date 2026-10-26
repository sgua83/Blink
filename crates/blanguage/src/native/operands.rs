//! 指令直接消费分配位置；两地址写入保留右操作数，位复制不改变标量数值。
use super::*;
use allocation::Location;

pub(super) fn bit_copy(from: Scalar, into: Scalar) -> bool {
    from == into
        || matches!(from, Scalar::U32 | Scalar::I32 | Scalar::Bool)
            && matches!(into, Scalar::U32 | Scalar::I32)
}

#[derive(Clone, Copy)]
pub(super) struct Memory {
    base: u8,
    index: Option<(u8, u8)>,
    offset: u32,
}
impl Memory {
    pub(super) fn new(base: u8, offset: u32) -> Self {
        Self {
            base,
            index: None,
            offset,
        }
    }
    pub(super) fn offset(mut self, offset: u32) -> Self {
        self.offset += offset;
        self
    }
    fn uses_register(self, register: u8) -> bool {
        self.base == register || self.index.is_some_and(|(index, _)| index == register)
    }
    fn temporary_register(self) -> u8 {
        if !self.uses_register(0) {
            0
        } else if !self.uses_register(1) {
            1
        } else {
            2
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Operand {
    Register(u8),
    Memory(Memory),
}
impl Operand {
    pub(super) fn memory(base: u8, offset: u32) -> Self {
        Self::Memory(Memory::new(base, offset))
    }
}
impl From<Memory> for Operand {
    fn from(memory: Memory) -> Self {
        Self::Memory(memory)
    }
}
impl Emitter<'_> {
    fn operand(&mut self, location: Location, temporary: u8) -> Operand {
        match location {
            Location::Gpr(r) | Location::Xmm(r) => Operand::Register(r),
            Location::Stack(offset) => Operand::memory(4, offset),
            Location::Immediate(bits) => {
                self.literal(Location::Gpr(temporary), bits);
                Operand::Register(temporary)
            }
        }
    }
    pub(super) fn value_memory(
        &mut self,
        base: u8,
        index: Location,
        stride: u32,
        offset: u32,
    ) -> Memory {
        let shift = stride.trailing_zeros().min(3) as u8;
        if stride >> shift == 1
            && let Location::Gpr(index) = index
        {
            Memory {
                base,
                index: Some((index, shift)),
                offset,
            }
        } else {
            self.load_location(index);
            self.scaled_memory(base, 0, stride, offset)
        }
    }
    pub(super) fn scaled_memory(
        &mut self,
        base: u8,
        index: u8,
        stride: u32,
        offset: u32,
    ) -> Memory {
        let shift = stride.trailing_zeros().min(3) as u8;
        self.scale_index(index, stride >> shift);
        Memory {
            base,
            index: Some((index, shift)),
            offset,
        }
    }
    pub(super) fn scale_index(&mut self, register: u8, stride: u32) {
        if stride == 1 {
            return;
        }
        if stride.is_power_of_two() {
            self.instruction(None, &[0xc1], true, 4, Operand::Register(register));
            self.bytes(&[stride.trailing_zeros() as u8]);
        } else {
            self.instruction(None, &[0x69], true, register, Operand::Register(register));
            self.u32(stride);
        }
    }
    // R15 holds the base, RAX the zero-extended index; only the address is consumed.
    pub(super) fn offset_address(&mut self, stride: u32) {
        if stride.is_power_of_two() && stride <= 8 {
            self.bytes(&[
                0x4d,
                0x8d,
                0x3c,
                0x07 | ((stride.trailing_zeros() as u8) << 6),
            ]);
        } else {
            self.scale_index(0, stride);
            self.bytes(&[0x49, 0x01, 0xc7]);
        }
    }
    fn rex(&mut self, wide: bool, r: u8, b: u8, index: u8) {
        let prefix =
            0x40 | (u8::from(wide) << 3) | ((r >> 3) << 2) | ((index >> 3) << 1) | (b >> 3);
        if prefix != 0x40 {
            self.bytes(&[prefix]);
        }
    }
    pub(super) fn instruction(
        &mut self,
        prefix: Option<u8>,
        opcode: &[u8],
        wide: bool,
        register: u8,
        operand: Operand,
    ) {
        if let Some(prefix) = prefix {
            self.bytes(&[prefix]);
        }
        let field = (register & 7) << 3;
        match operand {
            Operand::Register(r) => {
                self.rex(wide, register, r, 0);
                self.bytes(opcode);
                self.bytes(&[0xc0 | field | (r & 7)]);
            }
            Operand::Memory(Memory {
                base,
                index,
                offset,
            }) => {
                self.rex(wide, register, base, index.map_or(0, |(r, _)| r));
                self.bytes(opcode);
                let mode = if offset == 0 && base & 7 != 5 {
                    0
                } else if offset <= 127 {
                    1
                } else {
                    2
                };
                let indexed = index.is_some() || base & 7 == 4;
                self.bytes(&[(mode << 6) | field | if indexed { 4 } else { base & 7 }]);
                if let Some((r, shift)) = index {
                    self.bytes(&[(shift << 6) | ((r & 7) << 3) | (base & 7)]);
                } else if indexed {
                    self.bytes(&[0x24]);
                }
                match mode {
                    0 => {}
                    1 => self.bytes(&[offset as u8]),
                    _ => self.u32(offset),
                }
            }
        }
    }
    pub(super) fn location(&self, r: Reg) -> Location {
        self.allocation.location(PrivateSlot::Register(r))
    }
    pub(super) fn copy(&mut self, dst: Location, src: Location) {
        if dst == src {
            return;
        }
        match (dst, src) {
            // A literal has no writable storage, including phase restoration.
            (Location::Immediate(_), _) => {}
            (_, Location::Immediate(bits)) => self.literal(dst, bits),
            (Location::Gpr(d), Location::Gpr(_) | Location::Stack(_)) => {
                let operand = self.operand(src, 1);
                self.instruction(None, &[0x8b], false, d, operand);
            }
            (Location::Stack(_), Location::Gpr(s)) => {
                let operand = self.operand(dst, 1);
                self.instruction(None, &[0x89], false, s, operand);
            }
            (Location::Xmm(d), Location::Xmm(_)) => {
                let operand = self.operand(src, 1);
                self.instruction(None, &[0x0f, 0x28], false, d, operand);
            }
            (Location::Xmm(d), Location::Stack(_)) => {
                let operand = self.operand(src, 1);
                self.instruction(Some(0xf3), &[0x0f, 0x10], false, d, operand);
            }
            (Location::Stack(_), Location::Xmm(s)) => {
                let operand = self.operand(dst, 1);
                self.instruction(Some(0xf3), &[0x0f, 0x11], false, s, operand);
            }
            (Location::Xmm(d), Location::Gpr(_)) => {
                let operand = self.operand(src, 1);
                self.instruction(Some(0x66), &[0x0f, 0x6e], false, d, operand);
            }
            (Location::Gpr(d), Location::Xmm(s)) => {
                self.instruction(Some(0x66), &[0x0f, 0x7e], false, s, Operand::Register(d));
            }
            (Location::Stack(_), Location::Stack(_)) => {
                self.copy(Location::Gpr(0), src);
                self.copy(dst, Location::Gpr(0));
            }
        }
    }
    pub(super) fn load_location(&mut self, src: Location) {
        self.copy(Location::Gpr(0), src);
    }
    pub(super) fn store_location(&mut self, dst: Location) {
        self.copy(dst, Location::Gpr(0));
    }
    pub(super) fn load(&mut self, r: Reg) {
        self.load_location(self.location(r));
    }
    pub(super) fn store(&mut self, r: Reg) {
        self.store_location(self.location(r));
    }
    pub(super) fn float(&mut self, r: Reg, xmm: u8) {
        self.copy(Location::Xmm(xmm), self.location(r));
    }
    pub(super) fn store_float(&mut self, r: Reg) {
        self.copy(self.location(r), Location::Xmm(0));
    }
    pub(super) fn constant(&mut self, r: Reg, bits: u32) {
        self.literal(self.location(r), bits);
    }
    fn literal(&mut self, dst: Location, bits: u32) {
        match dst {
            Location::Immediate(_) => {}
            Location::Gpr(d) => {
                self.rex(false, 0, d, 0);
                self.bytes(&[0xb8 | (d & 7)]);
                self.u32(bits);
            }
            dst @ Location::Stack(_) => {
                let operand = self.operand(dst, 1);
                self.instruction(None, &[0xc7], false, 0, operand);
                self.u32(bits);
            }
            dst @ Location::Xmm(_) => {
                self.bytes(&[0xb8]);
                self.u32(bits);
                self.copy(dst, Location::Gpr(0));
            }
        }
    }
    pub(super) fn read_address(&mut self, dst: Location, memory: Memory) {
        let source = memory.into();
        match dst {
            Location::Gpr(d) => self.instruction(None, &[0x8b], false, d, source),
            Location::Xmm(d) => self.instruction(Some(0xf3), &[0x0f, 0x10], false, d, source),
            Location::Stack(_) | Location::Immediate(_) => {
                let temporary = memory.temporary_register();
                self.instruction(None, &[0x8b], false, temporary, source);
                self.copy(dst, Location::Gpr(temporary));
            }
        }
    }
    pub(super) fn read_scalar(&mut self, dst: Reg, base: u8, offset: u32) {
        self.read_scalar_address(dst, Memory::new(base, offset));
    }
    pub(super) fn read_scalar_address(&mut self, dst: Reg, memory: Memory) {
        if self.stage.registers[dst] == Scalar::Bool {
            let temporary = memory.temporary_register();
            self.instruction(None, &[0x83], false, 7, memory.into());
            self.bytes(&[0, 0x0f, 0x95, 0xc0 | temporary]);
            self.instruction(
                None,
                &[0x0f, 0xb6],
                false,
                temporary,
                Operand::Register(temporary),
            );
            self.copy(self.location(dst), Location::Gpr(temporary));
        } else {
            self.read_address(self.location(dst), memory);
        }
    }
    pub(super) fn write_memory(&mut self, src: Location, base: u8, offset: u32) {
        self.write_address(src, Memory::new(base, offset));
    }
    pub(super) fn write_address(&mut self, src: Location, memory: Memory) {
        let target = memory.into();
        match src {
            Location::Immediate(bits) => {
                self.instruction(None, &[0xc7], false, 0, target);
                self.u32(bits);
            }
            Location::Gpr(s) => self.instruction(None, &[0x89], false, s, target),
            Location::Xmm(s) => self.instruction(Some(0xf3), &[0x0f, 0x11], false, s, target),
            Location::Stack(_) => {
                let temporary = memory.temporary_register();
                self.copy(Location::Gpr(temporary), src);
                self.instruction(None, &[0x89], false, temporary, target);
            }
        }
    }
    pub(super) fn float_binary(&mut self, dst: Reg, a: Reg, b: Reg, opcode: u8) {
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        // legacy SSE 是两地址指令：目标若占了尚未读取的右值，先在 XMM0 计算。
        let target = match d {
            Location::Xmm(r) if d != b || a == b => r,
            _ => 0,
        };
        self.copy(Location::Xmm(target), a);
        let right = self.operand(b, 1);
        self.instruction(Some(0xf3), &[0x0f, opcode], false, target, right);
        self.copy(d, Location::Xmm(target));
    }
    pub(super) fn integer_binary(
        &mut self,
        dst: Reg,
        a: Reg,
        b: Reg,
        opcode: &[u8],
        immediate: Option<u8>,
    ) {
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        let target = match d {
            Location::Gpr(r) if d != b || a == b => r,
            _ => 0,
        };
        self.copy(Location::Gpr(target), a);
        if let Location::Immediate(bits) = b {
            if let Some(extension) = immediate {
                self.integer_immediate(extension, target, bits);
            } else {
                self.instruction(None, &[0x69], false, target, Operand::Register(target));
                self.u32(bits);
            }
        } else {
            let right = self.operand(b, 1);
            self.instruction(None, opcode, false, target, right);
        }
        self.copy(d, Location::Gpr(target));
    }
    fn integer_immediate(&mut self, extension: u8, target: u8, bits: u32) {
        if i8::try_from(bits as i32).is_ok() {
            self.instruction(None, &[0x83], false, extension, Operand::Register(target));
            self.bytes(&[bits as u8]);
        } else {
            self.instruction(None, &[0x81], false, extension, Operand::Register(target));
            self.u32(bits);
        }
    }
    pub(super) fn float_sqrt(&mut self, dst: Reg, source: Reg) {
        let d = self.location(dst);
        let target = match d {
            Location::Xmm(r) => r,
            _ => 0,
        };
        let source = self.operand(self.location(source), 1);
        self.instruction(Some(0xf3), &[0x0f, 0x51], false, target, source);
        self.copy(d, Location::Xmm(target));
    }
    pub(super) fn integer_unary(
        &mut self,
        dst: Reg,
        source: Reg,
        opcode: u8,
        extension: u8,
        immediate: Option<u8>,
    ) {
        let (d, source) = (self.location(dst), self.location(source));
        let target = match d {
            Location::Gpr(_) => d,
            Location::Stack(_) if d == source => d,
            _ => Location::Gpr(0),
        };
        self.copy(target, source);
        let operand = self.operand(target, 1);
        self.instruction(None, &[opcode], false, extension, operand);
        if let Some(value) = immediate {
            self.bytes(&[value]);
        }
        self.copy(d, target);
    }
    pub(super) fn shift(&mut self, dst: Reg, a: Reg, b: Reg, kind: u8) {
        let shift = self.location(b);
        if !matches!(shift, Location::Immediate(_)) {
            self.copy(Location::Gpr(1), shift);
        }
        let d = self.location(dst);
        let target = match d {
            Location::Gpr(r) => r,
            _ => 0,
        };
        self.copy(Location::Gpr(target), self.location(a));
        if let Location::Immediate(bits) = shift {
            self.instruction(None, &[0xc1], false, kind, Operand::Register(target));
            self.bytes(&[(bits & 31) as u8]);
        } else {
            self.instruction(None, &[0xd3], false, kind, Operand::Register(target));
        }
        self.copy(d, Location::Gpr(target));
    }
    pub(super) fn compare(&mut self, dst: Reg, a: Reg, b: Reg, comparison: Comparison) {
        let scalar = self.stage.registers[a];
        let (a, b) = (self.location(a), self.location(b));
        if scalar == Scalar::F32 {
            let left = match a {
                Location::Xmm(r) => r,
                _ => 0,
            };
            self.copy(Location::Xmm(left), a);
            let right = self.operand(b, 1);
            self.instruction(None, &[0x0f, 0x2e], false, left, right);
        } else {
            let left = match a {
                Location::Gpr(r) => r,
                _ => 0,
            };
            self.copy(Location::Gpr(left), a);
            if let Location::Immediate(bits) = b {
                self.integer_immediate(7, left, bits);
            } else {
                let right = self.operand(b, 1);
                self.instruction(None, &[0x3b], false, left, right);
            }
        }
        self.comparison_result(dst, comparison, scalar);
    }
    pub(super) fn select(&mut self, dst: Reg, condition: Reg, a: Reg, b: Reg) {
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        if let Location::Immediate(bits) = self.location(condition) {
            self.copy(d, if bits != 0 { a } else { b });
            return;
        }
        let condition = self.operand(self.location(condition), 2);
        self.instruction(None, &[0x83], false, 7, condition);
        self.bytes(&[0]);
        let target = match d {
            Location::Gpr(r) => r,
            _ => 0,
        };
        // 目标已有真值时改用 CMOVZ，避免先写入假值破坏尚未读取的真值。
        let (initial, source, opcode) = if d == a { (a, b, 0x44) } else { (b, a, 0x45) };
        self.copy(Location::Gpr(target), initial);
        let source = match source {
            Location::Xmm(_) | Location::Immediate(_) => {
                self.copy(Location::Gpr(1), source);
                Operand::Register(1)
            }
            location => self.operand(location, 1),
        };
        self.instruction(None, &[0x0f, opcode], false, target, source);
        self.copy(d, Location::Gpr(target));
    }
    pub(super) fn preserve_registers(&mut self, restore: bool) {
        for (r, offset) in self.frame.saved_gpr.clone() {
            self.instruction(
                None,
                &[if restore { 0x8b } else { 0x89 }],
                true,
                r,
                Operand::memory(4, offset),
            );
        }
        for (r, offset) in self.frame.saved_xmm.clone() {
            self.instruction(
                None,
                &[0x0f, if restore { 0x28 } else { 0x29 }],
                false,
                r,
                Operand::memory(4, offset),
            );
        }
    }
}
