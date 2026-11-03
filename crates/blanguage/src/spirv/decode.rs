//! Read-only SPIR-V module facts; parsing is independent of shader generation and execution.
use rspirv::{binary::{ParseAction as Action, Consumer}, dr::{Instruction, ModuleHeader, Operand}, spirv::Op};
use std::{collections::BTreeMap, error::Error, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    /// The actual failing instruction start, or a known header field; None means unavailable.
    pub offset_words: Option<usize>,
    pub reason: String,
}
impl fmt::Display for DecodeError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset_words {
            Some(offset) => write!(out, "SPIR-V decode failed at word {offset} / byte {}: {}", offset * 4, self.reason),
            None => write!(out, "SPIR-V decode failed (position unavailable): {}", self.reason),
        }
    }
}
impl Error for DecodeError {}
fn text(value: &str) -> String { let mut out = String::new(); crate::json::string(&mut out, value); out }
fn array(values: impl IntoIterator<Item = String>) -> String { format!("[{}]", values.into_iter().collect::<Vec<_>>().join(",")) }
fn words(values: &[u32]) -> String { array(values.iter().map(u32::to_string)) }
fn object(values: &[(&str, String)]) -> String {
    format!("{{{}}}", values.iter().map(|(key, value)| format!("{}:{value}", text(key))).collect::<Vec<_>>().join(","))
}
fn id(value: Option<u32>) -> String { value.map_or_else(|| "null".into(), |id| id.to_string()) }
fn kind(value: &Operand) -> &'static str {
    macro_rules! kinds { ($($name:ident),* $(,)?) => { match value { $(Operand::$name(_) => stringify!($name),)* } }; }
    kinds!(ImageOperands, FPFastMathMode, SelectionControl, LoopControl, FunctionControl,
        MemorySemantics, MemoryAccess, KernelProfilingInfo, RayFlags, FragmentShadingRate,
        RawAccessChainOperands, SourceLanguage, ExecutionModel, AddressingModel, MemoryModel,
        ExecutionMode, StorageClass, Dim, SamplerAddressingMode, SamplerFilterMode, ImageFormat,
        ImageChannelOrder, ImageChannelDataType, FPRoundingMode, FPDenormMode, QuantizationModes,
        FPOperationMode, OverflowModes, LinkageType, AccessQualifier, HostAccessQualifier,
        FunctionParameterAttribute, Decoration, BuiltIn, Scope, GroupOperation, KernelEnqueueFlags,
        Capability, RayQueryIntersection, RayQueryCommittedIntersectionType, RayQueryCandidateIntersectionType,
        PackedVectorFormat, CooperativeMatrixOperands, CooperativeMatrixLayout, CooperativeMatrixUse,
        CooperativeMatrixReduce, TensorClampMode, TensorAddressingOperands, InitializationModeQualifier,
        LoadCacheControl, StoreCacheControl, NamedMaximumNumberOfRegisters, MatrixMultiplyAccumulateOperands,
        FPEncoding, CooperativeVectorMatrixLayout, ComponentType, TensorOperands,
        IdMemorySemantics, IdScope, IdRef, LiteralBit32, LiteralBit64, LiteralExtInstInteger,
        LiteralSpecConstantOpInteger, LiteralString)
}
struct Query<'a> {
    input: &'a [u32],
    cursor: usize,
    header: bool,
    instructions: Vec<String>,
    entries: Vec<String>,
    sets: BTreeMap<u32, String>,
}
impl Query<'_> {
    fn instruction(&mut self, inst: Instruction) -> Result<(), DecodeError> {
        let start = self.cursor;
        let length = (self.input[start] >> 16) as usize;
        let raw = self.input.get(start..start + length).ok_or_else(|| DecodeError {
            offset_words: Some(start), reason: "instruction extends past the original module".into(),
        })?;
        let mut offset = 1 + usize::from(inst.result_type.is_some()) + usize::from(inst.result_id.is_some());
        let mut operands = Vec::new();
        for operand in &inst.operands {
            let length = match operand {
                Operand::LiteralString(value) => (value.len() + 1).div_ceil(4),
                Operand::LiteralBit64(_) => 2,
                _ => 1,
            };
            let encoded = raw.get(offset..offset + length).ok_or_else(|| DecodeError {
                offset_words: Some(start), reason: "decoded operand is outside its original instruction".into(),
            })?;
            let mut fields = vec![("kind", text(kind(operand))), ("offset_words", (start + offset).to_string()),
                ("word_count", length.to_string()), ("words", words(encoded))];
            match operand {
                Operand::IdRef(value) | Operand::IdScope(value) | Operand::IdMemorySemantics(value) => fields.push(("id", value.to_string())),
                Operand::LiteralBit32(value) => fields.push(("bits", text(&format!("0x{value:08x}")))),
                Operand::LiteralBit64(value) => fields.push(("bits", text(&format!("0x{value:016x}")))),
                Operand::LiteralString(value) => fields.push(("string", text(value))),
                Operand::LiteralExtInstInteger(value) => fields.push(("value", value.to_string())),
                Operand::LiteralSpecConstantOpInteger(value) => fields.push(("opcode", (*value as u32).to_string())),
                _ => fields.push(("symbolic", text(&format!("{operand:?}")))),
            }
            operands.push(object(&fields)); offset += length;
        }
        if offset != raw.len() { return Err(DecodeError { offset_words: Some(start), reason: "decoded operands do not cover the original instruction".into() }); }
        let mut fields = vec![("offset_words", start.to_string()), ("offset_bytes", (start * 4).to_string()),
            ("word_count", raw.len().to_string()), ("words", words(raw)),
            ("opcode", (raw[0] & 0xffff).to_string()), ("name", text(&format!("Op{:?}", inst.class.opcode))),
            ("result_type_id", id(inst.result_type)), ("result_id", id(inst.result_id)), ("operands", array(operands))];
        match (inst.class.opcode, inst.operands.as_slice()) {
            (Op::ExtInstImport, [Operand::LiteralString(name)]) => {
                self.sets.insert(inst.result_id.unwrap(), name.clone());
            }
            (Op::EntryPoint, [Operand::ExecutionModel(model), Operand::IdRef(entry), Operand::LiteralString(name), interface @ ..]) => {
                self.entries.push(object(&[("offset_words", start.to_string()), ("execution_model", (*model as u32).to_string()),
                    ("execution_model_name", text(&format!("{model:?}"))), ("entry_id", entry.to_string()),
                    ("name", text(name)), ("interface_ids", array(interface.iter().map(|value| value.id_ref_any().unwrap().to_string())))]));
            }
            (Op::ExtInst, [Operand::IdRef(set), Operand::LiteralExtInstInteger(number), ..]) => {
                let set_name = self.sets.get(set);
                let name = set_name.filter(|name| name.as_str() == "GLSL.std.450")
                    .and_then(|_| rspirv::spirv::GlslStd450Op::from_u32(*number)).map(|op| text(&format!("{op:?}")));
                fields.push(("extended_instruction", object(&[("set_id", set.to_string()),
                    ("set_name", set_name.map_or_else(|| "null".into(), |name| text(name))),
                    ("number", number.to_string()), ("name", name.unwrap_or_else(|| "null".into()))])));
            }
            _ => {}
        }
        self.instructions.push(object(&fields)); self.cursor += raw.len(); Ok(())
    }
}
impl Consumer for Query<'_> {
    fn initialize(&mut self) -> Action { Action::Continue }
    fn finalize(&mut self) -> Action { Action::Continue }
    fn consume_header(&mut self, _: ModuleHeader) -> Action { self.header = true; self.cursor = 5; Action::Continue }
    fn consume_instruction(&mut self, inst: Instruction) -> Action {
        match self.instruction(inst) { Ok(()) => Action::Continue, Err(error) => Action::Error(Box::new(error)) }
    }
}
/// Reads every original instruction in order. This does not validate shader semantics,
/// create compiler/device state, or prove final GPU instructions or actual execution.
pub fn module_json(input: &[u32]) -> Result<String, DecodeError> {
    let mut query = Query { input, cursor: 0, header: false, instructions: Vec::new(), entries: Vec::new(), sets: BTreeMap::new() };
    rspirv::binary::parse_words(input, &mut query).map_err(|error| DecodeError {
        offset_words: match error {
            rspirv::binary::ParseState::HeaderIncorrect | rspirv::binary::ParseState::EndiannessUnsupported => Some(0),
            _ => query.header.then_some(query.cursor),
        }, reason: error.to_string(),
    })?;
    Ok(object(&[("kind", text("spirv_module")), ("representation", text("spirv_intermediate")),
        ("endianness", text("little")), ("word_count", input.len().to_string()), ("byte_length", (input.len() * 4).to_string()),
        ("header", object(&[("words", words(&input[..5])), ("magic", text(&format!("0x{:08x}", input[0]))),
            ("version_word", text(&format!("0x{:08x}", input[1]))), ("generator", input[2].to_string()),
            ("id_bound", input[3].to_string()), ("schema", input[4].to_string())])),
        ("entry_points", array(query.entries)), ("instructions", array(query.instructions))]))
}
