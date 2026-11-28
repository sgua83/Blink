//! 指令直接消费分配位置；两地址写入保留右操作数，位复制不改变标量数值。
use super::*;
use allocation::Location;
use crate::builtin_types::ScalarEncoding;
use crate::ir::{TileKey, dataflow::{HOST, Host, Span, View}};

#[derive(Clone, Copy)]
pub(super) struct Memory {
    base: u8,
    index: Option<(u8, u8)>,
    offset: u32,
}
impl Memory {
    pub(super) fn new(base: u8, offset: u32) -> Self {
        Self {
            base,
            index: None,
            offset,
        }
    }
    pub(super) fn indexed(base: u8, index: u8, shift: u8, offset: u32) -> Self {
        Self { base, index: Some((index, shift)), offset }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Operand {
    Register(u8),
    Memory(Memory),
    Literal(Bits, bool),
}
impl Operand {
    pub(super) fn memory(base: u8, offset: u32) -> Self {
        Self::Memory(Memory::new(base, offset))
    }
}
impl From<Memory> for Operand {
    fn from(memory: Memory) -> Self {
        Self::Memory(memory)
    }
}
#[repr(u8)]
enum InstructionForm { Register = 1, Memory, RipRelative }

#[repr(C)]
struct InstructionEncoding {
    displacement: u32,
    reg: u8,
    base: u8,
    index: u8,
    shift: u8,
    form: InstructionForm,
    prefix: u8,
    wide: u16,
}

const NO_INDEX: u8 = u8::MAX;

#[repr(u8)]
enum MoveKind { Gpr, Xmm, Memory, Immediate }

#[repr(C)]
pub(super) struct MoveLocation {
    value: u64,
    kind: MoveKind,
    base: u8,
    index: u8,
    shift: u8,
}

impl From<Memory> for MoveLocation {
    fn from(memory: Memory) -> Self {
        let (index, shift) = memory.index.unwrap_or((NO_INDEX, 0));
        Self {
            value: u64::from(memory.offset), kind: MoveKind::Memory,
            base: memory.base, index, shift,
        }
    }
}

impl From<Location> for MoveLocation {
    fn from(location: Location) -> Self {
        let (kind, base, value) = match location {
            Location::Gpr(reg) => (MoveKind::Gpr, reg, 0),
            Location::Xmm(reg) => (MoveKind::Xmm, reg, 0),
            Location::Immediate(bits) => (MoveKind::Immediate, 0, bits),
            Location::Stack(offset) => return Memory::new(4, offset).into(),
        };
        Self { value, kind, base, index: NO_INDEX, shift: 0 }
    }
}

#[repr(C)]
struct MoveEncoding {
    length: u64,
    literal_field: u64,
}

#[repr(C)]
pub(super) struct AddressEncoding {
    length: u64,
    offset: u32,
    base: u8,
    index: u8,
    shift: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ScalarValueHost {
    pub(super) ir: Host,
    pub(super) context: *const std::ffi::c_void,
    pub(super) location: unsafe extern "C" fn(*const std::ffi::c_void, Reg) -> MoveLocation,
    pub(super) scalar: unsafe extern "C" fn(*const std::ffi::c_void, Reg) -> ScalarEncoding,
}

pub(super) struct ScalarLocations<'a> {
    pub(super) allocation: &'a Allocation,
    pub(super) registers: &'a [Scalar],
    pub(super) tiles: &'a [Tile],
    pub(super) frame: &'a Frame,
    pub(super) workgroup: u32,
}

impl<'a> ScalarLocations<'a> {
    fn from_emitter(emitter: &'a Emitter<'_>) -> Self {
        Self { allocation: &emitter.allocation, registers: emitter.registers, tiles: emitter.tiles,
            frame: &emitter.frame, workgroup: emitter.workgroup }
    }

    pub(super) fn indexed_host(&self) -> IndexedAddressHost {
        IndexedAddressHost::new(self.host(), TileSlots { tiles: Span::slice(self.tiles), slot: indexed_tile_slot }, self.workgroup)
    }

    pub(super) fn host(&self) -> ScalarValueHost {
        ScalarValueHost {
            ir: HOST, context: std::ptr::from_ref(self).cast(),
            location: value_location, scalar: value_scalar,
        }
    }

    fn location(&self, slot: PrivateSlot) -> MoveLocation {
        self.allocation.location(slot).into()
    }

    fn scalar(&self, reg: Reg) -> ScalarEncoding {
        self.registers[reg].into()
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct TileSlots {
    pub(super) tiles: Span,
    pub(super) slot: unsafe extern "C" fn(*const std::ffi::c_void, TileKey) -> u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct IndexedAddressHost {
    values: ScalarValueHost,
    tiles: TileSlots,
    buffer_bytes: u32,
    stride_field: u32,
    workgroup: u32,
}

impl IndexedAddressHost {
    fn new(values: ScalarValueHost, tiles: TileSlots, workgroup: u32) -> Self {
        Self { values, tiles, buffer_bytes: BUFFER_BYTES, stride_field: BUFFER_STRIDE, workgroup }
    }
}

unsafe extern "C" fn indexed_tile_slot(context: *const std::ffi::c_void, key: TileKey) -> u32 {
    let locations = unsafe { &*context.cast::<ScalarLocations<'_>>() };
    locations.frame.shared_offset(key)
}

pub(super) unsafe extern "C" fn value_local(context: *const std::ffi::c_void, local: usize) -> MoveLocation {
    unsafe { &*context.cast::<ScalarLocations<'_>>() }.location(PrivateSlot::Local(local))
}

unsafe extern "C" fn value_location(context: *const std::ffi::c_void, reg: Reg) -> MoveLocation {
    unsafe { &*context.cast::<ScalarLocations<'_>>() }.location(PrivateSlot::Register(reg))
}

unsafe extern "C" fn value_scalar(context: *const std::ffi::c_void, reg: Reg) -> ScalarEncoding {
    unsafe { &*context.cast::<ScalarLocations<'_>>() }.scalar(reg)
}

pub(super) fn scalar_body_addresses(context: *const std::ffi::c_void, workgroup: u32, tiles: Span) -> IndexedAddressHost {
    IndexedAddressHost::new(
        ScalarValueHost { ir: HOST, context, location: body_value_location, scalar: body_value_scalar },
        TileSlots { tiles, slot: body_tile_slot },
        workgroup,
    )
}

pub(super) unsafe extern "C" fn scalar_body_local(context: *const std::ffi::c_void, local: usize) -> MoveLocation {
    ScalarLocations::from_emitter(unsafe { &*context.cast::<Emitter<'_>>() }).location(PrivateSlot::Local(local))
}

pub(super) unsafe extern "C" fn body_value_location(context: *const std::ffi::c_void, reg: Reg) -> MoveLocation {
    ScalarLocations::from_emitter(unsafe { &*context.cast::<Emitter<'_>>() }).location(PrivateSlot::Register(reg))
}

pub(super) unsafe extern "C" fn body_value_scalar(context: *const std::ffi::c_void, reg: Reg) -> ScalarEncoding {
    ScalarLocations::from_emitter(unsafe { &*context.cast::<Emitter<'_>>() }).scalar(reg)
}

pub(super) unsafe extern "C" fn body_tile_slot(context: *const std::ffi::c_void, key: TileKey) -> u32 {
    let emitter = unsafe { &*context.cast::<Emitter<'_>>() };
    emitter.frame.shared_offset(key)
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::scaled_memory"]
    fn encode_scaled_memory(base: u8, index: u8, stride: u32, offset: u32, output: *mut u8) -> AddressEncoding;
    #[link_name = "bootstrap.x64_instruction::scalar_math_outgoing"]
    pub(super) fn scalar_math_outgoing(operation: u32) -> u64;
    #[link_name = "bootstrap.x64_instruction::encode"]
    fn encode_instruction(input: InstructionEncoding, opcode: *const u8, length: u64, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::copy_scalar_move"]
    fn copy_scalar_move(dst: MoveLocation, src: MoveLocation, size: u32, signed: u32, output: *mut u8) -> MoveEncoding;
    #[link_name = "bootstrap.x64_instruction::write_scalar_move"]
    fn write_scalar_move(memory: MoveLocation, src: MoveLocation, size: u32, output: *mut u8) -> u64;
}

impl Emitter<'_> {
    pub(super) fn operand(&mut self, location: Location, temporary: u8) -> Operand {
        match location {
            Location::Gpr(r) | Location::Xmm(r) => Operand::Register(r),
            Location::Stack(offset) => Operand::memory(4, offset),
            Location::Immediate(bits) => {
                self.literal_width(Location::Gpr(temporary), bits, bits > u32::MAX as Bits);
                Operand::Register(temporary)
            }
        }
    }
    pub(super) fn emit_address(&mut self, capacity: usize, emit: impl FnOnce(*mut u8) -> AddressEncoding) -> Memory {
        self.code.reserve(capacity);
        let start = self.code.len();
        unsafe {
            let result = emit(self.code.as_mut_ptr().add(start));
            self.code.set_len(start + result.length as usize);
            Memory::indexed(result.base, result.index, result.shift, result.offset)
        }
    }
    pub(super) fn scaled_memory(&mut self, base: u8, index: u8, stride: u32, offset: u32) -> Memory {
        self.emit_address(16, |output| unsafe { encode_scaled_memory(base, index, stride, offset, output) })
    }
    fn emit_instruction(&mut self, opcode: &[u8], input: InstructionEncoding) {
        // One legacy prefix, REX, ModR/M, SIB and at most four displacement bytes.
        self.code.reserve(opcode.len() + 8);
        let start = self.code.len();
        unsafe {
            let output = self.code.as_mut_ptr().add(start);
            let count = encode_instruction(input, opcode.as_ptr(), opcode.len() as u64, output);
            self.code.set_len(start + count as usize);
        }
    }
    pub(super) fn instruction(
        &mut self,
        prefix: Option<u8>,
        opcode: &[u8],
        wide: bool,
        register: u8,
        operand: Operand,
    ) {
        let (form, base, index, shift, displacement, wide) = match operand {
            Operand::Register(r) => (InstructionForm::Register, r, NO_INDEX, 0, 0, wide),
            Operand::Memory(Memory { base, index, offset }) => {
                let (index, shift) = index.unwrap_or((NO_INDEX, 0));
                (InstructionForm::Memory, base, index, shift, offset, wide)
            }
            Operand::Literal(_, wide) => (InstructionForm::RipRelative, 0, NO_INDEX, 0, 0, wide),
        };
        self.emit_instruction(opcode, InstructionEncoding {
            displacement, reg: register, base, index, shift, form,
            prefix: prefix.unwrap_or(0), wide: u16::from(wide),
        });
        if let Operand::Literal(bits, wide) = operand {
            let at = self.code.len() - 4;
            self.record_literal_at(at, (bits, wide));
        }
    }
    pub(super) fn location(&self, r: Reg) -> Location {
        self.allocation.location(PrivateSlot::Register(r))
    }
    pub(super) fn emit_bytes(&mut self, capacity: usize, emit: impl FnOnce(*mut u8) -> u64) {
        self.code.reserve(capacity);
        let start = self.code.len();
        unsafe {
            let length = emit(self.code.as_mut_ptr().add(start));
            self.code.set_len(start + length as usize);
        }
    }
    pub(super) fn copy(&mut self, dst: Location, src: Location) {
        self.copy_move(dst, src, 4, false);
    }
    pub(super) fn copy_scalar(&mut self, dst: Location, src: Location, scalar: Scalar) {
        self.copy_move(dst, src, scalar.size(), scalar.is_signed());
    }
    fn copy_move(&mut self, dst: Location, src: Location, size: u32, signed: bool) {
        let start = self.code.len();
        let source = MoveLocation::from(src);
        let literal = (source.value, false);
        self.code.reserve(24);
        let encoded = unsafe {
            let encoded = copy_scalar_move(dst.into(), source, size, u32::from(signed),
                self.code.as_mut_ptr().add(start));
            self.code.set_len(start + encoded.length as usize);
            encoded
        };
        self.record_literal(start, literal, encoded.literal_field as usize);
    }
    pub(super) fn load(&mut self, r: Reg) {
        self.copy_scalar(Location::Gpr(0), self.location(r), self.registers[r]);
    }
    pub(super) fn store(&mut self, r: Reg) {
        self.copy_scalar(self.location(r), Location::Gpr(0), self.registers[r]);
    }
    pub(super) fn literal_width(&mut self, dst: Location, bits: Bits, wide: bool) {
        self.copy_move(dst, Location::Immediate(bits), if wide { 8 } else { 4 }, false);
    }
    pub(super) fn write_memory(&mut self, src: Location, base: u8, offset: u32) {
        self.write_address(src, Memory::new(base, offset));
    }
    pub(super) fn write_address(&mut self, src: Location, memory: Memory) {
        self.write_value_address(src, memory, 4);
    }
    pub(super) fn write_value_address(&mut self, src: Location, memory: Memory, size: u32) {
        self.emit_bytes(24, |output| unsafe { write_scalar_move(memory.into(), src.into(), size, output) });
    }
    pub(super) fn record_literal_at(&mut self, at: usize, (bits, wide): (Bits, bool)) {
        let ordinal = self.literals.len();
        self.literals.push(LiteralUse { bits, field: at, ordinal, wide: u64::from(wide) });
    }

    pub(super) fn record_literal(&mut self, start: usize, literal: (Bits, bool), field: usize) {
        if field != 0 {
            self.record_literal_at(start + field, literal);
        }
    }

}
