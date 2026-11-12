//! 指令直接消费分配位置；两地址写入保留右操作数，位复制不改变标量数值。
use super::*;
use allocation::Location;
use crate::builtin_types::ScalarEncoding;

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
    pub(super) fn offset(mut self, offset: u32) -> Self {
        self.offset += offset;
        self
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
struct MoveLocation {
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
struct BinaryEncoding {
    length: u64,
    left_literal: u64,
    right_literal: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::bit_copy"]
    fn scalar_bit_copy(from: ScalarEncoding, into: ScalarEncoding) -> u32;
    #[link_name = "bootstrap.x64_instruction::value_unary"]
    fn value_unary(dst: MoveLocation, src: MoveLocation, scalar: ScalarEncoding,
        operation: u32, output: *mut u8) -> MoveEncoding;
    #[link_name = "bootstrap.x64_instruction::value_math"]
    fn value_math(dst: MoveLocation, src: MoveLocation, size: u32,
        sqrt: u32, output: *mut u8) -> MoveEncoding;
    #[link_name = "bootstrap.x64_instruction::value_cast"]
    fn value_cast(dst: MoveLocation, src: MoveLocation, from: ScalarEncoding,
        into: ScalarEncoding, output: *mut u8) -> BinaryEncoding;
    #[link_name = "bootstrap.x64_instruction::value_select"]
    fn value_select(dst: MoveLocation, condition: MoveLocation, left: MoveLocation,
        right: MoveLocation, size: u32, signed: u32, output: *mut u8) -> BinaryEncoding;
    #[link_name = "bootstrap.x64_instruction::scalar_binary"]
    fn scalar_binary(dst: MoveLocation, left: MoveLocation, right: MoveLocation,
        size: u32, signed: u32, floating: u32, count_size: u32, count_signed: u32,
        operation: crate::builtin_types::ScalarBinaryOp, output: *mut u8) -> BinaryEncoding;
    #[link_name = "bootstrap.x64_instruction::float_binary"]
    fn encode_float_binary(dst: MoveLocation, left: MoveLocation, right: MoveLocation,
        size: u32, opcode: u8, output: *mut u8) -> BinaryEncoding;
    #[link_name = "bootstrap.x64_instruction::scalar_compare"]
    fn scalar_compare(dst: MoveLocation, left: MoveLocation, right: MoveLocation,
        size: u32, signed: u32, floating: u32, operation: crate::builtin_types::ScalarBinaryOp,
        output: *mut u8) -> BinaryEncoding;
    #[link_name = "bootstrap.x64_instruction::encode"]
    fn encode_instruction(input: InstructionEncoding, opcode: *const u8, length: u64, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::copy_scalar_move"]
    fn copy_scalar_move(dst: MoveLocation, src: MoveLocation, size: u32, signed: u32, output: *mut u8) -> MoveEncoding;
    #[link_name = "bootstrap.x64_instruction::read_scalar_move"]
    fn read_scalar_move(dst: MoveLocation, memory: MoveLocation, size: u32, signed: u32, boolean: u32, output: *mut u8) -> u64;
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
    pub(super) fn value_memory(
        &mut self,
        base: u8,
        index: Location,
        stride: u32,
        offset: u32,
    ) -> Memory {
        let shift = stride.trailing_zeros().min(3) as u8;
        if stride >> shift == 1
            && let Location::Gpr(index) = index
        {
            Memory {
                base,
                index: Some((index, shift)),
                offset,
            }
        } else {
            self.load_location(index);
            self.scaled_memory(base, 0, stride, offset)
        }
    }
    pub(super) fn view_memory(&mut self,base:u8,index:Location,stride:ElementStride,offset:u32)->Memory{
        match stride {
            ElementStride::Constant(bytes) if bytes<=u64::from(u32::MAX)=>return self.value_memory(base,index,bytes as u32,offset),
            ElementStride::Constant(bytes)=>self.literal_width(Location::Gpr(1),bytes,true),
            ElementStride::Value(words)=>{
                self.load(words[1]);self.bytes(&[0x48,0xc1,0xe0,32,0x48,0x89,0xc1]);
                self.load(words[0]);self.bytes(&[0x48,0x09,0xc1]);
            },
        }
        self.load_location(index);self.bytes(&[0x48,0x0f,0xaf,0xc1]);
        Memory{base,index:Some((0,0)),offset}
    }
    pub(super) fn scaled_memory(
        &mut self,
        base: u8,
        index: u8,
        stride: u32,
        offset: u32,
    ) -> Memory {
        let shift = stride.trailing_zeros().min(3) as u8;
        self.scale_index(index, stride >> shift);
        Memory {
            base,
            index: Some((index, shift)),
            offset,
        }
    }
    pub(super) fn scale_index(&mut self, register: u8, stride: u32) {
        if stride == 1 {
            return;
        }
        if stride.is_power_of_two() {
            self.instruction(None, &[0xc1], true, 4, Operand::Register(register));
            self.bytes(&[stride.trailing_zeros() as u8]);
        } else {
            self.instruction(None, &[0x69], true, register, Operand::Register(register));
            self.u32(stride);
        }
    }
    // R15 holds the base, RAX the zero-extended index; only the address is consumed.
    pub(super) fn offset_address(&mut self, stride: u32) {
        if stride.is_power_of_two() && stride <= 8 {
            let memory = Memory::indexed(15, 0, stride.trailing_zeros() as u8, 0);
            self.instruction(None, &[0x8d], true, 15, memory.into());
        } else {
            self.scale_index(0, stride);
            self.bytes(&[0x49, 0x01, 0xc7]);
        }
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
    fn emit_bytes(&mut self, capacity: usize, emit: impl FnOnce(*mut u8) -> u64) {
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
        self.emit_scalar(24, dst, src, false, |d, s, output| unsafe {
            copy_scalar_move(d, s, size, u32::from(signed), output)
        });
    }
    fn emit_scalar(&mut self, capacity: usize, dst: Location, src: Location, wide: bool,
        emit: impl FnOnce(MoveLocation, MoveLocation, *mut u8) -> MoveEncoding) {
        let destination = MoveLocation::from(dst);
        let source = MoveLocation::from(src);
        let literal = (source.value, wide);
        self.emit_literals(capacity, literal, (0, false), |output| {
            let result = emit(destination, source, output);
            BinaryEncoding { length: result.length, left_literal: result.literal_field, right_literal: 0 }
        });
    }
    pub(super) fn load_location(&mut self, src: Location) {
        self.copy(Location::Gpr(0), src);
    }
    pub(super) fn load(&mut self, r: Reg) {
        self.copy_scalar(Location::Gpr(0), self.location(r), self.stage.registers[r]);
    }
    pub(super) fn store(&mut self, r: Reg) {
        self.copy_scalar(self.location(r), Location::Gpr(0), self.stage.registers[r]);
    }
    pub(super) fn float(&mut self, r: Reg, xmm: u8) {
        self.copy_scalar(Location::Xmm(xmm), self.location(r), self.stage.registers[r]);
    }
    pub(super) fn store_float(&mut self, r: Reg) {
        self.copy_scalar(self.location(r), Location::Xmm(0), self.stage.registers[r]);
    }
    pub(super) fn constant(&mut self, r: Reg, bits: Bits) {
        self.copy_scalar(self.location(r), Location::Immediate(bits), self.stage.registers[r]);
    }
    pub(super) fn literal_width(&mut self, dst: Location, bits: Bits, wide: bool) {
        self.copy_move(dst, Location::Immediate(bits), if wide { 8 } else { 4 }, false);
    }
    pub(super) fn read_scalar(&mut self, dst: Reg, base: u8, offset: u32) {
        self.read_scalar_address(dst, Memory::new(base, offset));
    }
    pub(super) fn read_scalar_address(&mut self, dst: Reg, memory: Memory) {
        let scalar = self.stage.registers[dst];
        let dst = self.location(dst);
        self.emit_bytes(24, |output| unsafe {
            read_scalar_move(dst.into(), memory.into(), scalar.size(), u32::from(scalar.is_signed()), u32::from(scalar == Scalar::Bool), output)
        });
    }
    pub(super) fn write_memory(&mut self, src: Location, base: u8, offset: u32) {
        self.write_address(src, Memory::new(base, offset));
    }
    pub(super) fn write_address(&mut self, src: Location, memory: Memory) {
        self.write_value_address(src, memory, 4);
    }
    pub(super) fn write_scalar(&mut self, src: Reg, base: u8, offset: u32) {
        self.write_scalar_address(src, Memory::new(base, offset));
    }
    pub(super) fn write_scalar_address(&mut self, src: Reg, memory: Memory) {
        self.write_value_address(self.location(src), memory, self.stage.registers[src].size());
    }
    pub(super) fn write_value_address(&mut self, src: Location, memory: Memory, size: u32) {
        self.emit_bytes(24, |output| unsafe { write_scalar_move(memory.into(), src.into(), size, output) });
    }
    pub(super) fn float_binary(&mut self, dst: Reg, a: Reg, b: Reg, opcode: u8) {
        let scalar = self.stage.registers[dst];
        self.emit_binary(dst, a, b, scalar, |d, a, b, output| unsafe {
            encode_float_binary(d, a, b, scalar.size(), opcode, output)
        });
    }
    pub(super) fn binary(&mut self, dst: Reg, a: Reg, b: Reg, operation: BinaryOp) {
        let scalar = self.stage.registers[dst];
        let count = self.stage.registers[b];
        self.emit_binary(dst, a, b, scalar, |d, a, b, output| unsafe {
            scalar_binary(d, a, b, scalar.size(), u32::from(scalar.is_signed()),
                u32::from(scalar.is_float()), count.size(), u32::from(count.is_signed()),
                operation.into(), output)
        });
    }
    pub(super) fn cast(&mut self, dst: Reg, src: Reg) {
        let from = self.stage.registers[src];
        let into = self.stage.registers[dst];
        let destination = MoveLocation::from(self.location(dst));
        let source = MoveLocation::from(self.location(src));
        let literal = (source.value, from.size() == 8);
        self.emit_literals(80, literal, (0x5f000000, false), |output| unsafe {
            value_cast(destination, source, from.into(), into.into(), output)
        });
    }
    pub(super) fn unary(&mut self, dst: Reg, src: Reg, operation: UnaryOp) {
        let scalar = self.stage.registers[dst];
        self.emit_scalar(64, self.location(dst), self.location(src), false, |d, s, output| unsafe {
            value_unary(d, s, scalar.into(), operation as u32, output)
        });
    }
    pub(super) fn float_math(&mut self, dst: Reg, src: Reg, sqrt: bool) {
        let scalar = self.stage.registers[src];
        self.emit_scalar(64, self.location(dst), self.location(src), scalar.size() == 8, |d, s, output| unsafe {
            value_math(d, s, scalar.size(), u32::from(sqrt), output)
        });
    }
    pub(super) fn compare(&mut self, dst: Reg, a: Reg, b: Reg, comparison: Comparison) {
        let scalar = self.stage.registers[a];
        self.emit_binary(dst, a, b, scalar, |d, a, b, output| unsafe {
            scalar_compare(d, a, b, scalar.size(), u32::from(scalar.is_signed()),
                u32::from(scalar.is_float()), BinaryOp::Compare(comparison).into(), output)
        });
    }
    fn emit_binary(&mut self, dst: Reg, a: Reg, b: Reg, scalar: Scalar,
        emit: impl FnOnce(MoveLocation, MoveLocation, MoveLocation, *mut u8) -> BinaryEncoding) {
        let destination = MoveLocation::from(self.location(dst));
        let left = MoveLocation::from(self.location(a));
        let right = MoveLocation::from(self.location(b));
        self.emit_literals(64, (left.value, false), (right.value, scalar.size() == 8),
            |output| emit(destination, left, right, output));
    }
    fn emit_literals(&mut self, capacity: usize, left: (Bits, bool), right: (Bits, bool),
        emit: impl FnOnce(*mut u8) -> BinaryEncoding) {
        let start = self.code.len();
        let mut fields = (0, 0);
        self.emit_bytes(capacity, |output| {
            let result = emit(output);
            fields = (result.left_literal as usize, result.right_literal as usize);
            result.length
        });
        for (literal, field) in [(left, fields.0), (right, fields.1)] {
            if field != 0 {
                self.literals.entry(literal).or_default().push(start + field);
            }
        }
    }
    pub(super) fn select(&mut self, dst: Reg, condition: Reg, a: Reg, b: Reg) {
        let scalar = self.stage.registers[dst];
        let condition = MoveLocation::from(self.location(condition));
        self.emit_binary(dst, a, b, scalar, |d, a, b, output| unsafe {
            value_select(d, condition, a, b, scalar.size(), u32::from(scalar.is_signed()), output)
        });
    }
    pub(super) fn preserve_registers(&mut self, restore: bool) {
        for index in 0..self.frame.saved.len() {
            let saved = self.frame.saved[index];
            let register = saved.register as u8;
            let xmm = saved.bank != 0;
            let opcode: &[u8] = if xmm {
                &[0x0f, if restore { 0x28 } else { 0x29 }]
            } else {
                &[if restore { 0x8b } else { 0x89 }]
            };
            self.instruction(None, opcode, !xmm, register, Operand::memory(4, saved.offset));
            if !restore {
                let at = self.code.len() as u8;
                self.unwind.push(UnwindOperation::save(at, register, saved.offset, xmm));
                self.prolog = at;
            }
        }
    }
}
