//! 指令直接消费分配位置；两地址写入保留右操作数，位复制不改变标量数值。
use super::*;
use allocation::Location;

#[derive(Clone, Copy)]
pub(super) enum Operand {
    Register(u8),
    Memory { base: u8, offset: u32 },
}
impl From<Location> for Operand {
    fn from(location: Location) -> Self {
        match location {
            Location::Gpr(r) | Location::Xmm(r) => Self::Register(r),
            Location::Stack(offset) => Self::Memory { base: 4, offset },
        }
    }
}
impl Emitter<'_> {
    fn rex(&mut self, wide: bool, r: u8, b: u8) {
        let prefix = 0x40 | (u8::from(wide) << 3) | ((r >> 3) << 2) | (b >> 3);
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
                self.rex(wide, register, r);
                self.bytes(opcode);
                self.bytes(&[0xc0 | field | (r & 7)]);
            }
            Operand::Memory { base, offset } => {
                self.rex(wide, register, base);
                self.bytes(opcode);
                let mode = if offset == 0 && base & 7 != 5 {
                    0
                } else if offset <= 127 {
                    1
                } else {
                    2
                };
                self.bytes(&[(mode << 6) | field | (base & 7)]);
                if base & 7 == 4 {
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
            (Location::Gpr(d), Location::Gpr(_) | Location::Stack(_)) => {
                self.instruction(None, &[0x8b], false, d, src.into());
            }
            (Location::Stack(_), Location::Gpr(s)) => {
                self.instruction(None, &[0x89], false, s, dst.into());
            }
            (Location::Xmm(d), Location::Xmm(_)) => {
                self.instruction(None, &[0x0f, 0x28], false, d, src.into());
            }
            (Location::Xmm(d), Location::Stack(_)) => {
                self.instruction(Some(0xf3), &[0x0f, 0x10], false, d, src.into());
            }
            (Location::Stack(_), Location::Xmm(s)) => {
                self.instruction(Some(0xf3), &[0x0f, 0x11], false, s, dst.into());
            }
            (Location::Xmm(d), Location::Gpr(_)) => {
                self.instruction(Some(0x66), &[0x0f, 0x6e], false, d, src.into());
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
        match self.location(r) {
            Location::Gpr(d) => {
                self.rex(false, 0, d);
                self.bytes(&[0xb8 | (d & 7)]);
                self.u32(bits);
            }
            dst @ Location::Stack(_) => {
                self.instruction(None, &[0xc7], false, 0, dst.into());
                self.u32(bits);
            }
            dst @ Location::Xmm(_) => {
                self.bytes(&[0xb8]);
                self.u32(bits);
                self.copy(dst, Location::Gpr(0));
            }
        }
    }
    pub(super) fn read_memory(&mut self, dst: Location, base: u8, offset: u32) {
        let source = Operand::Memory { base, offset };
        match dst {
            Location::Gpr(d) => self.instruction(None, &[0x8b], false, d, source),
            Location::Xmm(d) => self.instruction(Some(0xf3), &[0x0f, 0x10], false, d, source),
            Location::Stack(_) => {
                self.instruction(None, &[0x8b], false, 0, source);
                self.copy(dst, Location::Gpr(0));
            }
        }
    }
    pub(super) fn read_scalar(&mut self, dst: Reg, base: u8, offset: u32) {
        if self.stage.registers[dst] == Scalar::Bool {
            self.instruction(None, &[0x83], false, 7, Operand::Memory { base, offset });
            self.bytes(&[0, 0x0f, 0x95, 0xc0, 0x0f, 0xb6, 0xc0]);
            self.store(dst);
        } else {
            self.read_memory(self.location(dst), base, offset);
        }
    }
    pub(super) fn write_memory(&mut self, src: Location, base: u8, offset: u32) {
        let target = Operand::Memory { base, offset };
        match src {
            Location::Gpr(s) => self.instruction(None, &[0x89], false, s, target),
            Location::Xmm(s) => self.instruction(Some(0xf3), &[0x0f, 0x11], false, s, target),
            Location::Stack(_) => {
                self.copy(Location::Gpr(0), src);
                self.instruction(None, &[0x89], false, 0, target);
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
        self.instruction(Some(0xf3), &[0x0f, opcode], false, target, b.into());
        self.copy(d, Location::Xmm(target));
    }
    pub(super) fn integer_binary(&mut self, dst: Reg, a: Reg, b: Reg, opcode: &[u8]) {
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        let target = match d {
            Location::Gpr(r) if d != b || a == b => r,
            _ => 0,
        };
        self.copy(Location::Gpr(target), a);
        self.instruction(None, opcode, false, target, b.into());
        self.copy(d, Location::Gpr(target));
    }
    pub(super) fn float_sqrt(&mut self, dst: Reg, source: Reg) {
        let d = self.location(dst);
        let target = match d {
            Location::Xmm(r) => r,
            _ => 0,
        };
        self.copy(Location::Xmm(target), self.location(source));
        self.instruction(
            Some(0xf3),
            &[0x0f, 0x51],
            false,
            target,
            Operand::Register(target),
        );
        self.copy(d, Location::Xmm(target));
    }
    pub(super) fn shift(&mut self, dst: Reg, a: Reg, b: Reg, kind: u8) {
        self.copy(Location::Gpr(1), self.location(b));
        let d = self.location(dst);
        let target = match d {
            Location::Gpr(r) => r,
            _ => 0,
        };
        self.copy(Location::Gpr(target), self.location(a));
        self.instruction(None, &[0xd3], false, kind, Operand::Register(target));
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
            self.instruction(None, &[0x0f, 0x2e], false, left, b.into());
        } else {
            let left = match a {
                Location::Gpr(r) => r,
                _ => 0,
            };
            self.copy(Location::Gpr(left), a);
            self.instruction(None, &[0x3b], false, left, b.into());
        }
        self.comparison_result(comparison, scalar);
        self.store(dst);
    }
    pub(super) fn select(&mut self, dst: Reg, condition: Reg, a: Reg, b: Reg) {
        self.load(condition);
        self.bytes(&[0x85, 0xc0]);
        self.load(b);
        let source = match self.location(a) {
            Location::Xmm(_) => {
                self.copy(Location::Gpr(1), self.location(a));
                Operand::Register(1)
            }
            location => location.into(),
        };
        self.instruction(None, &[0x0f, 0x45], false, 0, source);
        self.store(dst);
    }
    pub(super) fn preserve_registers(&mut self, restore: bool) {
        for (r, offset) in self.frame.saved_gpr.clone() {
            self.instruction(
                None,
                &[if restore { 0x8b } else { 0x89 }],
                true,
                r,
                Operand::Memory { base: 4, offset },
            );
        }
        for (r, offset) in self.frame.saved_xmm.clone() {
            self.instruction(
                None,
                &[0x0f, if restore { 0x28 } else { 0x29 }],
                false,
                r,
                Operand::Memory { base: 4, offset },
            );
        }
    }
}
