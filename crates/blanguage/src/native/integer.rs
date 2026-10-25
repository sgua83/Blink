//! 常量使用精确倒数；动态有符号值在内部扩大除法宽度，保持 32 位取低位语义。
use super::*;
use allocation::Location;
use std::num::NonZeroU32;

pub(super) enum Division {
    Shift(u8),
    Multiply { magic: u32, shift: u8, add: bool },
}
impl Division {
    pub fn new(divisor: NonZeroU32, signed: bool) -> Self {
        let bits = divisor.get();
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
        divisor: NonZeroU32,
    ) {
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
            self.u32(divisor.get());
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
}
