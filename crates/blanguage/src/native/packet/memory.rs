//! 原计划与分配交接实际位置；完整载荷机器序列由同一 B 编码器写入。
use super::*;

#[repr(C)]
struct PacketMemory {
    index: operands::MoveLocation,
    data_offset: u32,
    stride_offset: u32,
    frame_offset: u32,
    offset: u32,
    index_kind: u32,
    contiguous: u32,
}

#[repr(C)]
struct PacketGather {
    index: operands::MoveLocation,
    base_low: operands::MoveLocation,
    base_high: operands::MoveLocation,
    stride_low: operands::MoveLocation,
    stride_high: operands::MoveLocation,
    stride: u64,
    data_offset: u32,
    stride_offset: u32,
    offset: u32,
    kind: u32,
    uniform: u32,
    broadcast: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::packet_lane"]
    fn encode_packet_lane(src: operands::MoveLocation, lane: u8, gpr: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_read"]
    fn encode_packet_read(dst: operands::MoveLocation, memory: PacketMemory, boolean: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_write"]
    fn encode_packet_write(src: operands::MoveLocation, memory: PacketMemory, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_gather"]
    fn encode_packet_gather(dst: operands::MoveLocation, input: PacketGather, boolean: u32, output: *mut u8) -> u64;
}

impl Emitter<'_> {
    pub(super) fn packet_lane(&mut self, allocation: &PacketAllocation, value: Reg, lane: u8, gpr: u8) {
        let source = allocation.location(PrivateSlot::Register(value)).into();
        self.emit_bytes(16, |output| unsafe { encode_packet_lane(source, lane, gpr, output) });
    }

    fn packet_memory_input(&self, allocation: &PacketAllocation, binding: usize, index: &Address,
        offset: u32, contiguous: bool) -> PacketMemory {
        let (index_kind, index) = match index {
            Address::Element => (u32::from(self.stage.selection.is_some()), allocation::Location::Immediate(0).into()),
            Address::Uniform(value) => (2, allocation.location(PrivateSlot::Register(*value)).into()),
        };
        PacketMemory { index, data_offset: binding as u32 * BUFFER_BYTES,
            stride_offset: binding as u32 * BUFFER_BYTES + BUFFER_STRIDE,
            frame_offset: self.frame.end as u32, offset, index_kind, contiguous: u32::from(contiguous) }
    }

    pub(super) fn packet_read(&mut self, allocation: &PacketAllocation, dst: Reg, binding: usize,
        index: &Address, offset: u32) {
        let contiguous = self.stage.selection.is_none() && !matches!(index, Address::Uniform(_)) && self.buffers[binding].stride == 4;
        let memory = self.packet_memory_input(allocation, binding, index, offset, contiguous);
        let dst_location = allocation.location(PrivateSlot::Register(dst)).into();
        let boolean = u32::from(self.stage.registers[dst] == Scalar::Bool);
        self.emit_bytes(512, |output| unsafe { encode_packet_read(dst_location, memory, boolean, output) });
    }

    pub(super) fn packet_write(&mut self, allocation: &PacketAllocation, value: Reg, binding: usize,
        index: &Address, offset: u32) {
        let contiguous = self.stage.selection.is_none() && self.buffers[binding].stride == 4;
        let memory = self.packet_memory_input(allocation, binding, index, offset, contiguous);
        let src = allocation.location(PrivateSlot::Register(value)).into();
        self.emit_bytes(512, |output| unsafe { encode_packet_write(src, memory, output) });
    }

    pub(super) fn packet_gather(&mut self, allocation: &PacketAllocation, dst: Reg, source: &ReadSource,
        index: Reg, offset: u32, broadcast: bool) {
        let empty = || allocation::Location::Immediate(0).into();
        let mut input = PacketGather { index: allocation.location(PrivateSlot::Register(index)).into(),
            base_low: empty(), base_high: empty(), stride_low: empty(), stride_high: empty(), stride: 0,
            data_offset: 0, stride_offset: 0, offset, kind: 0, uniform: 0, broadcast: u32::from(broadcast) };
        match source {
            ReadSource::Dataset(binding) => {
                input.data_offset = *binding as u32 * BUFFER_BYTES;
                input.stride_offset = input.data_offset + BUFFER_STRIDE;
            }
            ReadSource::View { address, stride, uniform } => {
                input.base_low = allocation.location(PrivateSlot::Register(address[0])).into();
                input.base_high = allocation.location(PrivateSlot::Register(address[1])).into();
                input.uniform = u32::from(*uniform);
                match stride {
                    ElementStride::Constant(bytes) => { input.kind = 1; input.stride = *bytes; }
                    ElementStride::Value(parts) => {
                        input.kind = 2;
                        input.stride_low = allocation.location(PrivateSlot::Register(parts[0])).into();
                        input.stride_high = allocation.location(PrivateSlot::Register(parts[1])).into();
                    }
                }
            }
        }
        let dst_location = allocation.location(PrivateSlot::Register(dst)).into();
        let boolean = u32::from(self.stage.registers[dst] == Scalar::Bool);
        self.emit_bytes(1024, |output| unsafe { encode_packet_gather(dst_location, input, boolean, output) });
    }
}
