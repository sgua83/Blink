//! 只读间接数据逐 lane 寻址，结果交回既有 SSE2 批次；不开放散写权限。
use super::*;

impl Emitter<'_> {
    pub(super) fn packet_lane(
        &mut self,
        allocation: &PacketAllocation,
        value: Reg,
        lane: u8,
        gpr: u8,
    ) {
        match allocation.location(PrivateSlot::Register(value)) {
            PacketPlace::Register(xmm) => {
                let source = if lane == 0 {
                    xmm
                } else {
                    self.shuffle(3, xmm, lane * 0x55);
                    3
                };
                self.instruction(
                    Some(0x66),
                    &[0x0f, 0x7e],
                    false,
                    source,
                    Operand::Register(gpr),
                );
            }
            PacketPlace::Stack(offset) => self.instruction(
                None,
                &[0x8b],
                false,
                gpr,
                Operand::memory(4, offset + u32::from(lane) * 4),
            ),
        }
    }

    pub(super) fn packet_gather(
        &mut self,
        allocation: &PacketAllocation,
        dst: Reg,
        source: &ReadSource,
        index: Reg,
        word: u32,
        broadcast: bool,
    ) {
        let shared = matches!(source, ReadSource::View { uniform: true, .. }) && !broadcast;
        if shared {
            self.packet_read_base(allocation, source, 0);
            self.instruction(None, &[0x8b], true, 2, Operand::Register(15));
        }
        let stride = match source {
            ReadSource::Dataset(binding) => self.program.buffers[*binding].stride,
            ReadSource::View { stride, .. } => *stride,
        };
        for lane in 0..if broadcast { 1 } else { 4 } {
            if shared {
                self.instruction(None, &[0x8b], true, 15, Operand::Register(2));
            } else {
                self.packet_read_base(allocation, source, lane);
            }
            self.packet_lane(allocation, index, lane, 0);
            let memory = self.scaled_memory(15, 0, stride, word * 4);
            // XMM3 只在取 lane 时暂用，第四个读取在地址完成后才写入它。
            // 目标分配位置在四个源 lane 全部读取后写回，允许实际的寄存器复用。
            self.instruction(Some(0x66), &[0x0f, 0x6e], false, lane, memory.into());
        }
        if broadcast {
            self.shuffle(0, 0, 0);
        } else {
            self.packed(Some(0x66), 0x62, 0, 1);
            self.packed(Some(0x66), 0x62, 2, 3);
            self.packed(Some(0x66), 0x6c, 0, 2);
        }
        self.packet_from_word(dst, 0);
        self.packet_store(allocation, PrivateSlot::Register(dst), 0);
    }

    fn packet_read_base(&mut self, allocation: &PacketAllocation, source: &ReadSource, lane: u8) {
        match source {
            ReadSource::Dataset(binding) => {
                self.bytes(&[0x4d, 0x8b, 0xbc, 0x24]);
                self.u32(*binding as u32 * 8);
            }
            ReadSource::View { address, .. } => {
                self.packet_lane(allocation, address[1], lane, 15);
                self.bytes(&[0x49, 0xc1, 0xe7, 32]);
                self.packet_lane(allocation, address[0], lane, 0);
                self.bytes(&[0x49, 0x09, 0xc7]);
            }
        }
    }
}
