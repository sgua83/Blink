//! B选择访存形态并写出机器码，原IR、资源布局与分配交接实际事实和位置。
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct PacketAddress(u32);

impl PacketAddress {
    const UNSUPPORTED: Self = Self(0);
    const UNIFORM: Self = Self(4);
    const GATHER: Self = Self(5);
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct PacketViewAddress(u32);

#[repr(C)]
struct PacketMemory {
    index: operands::MoveLocation,
    data_offset: u32,
    stride_offset: u32,
    frame_offset: u32,
    offset: u32,
    mode: PacketAddress,
}

#[repr(C)]
struct PacketView {
    index: operands::MoveLocation,
    base_low: operands::MoveLocation,
    base_high: operands::MoveLocation,
    stride_low: operands::MoveLocation,
    stride_high: operands::MoveLocation,
    stride: u64,
    offset: u32,
    dynamic_stride: u32,
    mode: PacketViewAddress,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::packet_dataset_address"]
    fn classify_dataset(
        stride: u32,
        word: u32,
        selected: u32,
        writes_binding: u32,
        element: u32,
        uniform: u32,
    ) -> PacketAddress;
    #[link_name = "bootstrap.x64_instruction::packet_view_address"]
    fn classify_view(
        word: u32,
        uniform_base: u32,
        uniform_index: u32,
        uniform_stride: u32,
    ) -> PacketViewAddress;
    #[link_name = "bootstrap.x64_instruction::packet_lane"]
    fn encode_packet_lane(src: operands::MoveLocation, lane: u8, gpr: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_read"]
    fn encode_packet_read(
        dst: operands::MoveLocation,
        memory: PacketMemory,
        boolean: u32,
        output: *mut u8,
    ) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_write"]
    fn encode_packet_write(
        src: operands::MoveLocation,
        memory: PacketMemory,
        output: *mut u8,
    ) -> u64;
    #[link_name = "bootstrap.x64_instruction::packet_view"]
    fn encode_packet_view(
        dst: operands::MoveLocation,
        input: PacketView,
        boolean: u32,
        output: *mut u8,
    ) -> u64;
}

pub(super) fn packet_dataset_address(
    stride: u32,
    word: u32,
    selected: bool,
    writes_binding: bool,
    element: bool,
    uniform: bool,
) -> Option<PacketAddress> {
    let mode = unsafe {
        classify_dataset(
            stride,
            word,
            u32::from(selected),
            u32::from(writes_binding),
            u32::from(element),
            u32::from(uniform),
        )
    };
    (mode != PacketAddress::UNSUPPORTED).then_some(mode)
}

pub(super) fn packet_view_address(
    word: u32,
    uniform_base: bool,
    uniform_index: bool,
    uniform_stride: bool,
) -> Option<PacketViewAddress> {
    let mode = unsafe {
        classify_view(
            word,
            uniform_base.into(),
            uniform_index.into(),
            uniform_stride.into(),
        )
    };
    (mode != PacketViewAddress(0)).then_some(mode)
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
        self.emit_bytes(16, |output| unsafe {
            encode_packet_lane(source, lane, gpr, output)
        });
    }

    fn packet_memory_input(
        &self,
        binding: usize,
        index: operands::MoveLocation,
        offset: u32,
        mode: PacketAddress,
    ) -> PacketMemory {
        let data_offset = binding as u32 * BUFFER_BYTES;
        PacketMemory {
            index,
            data_offset,
            stride_offset: data_offset + BUFFER_STRIDE,
            frame_offset: self.frame.end as u32,
            offset,
            mode,
        }
    }

    pub(super) fn packet_read(
        &mut self,
        allocation: &PacketAllocation,
        dst: Reg,
        binding: usize,
        index: Reg,
        offset: u32,
        mode: PacketAddress,
    ) {
        let index = if mode == PacketAddress::UNIFORM || mode == PacketAddress::GATHER {
            allocation.location(PrivateSlot::Register(index)).into()
        } else {
            allocation::Location::Immediate(0).into()
        };
        let memory = self.packet_memory_input(binding, index, offset, mode);
        let boolean = u32::from(self.stage.registers[dst] == Scalar::Bool);
        let dst = allocation.location(PrivateSlot::Register(dst)).into();
        self.emit_bytes(512, |output| unsafe {
            encode_packet_read(dst, memory, boolean, output)
        });
    }

    pub(super) fn packet_write(
        &mut self,
        allocation: &PacketAllocation,
        value: Reg,
        binding: usize,
        offset: u32,
        mode: PacketAddress,
    ) {
        let memory = self.packet_memory_input(
            binding,
            allocation::Location::Immediate(0).into(),
            offset,
            mode,
        );
        let src = allocation.location(PrivateSlot::Register(value)).into();
        self.emit_bytes(512, |output| unsafe {
            encode_packet_write(src, memory, output)
        });
    }

    pub(super) fn packet_view(
        &mut self,
        allocation: &PacketAllocation,
        dst: Reg,
        address: [Reg; 2],
        stride: ElementStride,
        index: Reg,
        offset: u32,
        mode: PacketViewAddress,
    ) {
        let location = |reg| allocation.location(PrivateSlot::Register(reg)).into();
        let empty = || allocation::Location::Immediate(0).into();
        let (stride, stride_low, stride_high, dynamic_stride) = match stride {
            ElementStride::Constant(bytes) => (bytes, empty(), empty(), 0),
            ElementStride::Value([low, high]) => (0, location(low), location(high), 1),
        };
        let input = PacketView {
            index: location(index),
            base_low: location(address[0]),
            base_high: location(address[1]),
            stride_low,
            stride_high,
            stride,
            offset,
            dynamic_stride,
            mode,
        };
        let boolean = u32::from(self.stage.registers[dst] == Scalar::Bool);
        self.emit_bytes(1024, |output| unsafe {
            encode_packet_view(location(dst), input, boolean, output)
        });
    }
}
