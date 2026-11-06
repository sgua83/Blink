//! Queries borrow the original emitted image; execution ABI bytes carry no observation ranges.
use super::{ContinuationCode, DecodeError, NativeCode, NativeSymbol, ProgramImage, RangeResult, RegionKind, StageCode};
use super::disassemble::{array, bits, bytes, object, text};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn region(code: &NativeCode, kind: &str, start: usize, end: usize, ip: u64, instructions: bool) -> Result<String> {
    let raw = &code.bytes[start..end];
    let decoded = if instructions {
        super::instructions_json(raw, ip.wrapping_add(start as u64)).map_err(|error| DecodeError {
            offset: start + error.offset, ip: error.ip, reason: error.reason,
        })?
    } else { "null".into() };
    Ok(object(&[("kind", text(kind)), ("offset_bytes", start.to_string()),
        ("length_bytes", raw.len().to_string()), ("bytes", bytes(raw)), ("instructions", decoded)]))
}
fn symbol(value: &NativeSymbol) -> String {
    match value {
        NativeSymbol::Print(scalar) => object(&[("kind", text("print")), ("scalar", text(&format!("{scalar:?}")))]),
        NativeSymbol::External(name) => object(&[("kind", text("external")),
            ("library", text(&name.library)), ("symbol", text(&name.symbol))]),
        NativeSymbol::Function(name) => object(&[("kind", text("function")), ("name", text(name))]),
    }
}
fn function_abi(value: &super::FunctionAbi) -> String {
    object(&[("kind", text("function_abi")),
        ("definition", value.definition.as_ref().map_or_else(|| "null".into(), crate::ir::SourceOrigin::ir_json)),
        ("instance", value.instance.as_deref().map_or_else(|| "null".into(), text)),
        ("ranges", array(value.ranges.iter().map(|range| {
            let mut fields = vec![("start_byte", range.start.to_string()),
                ("end_byte", range.end.to_string()), ("byte_length", (range.end - range.start).to_string())];
            let kind = match &range.kind {
                super::AbiRangeKind::HiddenResultHome { slot } => {
                    fields.push(("abi_slot", slot.to_string())); "hidden_result_home"
                }
                super::AbiRangeKind::ParameterHome { slot, parameter_index, origin }
                | super::AbiRangeKind::ParameterInput { slot, parameter_index, origin } => {
                    fields.extend([("abi_slot", slot.to_string()), ("parameter_index", parameter_index.to_string()),
                        ("origin", origin.as_deref().map_or_else(|| "null".into(), crate::ir::ParameterOrigin::ir_json))]);
                    if matches!(&range.kind, super::AbiRangeKind::ParameterHome { .. }) { "parameter_home" } else { "parameter_input" }
                }
                super::AbiRangeKind::ScratchSave => "scratch_save",
                super::AbiRangeKind::ScratchRestore => "scratch_restore",
                super::AbiRangeKind::FrameAllocate { bytes } => { fields.push(("frame_bytes", bytes.to_string())); "frame_allocate" }
                super::AbiRangeKind::FrameRelease { bytes } => { fields.push(("frame_bytes", bytes.to_string())); "frame_release" }
                super::AbiRangeKind::NonvolatileSave => "nonvolatile_save",
                super::AbiRangeKind::NonvolatileRestore => "nonvolatile_restore",
                super::AbiRangeKind::ResultAddressSave { stack_offset } => { fields.push(("stack_offset_bytes", stack_offset.to_string())); "result_address_save" }
                super::AbiRangeKind::IndirectResult { stack_offset } => { fields.push(("stack_offset_bytes", stack_offset.to_string())); "indirect_result" }
                super::AbiRangeKind::FloatResult => "float_result",
                super::AbiRangeKind::IntegerResult => "integer_result",
            };
            fields.insert(0, ("kind", text(kind)));
            object(&fields)
        }))),
    ])
}
impl NativeCode {
    /// Queries this actual compiler result. `ip` is the caller's address for byte zero.
    /// Execution-only deserialization does not retain these observation facts.
    pub fn code_json(&self, ip: u64) -> Result<String> {
        let tails = self.tail_regions.as_ref().ok_or("native image observation ranges were not retained")?;
        let mut regions = vec![region(self, "main_function", 0, self.function_end, ip, true)?];
        let mut start = self.function_end;
        for part in tails {
            let (kind, instructions) = match part.kind {
                RegionKind::StackProbe => ("stack_probe", true),
                RegionKind::Alignment => ("alignment", false),
                RegionKind::Literal => ("literal", false),
            };
            regions.push(region(self, kind, start, part.end, ip, instructions)?);
            start = part.end;
        }
        Ok(object(&[("kind", text("native_code")), ("bitness", "64".into()),
            ("origin_ip", bits(ip)), ("byte_length", self.bytes.len().to_string()),
            ("function_end", self.function_end.to_string()), ("stack_bytes", self.stack_bytes.to_string()),
            ("call_sites", array(self.calls.iter().map(|value| value.map_or_else(|| "null".into(), |offset| offset.to_string())))),
            ("unwind_bytes", bytes(&self.unwind)),
            ("relocations", array(self.relocations.iter().map(|value| object(&[
                ("offset_bytes", value.offset.to_string()), ("target", symbol(&value.target)),
            ])))), ("regions", array(regions)),
            ("operation_ranges", self.operation_ranges.as_ref().map_or_else(|| "null".into(), |ranges| array(ranges.iter().map(|range| object(&[
                ("start_byte", range.start.to_string()), ("end_byte", range.end.to_string()),
                ("byte_length", (range.end - range.start).to_string()),
                ("includes_nested_operations", range.includes_nested_operations.to_string()), ("origin", range.origin.ir_json()),
            ]))))),
            ("function_abi", self.function_abi.as_ref().map_or_else(|| "null".into(), function_abi))]))
    }
}
impl StageCode {
    /// Each nested code image uses its own byte-zero origin, not a shared loaded address.
    pub fn code_json(&self) -> Result<String> {
        Ok(match self {
            Self::Element { main, continuation, result } => {
                let continuation = match continuation {
                    None => "null".into(),
                    Some(ContinuationCode::ReductionTail(code)) => single("reduction_tail", code)?,
                    Some(ContinuationCode::ScanAdd(code)) => single("scan_add", code)?,
                    Some(ContinuationCode::CompactScatter(code)) => single("compact_scatter", code)?,
                    Some(ContinuationCode::SegmentedFinish(code)) => single("segmented_finish", code)?,
                    Some(ContinuationCode::Scan { prefix, apply, source, target }) => object(&[
                        ("kind", text("scan")), ("source", source.to_string()), ("target", target.to_string()),
                        ("prefix", prefix.code_json(0)?), ("apply", apply.code_json(0)?),
                    ]),
                };
                let result = match result {
                    RangeResult::None => object(&[("kind", text("none"))]),
                    RangeResult::Values(words) => object(&[("kind", text("values")), ("words", words.to_string())]),
                };
                object(&[("kind", text("element")), ("main", main.code_json(0)?),
                    ("continuation", continuation), ("result", result)])
            }
            Self::Sort(code) => object(&[("kind", text("sort")), ("source", code.source.to_string()),
                ("target", code.target.to_string()),
                ("initialize", array(code.initialize.iter().map(|code| code.code_json(0)).collect::<Result<Vec<_>>>()?)),
                ("histogram", code.histogram.code_json(0)?), ("scatter", code.scatter.code_json(0)?),
                ("gather", code.gather.code_json(0)?)]),
        })
    }
}
fn single(kind: &str, code: &NativeCode) -> Result<String> {
    Ok(object(&[("kind", text(kind)), ("code", code.code_json(0)?)]))
}
impl<P> ProgramImage<P> {
    /// Observes all existing stage and named function images without recompiling or loading executable memory.
    pub fn code_json(&self) -> Result<String> {
        use crate::ir::ProgramInterface;
        Ok(object(&[("kind", text("native_program_image")), ("program", text(self.program().name())),
            ("address_basis", text("each_image_relative")),
            ("stages", array(self.stages().iter().map(StageCode::code_json).collect::<Result<Vec<_>>>()?)),
            ("functions", array(self.functions().iter().map(|(name, code)| {
                Ok(object(&[("name", text(name)), ("code", code.code_json(0)?)]))
            }).collect::<Result<Vec<_>>>()?))]))
    }
}
