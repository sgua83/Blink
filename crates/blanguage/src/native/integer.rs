//! 32位常量使用精确倒数；64位按实际无符号幅值除法保持有符号取低位语义。
use super::*;
use allocation::Location;
use super::operands::MoveLocation;
use crate::builtin_types::ScalarEncoding;
use std::num::NonZeroU64;

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::divide_constant32"]
    fn divide_constant32(result: DivisionOutputs, src: MoveLocation, divisor: u32,
        scalar: ScalarEncoding, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::divide_value"]
    fn divide_value(result: DivisionOutputs, src: MoveLocation, divisor: MoveLocation,
        scalar: ScalarEncoding, divisor_scalar: ScalarEncoding, output: *mut u8) -> u64;
}

#[repr(C)]
pub(super) struct DivisionOutputs {
    first: MoveLocation,
    second: MoveLocation,
    kind: u32,
}

impl DivisionOutputs {
    pub(super) fn new(result: IntegerDivisionResult, location: impl Fn(Reg) -> MoveLocation) -> Self {
        let (first, second, kind) = match result {
            IntegerDivisionResult::Quotient(dst) => (dst, dst, 1),
            IntegerDivisionResult::Remainder(dst) => (dst, dst, 2),
            IntegerDivisionResult::Both { quotient, remainder } => (quotient, remainder, 3),
        };
        Self { first: location(first), second: location(second), kind }
    }
}

impl Emitter<'_> {
    pub(super) fn integer_divide_value(&mut self, result: IntegerDivisionResult, src: Reg, divisor: Reg) {
        let outputs = DivisionOutputs::new(result, |reg| self.location(reg).into());
        let source = MoveLocation::from(self.location(src));
        let right = MoveLocation::from(self.location(divisor));
        let scalar = self.stage.registers[src];
        let divisor_scalar = self.stage.registers[divisor];
        self.emit_bytes(128, |output| unsafe {
            divide_value(outputs, source, right, scalar.into(), divisor_scalar.into(), output)
        });
    }
    pub(super) fn integer_divide(&mut self, result: IntegerDivisionResult, src: Reg, divisor: NonZeroU64) {
        let outputs = DivisionOutputs::new(result, |reg| self.location(reg).into());
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
