//! Queries borrow the original emitted image; execution ABI bytes carry no observation ranges.
use super::{CodeSources, ContinuationCode, DecodeError, NativeCode, NativeSymbol, ProgramImage, RangeResult, RegionKind, StageCode};
use super::disassemble::{array, bits, bytes, object, text};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn region(code: &[u8], kind: &str, start: usize, end: usize, ip: u64, instructions: bool) -> Result<Vec<(&'static str, String)>> {
    let raw = &code[start..end];
    let decoded = if instructions {
        super::instructions_json(raw, ip.wrapping_add(start as u64)).map_err(|error| DecodeError {
            offset: start + error.offset, ip: error.ip, reason: error.reason,
        })?
    } else { "null".into() };
    Ok(vec![("kind", text(kind)), ("offset_bytes", start.to_string()),
        ("length_bytes", raw.len().to_string()), ("bytes", bytes(raw)), ("instructions", decoded)])
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
                super::AbiRangeKind::FallibleSuccessStatus => "fallible_success_status",
                super::AbiRangeKind::FunctionReturn => "function_return",
                super::AbiRangeKind::ParallelRange { part, origin } => {
                    fields.extend(parallel_abi_fields(part, origin.ir_json())); "parallel_range"
                }
            };
            fields.insert(0, ("kind", text(kind)));
            object(&fields)
        }))),
    ])
}
fn parallel_abi_fields(part: &super::ParallelRangePart, origin: String) -> Vec<(&'static str, String)> {
    use super::ParallelRangePart::*;
    let mut fields = vec![("stage_origin", origin)];
    let part = match part {
        Enter { frame_bytes } => { fields.push(("local_frame_bytes", frame_bytes.to_string())); "enter" }
        Exit { frame_bytes } => { fields.push(("local_frame_bytes", frame_bytes.to_string())); "exit" }
        ScalarSelection { binding, offset } | PacketCollect { binding, offset } => {
            fields.extend([("binding_index", binding.to_string()), ("field_offset_bytes", offset.to_string())]);
            if matches!(part, ScalarSelection { .. }) { "scalar_selection" } else { "packet_collect" }
        }
        TileBase { storage_offset } => { fields.push(("storage_offset_bytes", storage_offset.to_string())); "tile_base" }
        TilePhaseEnter { phase } | TilePhaseAdvance { phase } => {
            fields.push(("phase_index", phase.to_string()));
            if matches!(part, TilePhaseEnter { .. }) { "tile_phase_enter" } else { "tile_phase_advance" }
        }
        TilePrivatePointer { phase, restore } => {
            fields.push(("phase_index", phase.to_string()));
            if *restore { "tile_private_restore_pointer" } else { "tile_private_save_pointer" }
        }
        TilePrivate { phase, restore, local, slot, storage_offset, scalar } => {
            fields.extend([("phase_index", phase.to_string()), ("slot_kind", text(if *local { "local" } else { "register" })),
                ("slot_index", slot.to_string()), ("storage_offset_bytes", storage_offset.to_string()), ("scalar", text(scalar.name()))]);
            if *restore { "tile_private_restore" } else { "tile_private_save" }
        }
        TilePublishPointer { tile } => { fields.push(("tile_index", tile.to_string())); "tile_publish_pointer" }
        TilePublishLoad { tile, word, value, scalar } => {
            fields.extend([("tile_index", tile.to_string()), ("value_offset_bytes", word.to_string()),
                ("value_register_index", value.to_string()), ("scalar", text(scalar.name()))]);
            "tile_publish_load"
        }
        TilePublishShift { tile, word, value, scalar, part } => {
            fields.extend([("tile_index", tile.to_string()), ("value_offset_bytes", word.to_string()),
                ("value_register_index", value.to_string()), ("scalar", text(scalar.name()))]);
            fields.push(("part_index", part.to_string())); "tile_publish_shift"
        }
        TilePublishStore { tile, word, value, scalar, part, storage_offset, unit_bytes } => {
            fields.extend([("tile_index", tile.to_string()), ("value_offset_bytes", word.to_string()),
                ("value_register_index", value.to_string()), ("scalar", text(scalar.name()))]);
            fields.extend([("part_index", part.to_string()), ("storage_offset_bytes", storage_offset.to_string()), ("unit_bytes", unit_bytes.to_string())]); "tile_publish_store"
        }
        TileInputBoundary { width } => { fields.push(("width", width.to_string())); "tile_input_boundary" }
        TileInputCopy { binding, source_offset, storage_offset, unit_bytes } => {
            fields.extend([("binding_index", binding.to_string()), ("source_offset_bytes", source_offset.to_string()),
                ("storage_offset_bytes", storage_offset.to_string()), ("unit_bytes", unit_bytes.to_string())]); "tile_input_copy"
        }
        ScalarBounds => "scalar_bounds", ScalarAdvance => "scalar_advance", ScalarBackEdge => "scalar_back_edge",
        PacketBounds => "packet_bounds", PacketAdvance => "packet_advance", PacketBackEdge => "packet_back_edge", PacketTail => "packet_tail",
    };
    fields.insert(0, ("part", text(part))); fields
}
pub(crate) mod material;
pub use material::SavedSources;
impl CodeSources {
    pub(crate) fn function_abi_json(&self) -> String {
        match self {
            Self::Emitted { function_abi: abi, .. } => abi.as_ref().map_or_else(|| "null".into(), function_abi),
            Self::Restored(saved) => saved.function_abi_json(),
            Self::Unavailable => "null".into(),
        }
    }
    pub(crate) fn operation_ranges_json(&self) -> String {
        match self {
            Self::Emitted { operation_ranges, .. } => operation_ranges.as_ref().map_or_else(|| "null".into(), |ranges| array(ranges.iter().map(|range| object(&[
                ("start_byte", range.start.to_string()), ("end_byte", range.end.to_string()),
                ("byte_length", (range.end - range.start).to_string()),
                ("includes_nested_operations", range.includes_nested_operations.to_string()), ("origin", range.origin.ir_json()),
            ])))),
            Self::Restored(saved) => saved.operation_ranges_json(),
            Self::Unavailable => "null".into(),
        }
    }
}
impl NativeCode {
    /// Queries this actual compiler result. `ip` is the caller's address for byte zero.
    /// Execution-only deserialization does not retain these observation facts.
    pub fn code_json(&self, ip: u64) -> Result<String> {
        let tails = self.tail_regions.as_ref().ok_or("native image observation ranges were not retained")?;
        let mut regions = vec![object(&region(&self.bytes, "main_function", 0, self.function_end, ip, true)?)];
        let mut start = self.function_end;
        for part in tails {
            let (kind, instructions) = match part.kind {
                RegionKind::StackProbe => ("stack_probe", true),
                RegionKind::Alignment => ("alignment", false),
                RegionKind::Literal => ("literal", false),
            };
            regions.push(object(&region(&self.bytes, kind, start, part.end, ip, instructions)?));
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
            ("operation_ranges", self.sources.operation_ranges_json()),
            ("function_abi", self.sources.function_abi_json()),
            ("tail_references", self.tail_references.as_ref().map_or_else(|| "null".into(), |references| array(references.iter().map(|reference| object(&[
                ("target_offset_bytes", reference.target.to_string()),
                ("rel32_field_offsets_bytes", array(reference.rel32_fields.iter().map(usize::to_string))),
            ])))))]))
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
impl super::ModuleCode {
    /// Borrows the selected sections. Addresses remain section-relative until actual loading.
    pub fn code_json(&self) -> Result<String> {
        let address = |section: &super::ModuleSection, at: usize| -> Result<(Option<(usize, usize)>, String)> {
            let addend = u32::from_le_bytes(section.bytes[at..at + 4].try_into()?);
            let mut references = section.relocations.iter().enumerate().filter(|(_, row)| row.offset == at);
            let relocation = references.next();
            if references.next().is_some() { return Err("COFF module .pdata field has multiple relocations".into()); }
            let (position, status) = match relocation {
                None => (None, "no_relocation"),
                Some((_, row)) if row.kind != 3 => (None, "relocation_type_not_supported"),
                Some((_, row)) => match &row.target {
                    super::ModuleTarget::Import(_) => (None, "target_not_section_relative"),
                    super::ModuleTarget::Section { section, value } => {
                        let target = self.sections.get(*section).ok_or("COFF module .pdata target section is outside the section table")?;
                        let offset = value.wrapping_add(addend) as usize;
                        if offset > target.bytes.len() { return Err("COFF module .pdata address is outside its target section".into()); }
                        (Some((*section, offset)), "resolved")
                    }
                },
            };
            Ok((position, object(&[("addend", addend.to_string()), ("status", text(status)),
                ("relocation_index", relocation.map_or_else(|| "null".into(), |(index, _)| index.to_string())),
                ("position", position.map_or_else(|| "null".into(), |(section, offset)| object(&[
                    ("section_index", section.to_string()), ("offset_bytes", offset.to_string()),
                ])))])))
        };
        let mut spans = Vec::new(); let mut records = Vec::new();
        for (index, section) in self.sections.iter().enumerate() {
            if section.name != ".pdata" { continue; }
            if section.bytes.len() % 12 != 0 { return Err("COFF module x64 .pdata contains an incomplete function record".into()); }
            for at in (0..section.bytes.len()).step_by(12) {
                let (begin, begin_json) = address(section, at)?;
                let (end, end_json) = address(section, at + 4)?;
                let (_, unwind_json) = address(section, at + 8)?;
                let status = match (begin, end) {
                    (Some((a, _)), Some((b, _))) if a != b => "cross_section_range",
                    (Some((section, start)), Some((_, end))) => {
                        if start > end { return Err("COFF module .pdata function range is reversed".into()); }
                        spans.push((section, start, end, records.len())); "resolved"
                    }
                    _ => "unresolved",
                };
                records.push(object(&[("section_index", index.to_string()), ("offset_bytes", at.to_string()),
                    ("record_bytes", bytes(&section.bytes[at..at + 12])), ("status", text(status)),
                    ("begin", begin_json), ("end", end_json), ("unwind", unwind_json)]));
            }
        }
        Ok(object(&[("address_basis", text("section_relative")), ("function_tables", array(records)),
            ("sections", array(self.sections.iter().enumerate().map(|(index, section)| -> Result<String> {
                let mut code = spans.iter().filter(|(at, _, _, _)| *at == index).collect::<Vec<_>>();
                code.sort_by_key(|(_, start, end, _)| (*start, *end));
                let mut regions = Vec::new(); let mut end = 0;
                for &&(_, start, stop, record) in &code {
                    if start > end { regions.push(object(&region(&section.bytes, "unclassified", end, start, 0, false)?)); }
                    let mut fields = region(&section.bytes, "runtime_function", start, stop, 0, true)
                        .map_err(|error| format!("COFF module section {index}: {error}"))?;
                    fields.push(("function_table_index", record.to_string()));
                    regions.push(object(&fields)); end = end.max(stop);
                }
                if end < section.bytes.len() { regions.push(object(&region(&section.bytes, "unclassified", end, section.bytes.len(), 0, false)?)); }
                Ok(object(&[("section_index", index.to_string()), ("name", text(&section.name)),
                    ("flags", section.flags.to_string()), ("byte_length", section.bytes.len().to_string()), ("bytes", bytes(&section.bytes)),
                    ("regions", array(regions)),
                    ("relocations", array(section.relocations.iter().map(|relocation| object(&[
                        ("offset_bytes", relocation.offset.to_string()), ("type", relocation.kind.to_string()),
                        ("target", match &relocation.target {
                            super::ModuleTarget::Section { section, value } => object(&[
                                ("kind", text("section")), ("section_index", section.to_string()), ("offset_bytes", value.to_string()),
                            ]),
                            super::ModuleTarget::Import(name) => object(&[
                                ("kind", text("import")), ("library", text(&name.library)), ("symbol", text(&name.symbol)),
                            ]),
                        }),
                    ]))))]))
            }).collect::<Result<Vec<_>>>()?)),
            ("symbols", array(self.symbols.iter().map(|(name, (section, offset))| object(&[
                ("name", text(name)), ("section_index", section.to_string()), ("offset_bytes", offset.to_string()),
            ]))))]))
    }
}
impl<P> ProgramImage<P> {
    /// Observes the original root and resource images without recompiling or loading executable memory.
    pub fn code_json(&self) -> Result<String> {
        use crate::ir::ProgramInterface;
        Ok(object(&[("kind", text("native_program_image")), ("program", text(self.program().name())),
            ("address_basis", text("each_image_relative")),
            ("stages", array(self.stages().iter().map(StageCode::code_json).collect::<Result<Vec<_>>>()?)),
            ("functions", array(self.functions().iter().map(|(name, code)| {
                Ok(object(&[("name", text(name)), ("code", code.code_json(0)?)]))
            }).collect::<Result<Vec<_>>>()?)),
            ("module_code", self.module_code.as_ref().map(super::ModuleCode::code_json).transpose()?.unwrap_or_else(|| "null".into())),
            ("resource_images", array(self.resources.iter().flat_map(|module| &module.images).enumerate().map(|(index, image)| {
                let (target, code) = match image {
                    crate::resource::Image::Cpu(image) => ("cpu", image.code_json()?),
                    crate::resource::Image::Gpu(image) => ("gpu", image.code_json()?),
                };
                Ok(object(&[("index", index.to_string()), ("target", text(target)), ("code", code)]))
            }).collect::<Result<Vec<_>>>()?))]))
    }
}
