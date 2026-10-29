//! 32位常量使用精确倒数；64位按实际无符号幅值除法保持有符号取低位语义。
use super::*;
use allocation::Location;
use std::num::NonZeroU64;

pub(super) enum Division {
    Shift(u8),
    Multiply { magic: u32, shift: u8, add: bool },
}
impl Division {
    pub fn new(divisor: NonZeroU64, signed: bool) -> Self {
        let bits = divisor.get() as u32;
        let d = if signed {
            (bits as i32).unsigned_abs()
        } else {
            bits
        };
        let shift = (31 - d.leading_zeros()) as u8;
        if d.is_power_of_two() {
            return Self::Shift(shift);
        }
        let numerator = 1u128 << (32 + shift);
        let d = d as u128;
        let multiplier = numerator.div_ceil(d);
        // m=ceil(2^(32+l)/d)，e=m*d-2^(32+l)。e<2^l 时直接取高位再右移；
        // 否则改用多一位的倒数，以 ((n-q0)>>1)+q0 避免 33 位求和溢出。
        if multiplier * d - numerator < 1u128 << shift {
            Self::Multiply {
                magic: multiplier as u32,
                shift,
                add: false,
            }
        } else {
            Self::Multiply {
                magic: ((2 * numerator).div_ceil(d) - (1u128 << 32)) as u32,
                shift,
                add: true,
            }
        }
    }
}
impl Emitter<'_> {
    pub(super) fn integer_divide_value(
        &mut self,
        result: IntegerDivisionResult,
        src: Reg,
        divisor: Reg,
    ) {
        if self.stage.registers[src].size() == 8 {
            self.integer_divide64(result, src, self.location(divisor));
            return;
        }
        self.copy(Location::Gpr(1), self.location(divisor));
        self.load(src);
        self.divide_value_instruction(self.stage.registers[src]);
        if let Some(dst) = result.quotient() {
            self.store(dst);
        }
        if let Some(dst) = result.remainder() {
            self.copy(self.location(dst), Location::Gpr(2));
        }
    }
    pub(super) fn divide_value_instruction(&mut self, scalar: Scalar) {
        if scalar == Scalar::I32 {
            // sign_extend(i32) 后执行 IDIV64，MIN32 / -1 在 i64 内可表示。
            self.bytes(&[
                0x48, 0x63, 0xc0, 0x48, 0x63, 0xc9, 0x48, 0x99, 0x48, 0xf7, 0xf9,
            ]);
        } else {
            self.bytes(&[0x31, 0xd2, 0xf7, 0xf1]);
        }
    }
    fn unsigned_magnitude(&mut self, src: Reg, signed: bool) {
        self.load(src);
        if signed {
            // ECX 为符号掩码；不破坏乘法得到的 EDX 高 32 位。
            self.bytes(&[0x89, 0xc1, 0xc1, 0xf9, 31, 0x31, 0xc8, 0x29, 0xc8]);
        }
    }
    pub(super) fn integer_divide(
        &mut self,
        result: IntegerDivisionResult,
        src: Reg,
        divisor: NonZeroU64,
    ) {
        if self.stage.registers[src].size() == 8 {
            self.integer_divide64(result, src, Location::Immediate(divisor.get()));
            return;
        }
        let signed = self.stage.registers[src] == Scalar::I32;
        self.unsigned_magnitude(src, signed);
        match Division::new(divisor, signed) {
            Division::Shift(shift) => {
                if shift != 0 {
                    self.bytes(&[0xc1, 0xe8, shift]);
                }
            }
            Division::Multiply { magic, shift, add } => {
                self.bytes(&[0xb9]);
                self.u32(magic);
                self.bytes(&[0xf7, 0xe1]); // EDX:EAX = unsigned EAX * ECX。
                if add {
                    self.unsigned_magnitude(src, signed);
                    self.bytes(&[0x29, 0xd0, 0xd1, 0xe8, 0x01, 0xd0]);
                } else {
                    self.bytes(&[0x89, 0xd0]);
                }
                self.bytes(&[0xc1, 0xe8, shift]);
            }
        }
        if signed {
            self.copy(Location::Gpr(2), self.location(src));
            self.bytes(&[0xc1, 0xfa, 31]);
            if (divisor.get() as i32) < 0 {
                self.bytes(&[0xf7, 0xd2]);
            }
            self.bytes(&[0x31, 0xd0, 0x29, 0xd0]);
        }
        if let Some(remainder) = result.remainder() {
            if result.quotient().is_some() {
                self.bytes(&[0x89, 0xc1]); // ECX 保留商；生成余数之前不覆盖原分子。
            }
            self.bytes(&[0x69, 0xc0]);
            self.u32(divisor.get() as u32);
            self.copy(Location::Gpr(2), self.location(src));
            self.bytes(&[0x29, 0xc2]);
            if let Some(quotient) = result.quotient() {
                self.copy(self.location(quotient), Location::Gpr(1));
            }
            self.copy(self.location(remainder), Location::Gpr(2));
        } else if let Some(dst) = result.quotient() {
            self.store(dst);
        }
    }

    fn integer_divide64(&mut self, result: IntegerDivisionResult, src: Reg, divisor: Location) {
        let scalar = self.stage.registers[src];
        self.copy_scalar(Location::Gpr(1), divisor, scalar);
        self.load(src);
        if scalar.is_signed() {
            // Divide unsigned magnitudes. MIN64 / -1 then has the source
            // language's low-64-bit result, rather than an IDIV overflow trap.
            self.bytes(&[
                0x48, 0x89, 0xc2, 0x48, 0xc1, 0xfa, 63,
                0x48, 0x31, 0xd0, 0x48, 0x29, 0xd0,
                0x48, 0x89, 0xca, 0x48, 0xc1, 0xfa, 63,
                0x48, 0x31, 0xd1, 0x48, 0x29, 0xd1,
            ]);
        }
        self.bytes(&[0x31, 0xd2, 0x48, 0xf7, 0xf1]);
        if scalar.is_signed() {
            if result.quotient().is_some() {
                self.copy_scalar(Location::Gpr(1), self.location(src), scalar);
                match divisor {
                    Location::Immediate(bits) => {
                        if (bits as i64) < 0 { self.bytes(&[0x48, 0xf7, 0xd1]); }
                    }
                    location => {
                        let operand = self.operand(location, 1);
                        self.instruction(None, &[0x33], true, 1, operand);
                    }
                }
                self.bytes(&[0x48, 0xc1, 0xf9, 63, 0x48, 0x31, 0xc8, 0x48, 0x29, 0xc8]);
            }
            if result.remainder().is_some() {
                self.copy_scalar(Location::Gpr(1), self.location(src), scalar);
                self.bytes(&[0x48, 0xc1, 0xf9, 63, 0x48, 0x31, 0xca, 0x48, 0x29, 0xca]);
            }
        }
        if let Some(dst) = result.quotient() { self.store(dst); }
        if let Some(dst) = result.remainder() {
            self.copy_scalar(self.location(dst), Location::Gpr(2), scalar);
        }
    }
}
