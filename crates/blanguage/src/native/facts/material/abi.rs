//! Source-free ABI facts in the optional COFF observation material.
use super::super::{array, object, text};
use crate::{codec::{Encode, Writer, record, variants}, ir};
use super::source::{Definition, Site, optional};

#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub(crate) struct FunctionAbi { definition: Option<Definition>, instance: Option<String>, ranges: Vec<AbiRange> }
#[derive(Clone, Debug)]
struct AbiRange { start: usize, end: usize, kind: AbiRangeKind }
record!(FunctionAbi { definition, instance, ranges });
record!(AbiRange { start, end, kind });
#[derive(Clone, Debug)]
enum AbiRangeKind<O = super::operations::OperationOrigin> {
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
    ParallelRange { part: crate::native::ParallelRangePart, origin: O },
}
variants!(@<O> AbiRangeKind<O> {
    0=>HiddenResultHome{slot}, 1=>ParameterHome{slot,parameter_index,origin},
    2=>ParameterInput{slot,parameter_index,origin}, 3=>ScratchSave, 4=>ScratchRestore,
    5=>FrameAllocate{bytes}, 6=>FrameRelease{bytes}, 7=>NonvolatileSave, 8=>NonvolatileRestore,
    9=>ResultAddressSave{stack_offset}, 10=>IndirectResult{stack_offset},
    11=>FloatResult, 12=>IntegerResult, 13=>FallibleSuccessStatus, 14=>FunctionReturn, 15=>ParallelRange{part,origin},
});
variants!(crate::native::ParallelRangePart {
    0=>Enter{frame_bytes}, 1=>ScalarBounds, 2=>ScalarSelection{binding,offset}, 3=>ScalarAdvance, 4=>ScalarBackEdge,
    5=>Exit{frame_bytes}, 6=>PacketBounds, 7=>PacketCollect{binding,offset}, 8=>PacketAdvance, 9=>PacketBackEdge, 10=>PacketTail,
    11=>TileInputBoundary{width}, 12=>TileInputCopy{binding,source_offset,storage_offset,unit_bytes},
    13=>TileBase{storage_offset}, 14=>TilePhaseEnter{phase}, 15=>TilePhaseAdvance{phase},
    16=>TilePrivatePointer{phase,restore}, 17=>TilePrivate{phase,restore,local,slot,storage_offset,scalar},
    18=>TilePublishPointer{tile}, 19=>TilePublishLoad{tile,word,value,scalar},
    20=>TilePublishShift{tile,word,value,scalar,part}, 21=>TilePublishStore{tile,word,value,scalar,part,storage_offset,unit_bytes},
    22=>TileCollective{tile,word,result,scalar,operation,part},
});
variants!(crate::native::TileCollectivePart {
    0=>Setup{input_offset,output_offset,length_offset,plane_stride}, 1=>Identity{bits}, 2=>ScanOutput,
    3=>Accumulate{input_high_delta,loop_start}, 4=>Fill{length_offset,loop_start},
});
variants!(ir::TileOp {
    0=>ReduceSum, 1=>ScanExclusiveSum, 2=>ReduceMin, 3=>ReduceMax,
    4=>ScanExclusiveMin, 5=>ScanExclusiveMax, 6=>ReduceBitAnd, 7=>ReduceBitOr, 8=>ReduceBitXor,
    9=>ScanExclusiveBitAnd, 10=>ScanExclusiveBitOr, 11=>ScanExclusiveBitXor,
});
impl Encode for crate::native::FunctionAbi {
    fn encode(&self, writer: &mut Writer) {
        self.definition.as_ref().map(Definition::capture).encode(writer);
        self.instance.encode(writer); self.ranges.encode(writer);
    }
}
impl Encode for crate::native::AbiRange {
    fn encode(&self, writer: &mut Writer) {
        self.start.encode(writer); self.end.encode(writer); self.kind.encode(writer);
    }
}
impl Encode for crate::native::AbiRangeKind {
    fn encode(&self, writer: &mut Writer) {
        let value: AbiRangeKind<&ir::OperationOrigin> = match self {
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
                    crate::native::AbiRangeKind::ParallelRange { part, origin } => AbiRangeKind::ParallelRange { part: *part, origin },
        };
        value.encode(writer);
    }
}
impl FunctionAbi {
    pub(super) fn validate(&self, main_end: usize) -> super::Result<()> {
        if self.ranges.iter().any(|r| r.start > r.end || r.end > main_end) {
            return Err("CPU ABI range is outside its original main function".into());
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
                    AbiRangeKind::ParallelRange { part, origin } => {
                        fields.extend(super::super::parallel_abi_fields(part, origin.ir_json())); "parallel_range"
                    }
                };
                fields.insert(0, ("kind", text(kind)));
                object(&fields)
            }))),
        ])
    }
}
