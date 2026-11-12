//! 指令直接消费分配位置；两地址写入保留右操作数，位复制不改变标量数值。
use super::*;
use allocation::Location;

pub(super) fn bit_copy(from: Scalar, into: Scalar) -> bool {
    from == into
        || from.is_integer() && into.is_integer() && from.size() == into.size()
        || from == Scalar::Bool && matches!(into, Scalar::U32 | Scalar::I32)
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
struct ComparisonEncoding {
    length: u64,
    left_literal: u64,
    right_literal: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::scalar_compare"]
    fn scalar_compare(dst: MoveLocation, left: MoveLocation, right: MoveLocation,
        size: u32, signed: u32, floating: u32, operation: crate::builtin_types::ScalarBinaryOp,
        output: *mut u8) -> ComparisonEncoding;
    #[link_name = "bootstrap.x64_instruction::cast_boolean"]
    fn cast_boolean(dst: MoveLocation, src: MoveLocation,
        size: u32, signed: u32, floating: u32, output: *mut u8) -> MoveEncoding;
    #[link_name = "bootstrap.x64_instruction::encode"]
    fn encode_instruction(input: InstructionEncoding, opcode: *const u8, length: u64, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::copy_scalar_move"]
    fn copy_scalar_move(dst: MoveLocation, src: MoveLocation, size: u32, signed: u32, output: *mut u8) -> MoveEncoding;
    #[link_name = "bootstrap.x64_instruction::read_scalar_move"]
    fn read_scalar_move(dst: MoveLocation, memory: MoveLocation, size: u32, signed: u32, boolean: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::write_scalar_move"]
    fn write_scalar_move(memory: MoveLocation, src: MoveLocation, size: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::unsigned_to_float"]
    fn encode_unsigned_to_float(target: u8, prefix: u8, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::float_to_unsigned"]
    fn encode_float_to_unsigned(target: u8, prefix: u8, output: *mut u8) -> u64;
}

impl Emitter<'_> {
    fn float_operand(&mut self, location: Location, scalar: Scalar) -> Operand {
        match location {
            Location::Immediate(bits) => Operand::Literal(bits, scalar.size() == 8),
            location => self.operand(location, 1),
        }
    }
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
        let start = self.code.len();
        let src = MoveLocation::from(src);
        let bits = src.value;
        let mut literal = 0;
        self.emit_bytes(24, |output| {
            let result = unsafe { copy_scalar_move(dst.into(), src, size, u32::from(signed), output) };
            literal = result.literal_field as usize;
            result.length
        });
        if literal != 0 {
            self.literals.entry((bits, false)).or_default().push(start + literal);
        }
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
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        // legacy SSE 是两地址指令：目标若占了尚未读取的右值，先在 XMM0 计算。
        let target = match d {
            Location::Xmm(r) if d != b || a == b => r,
            _ => 0,
        };
        self.copy_scalar(Location::Xmm(target), a, scalar);
        let right = self.float_operand(b, scalar);
        self.instruction(Some(if scalar == Scalar::F64 { 0xf2 } else { 0xf3 }), &[0x0f, opcode], false, target, right);
        self.copy_scalar(d, Location::Xmm(target), self.stage.registers[dst]);
    }
    pub(super) fn integer_binary(
        &mut self,
        dst: Reg,
        a: Reg,
        b: Reg,
        opcode: &[u8],
        immediate: Option<u8>,
    ) {
        let scalar = self.stage.registers[dst];
        let wide = scalar.size() == 8;
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        let target = match d {
            Location::Gpr(r) if d != b || a == b => r,
            _ => 0,
        };
        self.copy_scalar(Location::Gpr(target), a, scalar);
        if scalar.size() < 4 {
            self.copy_scalar(Location::Gpr(1), b, scalar);
            self.instruction(None, opcode, false, target, Operand::Register(1));
        } else if let Location::Immediate(bits) = b {
            if wide && bits as i64 != bits as i32 as i64 {
                self.literal_width(Location::Gpr(1), bits, true);
                self.instruction(None, opcode, true, target, Operand::Register(1));
            } else if let Some(extension) = immediate {
                self.integer_immediate(extension, target, bits, wide);
            } else {
                self.instruction(None, &[0x69], wide, target, Operand::Register(target));
                self.u32(bits as u32);
            }
        } else {
            let right = self.operand(b, 1);
            self.instruction(None, opcode, wide, target, right);
        }
        self.copy_scalar(d, Location::Gpr(target), scalar);
    }
    fn integer_immediate(&mut self, extension: u8, target: u8, bits: Bits, wide: bool) {
        let signed = if wide { bits as i64 } else { bits as i32 as i64 };
        if i8::try_from(signed).is_ok() {
            self.instruction(None, &[0x83], wide, extension, Operand::Register(target));
            self.bytes(&[bits as u8]);
        } else {
            self.instruction(None, &[0x81], wide, extension, Operand::Register(target));
            self.u32(bits as u32);
        }
    }
    pub(super) fn cast(&mut self, dst: Reg, src: Reg) {
        let from = self.stage.registers[src];
        let into = self.stage.registers[dst];
        let (d, source) = (self.location(dst), self.location(src));
        if from == Scalar::U64 && into.is_float() {
            self.unsigned64_to_float(dst, src);
        } else if from.is_float() && into == Scalar::U64 {
            self.float_to_unsigned64(dst, src);
        } else if from != into && from.is_float() && into.is_float() {
            let target = match d { Location::Xmm(r) => r, _ => 0 };
            let operand = self.float_operand(source, from);
            self.instruction(Some(if from == Scalar::F64 { 0xf2 } else { 0xf3 }), &[0x0f, 0x5a], false, target, operand);
            self.copy_scalar(d, Location::Xmm(target), into);
        } else if bit_copy(from, into) {
            self.copy_scalar(d, source, into);
        } else if into == Scalar::Bool {
            let start = self.code.len();
            let source = MoveLocation::from(source);
            let bits = source.value;
            let mut literal = 0;
            self.emit_bytes(64, |output| {
                let result = unsafe { cast_boolean(d.into(), source, from.size(),
                    u32::from(from.is_signed()), u32::from(from.is_float()), output) };
                literal = result.literal_field as usize;
                result.length
            });
            if literal != 0 {
                self.literals.entry((bits, false)).or_default().push(start + literal);
            }
        } else if !from.is_float() && !into.is_float() {
            let target = match d { Location::Gpr(r) => r, _ => 0 };
            let register = Location::Gpr(target);
            if into.size() <= 4 {
                self.copy_scalar(register, source, if from.size() < 4 { from } else { into });
                if into.size() == 4 && from.size() >= 4 && source == register {
                    // A narrowing cast produces a real 32-bit value, including
                    // the zeroed upper half when allocation reuses the input.
                    self.instruction(None, &[0x8b], false, target, Operand::Register(target));
                }
            } else {
                self.copy_scalar(register, source, from);
                if from.is_signed() && from.size() < 8 {
                    self.instruction(None, &[0x63], true, target, Operand::Register(target));
                }
            }
            self.copy_scalar(d, register, into);
        } else if from.is_float() {
            let target = match d {
                Location::Gpr(r) => r,
                _ => 0,
            };
            let source = self.float_operand(source, from);
            self.instruction(
                Some(if from == Scalar::F64 { 0xf2 } else { 0xf3 }),
                &[0x0f, 0x2c],
                into == Scalar::U32 || into.size() == 8,
                target,
                source,
            );
            if into == Scalar::U32 {
                self.instruction(None, &[0x8b], false, target, Operand::Register(target));
            }
            self.copy_scalar(d, Location::Gpr(target), into);
        } else {
            let target = match d {
                Location::Xmm(r) => r,
                _ => 0,
            };
            let source = if from.size() < 4 || from == Scalar::U32 && matches!(source, Location::Stack(_)) {
                // 无符号转换读取零扩展的 64 位整数；栈值只有四字节。
                self.copy_scalar(Location::Gpr(0), source, from);
                Operand::Register(0)
            } else {
                // GPR 的 32 位生产者已清零高位，bool 则只有规范的 0／1。
                self.operand(source, 1)
            };
            // CVTSI2SS 保留目标高 96 位，不让旧浮点结果成为新转换的依赖。
            self.instruction(
                None,
                &[0x0f, 0x57],
                false,
                target,
                Operand::Register(target),
            );
            self.instruction(
                Some(if into == Scalar::F64 { 0xf2 } else { 0xf3 }),
                &[0x0f, 0x2a],
                from == Scalar::U32 || from.size() == 8,
                target,
                source,
            );
            self.copy_scalar(d, Location::Xmm(target), self.stage.registers[dst]);
        }
    }
    fn unsigned64_to_float(&mut self, dst: Reg, src: Reg) {
        let location = self.location(dst);
        let prefix = if self.stage.registers[dst] == Scalar::F64 { 0xf2 } else { 0xf3 };
        let target = match location { Location::Xmm(r) => r, _ => 0 };
        self.load(src);
        self.code.reserve(49);
        let start = self.code.len();
        unsafe {
            let count = encode_unsigned_to_float(target, prefix, self.code.as_mut_ptr().add(start));
            self.code.set_len(start + count as usize);
        }
        self.copy_scalar(location, Location::Xmm(target), self.stage.registers[dst]);
    }
    fn float_to_unsigned64(&mut self, dst: Reg, src: Reg) {
        let location = self.location(dst);
        let prefix = if self.stage.registers[src] == Scalar::F64 { 0xf2 } else { 0xf3 };
        let target = match location { Location::Gpr(r) => r, _ => 0 };
        self.float(src, 0);
        self.literal_width(Location::Xmm(1), if prefix == 0xf2 { (2.0f64.powi(63)).to_bits() } else { 0x5f000000 }, prefix == 0xf2);
        self.code.reserve(34);
        let start = self.code.len();
        unsafe {
            let count = encode_float_to_unsigned(target, prefix, self.code.as_mut_ptr().add(start));
            self.code.set_len(start + count as usize);
        }
        self.copy_scalar(location, Location::Gpr(target), Scalar::U64);
    }
    pub(super) fn float_sign(&mut self, dst: Reg, src: Reg, negate: bool) {
        let scalar = self.stage.registers[src];
        self.load(src);
        self.instruction(None, &[0x0f, 0xba], scalar.size() == 8, if negate { 7 } else { 6 }, Operand::Register(0));
        self.bytes(&[(scalar.bit_width()-1) as u8]);
        self.store(dst);
    }
    pub(super) fn float_sqrt(&mut self, dst: Reg, source: Reg) {
        let d = self.location(dst);
        let target = match d {
            Location::Xmm(r) => r,
            _ => 0,
        };
        let source = self.float_operand(self.location(source), self.stage.registers[source]);
        self.instruction(Some(if self.stage.registers[dst] == Scalar::F64 { 0xf2 } else { 0xf3 }), &[0x0f, 0x51], false, target, source);
        self.copy_scalar(d, Location::Xmm(target), self.stage.registers[dst]);
    }
    pub(super) fn integer_unary(
        &mut self,
        dst: Reg,
        source: Reg,
        opcode: u8,
        extension: u8,
        immediate: Option<u8>,
    ) {
        let scalar = self.stage.registers[dst];
        let wide = scalar.size() == 8;
        let (d, source) = (self.location(dst), self.location(source));
        let target = match d {
            Location::Gpr(_) => d,
            Location::Stack(_) if d == source && scalar.size() >= 4 => d,
            _ => Location::Gpr(0),
        };
        self.copy_scalar(target, source, scalar);
        let operand = self.operand(target, 1);
        self.instruction(None, &[opcode], wide, extension, operand);
        if let Some(value) = immediate {
            self.bytes(&[value]);
        }
        self.copy_scalar(d, target, scalar);
    }
    pub(super) fn shift(&mut self, dst: Reg, a: Reg, b: Reg, kind: u8) {
        let scalar = self.stage.registers[dst];
        let wide = scalar.size() == 8;
        let shift = self.location(b);
        if !matches!(shift, Location::Immediate(_)) {
            self.copy_scalar(Location::Gpr(1), shift, self.stage.registers[b]);
            if scalar.size() < 4 {
                self.instruction(None, &[0x83], false, 4, Operand::Register(1));
                self.bytes(&[(scalar.bit_width() - 1) as u8]);
            }
        }
        let d = self.location(dst);
        let target = match d {
            Location::Gpr(r) => r,
            _ => 0,
        };
        self.copy_scalar(Location::Gpr(target), self.location(a), scalar);
        if let Location::Immediate(bits) = shift {
            self.instruction(None, &[0xc1], wide, kind, Operand::Register(target));
            self.bytes(&[(bits & Bits::from(scalar.bit_width() - 1)) as u8]);
        } else {
            self.instruction(None, &[0xd3], wide, kind, Operand::Register(target));
        }
        self.copy_scalar(d, Location::Gpr(target), scalar);
    }
    pub(super) fn compare(&mut self, dst: Reg, a: Reg, b: Reg, comparison: Comparison) {
        let scalar = self.stage.registers[a];
        let destination = MoveLocation::from(self.location(dst));
        let left = MoveLocation::from(self.location(a));
        let right = MoveLocation::from(self.location(b));
        let (left_bits, right_bits) = (left.value, right.value);
        let start = self.code.len();
        let mut fields = (0, 0);
        self.emit_bytes(64, |output| {
            let result = unsafe { scalar_compare(destination, left, right, scalar.size(),
                u32::from(scalar.is_signed()), u32::from(scalar.is_float()),
                BinaryOp::Compare(comparison).into(), output) };
            fields = (result.left_literal as usize, result.right_literal as usize);
            result.length
        });
        if fields.0 != 0 {
            self.literals.entry((left_bits, false)).or_default().push(start + fields.0);
        }
        if fields.1 != 0 {
            self.literals.entry((right_bits, scalar.size() == 8)).or_default().push(start + fields.1);
        }
    }
    pub(super) fn select(&mut self, dst: Reg, condition: Reg, a: Reg, b: Reg) {
        let scalar = self.stage.registers[dst];
        let wide = scalar.size() == 8;
        let (d, a, b) = (self.location(dst), self.location(a), self.location(b));
        if let Location::Immediate(bits) = self.location(condition) {
            self.copy_scalar(d, if bits != 0 { a } else { b }, scalar);
            return;
        }
        let condition = self.operand(self.location(condition), 2);
        self.instruction(None, &[0x83], false, 7, condition);
        self.bytes(&[0]);
        let target = match d {
            Location::Gpr(r) => r,
            _ => 0,
        };
        // 目标已有真值时改用 CMOVZ，避免先写入假值破坏尚未读取的真值。
        let (initial, source, opcode) = if d == a { (a, b, 0x44) } else { (b, a, 0x45) };
        self.copy_scalar(Location::Gpr(target), initial, scalar);
        let source = match source {
            source if scalar.size() < 4 || matches!(source, Location::Xmm(_) | Location::Immediate(_)) => {
                self.copy_scalar(Location::Gpr(1), source, scalar);
                Operand::Register(1)
            }
            location => self.operand(location, 1),
        };
        self.instruction(None, &[0x0f, opcode], wide, target, source);
        self.copy_scalar(d, Location::Gpr(target), scalar);
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
