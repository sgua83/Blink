//! SSE2 整数商余机器序列使用原位置分配及B发射。
use super::*;
use super::super::integer::DivisionOutputs;
use super::super::operands::MoveLocation;
use std::num::NonZeroU64;

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::packet_divide_value"]
    fn emit_packet_divide_value(result: DivisionOutputs, src: MoveLocation, divisor: MoveLocation,
        signed: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_divide_constant"]
    fn emit_packet_divide_constant(result: DivisionOutputs, src: MoveLocation, divisor: u32,
        signed: u32, output: *mut u8) -> u64;
}

impl Emitter<'_> {
    pub(super) fn packet_divide_value(
        &mut self, allocation: &PacketAllocation, result: IntegerDivisionResult, src: Reg, divisor: Reg,
    ) {
        let location = |reg| allocation.location(PrivateSlot::Register(reg)).into();
        let outputs = DivisionOutputs::new(result, location);
        let signed = u32::from(self.stage.registers[src].is_signed());
        self.emit_bytes(256, |output| unsafe {
            emit_packet_divide_value(outputs, location(src), location(divisor), signed, output)
        });
    }

    pub(super) fn packet_divide(
        &mut self, allocation: &PacketAllocation, result: IntegerDivisionResult, src: Reg, divisor: NonZeroU64,
    ) {
        let location = |reg| allocation.location(PrivateSlot::Register(reg)).into();
        let outputs = DivisionOutputs::new(result, location);
        let signed = u32::from(self.stage.registers[src] == Scalar::I32);
        self.emit_bytes(256, |output| unsafe {
            emit_packet_divide_constant(outputs, location(src), divisor.get() as u32, signed, output)
        });
    }
}
