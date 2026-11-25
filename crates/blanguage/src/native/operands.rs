//! 指令直接消费分配位置；两地址写入保留右操作数，位复制不改变标量数值。
use super::*;
use allocation::Location;
use crate::builtin_types::ScalarEncoding;
use crate::ir::dataflow::{HOST, Host, Node};

pub(super) fn bit_copy(from: Scalar, into: Scalar) -> bool {
    unsafe { scalar_bit_copy(from.into(), into.into()) != 0 }
}

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
struct ScalarLiteral {
    bits: u64,
    field: u64,
    wide: u32,
}

#[repr(C)]
struct ScalarValueEncoding {
    length: u64,
    first: ScalarLiteral,
    second: ScalarLiteral,
    call_field: u64,
    symbol_length: u32,
    symbol: [u8; 6],
}

#[repr(C)]
pub(super) struct ScalarValueHost {
    pub(super) ir: Host,
    pub(super) context: *const std::ffi::c_void,
    pub(super) location: unsafe extern "C" fn(*const std::ffi::c_void, Reg) -> MoveLocation,
    pub(super) scalar: unsafe extern "C" fn(*const std::ffi::c_void, Reg) -> ScalarEncoding,
}

pub(super) struct ScalarLocations<'a> {
    pub(super) allocation: &'a Allocation,
    pub(super) stage: &'a Stage,
    pub(super) frame: &'a Frame,
}

impl ScalarLocations<'_> {
    pub(super) fn indexed_host(&self) -> IndexedAddressHost {
        IndexedAddressHost {
            values: self.host(), tile_stream: indexed_tile_stream,
            buffer_bytes: BUFFER_BYTES, stride_field: BUFFER_STRIDE,
            workgroup: self.stage.workgroup_width(),
        }
    }

    pub(super) fn host(&self) -> ScalarValueHost {
        ScalarValueHost {
            ir: HOST, context: std::ptr::from_ref(self).cast(),
            location: value_location, scalar: value_scalar,
        }
    }
}

#[repr(C)]
struct ScalarStorageEncoding {
    length: u64,
    literal: Bits,
    field: u64,
}

#[repr(C)]
struct ScalarStorageHost {
    values: ScalarValueHost,
    local: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> MoveLocation,
    buffer_bytes: u32,
    stride_field: u32,
}

#[repr(C)]
pub(super) struct IndexedAddressHost {
    values: ScalarValueHost,
    tile_stream: unsafe extern "C" fn(*const std::ffi::c_void, usize, u32) -> IndexedTileWord,
    buffer_bytes: u32,
    stride_field: u32,
    workgroup: u32,
}

#[repr(C)]
struct IndexedTileWord { offset: u32, byte_offset: u32 }

unsafe extern "C" fn indexed_tile_stream(context: *const std::ffi::c_void, tile: usize, word: u32) -> IndexedTileWord {
    let locations = unsafe { &*context.cast::<ScalarLocations<'_>>() };
    let source = locations.stage.shared_word(tile, word);
    let byte_offset = match source {
        SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4,
    };
    IndexedTileWord { offset: locations.frame.tile_shared[&source.aligned()], byte_offset }
}

pub(super) unsafe extern "C" fn value_local(context: *const std::ffi::c_void, local: usize) -> MoveLocation {
    let locations = unsafe { &*context.cast::<ScalarLocations<'_>>() };
    locations.allocation.location(PrivateSlot::Local(local)).into()
}

unsafe extern "C" fn value_location(context: *const std::ffi::c_void, reg: Reg) -> MoveLocation {
    let locations = unsafe { &*context.cast::<ScalarLocations<'_>>() };
    locations.allocation.location(PrivateSlot::Register(reg)).into()
}

unsafe extern "C" fn value_scalar(context: *const std::ffi::c_void, reg: Reg) -> ScalarEncoding {
    let locations = unsafe { &*context.cast::<ScalarLocations<'_>>() };
    locations.stage.registers[reg].into()
}

unsafe extern "C" {
    #[link_name = "bootstrap.frame_layout::preserve_registers"]
    fn encode_preserved_registers(
        saved: *const allocation::SavedRegister, count: usize, offset: usize,
        operations: *mut UnwindOperation, output: *mut u8,
    ) -> usize;
    #[link_name = "bootstrap.x64_instruction::scaled_memory"]
    fn encode_scaled_memory(base: u8, index: u8, stride: u32, offset: u32, output: *mut u8) -> AddressEncoding;
    #[link_name = "bootstrap.x64_instruction::bit_copy"]
    fn scalar_bit_copy(from: ScalarEncoding, into: ScalarEncoding) -> u32;
    #[link_name = "bootstrap.x64_instruction::emit_indexed_storage"]
    fn emit_indexed_storage(instruction: Node, host: IndexedAddressHost,
        local: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> MoveLocation, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::emit_append"]
    fn emit_append(instruction: Node, host: ScalarValueHost, buffer_bytes: u32,
        stride_field: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::emit_atomic"]
    fn emit_atomic(instruction: Node, host: IndexedAddressHost, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::emit_view_load"]
    fn emit_view_load(instruction: Node, host: ScalarValueHost, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::emit_scalar_storage"]
    fn emit_scalar_storage(instruction: Node, host: ScalarStorageHost, output: *mut u8) -> ScalarStorageEncoding;
    #[link_name = "bootstrap.x64_instruction::emit_scalar_value"]
    fn emit_scalar_value(value: Node, host: ScalarValueHost, output: *mut u8) -> ScalarValueEncoding;
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
    pub(super) fn view_load(&mut self, inst: &Inst) {
        let locations = ScalarLocations { allocation: &self.allocation, stage: self.stage, frame: &self.frame };
        let host = locations.host();
        let start = self.code.len();
        self.code.reserve(112);
        unsafe {
            let length = emit_view_load(Node::instruction(inst), host, self.code.as_mut_ptr().add(start));
            self.code.set_len(start + length as usize);
        }
    }

    pub(super) fn indexed_storage(&mut self, inst: &Inst) {
        let locations = ScalarLocations { allocation: &self.allocation, stage: self.stage, frame: &self.frame };
        let host = locations.indexed_host();
        let start = self.code.len();
        self.code.reserve(208);
        unsafe {
            let length = emit_indexed_storage(Node::instruction(inst), host, value_local,
                self.code.as_mut_ptr().add(start));
            self.code.set_len(start + length as usize);
        }
    }

    pub(super) fn append(&mut self, inst: &Inst, field_count: usize) {
        let locations = ScalarLocations { allocation: &self.allocation, stage: self.stage, frame: &self.frame };
        let host = locations.host();
        let start = self.code.len();
        self.code.reserve(148 + field_count * 24);
        unsafe {
            let length = emit_append(Node::instruction(inst), host, BUFFER_BYTES, BUFFER_STRIDE,
                self.code.as_mut_ptr().add(start));
            self.code.set_len(start + length as usize);
        }
    }

    pub(super) fn atomic(&mut self, inst: &Inst) {
        let locations = ScalarLocations { allocation: &self.allocation, stage: self.stage, frame: &self.frame };
        let host = locations.indexed_host();
        let start = self.code.len();
        self.code.reserve(312);
        unsafe {
            let length = emit_atomic(Node::instruction(inst), host, self.code.as_mut_ptr().add(start));
            self.code.set_len(start + length as usize);
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
            self.literals.entry((bits, wide)).or_default().push(at);
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
        self.copy_scalar(Location::Gpr(0), self.location(r), self.stage.registers[r]);
    }
    pub(super) fn store(&mut self, r: Reg) {
        self.copy_scalar(self.location(r), Location::Gpr(0), self.stage.registers[r]);
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
    pub(super) fn scalar_storage(&mut self, inst: &Inst) {
        let locations = ScalarLocations { allocation: &self.allocation, stage: self.stage, frame: &self.frame };
        let host = ScalarStorageHost {
            values: locations.host(), local: value_local,
            buffer_bytes: BUFFER_BYTES, stride_field: BUFFER_STRIDE,
        };
        let start = self.code.len();
        self.code.reserve(68);
        let encoding = unsafe {
            let encoding = emit_scalar_storage(Node::instruction(inst), host, self.code.as_mut_ptr().add(start));
            self.code.set_len(start + encoding.length as usize);
            encoding
        };
        self.record_literal(start, (encoding.literal, false), encoding.field as usize);
    }
    pub(super) fn scalar_value(&mut self, inst: &Inst) {
        let locations = ScalarLocations { allocation: &self.allocation, stage: self.stage, frame: &self.frame };
        let host = locations.host();
        let start = self.code.len();
        self.code.reserve(128);
        let encoded = unsafe {
            let encoded = emit_scalar_value(Node::instruction(inst), host, self.code.as_mut_ptr().add(start));
            self.code.set_len(start + encoded.length as usize);
            encoded
        };
        if encoded.symbol_length != 0 {
            // The B encoder produces the actual ASCII CRT symbol.
            let name = unsafe { std::str::from_utf8_unchecked(&encoded.symbol[..encoded.symbol_length as usize]) };
            let target = NativeSymbol::External(crate::ast::ExternalName {
                library: "ucrtbase.dll".into(), symbol: name.into(),
            });
            self.record_symbol_target(start + encoded.call_field as usize, target, true);
        }
        for literal in [encoded.first, encoded.second] {
            self.record_literal(start, (literal.bits, literal.wide != 0), literal.field as usize);
        }
    }
    pub(super) fn record_literal(&mut self, start: usize, literal: (Bits, bool), field: usize) {
        if field != 0 {
            self.literals.entry(literal).or_default().push(start + field);
        }
    }
    pub(super) fn save_registers(&mut self) {
        let saved = &self.frame.saved;
        if saved.is_empty() {
            return;
        }
        let start = self.code.len();
        let count = saved.len();
        let unwind_start = self.unwind.len();
        self.unwind.reserve(count);
        self.code.reserve(9 * count);
        unsafe {
            let bytes = encode_preserved_registers(
                saved.as_ptr(), count, start, self.unwind.as_mut_ptr().add(unwind_start),
                self.code.as_mut_ptr().add(start),
            );
            self.code.set_len(start + bytes);
            self.unwind.set_len(unwind_start + count);
            self.prolog = (start + bytes) as u8;
        }
    }

}
