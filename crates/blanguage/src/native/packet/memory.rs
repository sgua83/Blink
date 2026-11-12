//! 只读间接数据逐 lane 寻址，结果交回既有 SSE2 批次；不开放散写权限。
use super::*;

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::packet_lane"]
    fn encode_packet_lane(src: operands::MoveLocation, lane: u8, gpr: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_view_memory"]
    fn encode_packet_view_memory(base: u8, index: operands::MoveLocation, lane: u8, stride: u64, offset: u32, output: *mut u8) -> operands::AddressEncoding;
    #[link_name = "bootstrap.x64_instruction::packet_view_memory_dynamic"]
    fn encode_packet_view_memory_dynamic(base: u8, index: operands::MoveLocation, low: operands::MoveLocation, high: operands::MoveLocation, lane: u8, offset: u32, output: *mut u8) -> operands::AddressEncoding;
}

impl Emitter<'_> {
    pub(super) fn packet_lane(
        &mut self,
        allocation: &PacketAllocation,
        value: Reg,
        lane: u8,
        gpr: u8,
    ) {
        let source = allocation.location(PrivateSlot::Register(value)).into();
        self.emit_bytes(16, |output| unsafe { encode_packet_lane(source, lane, gpr, output) });
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
        let shared = matches!(
            source,
            ReadSource::Dataset(_) | ReadSource::View { uniform: true, .. }
        );
        if shared {
            self.packet_read_base(allocation, source, 0);
        }
        for lane in 0..if broadcast { 1 } else { 4 } {
            // Lane extraction and indexed reads preserve R15's base.
            if !shared {
                self.packet_read_base(allocation, source, lane);
            }
            let memory = match source {
                ReadSource::Dataset(binding) => {
                    self.packet_lane(allocation,index,lane,0);self.buffer_memory(*binding,0,word)
                },
                ReadSource::View { stride, .. } => {
                    let index = allocation.location(PrivateSlot::Register(index)).into();
                    match stride {
                        ElementStride::Constant(bytes) => self.emit_address(64, |output| unsafe {
                            encode_packet_view_memory(15, index, lane, *bytes, word, output)
                        }),
                        ElementStride::Value(parts) => {
                            let low = allocation.location(PrivateSlot::Register(parts[0])).into();
                            let high = allocation.location(PrivateSlot::Register(parts[1])).into();
                            self.emit_address(64, |output| unsafe {
                                encode_packet_view_memory_dynamic(15, index, low, high, lane, word, output)
                            })
                        },
                    }
                },
            };
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
                self.u32(*binding as u32 * BUFFER_BYTES);
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
