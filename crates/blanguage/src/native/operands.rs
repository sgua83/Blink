//! 指令直接消费分配位置；两地址写入保留右操作数，位复制不改变标量数值。
use super::*;
use allocation::Location;

pub(super) fn bit_copy(from: Scalar, into: Scalar) -> bool {
    from == into
        || from.is_integer() && into.is_integer() && from.size() == into.size()
        || from == Scalar::Bool && matches!(into, Scalar::U32 | Scalar::I32)
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
    pub(super) fn indexed(base: u8, index: u8, shift: u8, offset: u32) -> Self {
        Self { base, index: Some((index, shift)), offset }
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
    Literal(Bits, bool),
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
    fn float_operand(&mut self, location: Location, scalar: Scalar) -> Operand {
        match location {
            Location::Immediate(bits) => Operand::Literal(bits, scalar.size() == 8),
            location => self.operand(location, 1),
        }
    }
    pub(super) fn operand(&mut self, location: Location, temporary: u8) -> Operand {
        match location {
            Location::Gpr(r) | Location::Xmm(r) => Operand::Register(r),
            Location::Stack(offset) => Operand::memory(4, offset),
            Location::Immediate(bits) => {
                self.literal_width(Location::Gpr(temporary), bits, bits > u32::MAX as Bits);
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
    fn rex(&mut self, wide: bool, r: u8, b: u8, index: u8, byte: bool) {
        let prefix =
            0x40 | (u8::from(wide) << 3) | ((r >> 3) << 2) | ((index >> 3) << 1) | (b >> 3);
        if prefix != 0x40 || byte {
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
        let byte = matches!(opcode, [0x88] | [0x8a] | [0x0f, 0xb6] | [0x0f, 0xbe]);
        match operand {
            Operand::Literal(bits, wide) => {
                // Scalar literal reads end at disp32: ModRM 00/r/101 is RIP-relative.
                self.rex(wide, register, 0, 0, byte);
                self.bytes(opcode);
                self.bytes(&[field | 5]);
                let at = self.code.len();
                self.u32(0);
                self.literals.entry((bits, wide)).or_default().push(at);
            }
            Operand::Register(r) => {
                self.rex(wide, register, r, 0, byte);
                self.bytes(opcode);
                self.bytes(&[0xc0 | field | (r & 7)]);
            }
            Operand::Memory(Memory {
                base,
                index,
                offset,
            }) => {
                self.rex(wide, register, base, index.map_or(0, |(r, _)| r), byte);
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
        self.copy_width(dst, src, false);
    }
    pub(super) fn copy_scalar(&mut self, dst: Location, src: Location, scalar: Scalar) {
        if scalar.size() >= 4 {
            self.copy_width(dst, src, scalar.size() == 8);
            return;
        }
        if matches!(dst, Location::Immediate(_)) { return; }
        let target = match dst { Location::Gpr(r) => r, _ => 0 };
        if let Location::Immediate(bits) = src {
            let bits = if scalar.is_signed() { scalar.signed(bits) as u32 as Bits } else { scalar.truncate(bits) };
            self.literal_width(Location::Gpr(target), bits, false);
        } else {
            let operand = self.operand(src, 1);
            let opcode = if scalar.is_signed() { 0xbe } else { 0xb6 } + u8::from(scalar.size() == 2);
            self.instruction(None, &[0x0f, opcode], false, target, operand);
        }
        if let Location::Stack(offset) = dst {
            self.write_value_address(Location::Gpr(target), Memory::new(4, offset), scalar.size());
        }
    }
    fn copy_width(&mut self, dst: Location, src: Location, wide: bool) {
        if dst == src {
            return;
        }
        match (dst, src) {
            // A literal has no writable storage, including phase restoration.
            (Location::Immediate(_), _) => {}
            (_, Location::Immediate(bits)) => self.literal_width(dst, bits, wide),
            (Location::Gpr(d), Location::Gpr(_) | Location::Stack(_)) => {
                let operand = self.operand(src, 1);
                self.instruction(None, &[0x8b], wide, d, operand);
            }
            (Location::Stack(_), Location::Gpr(s)) => {
                let operand = self.operand(dst, 1);
                self.instruction(None, &[0x89], wide, s, operand);
            }
            (Location::Xmm(d), Location::Xmm(_)) => {
                let operand = self.operand(src, 1);
                self.instruction(None, &[0x0f, 0x28], false, d, operand);
            }
            (Location::Xmm(d), Location::Stack(_)) => {
                let operand = self.operand(src, 1);
                self.instruction(Some(0xf3), &[0x0f, if wide { 0x7e } else { 0x10 }], false, d, operand);
            }
            (Location::Stack(_), Location::Xmm(s)) => {
                let operand = self.operand(dst, 1);
                self.instruction(Some(if wide { 0x66 } else { 0xf3 }), &[0x0f, if wide { 0xd6 } else { 0x11 }], false, s, operand);
            }
            (Location::Xmm(d), Location::Gpr(_)) => {
                let operand = self.operand(src, 1);
                self.instruction(Some(0x66), &[0x0f, 0x6e], wide, d, operand);
            }
            (Location::Gpr(d), Location::Xmm(s)) => {
                self.instruction(Some(0x66), &[0x0f, 0x7e], wide, s, Operand::Register(d));
            }
            (Location::Stack(_), Location::Stack(_)) => {
                self.copy_width(Location::Gpr(0), src, wide);
                self.copy_width(dst, Location::Gpr(0), wide);
            }
        }
    }
    pub(super) fn load_location(&mut self, src: Location) {
        self.copy(Location::Gpr(0), src);
    }
    pub(super) fn load(&mut self, r: Reg) {
        self.copy_scalar(Location::Gpr(0), self.location(r), self.stage.registers[r]);
    }
    pub(super) fn store(&mut self, r: Reg) {
        self.copy_scalar(self.location(r), Location::Gpr(0), self.stage.registers[r]);
    }
    pub(super) fn float(&mut self, r: Reg, xmm: u8) {
        self.copy_scalar(Location::Xmm(xmm), self.location(r), self.stage.registers[r]);
    }
    pub(super) fn store_float(&mut self, r: Reg) {
        self.copy_scalar(self.location(r), Location::Xmm(0), self.stage.registers[r]);
    }
    pub(super) fn constant(&mut self, r: Reg, bits: Bits) {
        self.copy_scalar(self.location(r), Location::Immediate(bits), self.stage.registers[r]);
    }
    pub(super) fn literal_width(&mut self, dst: Location, bits: Bits, wide: bool) {
        match dst {
            Location::Immediate(_) => {}
            Location::Gpr(d) => {
                let immediate64 = wide && bits > u32::MAX as Bits;
                self.rex(immediate64, 0, d, 0, false);
                self.bytes(&[0xb8 | (d & 7)]);
                if immediate64 { self.bytes(&bits.to_le_bytes()); } else { self.u32(bits as u32); }
            }
            dst @ Location::Stack(_) => {
                if wide {
                    self.literal_width(Location::Gpr(0), bits, true);
                    let operand = self.operand(dst, 1);
                    self.instruction(None, &[0x89], true, 0, operand);
                } else {
                    let operand = self.operand(dst, 1);
                    self.instruction(None, &[0xc7], false, 0, operand);
                    self.u32(bits as u32);
                }
            }
            Location::Xmm(d) => {
                if wide {
                    self.literal_width(Location::Gpr(0), bits, true);
                    self.instruction(Some(0x66), &[0x0f, 0x6e], true, d, Operand::Register(0));
                } else {
                    self.instruction(Some(0xf3), &[0x0f, 0x10], false, d, Operand::Literal(bits, false));
                }
            }
        }
    }
    fn read_value_address(&mut self, dst: Location, memory: Memory, wide: bool) {
        let source = memory.into();
        match dst {
            Location::Gpr(d) => self.instruction(None, &[0x8b], wide, d, source),
            Location::Xmm(d) => self.instruction(Some(0xf3), &[0x0f, if wide { 0x7e } else { 0x10 }], false, d, source),
            Location::Stack(_) | Location::Immediate(_) => {
                let temporary = memory.temporary_register();
                self.instruction(None, &[0x8b], wide, temporary, source);
                self.copy_width(dst, Location::Gpr(temporary), wide);
            }
        }
    }
    pub(super) fn read_scalar(&mut self, dst: Reg, base: u8, offset: u32) {
        self.read_scalar_address(dst, Memory::new(base, offset));
    }
    pub(super) fn read_scalar_address(&mut self, dst: Reg, memory: Memory) {
        let scalar = self.stage.registers[dst];
        if scalar.size() < 4 {
            let target = match self.location(dst) { Location::Gpr(r) => r, _ => memory.temporary_register() };
            let opcode = if scalar.is_signed() { 0xbe } else { 0xb6 } + u8::from(scalar.size() == 2);
            self.instruction(None, &[0x0f, opcode], false, target, memory.into());
            self.copy_scalar(self.location(dst), Location::Gpr(target), scalar);
        } else if scalar == Scalar::Bool {
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
            self.read_value_address(self.location(dst), memory, self.stage.registers[dst].size() == 8);
        }
    }
    pub(super) fn write_memory(&mut self, src: Location, base: u8, offset: u32) {
        self.write_address(src, Memory::new(base, offset));
    }
    pub(super) fn write_address(&mut self, src: Location, memory: Memory) {
        self.write_value_address(src, memory, 4);
    }
    pub(super) fn write_scalar(&mut self, src: Reg, base: u8, offset: u32) {
        self.write_scalar_address(src, Memory::new(base, offset));
    }
    pub(super) fn write_scalar_address(&mut self, src: Reg, memory: Memory) {
        self.write_value_address(self.location(src), memory, self.stage.registers[src].size());
    }
    pub(super) fn write_value_address(&mut self, src: Location, memory: Memory, size: u32) {
        let target = memory.into();
        let wide = size == 8;
        let prefix = (size == 2).then_some(0x66);
        let opcode = if size == 1 { 0x88 } else { 0x89 };
        match src {
            Location::Immediate(bits) => {
                if wide {
                    let temporary = memory.temporary_register();
                    self.literal_width(Location::Gpr(temporary), bits, true);
                    self.instruction(None, &[0x89], true, temporary, target);
                } else {
                    self.instruction(prefix, &[if size == 1 { 0xc6 } else { 0xc7 }], false, 0, target);
                    self.bytes(&bits.to_le_bytes()[..size as usize]);
                }
            }
            Location::Gpr(s) => self.instruction(prefix, &[opcode], wide, s, target),
            Location::Xmm(s) if size < 4 => {
                let temporary = memory.temporary_register();
                self.instruction(Some(0x66), &[0x0f, 0x7e], false, s, Operand::Register(temporary));
                self.instruction(prefix, &[opcode], false, temporary, target);
            }
            Location::Xmm(s) => self.instruction(Some(if wide { 0x66 } else { 0xf3 }), &[0x0f, if wide { 0xd6 } else { 0x11 }], false, s, target),
            Location::Stack(_) => {
                let temporary = memory.temporary_register();
                let source = self.operand(src, 1);
                let load: &[u8] = match size { 1 => &[0x0f, 0xb6], 2 => &[0x0f, 0xb7], _ => &[0x8b] };
                self.instruction(None, load, wide, temporary, source);
                self.instruction(prefix, &[opcode], wide, temporary, target);
            }
        }
    }
    pub(super) fn float_binary(&mut self, dst: Reg, a: Reg, b: Reg, opcode: u8) {
        let scalar = self.stage.registers[dst];
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        // legacy SSE 是两地址指令：目标若占了尚未读取的右值，先在 XMM0 计算。
        let target = match d {
            Location::Xmm(r) if d != b || a == b => r,
            _ => 0,
        };
        self.copy_scalar(Location::Xmm(target), a, scalar);
        let right = self.float_operand(b, scalar);
        self.instruction(Some(if scalar == Scalar::F64 { 0xf2 } else { 0xf3 }), &[0x0f, opcode], false, target, right);
        self.copy_scalar(d, Location::Xmm(target), self.stage.registers[dst]);
    }
    pub(super) fn integer_binary(
        &mut self,
        dst: Reg,
        a: Reg,
        b: Reg,
        opcode: &[u8],
        immediate: Option<u8>,
    ) {
        let scalar = self.stage.registers[dst];
        let wide = scalar.size() == 8;
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        let target = match d {
            Location::Gpr(r) if d != b || a == b => r,
            _ => 0,
        };
        self.copy_scalar(Location::Gpr(target), a, scalar);
        if scalar.size() < 4 {
            self.copy_scalar(Location::Gpr(1), b, scalar);
            self.instruction(None, opcode, false, target, Operand::Register(1));
        } else if let Location::Immediate(bits) = b {
            if wide && bits as i64 != bits as i32 as i64 {
                self.literal_width(Location::Gpr(1), bits, true);
                self.instruction(None, opcode, true, target, Operand::Register(1));
            } else if let Some(extension) = immediate {
                self.integer_immediate(extension, target, bits, wide);
            } else {
                self.instruction(None, &[0x69], wide, target, Operand::Register(target));
                self.u32(bits as u32);
            }
        } else {
            let right = self.operand(b, 1);
            self.instruction(None, opcode, wide, target, right);
        }
        self.copy_scalar(d, Location::Gpr(target), scalar);
    }
    fn integer_immediate(&mut self, extension: u8, target: u8, bits: Bits, wide: bool) {
        let signed = if wide { bits as i64 } else { bits as i32 as i64 };
        if i8::try_from(signed).is_ok() {
            self.instruction(None, &[0x83], wide, extension, Operand::Register(target));
            self.bytes(&[bits as u8]);
        } else {
            self.instruction(None, &[0x81], wide, extension, Operand::Register(target));
            self.u32(bits as u32);
        }
    }
    pub(super) fn cast(&mut self, dst: Reg, src: Reg) {
        let from = self.stage.registers[src];
        let into = self.stage.registers[dst];
        let (d, source) = (self.location(dst), self.location(src));
        if from == Scalar::U64 && into.is_float() {
            self.unsigned64_to_float(dst, src);
        } else if from.is_float() && into == Scalar::U64 {
            self.float_to_unsigned64(dst, src);
        } else if from != into && from.is_float() && into.is_float() {
            let target = match d { Location::Xmm(r) => r, _ => 0 };
            let operand = self.float_operand(source, from);
            self.instruction(Some(if from == Scalar::F64 { 0xf2 } else { 0xf3 }), &[0x0f, 0x5a], false, target, operand);
            self.copy_scalar(d, Location::Xmm(target), into);
        } else if bit_copy(from, into) {
            self.copy_scalar(d, source, into);
        } else if into == Scalar::Bool {
            if from.is_float() {
                let left = match source {
                    Location::Xmm(r) => r,
                    _ => 0,
                };
                self.copy_scalar(Location::Xmm(left), source, from);
                self.bytes(&[0x0f, 0x57, 0xc9]);
                self.instruction(if from == Scalar::F64 { Some(0x66) } else { None }, &[0x0f, 0x2e], false, left, Operand::Register(1));
            } else {
                let source = if from.size() < 4 {
                    self.copy_scalar(Location::Gpr(0), source, from);
                    Operand::Register(0)
                } else { self.operand(source, 1) };
                self.instruction(None, &[0x83], from.size() == 8, 7, source);
                self.bytes(&[0]);
            }
            self.comparison_result(dst, Comparison::NotEqual, from);
        } else if !from.is_float() && !into.is_float() {
            let target = match d { Location::Gpr(r) => r, _ => 0 };
            let register = Location::Gpr(target);
            if into.size() <= 4 {
                self.copy_scalar(register, source, if from.size() < 4 { from } else { into });
                if into.size() == 4 && from.size() >= 4 && source == register {
                    // A narrowing cast produces a real 32-bit value, including
                    // the zeroed upper half when allocation reuses the input.
                    self.instruction(None, &[0x8b], false, target, Operand::Register(target));
                }
            } else {
                self.copy_scalar(register, source, from);
                if from.is_signed() && from.size() < 8 {
                    self.instruction(None, &[0x63], true, target, Operand::Register(target));
                }
            }
            self.copy_scalar(d, register, into);
        } else if from.is_float() {
            let target = match d {
                Location::Gpr(r) => r,
                _ => 0,
            };
            let source = self.float_operand(source, from);
            self.instruction(
                Some(if from == Scalar::F64 { 0xf2 } else { 0xf3 }),
                &[0x0f, 0x2c],
                into == Scalar::U32 || into.size() == 8,
                target,
                source,
            );
            if into == Scalar::U32 {
                self.instruction(None, &[0x8b], false, target, Operand::Register(target));
            }
            self.copy_scalar(d, Location::Gpr(target), into);
        } else {
            let target = match d {
                Location::Xmm(r) => r,
                _ => 0,
            };
            let source = if from.size() < 4 || from == Scalar::U32 && matches!(source, Location::Stack(_)) {
                // 无符号转换读取零扩展的 64 位整数；栈值只有四字节。
                self.copy_scalar(Location::Gpr(0), source, from);
                Operand::Register(0)
            } else {
                // GPR 的 32 位生产者已清零高位，bool 则只有规范的 0／1。
                self.operand(source, 1)
            };
            // CVTSI2SS 保留目标高 96 位，不让旧浮点结果成为新转换的依赖。
            self.instruction(
                None,
                &[0x0f, 0x57],
                false,
                target,
                Operand::Register(target),
            );
            self.instruction(
                Some(if into == Scalar::F64 { 0xf2 } else { 0xf3 }),
                &[0x0f, 0x2a],
                from == Scalar::U32 || from.size() == 8,
                target,
                source,
            );
            self.copy_scalar(d, Location::Xmm(target), self.stage.registers[dst]);
        }
    }
    fn unsigned64_to_float(&mut self, dst: Reg, src: Reg) {
        let location = self.location(dst);
        let prefix = if self.stage.registers[dst] == Scalar::F64 { 0xf2 } else { 0xf3 };
        let target = match location { Location::Xmm(r) => r, _ => 0 };
        self.load(src);
        self.bytes(&[0x48, 0x85, 0xc0]);
        let low = self.jump(Some(0x89));
        // The low bit supplies the sticky bit before the signed conversion.
        // Rounding the half value to odd preserves the final floating-point rounding.
        self.bytes(&[0x48, 0x89, 0xc1, 0x83, 0xe1, 1, 0x48, 0xd1, 0xe8, 0x48, 0x09, 0xc8]);
        self.instruction(None, &[0x0f, 0x57], false, target, Operand::Register(target));
        self.instruction(Some(prefix), &[0x0f, 0x2a], true, target, Operand::Register(0));
        self.instruction(Some(prefix), &[0x0f, 0x58], false, target, Operand::Register(target));
        let end = self.jump(None);
        self.patch(low, self.code.len());
        self.instruction(None, &[0x0f, 0x57], false, target, Operand::Register(target));
        self.instruction(Some(prefix), &[0x0f, 0x2a], true, target, Operand::Register(0));
        self.patch(end, self.code.len());
        self.copy_scalar(location, Location::Xmm(target), self.stage.registers[dst]);
    }
    fn float_to_unsigned64(&mut self, dst: Reg, src: Reg) {
        let location = self.location(dst);
        let prefix = if self.stage.registers[src] == Scalar::F64 { 0xf2 } else { 0xf3 };
        let target = match location { Location::Gpr(r) => r, _ => 0 };
        self.float(src, 0);
        self.literal_width(Location::Xmm(1), if prefix == 0xf2 { (2.0f64.powi(63)).to_bits() } else { 0x5f000000 }, prefix == 0xf2);
        self.instruction(if prefix == 0xf2 { Some(0x66) } else { None }, &[0x0f, 0x2e], false, 0, Operand::Register(1));
        let low = self.jump(Some(0x82));
        self.instruction(Some(prefix), &[0x0f, 0x5c], false, 0, Operand::Register(1));
        self.instruction(Some(prefix), &[0x0f, 0x2c], true, target, Operand::Register(0));
        self.instruction(None, &[0x0f, 0xba], true, 5, Operand::Register(target));
        self.bytes(&[63]);
        let end = self.jump(None);
        self.patch(low, self.code.len());
        self.instruction(Some(prefix), &[0x0f, 0x2c], true, target, Operand::Register(0));
        self.patch(end, self.code.len());
        self.copy_scalar(location, Location::Gpr(target), Scalar::U64);
    }
    pub(super) fn float_sign(&mut self, dst: Reg, src: Reg, negate: bool) {
        let scalar = self.stage.registers[src];
        self.load(src);
        self.instruction(None, &[0x0f, 0xba], scalar.size() == 8, if negate { 7 } else { 6 }, Operand::Register(0));
        self.bytes(&[(scalar.bit_width()-1) as u8]);
        self.store(dst);
    }
    pub(super) fn float_sqrt(&mut self, dst: Reg, source: Reg) {
        let d = self.location(dst);
        let target = match d {
            Location::Xmm(r) => r,
            _ => 0,
        };
        let source = self.float_operand(self.location(source), self.stage.registers[source]);
        self.instruction(Some(if self.stage.registers[dst] == Scalar::F64 { 0xf2 } else { 0xf3 }), &[0x0f, 0x51], false, target, source);
        self.copy_scalar(d, Location::Xmm(target), self.stage.registers[dst]);
    }
    pub(super) fn integer_unary(
        &mut self,
        dst: Reg,
        source: Reg,
        opcode: u8,
        extension: u8,
        immediate: Option<u8>,
    ) {
        let scalar = self.stage.registers[dst];
        let wide = scalar.size() == 8;
        let (d, source) = (self.location(dst), self.location(source));
        let target = match d {
            Location::Gpr(_) => d,
            Location::Stack(_) if d == source && scalar.size() >= 4 => d,
            _ => Location::Gpr(0),
        };
        self.copy_scalar(target, source, scalar);
        let operand = self.operand(target, 1);
        self.instruction(None, &[opcode], wide, extension, operand);
        if let Some(value) = immediate {
            self.bytes(&[value]);
        }
        self.copy_scalar(d, target, scalar);
    }
    pub(super) fn shift(&mut self, dst: Reg, a: Reg, b: Reg, kind: u8) {
        let scalar = self.stage.registers[dst];
        let wide = scalar.size() == 8;
        let shift = self.location(b);
        if !matches!(shift, Location::Immediate(_)) {
            self.copy_scalar(Location::Gpr(1), shift, self.stage.registers[b]);
        }
        let d = self.location(dst);
        let target = match d {
            Location::Gpr(r) => r,
            _ => 0,
        };
        self.copy_scalar(Location::Gpr(target), self.location(a), scalar);
        if let Location::Immediate(bits) = shift {
            self.instruction(None, &[0xc1], wide, kind, Operand::Register(target));
            self.bytes(&[(bits & if wide { 63 } else { 31 }) as u8]);
        } else {
            self.instruction(None, &[0xd3], wide, kind, Operand::Register(target));
        }
        self.copy_scalar(d, Location::Gpr(target), scalar);
    }
    pub(super) fn compare(&mut self, dst: Reg, a: Reg, b: Reg, comparison: Comparison) {
        let scalar = self.stage.registers[a];
        let (a, b) = (self.location(a), self.location(b));
        if scalar.is_float() {
            let left = match a {
                Location::Xmm(r) => r,
                _ => 0,
            };
            self.copy_scalar(Location::Xmm(left), a, scalar);
            let right = self.float_operand(b, scalar);
            self.instruction(if scalar == Scalar::F64 { Some(0x66) } else { None }, &[0x0f, 0x2e], false, left, right);
        } else {
            let left = match a {
                Location::Gpr(r) => r,
                _ => 0,
            };
            let wide = scalar.size() == 8;
            self.copy_scalar(Location::Gpr(left), a, scalar);
            if scalar.size() < 4 {
                self.copy_scalar(Location::Gpr(1), b, scalar);
                self.instruction(None, &[0x3b], false, left, Operand::Register(1));
            } else if let Location::Immediate(bits) = b
                && (!wide || bits as i64 == bits as i32 as i64)
            {
                self.integer_immediate(7, left, bits, wide);
            } else {
                let right = self.operand(b, 1);
                self.instruction(None, &[0x3b], wide, left, right);
            }
        }
        self.comparison_result(dst, comparison, scalar);
    }
    pub(super) fn select(&mut self, dst: Reg, condition: Reg, a: Reg, b: Reg) {
        let scalar = self.stage.registers[dst];
        let wide = scalar.size() == 8;
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        if let Location::Immediate(bits) = self.location(condition) {
            self.copy_scalar(d, if bits != 0 { a } else { b }, scalar);
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
        self.copy_scalar(Location::Gpr(target), initial, scalar);
        let source = match source {
            source if scalar.size() < 4 || matches!(source, Location::Xmm(_) | Location::Immediate(_)) => {
                self.copy_scalar(Location::Gpr(1), source, scalar);
                Operand::Register(1)
            }
            location => self.operand(location, 1),
        };
        self.instruction(None, &[0x0f, opcode], wide, target, source);
        self.copy_scalar(d, Location::Gpr(target), scalar);
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
