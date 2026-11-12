//! 32位常量使用精确倒数；64位按实际无符号幅值除法保持有符号取低位语义。
use super::*;
use allocation::Location;
use std::num::NonZeroU64;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DivisionMode {
    Shift,
    Multiply,
    MultiplyAdd,
}

#[repr(C)]
pub(super) struct Division {
    pub(super) magic: u32,
    pub(super) shift: u8,
    pub(super) mode: DivisionMode,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::division_plan"]
    fn division_plan(divisor: u32) -> Division;
}

impl Division {
    pub fn new(divisor: NonZeroU64, signed: bool) -> Self {
        let bits = divisor.get() as u32;
        let magnitude = if signed { (bits as i32).unsigned_abs() } else { bits };
        // Scalar and packet callers both pass a nonzero divisor in the 32-bit lane.
        unsafe { division_plan(magnitude) }
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
        self.copy_scalar(Location::Gpr(1), self.location(divisor), self.stage.registers[divisor]);
        self.load(src);
        self.divide_value_instruction(self.stage.registers[src]);
        if let Some(dst) = result.quotient() {
            self.store(dst);
        }
        if let Some(dst) = result.remainder() {
            self.copy_scalar(self.location(dst), Location::Gpr(2), self.stage.registers[dst]);
        }
    }
    pub(super) fn divide_value_instruction(&mut self, scalar: Scalar) {
        if scalar.is_signed() {
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
        let scalar = self.stage.registers[src];
        let signed = scalar.is_signed();
        let divisor = if signed { NonZeroU64::new(scalar.signed(divisor.get()) as u32 as u64).unwrap() } else { divisor };
        self.unsigned_magnitude(src, signed);
        let Division { magic, shift, mode } = Division::new(divisor, signed);
        match mode {
            DivisionMode::Shift => {
                if shift != 0 {
                    self.bytes(&[0xc1, 0xe8, shift]);
                }
            }
            DivisionMode::Multiply | DivisionMode::MultiplyAdd => {
                self.bytes(&[0xb9]);
                self.u32(magic);
                self.bytes(&[0xf7, 0xe1]); // EDX:EAX = unsigned EAX * ECX。
                if mode == DivisionMode::MultiplyAdd {
                    self.unsigned_magnitude(src, signed);
                    self.bytes(&[0x29, 0xd0, 0xd1, 0xe8, 0x01, 0xd0]);
                } else {
                    self.bytes(&[0x89, 0xd0]);
                }
                self.bytes(&[0xc1, 0xe8, shift]);
            }
        }
        if signed {
            self.copy_scalar(Location::Gpr(2), self.location(src), scalar);
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
            self.copy_scalar(Location::Gpr(2), self.location(src), scalar);
            self.bytes(&[0x29, 0xc2]);
            if let Some(quotient) = result.quotient() {
                self.copy_scalar(self.location(quotient), Location::Gpr(1), scalar);
            }
            self.copy_scalar(self.location(remainder), Location::Gpr(2), scalar);
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
