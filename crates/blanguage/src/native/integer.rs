//! 32位常量使用精确倒数；64位按实际无符号幅值除法保持有符号取低位语义。
use super::*;
use allocation::Location;
use super::operands::MoveLocation;
use crate::builtin_types::ScalarEncoding;
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
    fn division_plan(divisor: u32, signed: u32) -> Division;
    #[link_name = "bootstrap.x64_instruction::divide_value_instruction"]
    fn emit_divide_instruction(signed: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::divide_constant32"]
    fn divide_constant32(result: DivisionOutputs, src: MoveLocation, divisor: u32,
        scalar: ScalarEncoding, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::divide_value"]
    fn divide_value(result: DivisionOutputs, src: MoveLocation, divisor: MoveLocation,
        scalar: ScalarEncoding, divisor_scalar: ScalarEncoding, output: *mut u8) -> u64;
}

impl Division {
    pub fn new(divisor: NonZeroU64, signed: bool) -> Self {
        // Both packet and scalar callers supply an actual nonzero 32-bit lane.
        unsafe { division_plan(divisor.get() as u32, u32::from(signed)) }
    }
}
#[repr(C)]
struct DivisionOutputs {
    first: MoveLocation,
    second: MoveLocation,
    kind: u32,
}

impl Emitter<'_> {
    fn division_outputs(&self, result: IntegerDivisionResult) -> DivisionOutputs {
        let (first, second, kind) = match result {
            IntegerDivisionResult::Quotient(dst) => (dst, dst, 1),
            IntegerDivisionResult::Remainder(dst) => (dst, dst, 2),
            IntegerDivisionResult::Both { quotient, remainder } => (quotient, remainder, 3),
        };
        DivisionOutputs { first: self.location(first).into(), second: self.location(second).into(), kind }
    }
    pub(super) fn integer_divide_value(&mut self, result: IntegerDivisionResult, src: Reg, divisor: Reg) {
        let outputs = self.division_outputs(result);
        let source = MoveLocation::from(self.location(src));
        let right = MoveLocation::from(self.location(divisor));
        let scalar = self.stage.registers[src];
        let divisor_scalar = self.stage.registers[divisor];
        self.emit_bytes(128, |output| unsafe {
            divide_value(outputs, source, right, scalar.into(), divisor_scalar.into(), output)
        });
    }
    pub(super) fn divide_value_instruction(&mut self, scalar: Scalar) {
        self.emit_bytes(11, |output| unsafe { emit_divide_instruction(u32::from(scalar.is_signed()), output) });
    }
    pub(super) fn integer_divide(&mut self, result: IntegerDivisionResult, src: Reg, divisor: NonZeroU64) {
        let outputs = self.division_outputs(result);
        let source = MoveLocation::from(self.location(src));
        let scalar = self.stage.registers[src];
        if scalar.size() == 8 {
            let right = MoveLocation::from(Location::Immediate(divisor.get()));
            self.emit_bytes(128, |output| unsafe {
                divide_value(outputs, source, right, scalar.into(), scalar.into(), output)
            });
        } else {
            let divisor = if scalar.is_signed() { scalar.signed(divisor.get()) as u32 } else { divisor.get() as u32 };
            self.emit_bytes(128, |output| unsafe { divide_constant32(outputs, source, divisor, scalar.into(), output) });
        }
    }
}
