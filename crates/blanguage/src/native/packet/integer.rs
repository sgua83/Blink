//! SSE2 四路整数乘法与除余；常量使用精确倒数，动态值逐 lane 原生执行。
use super::*;
use crate::native::integer::Division;
use std::num::NonZeroU32;

impl Emitter<'_> {
    pub(super) fn packet_divide_value(
        &mut self,
        allocation: &PacketAllocation,
        result: IntegerDivisionResult,
        src: Reg,
        divisor: Reg,
    ) {
        // SSE2 没有 packed 整数除法；逐 lane 使用同一原生除法规则，再合回批次。
        let paired = matches!(result, IntegerDivisionResult::Both { .. });
        let scratch = if paired { 2 } else { 1 };
        self.packed(Some(0x66), 0xef, 0, 0);
        if paired {
            self.packed(Some(0x66), 0xef, 1, 1);
        }
        for lane in 0..4 {
            self.packet_load(allocation, PrivateSlot::Register(divisor), scratch);
            if lane != 0 {
                self.packed(Some(0x66), 0x73, 3, scratch);
                self.bytes(&[lane * 4]);
            }
            self.instruction(
                Some(0x66),
                &[0x0f, 0x7e],
                false,
                scratch,
                Operand::Register(1),
            );
            self.packet_load(allocation, PrivateSlot::Register(src), scratch);
            if lane != 0 {
                self.packed(Some(0x66), 0x73, 3, scratch);
                self.bytes(&[lane * 4]);
            }
            self.instruction(
                Some(0x66),
                &[0x0f, 0x7e],
                false,
                scratch,
                Operand::Register(0),
            );
            self.divide_value_instruction(self.stage.registers[src]);
            for (accumulator, (op, _)) in result.outputs().enumerate() {
                self.instruction(
                    Some(0x66),
                    &[0x0f, 0x6e],
                    false,
                    scratch,
                    Operand::Register(if op == IntegerDivisionOp::Remainder {
                        2
                    } else {
                        0
                    }),
                );
                if lane != 0 {
                    self.packed(Some(0x66), 0x73, 7, scratch);
                    self.bytes(&[lane * 4]);
                }
                self.packed(Some(0x66), 0xeb, accumulator as u8, scratch);
            }
        }
        for (accumulator, (_, dst)) in result.outputs().enumerate() {
            self.packet_store(allocation, PrivateSlot::Register(dst), accumulator as u8);
        }
    }
    pub(super) fn packet_shift(&mut self, xmm: u8, kind: u8, count: u8) {
        self.packed(Some(0x66), 0x72, kind, xmm);
        self.bytes(&[count]);
    }
    pub(super) fn packet_multiply(&mut self, right_broadcast: bool) {
        self.packed(None, 0x28, 2, 0);
        if !right_broadcast {
            self.packed(None, 0x28, 3, 1);
        }
        self.packed(Some(0x66), 0x73, 3, 2);
        self.bytes(&[4]);
        if !right_broadcast {
            self.packed(Some(0x66), 0x73, 3, 3);
            self.bytes(&[4]);
        }
        self.packed(Some(0x66), 0xf4, 0, 1);
        self.packed(Some(0x66), 0xf4, 2, if right_broadcast { 1 } else { 3 });
        self.shuffle(0, 0, 0x88);
        self.shuffle(2, 2, 0x88);
        self.packed(Some(0x66), 0x62, 0, 2);
    }
    fn packet_magnitude(&mut self, allocation: &PacketAllocation, src: Reg, xmm: u8, signed: bool) {
        self.packet_load(allocation, PrivateSlot::Register(src), xmm);
        if signed {
            self.packed(None, 0x28, 3, xmm);
            self.packet_shift(3, 4, 31);
            self.packed(Some(0x66), 0xef, xmm, 3);
            self.packed(Some(0x66), 0xfa, xmm, 3);
        }
    }
    pub(super) fn packet_divide(
        &mut self,
        allocation: &PacketAllocation,
        result: IntegerDivisionResult,
        src: Reg,
        divisor: NonZeroU32,
    ) {
        let signed = self.stage.registers[src] == Scalar::I32;
        self.packet_magnitude(allocation, src, 0, signed);
        match Division::new(divisor, signed) {
            Division::Shift(shift) => {
                if shift != 0 {
                    self.packet_shift(0, 2, shift);
                }
            }
            Division::Multiply { magic, shift, add } => {
                self.packet_constant(1, magic);
                self.packed(None, 0x28, 2, 0);
                self.packed(Some(0x66), 0x73, 3, 2);
                self.bytes(&[4]);
                self.packed(Some(0x66), 0xf4, 0, 1);
                self.packed(Some(0x66), 0xf4, 2, 1);
                self.shuffle(0, 0, 0xdd);
                self.shuffle(2, 2, 0xdd);
                self.packed(Some(0x66), 0x62, 0, 2);
                if add {
                    self.packet_magnitude(allocation, src, 1, signed);
                    self.packed(Some(0x66), 0xfa, 1, 0);
                    self.packet_shift(1, 2, 1);
                    self.packed(Some(0x66), 0xfe, 0, 1);
                }
                self.packet_shift(0, 2, shift);
            }
        }
        if signed {
            self.packet_load(allocation, PrivateSlot::Register(src), 1);
            self.packet_shift(1, 4, 31);
            if (divisor.get() as i32) < 0 {
                self.packet_constant(2, u32::MAX);
                self.packed(Some(0x66), 0xef, 1, 2);
            }
            self.packed(Some(0x66), 0xef, 0, 1);
            self.packed(Some(0x66), 0xfa, 0, 1);
        }
        if let Some(remainder) = result.remainder() {
            if result.quotient().is_some() {
                self.packed(None, 0x28, 3, 0);
            }
            self.packet_constant(1, divisor.get());
            self.packet_multiply(true);
            self.packet_load(allocation, PrivateSlot::Register(src), 1);
            self.packed(Some(0x66), 0xfa, 1, 0);
            if let Some(quotient) = result.quotient() {
                self.packet_store(allocation, PrivateSlot::Register(quotient), 3);
            }
            self.packet_store(allocation, PrivateSlot::Register(remainder), 1);
        } else if let Some(dst) = result.quotient() {
            self.packet_store(allocation, PrivateSlot::Register(dst), 0);
        }
    }
}
