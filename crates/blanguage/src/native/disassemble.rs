//! A read-only view of an explicitly selected x64 instruction byte range.
use iced_x86::{ConstantOffsets, Decoder, DecoderOptions, Instruction, InstructionInfoFactory, OpKind, Register, RflagsBits};
use std::{fmt::{self, Display, Write}, error::Error};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub offset: usize,
    pub ip: u64,
    pub reason: String,
}
impl Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "x64 instruction decode failed at byte {} / IP 0x{:016x}: {}", self.offset, self.ip, self.reason)
    }
}
impl Error for DecodeError {}

pub(super) fn text(value: &str) -> String {
    let mut out = String::new(); crate::json::string(&mut out, value); out
}
fn name(value: &impl fmt::Debug) -> String { text(&format!("{value:?}")) }
pub(super) fn bits(value: u64) -> String { text(&format!("0x{value:016x}")) }
pub(super) fn bytes(value: &[u8]) -> String {
    let mut out = String::from("\"");
    for value in value { write!(&mut out, "{value:02x}").unwrap(); }
    out.push('"'); out
}
pub(super) fn array(values: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", values.into_iter().collect::<Vec<_>>().join(","))
}
pub(super) fn object(fields: &[(&str, String)]) -> String {
    format!("{{{}}}", fields.iter().map(|(key, value)| format!("{}:{value}", text(key))).collect::<Vec<_>>().join(","))
}
fn register(value: Register) -> String {
    if value == Register::None { "null".into() }
    else { object(&[("name", name(&value)), ("size_bytes", value.size().to_string())]) }
}
fn memory_size(value: iced_x86::MemorySize) -> String {
    object(&[("name", name(&value)), ("size_bytes", if value.size() == 0 { "null".into() } else { value.size().to_string() })])
}
fn constant(offset: usize, size: usize, raw: &[u8]) -> String {
    if size == 0 { "null".into() }
    else { object(&[("offset_bytes", offset.to_string()), ("length_bytes", size.to_string()), ("bytes", bytes(&raw[offset..offset + size]))]) }
}
fn flags(mask: u32) -> String {
    let names = [(RflagsBits::OF,"OF"), (RflagsBits::SF,"SF"), (RflagsBits::ZF,"ZF"),
        (RflagsBits::AF,"AF"), (RflagsBits::CF,"CF"), (RflagsBits::PF,"PF"),
        (RflagsBits::DF,"DF"), (RflagsBits::IF,"IF"), (RflagsBits::AC,"AC"),
        (RflagsBits::UIF,"UIF"), (RflagsBits::C0,"C0"), (RflagsBits::C1,"C1"),
        (RflagsBits::C2,"C2"), (RflagsBits::C3,"C3")];
    array(names.into_iter().filter(|(bit, _)| mask & bit != 0).map(|(_, value)| text(value)))
}
fn memory(instruction: &Instruction, kind: OpKind) -> String {
    let (base, segment) = match kind {
        OpKind::MemorySegSI => (Register::SI, instruction.memory_segment()),
        OpKind::MemorySegESI => (Register::ESI, instruction.memory_segment()),
        OpKind::MemorySegRSI => (Register::RSI, instruction.memory_segment()),
        OpKind::MemorySegDI => (Register::DI, instruction.memory_segment()),
        OpKind::MemorySegEDI => (Register::EDI, instruction.memory_segment()),
        OpKind::MemorySegRDI => (Register::RDI, instruction.memory_segment()),
        OpKind::MemoryESDI => (Register::DI, Register::ES),
        OpKind::MemoryESEDI => (Register::EDI, Register::ES),
        OpKind::MemoryESRDI => (Register::RDI, Register::ES),
        _ => (instruction.memory_base(), instruction.memory_segment()),
    };
    let explicit = kind == OpKind::Memory;
    let displacement = if explicit && instruction.is_ip_rel_memory_operand() {
        object(&[("kind", text("ip_relative_target")), ("address", bits(instruction.ip_rel_memory_address()))])
    } else {
        object(&[("kind", text("displacement")), ("bits", bits(if explicit { instruction.memory_displacement64() } else { 0 }))])
    };
    object(&[("segment", register(segment)), ("base", register(base)),
        ("index", register(if explicit { instruction.memory_index() } else { Register::None })),
        ("scale", if explicit { instruction.memory_index_scale() } else { 1 }.to_string()),
        ("displacement", displacement), ("size", memory_size(instruction.memory_size()))])
}
fn operand(instruction: &Instruction, operand: u32) -> Result<String, String> {
    let kind = instruction.op_kind(operand);
    let value = match kind {
        OpKind::Register => object(&[("register", register(instruction.op_register(operand)))]),
        OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64 => object(&[("target_ip", bits(instruction.near_branch_target()))]),
        OpKind::FarBranch16 => object(&[("selector", instruction.far_branch_selector().to_string()), ("offset_bits", bits(instruction.far_branch16() as u64))]),
        OpKind::FarBranch32 => object(&[("selector", instruction.far_branch_selector().to_string()), ("offset_bits", bits(instruction.far_branch32() as u64))]),
        OpKind::Immediate8 | OpKind::Immediate8_2nd | OpKind::Immediate16 | OpKind::Immediate32 | OpKind::Immediate64
        | OpKind::Immediate8to16 | OpKind::Immediate8to32 | OpKind::Immediate8to64 | OpKind::Immediate32to64 =>
            object(&[("value_bits", bits(instruction.immediate(operand)))]),
        OpKind::Memory | OpKind::MemorySegSI | OpKind::MemorySegESI | OpKind::MemorySegRSI
        | OpKind::MemorySegDI | OpKind::MemorySegEDI | OpKind::MemorySegRDI
        | OpKind::MemoryESDI | OpKind::MemoryESEDI | OpKind::MemoryESRDI => memory(instruction, kind),
        _ => return Err(format!("unsupported operand kind {kind:?}")),
    };
    Ok(object(&[("kind", name(&kind)), ("value", value)]))
}
fn instruction_json(instruction: &Instruction, offsets: &ConstantOffsets, raw: &[u8], offset: usize,
    factory: &mut InstructionInfoFactory) -> Result<String, String> {
    let operands = (0..instruction.op_count()).map(|index| operand(instruction, index)).collect::<Result<Vec<_>, _>>()?;
    let info = factory.info(instruction);
    Ok(object(&[
        ("offset_bytes", offset.to_string()), ("ip", bits(instruction.ip())), ("next_ip", bits(instruction.next_ip())),
        ("length_bytes", instruction.len().to_string()), ("bytes", bytes(raw)),
        ("code", name(&instruction.code())), ("mnemonic", name(&instruction.mnemonic())),
        ("encoding", name(&instruction.encoding())), ("flow_control", name(&instruction.flow_control())),
        ("operands", array(operands)),
        ("operand_access", array((0..instruction.op_count()).map(|operand| name(&info.op_access(operand))))),
        ("encoded_constants", object(&[
            ("displacement", constant(offsets.displacement_offset(), offsets.displacement_size(), raw)),
            ("immediate", constant(offsets.immediate_offset(), offsets.immediate_size(), raw)),
            ("immediate2", constant(offsets.immediate_offset2(), offsets.immediate_size2(), raw)),
        ])),
        ("prefixes", object(&[("lock", instruction.has_lock_prefix().to_string()),
            ("rep", instruction.has_rep_prefix().to_string()), ("repne", instruction.has_repne_prefix().to_string()),
            ("xacquire", instruction.has_xacquire_prefix().to_string()), ("xrelease", instruction.has_xrelease_prefix().to_string()),
            ("segment", register(instruction.segment_prefix()))])),
        ("vector", object(&[("op_mask", register(instruction.op_mask())), ("zeroing", instruction.zeroing_masking().to_string()),
            ("broadcast", instruction.is_broadcast().to_string()), ("rounding", name(&instruction.rounding_control())),
            ("suppress_all_exceptions", instruction.suppress_all_exceptions().to_string())])),
        ("used_registers", array(info.used_registers().iter().map(|value| object(&[("register", register(value.register())), ("access", name(&value.access()))])))),
        ("used_memory", array(info.used_memory().iter().map(|value| object(&[
            ("segment", register(value.segment())), ("base", register(value.base())), ("index", register(value.index())),
            ("scale", value.scale().to_string()), ("displacement_bits", bits(value.displacement())),
            ("size", memory_size(value.memory_size())), ("access", name(&value.access())),
            ("address_size", name(&value.address_size())), ("vsib_size", value.vsib_size().to_string()),
        ])))),
        ("flags", object(&[("read", flags(instruction.rflags_read())), ("written", flags(instruction.rflags_written())),
            ("cleared", flags(instruction.rflags_cleared())), ("set", flags(instruction.rflags_set())),
            ("undefined", flags(instruction.rflags_undefined()))])),
    ]))
}

/// Decodes exactly this declared instruction range in 64-bit mode. Does not classify code/data.
/// IP is the caller's address for byte zero; successful decoding is not execution evidence.
pub fn instructions_json(raw: &[u8], ip: u64) -> Result<String, DecodeError> {
    let mut decoder = Decoder::with_ip(64, raw, ip, DecoderOptions::NONE);
    let mut factory = InstructionInfoFactory::new();
    let mut instructions = Vec::new();
    while decoder.can_decode() {
        let offset = decoder.position();
        let instruction = decoder.decode();
        if instruction.is_invalid() {
            return Err(DecodeError { offset, ip: instruction.ip(), reason: format!("{:?}", decoder.last_error()) });
        }
        let offsets = decoder.get_constant_offsets(&instruction);
        let value = instruction_json(&instruction, &offsets, &raw[offset..decoder.position()], offset, &mut factory)
            .map_err(|reason| DecodeError { offset, ip: instruction.ip(), reason })?;
        instructions.push(value);
    }
    Ok(object(&[("kind", text("x64_instructions")), ("bitness", "64".into()),
        ("decoder", text("iced-x86")), ("origin_ip", bits(ip)), ("byte_length", raw.len().to_string()),
        ("instruction_count", instructions.len().to_string()), ("instructions", array(instructions))]))
}
