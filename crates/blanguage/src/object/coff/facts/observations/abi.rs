//! Source-free ABI facts in the optional COFF observation material.
use super::super::{array, object, text};
use crate::{codec::{Encode, Writer, record, variants}, ir};
use super::source::{Definition, Site, optional};

enum Parameter {
    Frame,
    Parameter { index: usize, name: String, instance: String, generated: bool, site: Option<Site> },
}
variants!(Parameter { 0=>Frame, 1=>Parameter{index,name,instance,generated,site} });
impl Parameter {
    fn capture(origin: &ir::ParameterOrigin) -> Self {
        match origin {
            ir::ParameterOrigin::Frame => Self::Frame,
            ir::ParameterOrigin::Parameter { index, name, instance, generated, site } => Self::Parameter {
                index: *index, name: name.clone(), instance: instance.to_string(), generated: *generated,
                site: site.as_deref().map(Site::capture),
            },
        }
    }
    fn json(&self) -> String {
        match self {
            Self::Frame => object(&[("kind", text("implicit_frame"))]),
            Self::Parameter { index, name, instance, generated, site } => object(&[
                ("kind", text(if *generated { "generated_parameter" } else { "source_parameter" })),
                ("index", index.to_string()), ("name", text(name)), ("instance", text(instance)),
                ("site", optional(site.as_ref(), Site::json))]),
        }
    }
}
pub(crate) struct FunctionAbi { definition: Option<Definition>, instance: Option<String>, ranges: Vec<AbiRange> }
struct AbiRange { start: usize, end: usize, kind: AbiRangeKind }
record!(FunctionAbi { definition, instance, ranges });
record!(AbiRange { start, end, kind });
enum AbiRangeKind {
    HiddenResultHome { slot: usize },
    ParameterHome { slot: usize, parameter_index: usize, origin: Option<Parameter> },
    ParameterInput { slot: usize, parameter_index: usize, origin: Option<Parameter> },
    ScratchSave,
    ScratchRestore,
    FrameAllocate { bytes: u32 },
    FrameRelease { bytes: u32 },
    NonvolatileSave,
    NonvolatileRestore,
    ResultAddressSave { stack_offset: u32 },
    IndirectResult { stack_offset: u32 },
    FloatResult,
    IntegerResult,
    FallibleSuccessStatus,
    FunctionReturn,
}
variants!(AbiRangeKind {
    0=>HiddenResultHome{slot}, 1=>ParameterHome{slot,parameter_index,origin},
    2=>ParameterInput{slot,parameter_index,origin}, 3=>ScratchSave, 4=>ScratchRestore,
    5=>FrameAllocate{bytes}, 6=>FrameRelease{bytes}, 7=>NonvolatileSave, 8=>NonvolatileRestore,
    9=>ResultAddressSave{stack_offset}, 10=>IndirectResult{stack_offset},
    11=>FloatResult, 12=>IntegerResult, 13=>FallibleSuccessStatus, 14=>FunctionReturn,
});
impl From<&crate::native::FunctionAbi> for FunctionAbi {
    fn from(value: &crate::native::FunctionAbi) -> Self {
        Self { definition: value.definition.as_ref().map(Definition::capture), instance: value.instance.clone(),
            ranges: value.ranges.iter().map(|range| AbiRange { start: range.start, end: range.end,
                kind: match &range.kind {
                    crate::native::AbiRangeKind::HiddenResultHome { slot } => AbiRangeKind::HiddenResultHome { slot: *slot },
                    crate::native::AbiRangeKind::ParameterHome { slot, parameter_index, origin } => AbiRangeKind::ParameterHome { slot: *slot, parameter_index: *parameter_index, origin: origin.as_deref().map(Parameter::capture) },
                    crate::native::AbiRangeKind::ParameterInput { slot, parameter_index, origin } => AbiRangeKind::ParameterInput { slot: *slot, parameter_index: *parameter_index, origin: origin.as_deref().map(Parameter::capture) },
                    crate::native::AbiRangeKind::ScratchSave => AbiRangeKind::ScratchSave,
                    crate::native::AbiRangeKind::ScratchRestore => AbiRangeKind::ScratchRestore,
                    crate::native::AbiRangeKind::FrameAllocate { bytes } => AbiRangeKind::FrameAllocate { bytes: *bytes },
                    crate::native::AbiRangeKind::FrameRelease { bytes } => AbiRangeKind::FrameRelease { bytes: *bytes },
                    crate::native::AbiRangeKind::NonvolatileSave => AbiRangeKind::NonvolatileSave,
                    crate::native::AbiRangeKind::NonvolatileRestore => AbiRangeKind::NonvolatileRestore,
                    crate::native::AbiRangeKind::ResultAddressSave { stack_offset } => AbiRangeKind::ResultAddressSave { stack_offset: *stack_offset },
                    crate::native::AbiRangeKind::IndirectResult { stack_offset } => AbiRangeKind::IndirectResult { stack_offset: *stack_offset },
                    crate::native::AbiRangeKind::FloatResult => AbiRangeKind::FloatResult,
                    crate::native::AbiRangeKind::IntegerResult => AbiRangeKind::IntegerResult,
                    crate::native::AbiRangeKind::FallibleSuccessStatus => AbiRangeKind::FallibleSuccessStatus,
                    crate::native::AbiRangeKind::FunctionReturn => AbiRangeKind::FunctionReturn,
                },
            }).collect() }
    }
}
impl Encode for crate::native::FunctionAbi {
    fn encode(&self, writer: &mut Writer) { FunctionAbi::from(self).encode(writer); }
}
impl FunctionAbi {
    pub(super) fn validate(&self, main_end: usize) -> super::Result<()> {
        if self.ranges.iter().any(|r| r.start > r.end || r.end > main_end) {
            return Err("COFF ABI range is outside its original main function".into());
        }
        Ok(())
    }
    pub(super) fn json(&self) -> String {
        object(&[("kind", text("function_abi")),
            ("definition", self.definition.as_ref().map_or_else(|| "null".into(), Definition::json)),
            ("instance", self.instance.as_deref().map_or_else(|| "null".into(), text)),
            ("ranges", array(self.ranges.iter().map(|range| {
                let mut fields = vec![("start_byte", range.start.to_string()),
                    ("end_byte", range.end.to_string()), ("byte_length", (range.end - range.start).to_string())];
                let kind = match &range.kind {
                    AbiRangeKind::HiddenResultHome { slot } => {
                        fields.push(("abi_slot", slot.to_string())); "hidden_result_home"
                    }
                    AbiRangeKind::ParameterHome { slot, parameter_index, origin }
                    | AbiRangeKind::ParameterInput { slot, parameter_index, origin } => {
                        fields.extend([("abi_slot", slot.to_string()), ("parameter_index", parameter_index.to_string()),
                            ("origin", origin.as_ref().map_or_else(|| "null".into(), Parameter::json))]);
                        if matches!(&range.kind, AbiRangeKind::ParameterHome { .. }) { "parameter_home" } else { "parameter_input" }
                    }
                    AbiRangeKind::ScratchSave => "scratch_save",
                    AbiRangeKind::ScratchRestore => "scratch_restore",
                    AbiRangeKind::FrameAllocate { bytes } => { fields.push(("frame_bytes", bytes.to_string())); "frame_allocate" }
                    AbiRangeKind::FrameRelease { bytes } => { fields.push(("frame_bytes", bytes.to_string())); "frame_release" }
                    AbiRangeKind::NonvolatileSave => "nonvolatile_save",
                    AbiRangeKind::NonvolatileRestore => "nonvolatile_restore",
                    AbiRangeKind::ResultAddressSave { stack_offset } => { fields.push(("stack_offset_bytes", stack_offset.to_string())); "result_address_save" }
                    AbiRangeKind::IndirectResult { stack_offset } => { fields.push(("stack_offset_bytes", stack_offset.to_string())); "indirect_result" }
                    AbiRangeKind::FloatResult => "float_result",
                    AbiRangeKind::IntegerResult => "integer_result",
                    AbiRangeKind::FallibleSuccessStatus => "fallible_success_status",
                    AbiRangeKind::FunctionReturn => "function_return",
                };
                fields.insert(0, ("kind", text(kind)));
                object(&fields)
            }))),
        ])
    }
}
