//! Source-free ABI facts in the optional COFF observation material.
use super::super::{array, object, text};
use crate::{ast::Span, build::hash, codec::{Encode, Writer, record, variants}, ir};

struct Position { line: usize, column: usize }
record!(Position { line, column });
impl Position {
    fn capture(source: &str, offset: usize) -> Self {
        let before = &source[..offset];
        let start = before.rfind('\n').map_or(0, |at| at + 1);
        Self { line: before.bytes().filter(|b| *b == b'\n').count() + 1,
            column: before[start..].chars().count() + 1 }
    }
    fn json(&self) -> String {
        object(&[("line", self.line.to_string()), ("column", self.column.to_string())])
    }
}
struct Range { span: Span, start: Option<Position>, end: Option<Position> }
record!(Range { span, start, end });
impl Range {
    fn capture(origin: Option<&ir::SourceOrigin>, span: Span) -> Self {
        Self { span, start: origin.map(|o| Position::capture(&o.source().text, span.start)),
            end: origin.map(|o| Position::capture(&o.source().text, span.end)) }
    }
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("span_unit", text("utf8_byte")), ("span_range", text("half_open")),
            ("start_byte", self.span.start.to_string()), ("end_byte", self.span.end.to_string()),
            ("column_unit", text("unicode_scalar")), ("position_base", "1".into()),
            ("start", optional(self.start.as_ref(), Position::json)),
            ("end", optional(self.end.as_ref(), Position::json))]
    }
}
enum PathEncoding { Windows(Vec<u16>), Unix(Vec<u8>) }
variants!(PathEncoding { 0=>Windows(units), 1=>Unix(bytes) });
impl PathEncoding {
    fn capture(path: &std::path::Path) -> Self {
        #[cfg(windows)] {
            use std::os::windows::ffi::OsStrExt;
            Self::Windows(path.as_os_str().encode_wide().collect())
        }
        #[cfg(unix)] {
            use std::os::unix::ffi::OsStrExt;
            Self::Unix(path.as_os_str().as_bytes().to_vec())
        }
    }
    fn field(&self) -> (&'static str, String) {
        match self {
            Self::Windows(units) => ("path_utf16", array(units.iter().map(u16::to_string))),
            Self::Unix(bytes) => ("path_bytes", array(bytes.iter().map(u8::to_string))),
        }
    }
}
struct Definition {
    module: String, name: String, path: Option<String>, digest: [u8; 32],
    bytes: usize, range: Range, encoding: PathEncoding,
}
record!(Definition { module, name, path, digest, bytes, range, encoding });
impl Definition {
    fn capture(origin: &ir::SourceOrigin) -> Self {
        let source = origin.source();
        Self { module: origin.module().into(), name: origin.definition().into(),
            path: source.path.to_str().map(str::to_owned), digest: hash::digest(source.text.as_bytes()),
            bytes: source.text.len(), range: Range::capture(Some(origin), origin.span()),
            encoding: PathEncoding::capture(&source.path) }
    }
    fn json(&self) -> String {
        let mut fields = vec![("kind", text("source_definition")), ("module", text(&self.module)),
            ("definition", text(&self.name)), ("source_path", optional(self.path.as_deref(), text)),
            ("source_sha256", text(&hash::hex(&self.digest))), ("source_bytes", self.bytes.to_string())];
        fields.extend(self.range.fields()); fields.push(self.encoding.field()); object(&fields)
    }
}
struct Inline { definition: Option<Definition>, instance: Option<String>, range: Range }
record!(Inline { definition, instance, range });
impl Inline {
    fn capture(site: &ir::InlineSite) -> Self {
        Self { definition: site.origin().map(Definition::capture),
            instance: site.instance().map(str::to_owned), range: Range::capture(site.origin(), site.span()) }
    }
    fn json(&self) -> String {
        let mut fields = vec![("kind", text("inline_site")),
            ("definition", optional(self.definition.as_ref(), Definition::json)),
            ("instance", optional(self.instance.as_deref(), text))];
        fields.extend(self.range.fields()); object(&fields)
    }
}
struct Domain {
    definition: Option<Definition>, instance: Option<String>, range: Range,
    target: String, transition: ir::DomainTransition,
}
record!(Domain { definition, instance, range, target, transition });
variants!(ir::DomainTransition { 0=>Expanded, 1=>Compiled });
impl Domain {
    fn capture(site: &ir::DomainSite) -> Self {
        Self { definition: site.origin().map(Definition::capture),
            instance: site.instance().map(str::to_owned), range: Range::capture(site.origin(), site.span()),
            target: site.target().into(), transition: site.transition() }
    }
    fn json(&self) -> String {
        let transition = match self.transition { ir::DomainTransition::Expanded => "expanded", ir::DomainTransition::Compiled => "compiled" };
        let mut fields = vec![("kind", text("domain_site")), ("transition", text(transition)),
            ("definition", optional(self.definition.as_ref(), Definition::json)),
            ("instance", optional(self.instance.as_deref(), text)), ("target", text(&self.target))];
        fields.extend(self.range.fields()); object(&fields)
    }
}
struct Site {
    definition: Definition, instance: Option<String>, syntax: ir::SourceSyntax,
    inlined_at: Vec<Inline>, domain_path: Vec<Domain>, range: Range,
}
record!(Site { definition, instance, syntax, inlined_at, domain_path, range });
variants!(ir::SourceSyntax { 0=>Statement, 1=>Expression, 2=>Place, 3=>Parameter });
impl Site {
    fn capture(site: &ir::SourceSite) -> Self {
        Self { definition: Definition::capture(site.origin()), instance: site.instance().map(str::to_owned),
            syntax: site.syntax(), inlined_at: site.inlined_at().iter().map(Inline::capture).collect(),
            domain_path: site.domain_path().iter().map(Domain::capture).collect(),
            range: Range::capture(Some(site.origin()), site.span()) }
    }
    fn json(&self) -> String {
        let syntax = match self.syntax { ir::SourceSyntax::Statement => "statement", ir::SourceSyntax::Expression => "expression",
            ir::SourceSyntax::Place => "place", ir::SourceSyntax::Parameter => "parameter" };
        let mut fields = vec![("kind", text("source_syntax")), ("syntax", text(syntax)),
            ("definition", self.definition.json()), ("instance", optional(self.instance.as_deref(), text)),
            ("inlined_at_order", text("outermost_first")), ("inlined_at", array(self.inlined_at.iter().map(Inline::json))),
            ("domain_path_order", text("outermost_first")), ("domain_path", array(self.domain_path.iter().map(Domain::json)))];
        fields.extend(self.range.fields()); object(&fields)
    }
}
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
fn optional<T: ?Sized>(value: Option<&T>, json: impl FnOnce(&T) -> String) -> String {
    value.map_or_else(|| "null".into(), json)
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
