//! Compiler-only definition facts retain the original immutable source snapshot.
use crate::ast::{Source, Span};

#[derive(Clone, Debug)]
pub struct SourceOrigin {
    pub(crate) source: Source,
    pub(crate) span: Span,
    pub(crate) module: String,
    pub(crate) definition: String,
}
fn text(value: &str) -> String { let mut out = String::new(); crate::json::string(&mut out, value); out }
fn position(source: &str, offset: usize) -> String {
    let before = &source[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    let column = before[start..].chars().count() + 1;
    format!("{{\"line\":{line},\"column\":{column}}}")
}
impl SourceOrigin {
    pub fn source(&self) -> &Source { &self.source }
    pub fn span(&self) -> Span { self.span }
    pub fn module(&self) -> &str { &self.module }
    pub fn definition(&self) -> &str { &self.definition }
    /// Queries this definition snapshot; the owning IR retains its concrete instance name.
    /// This is not an instruction, inlining, optimization or machine-address mapping.
    pub fn ir_json(&self) -> String {
        let mut fields = vec![
            ("kind", text("source_definition")), ("module", text(&self.module)),
            ("definition", text(&self.definition)),
            ("source_path", self.source.path.to_str().map_or_else(|| "null".into(), text)),
            ("source_sha256", text(&crate::build::hash::hex(&crate::build::hash::digest(self.source.text.as_bytes())))),
            ("source_bytes", self.source.text.len().to_string()),
            ("span_unit", text("utf8_byte")), ("span_range", text("half_open")),
            ("start_byte", self.span.start.to_string()), ("end_byte", self.span.end.to_string()),
            ("column_unit", text("unicode_scalar")), ("position_base", "1".into()),
            ("start", position(&self.source.text, self.span.start)),
            ("end", position(&self.source.text, self.span.end)),
        ];
        #[cfg(windows)] {
            use std::os::windows::ffi::OsStrExt;
            let units = self.source.path.as_os_str().encode_wide().map(|v| v.to_string()).collect::<Vec<_>>();
            fields.push(("path_utf16", format!("[{}]", units.join(","))));
        }
        #[cfg(unix)] {
            use std::os::unix::ffi::OsStrExt;
            let bytes = self.source.path.as_os_str().as_bytes().iter().map(u8::to_string).collect::<Vec<_>>();
            fields.push(("path_bytes", format!("[{}]", bytes.join(","))));
        }
        format!("{{{}}}", fields.iter().map(|(key, value)| format!("{}:{value}", text(key))).collect::<Vec<_>>().join(","))
    }
}

/// The actual syntax that drove this operation's emission, before later IR rewrites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSyntax { Statement, Expression, Place, Parameter }
/// One actual compiler inline expansion point, in its caller's source coordinates.
#[derive(Clone)]
pub struct InlineSite {
    pub(crate) origin: Option<std::sync::Arc<SourceOrigin>>,
    pub(crate) instance: Option<std::sync::Arc<str>>,
    pub(crate) span: Span,
}
impl std::fmt::Debug for InlineSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineSite")
            .field("origin", &self.origin.as_ref().map(|o| (&o.module, &o.definition, &o.source.path)))
            .field("instance", &self.instance).field("span", &self.span).finish()
    }
}
#[derive(Clone, Debug)]
pub(crate) struct SourceContext {
    pub instance: Option<std::sync::Arc<str>>,
    pub inlined_at: Box<[InlineSite]>,
    pub domain_path: std::sync::Arc<[DomainSite]>,
}
/// One actual domain transition, separate from ordinary function inlining.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainTransition { Expanded, Compiled }
#[derive(Clone, Debug)]
pub struct DomainSite {
    pub(crate) origin: Option<std::sync::Arc<SourceOrigin>>,
    pub(crate) instance: Option<std::sync::Arc<str>>,
    pub(crate) span: Span,
    pub(crate) target: std::sync::Arc<str>,
    pub(crate) transition: DomainTransition,
}
impl DomainSite {
    pub fn origin(&self) -> Option<&SourceOrigin> { self.origin.as_deref() }
    pub fn instance(&self) -> Option<&str> { self.instance.as_deref() }
    pub fn span(&self) -> Span { self.span }
    pub fn target(&self) -> &str { &self.target }
    pub fn transition(&self) -> DomainTransition { self.transition }
    pub fn ir_json(&self) -> String {
        let transition = match self.transition { DomainTransition::Expanded => "expanded", DomainTransition::Compiled => "compiled" };
        format!("{{\"kind\":\"domain_site\",\"transition\":{},\"definition\":{},\"instance\":{},\"target\":{},{}}}",
            text(transition), self.origin().map_or_else(|| "null".into(), SourceOrigin::ir_json),
            self.instance().map_or_else(|| "null".into(), text), text(self.target()),
            range_fields(self.origin().map(|o| o.source.text.as_ref()), self.span))
    }
}
#[derive(Clone, Debug)]
pub struct SourceSite {
    pub(crate) origin: std::sync::Arc<SourceOrigin>,
    pub(crate) context: std::sync::Arc<SourceContext>,
    pub(crate) span: Span,
    pub(crate) syntax: SourceSyntax,
}
fn range_fields(source: Option<&str>, span: Span) -> String {
    format!("\"span_unit\":\"utf8_byte\",\"span_range\":\"half_open\",\"start_byte\":{},\"end_byte\":{},\"column_unit\":\"unicode_scalar\",\"position_base\":1,\"start\":{},\"end\":{}",
        span.start, span.end, source.map_or_else(|| "null".into(), |s| position(s, span.start)),
        source.map_or_else(|| "null".into(), |s| position(s, span.end)))
}
impl InlineSite {
    pub fn origin(&self) -> Option<&SourceOrigin> { self.origin.as_deref() }
    pub fn instance(&self) -> Option<&str> { self.instance.as_deref() }
    pub fn span(&self) -> Span { self.span }
    pub fn ir_json(&self) -> String {
        format!("{{\"kind\":\"inline_site\",\"definition\":{},\"instance\":{},{}}}",
            self.origin().map_or_else(|| "null".into(), SourceOrigin::ir_json),
            self.instance().map_or_else(|| "null".into(), text),
            range_fields(self.origin().map(|o| o.source.text.as_ref()), self.span))
    }
}
impl SourceSite {
    pub fn origin(&self) -> &SourceOrigin { &self.origin }
    pub fn span(&self) -> Span { self.span }
    pub fn syntax(&self) -> SourceSyntax { self.syntax }
    pub fn instance(&self) -> Option<&str> { self.context.instance.as_deref() }
    /// Actual compiler inline expansion points, from the outermost to the innermost caller.
    pub fn inlined_at(&self) -> &[InlineSite] { &self.context.inlined_at }
    /// Actual source expansion and compiled domain boundaries, outermost first.
    pub fn domain_path(&self) -> &[DomainSite] { &self.context.domain_path }
    pub fn ir_json(&self) -> String {
        let syntax = match self.syntax {
            SourceSyntax::Statement => "statement", SourceSyntax::Expression => "expression",
            SourceSyntax::Place => "place",
            SourceSyntax::Parameter => "parameter",
        };
        format!("{{\"kind\":\"source_syntax\",\"syntax\":{},\"definition\":{},\"instance\":{},\"inlined_at_order\":\"outermost_first\",\"inlined_at\":[{}],\"domain_path_order\":\"outermost_first\",\"domain_path\":[{}],{}}}",
            text(syntax), self.origin.ir_json(), self.instance().map_or_else(|| "null".into(), text),
            self.inlined_at().iter().map(InlineSite::ir_json).collect::<Vec<_>>().join(","),
            self.domain_path().iter().map(DomainSite::ir_json).collect::<Vec<_>>().join(","),
            range_fields(Some(&self.origin.source.text), self.span))
    }
}

impl super::Stage {
    /// Original collective call or parallel statement, independent of generated code or later fusion.
    pub fn operation_source(&self) -> Option<&SourceSite> { self.operation_source.as_deref() }
}

/// The actual static value-function selection and its caller-side source facts.
#[derive(Clone, Debug)]
pub struct ValueCodeEntry {
    pub(crate) definition: Option<std::sync::Arc<SourceOrigin>>,
    pub(crate) instance: std::sync::Arc<str>,
    pub(crate) reference: Option<std::sync::Arc<SourceSite>>,
    pub(crate) capture_parameter_start: usize,
    pub(crate) captures: Box<[Option<std::sync::Arc<SourceSite>>]>,
}
impl ValueCodeEntry {
    pub fn definition(&self) -> Option<&SourceOrigin> { self.definition.as_deref() }
    pub fn instance(&self) -> &str { &self.instance }
    pub fn reference(&self) -> Option<&SourceSite> { self.reference.as_deref() }
    /// Zero-based B parameter index of the first capture, before scalar flattening.
    pub fn capture_parameter_start(&self) -> usize { self.capture_parameter_start }
    pub fn captures(&self) -> impl ExactSizeIterator<Item = Option<&SourceSite>> {
        self.captures.iter().map(|site| site.as_deref())
    }
    pub fn ir_json(&self) -> String {
        format!("{{\"kind\":\"value_entry\",\"definition\":{},\"instance\":{},\"reference\":{},\"parameter_index_base\":0,\"capture_parameter_start\":{},\"captures\":[{}]}}",
            self.definition().map_or_else(|| "null".into(), SourceOrigin::ir_json), text(self.instance()),
            self.reference().map_or_else(|| "null".into(), SourceSite::ir_json), self.capture_parameter_start,
            self.captures().enumerate().map(|(i, site)| format!("{{\"kind\":\"value_capture\",\"parameter\":{},\"source_site\":{}}}",
                self.capture_parameter_start + i, site.map_or_else(|| "null".into(), SourceSite::ir_json)))
                .collect::<Vec<_>>().join(","))
    }
}
impl super::ValueCode {
    pub fn entry_origin(&self) -> Option<&ValueCodeEntry> { self.entry_origin.as_deref() }
}

/// A real compiler generation responsibility, independent of an optional syntax trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationReason {
    ParallelIndex, ClearPrefix, ApplicationEntry, ApplicationEntryCall,
    FrameErrorReceiver, FrameErrorContext, FrameCreate, FrameReadiness, FrameFailureExit, FrameDestroy,
    DatasetAllocate,
    DatasetInitializeCursor, DatasetInitializeCondition, DatasetInitializeCall,
    DatasetInitializeWrite, DatasetInitializeAdvance, DatasetInitializeLoop,
    DatasetBindSlots, DatasetAssignSlots, DatasetRelease,
    DatasetDropCursor, DatasetDropCondition, DatasetDropIndex,
    DatasetDropElementRead, DatasetDropAdvance, DatasetDropLoop,
    UserOwnedDrop, ConditionalDropBranch, ConditionalDropCondition,
    VariantDropBranch, VariantDropCondition,
    DatasetResizeFill,
    DatasetResizeCall,
    DatasetAddress,
    DatasetStorageAddress,
    DatasetRootAddress,
    DatasetOffsetAddress,
    DatasetWriteAddress,
    DatasetAccessLayout,
    DatasetFieldLayout,
    DatasetProjectionAddress,
    DatasetUpload, DatasetDownload,
    ApplicationDomainCall,
    DomainEntry,
    DomainCallLiteral,
    DomainPartCall,
    DatasetCallArgument,
    DatasetBorrowStrideCast,
    ArrayParameterStore,
    IntegerDivisionPair,
    ExpressionReuse,
    LocalReadForward,
    IndexedStoreReadForward,
    IndexedReadReuse,
    UnusedValueResults,
    EmptyMapRemoval,
    IntegerDivisionResultPrune,
}
/// Only the immutable origin facts of a consumed operation, never its IR or values.
#[derive(Clone, Debug)]
pub struct OperationOrigin {
    pub(crate) source_site: Option<std::sync::Arc<SourceSite>>,
    pub(crate) generated_origin: Option<std::sync::Arc<GeneratedOrigin>>,
}
impl OperationOrigin {
    pub(crate) fn from_instruction(value: &super::Inst) -> Self {
        Self { source_site: value.source_site.clone(), generated_origin: value.generated_origin.clone() }
    }
    pub fn source_site(&self) -> Option<&SourceSite> { self.source_site.as_deref() }
    pub fn generated_origin(&self) -> Option<&GeneratedOrigin> { self.generated_origin.as_deref() }
    pub fn ir_json(&self) -> String {
        format!("{{\"kind\":\"operation_origin\",\"source_site\":{},\"generated_origin\":{}}}",
            self.source_site().map_or_else(|| "null".into(), SourceSite::ir_json),
            self.generated_origin().map_or_else(|| "null".into(), GeneratedOrigin::ir_json))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReuseResult { Value, Quotient, Remainder }
#[derive(Clone, Debug)]
pub struct ExpressionReuse {
    pub(crate) result: ReuseResult,
    pub(crate) paired_input: bool,
    pub(crate) origin: OperationOrigin,
}
impl ExpressionReuse {
    pub fn result(&self) -> ReuseResult { self.result }
    pub fn input_results(&self) -> &[ReuseResult] {
        if self.paired_input { &[ReuseResult::Quotient, ReuseResult::Remainder] }
        else { std::slice::from_ref(&self.result) }
    }
    pub fn origin(&self) -> &OperationOrigin { &self.origin }
    pub fn ir_json(&self) -> String {
        let name = |result| match result { ReuseResult::Value => "value", ReuseResult::Quotient => "quotient", ReuseResult::Remainder => "remainder" };
        format!("{{\"kind\":\"expression_reuse_input\",\"result\":{},\"input_results\":[{}],\"origin\":{}}}",
            text(name(self.result)), self.input_results().iter().map(|r|text(name(*r))).collect::<Vec<_>>().join(","), self.origin.ir_json())
    }
}
#[derive(Clone, Debug)]
pub struct LocalReadForward {
    pub(crate) result_index: usize,
    pub(crate) result_count: usize,
    pub(crate) result: ReuseResult,
    pub(crate) origin: OperationOrigin,
}
impl LocalReadForward {
    pub fn result_index(&self) -> usize { self.result_index }
    pub fn result_count(&self) -> usize { self.result_count }
    pub fn result(&self) -> ReuseResult { self.result }
    pub fn origin(&self) -> &OperationOrigin { &self.origin }
    pub fn ir_json(&self) -> String {
        let result = match self.result { ReuseResult::Value => "value", ReuseResult::Quotient => "quotient", ReuseResult::Remainder => "remainder" };
        format!("{{\"kind\":\"local_read_forward_input\",\"result_index\":{},\"result_count\":{},\"result\":{},\"origin\":{}}}",
            self.result_index, self.result_count, text(result), self.origin.ir_json())
    }
}
#[derive(Clone, Debug)]
pub struct IndexedStoreReadForward {
    pub(crate) result_index: usize,
    pub(crate) result_count: usize,
    pub(crate) result: ReuseResult,
    pub(crate) store: OperationOrigin,
    pub(crate) read: OperationOrigin,
}
impl IndexedStoreReadForward {
    pub fn result_index(&self) -> usize { self.result_index }
    pub fn result_count(&self) -> usize { self.result_count }
    pub fn result(&self) -> ReuseResult { self.result }
    pub fn store(&self) -> &OperationOrigin { &self.store }
    pub fn read(&self) -> &OperationOrigin { &self.read }
    pub fn ir_json(&self) -> String {
        let result = match self.result { ReuseResult::Value => "value", ReuseResult::Quotient => "quotient", ReuseResult::Remainder => "remainder" };
        format!("{{\"kind\":\"indexed_store_read_forward_input\",\"result_index\":{},\"result_count\":{},\"result\":{},\"store\":{},\"read\":{}}}",
            self.result_index, self.result_count, text(result), self.store.ir_json(), self.read.ir_json())
    }
}
#[derive(Clone, Debug)]
pub struct InputReadForward {
    pub(crate) input_index: usize,
    pub(crate) reads: Vec<OperationOrigin>,
}
impl InputReadForward {
    pub fn input_index(&self) -> usize { self.input_index }
    pub fn reads(&self) -> &[OperationOrigin] { &self.reads }
    pub fn ir_json(&self) -> String {
        format!("{{\"kind\":\"input_read_forward\",\"input_index\":{},\"reads\":[{}]}}", self.input_index,
            self.reads.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(","))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReductionInputRole { Source, Left, Right }
#[derive(Clone, Debug)]
pub struct ReductionInputReadForward {
    pub(crate) role: ReductionInputRole,
    pub(crate) component_index: usize,
    pub(crate) reads: Vec<OperationOrigin>,
}
impl ReductionInputReadForward {
    pub fn role(&self) -> ReductionInputRole { self.role }
    pub fn component_index(&self) -> usize { self.component_index }
    pub fn reads(&self) -> &[OperationOrigin] { &self.reads }
    pub fn ir_json(&self) -> String {
        let role = match self.role { ReductionInputRole::Source => "source", ReductionInputRole::Left => "left", ReductionInputRole::Right => "right" };
        format!("{{\"kind\":\"reduction_input_read_forward\",\"role\":{},\"component_index\":{},\"reads\":[{}]}}",
            text(role), self.component_index, self.reads.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(","))
    }
}
#[derive(Clone, Debug)]
pub struct EliminatedValue {
    pub(crate) result_types: Vec<super::Scalar>,
    pub(crate) integer_division_results: Option<Vec<super::IntegerDivisionOp>>,
    pub(crate) origin: OperationOrigin,
}
impl EliminatedValue {
    pub fn result_types(&self) -> &[super::Scalar] { &self.result_types }
    pub fn integer_division_results(&self) -> Option<&[super::IntegerDivisionOp]> { self.integer_division_results.as_deref() }
    pub fn origin(&self) -> &OperationOrigin { &self.origin }
    pub fn ir_json(&self) -> String {
        format!("{{\"kind\":\"eliminated_value\",\"result_types\":[{}],\"integer_division_results\":{},\"origin\":{}}}",
            self.result_types.iter().map(|value| text(&format!("{value:?}"))).collect::<Vec<_>>().join(","),
            self.integer_division_results().map_or_else(|| "null".into(), division_results_json), self.origin.ir_json())
    }
}
fn division_results_json(results: &[super::IntegerDivisionOp]) -> String {
    format!("[{}]", results.iter().map(|result| text(match result {
        super::IntegerDivisionOp::Quotient => "quotient", super::IntegerDivisionOp::Remainder => "remainder",
    })).collect::<Vec<_>>().join(","))
}
#[derive(Clone, Debug)]
pub enum GenerationAnchor {
    Syntax(std::sync::Arc<SourceSite>),
    Definition { origin: Option<std::sync::Arc<SourceOrigin>>, instance: std::sync::Arc<str> },
    IntegerDivisionPair { quotient: OperationOrigin, remainder: OperationOrigin },
    ExpressionReuse { retained: OperationOrigin, reused: Vec<ExpressionReuse> },
    LocalReadForward { retained: OperationOrigin, reads: Vec<LocalReadForward> },
    IndexedStoreReadForward { retained: OperationOrigin, reads: Vec<IndexedStoreReadForward> },
    IndexedReadReuse { retained: OperationOrigin, reused: Vec<OperationOrigin> },
    LocalInputForward { retained: Option<std::sync::Arc<GeneratedOrigin>>, inputs: Vec<InputReadForward> },
    ReductionInputForward { retained: Option<std::sync::Arc<GeneratedOrigin>>, inputs: Vec<ReductionInputReadForward> },
    EliminatedValues { retained: Option<std::sync::Arc<GeneratedOrigin>>, values: Vec<EliminatedValue> },
    EmptyMapRemoval { retained: Option<std::sync::Arc<GeneratedOrigin>>, stages: Vec<OperationOrigin> },
    IntegerDivisionResultPrune { retained: OperationOrigin, input_results: Vec<super::IntegerDivisionOp>, output_results: Vec<super::IntegerDivisionOp> },
}
impl GenerationAnchor {
    pub fn syntax(&self) -> Option<&SourceSite> { match self { Self::Syntax(site) => Some(site), Self::Definition { .. } | Self::IntegerDivisionPair { .. } | Self::ExpressionReuse { .. } | Self::LocalReadForward { .. } | Self::IndexedStoreReadForward { .. } | Self::IndexedReadReuse { .. } | Self::LocalInputForward { .. } | Self::ReductionInputForward { .. } | Self::EliminatedValues { .. } | Self::EmptyMapRemoval { .. } | Self::IntegerDivisionResultPrune { .. } => None } }
    pub fn origin(&self) -> Option<&SourceOrigin> { match self { Self::Syntax(site) => Some(site.origin()), Self::Definition { origin, .. } => origin.as_deref(), Self::IntegerDivisionPair { .. } | Self::ExpressionReuse { .. } | Self::LocalReadForward { .. } | Self::IndexedStoreReadForward { .. } | Self::IndexedReadReuse { .. } | Self::LocalInputForward { .. } | Self::ReductionInputForward { .. } | Self::EliminatedValues { .. } | Self::EmptyMapRemoval { .. } | Self::IntegerDivisionResultPrune { .. } => None } }
    pub fn instance(&self) -> Option<&str> { match self { Self::Syntax(site) => site.instance(), Self::Definition { instance, .. } => Some(instance), Self::IntegerDivisionPair { .. } | Self::ExpressionReuse { .. } | Self::LocalReadForward { .. } | Self::IndexedStoreReadForward { .. } | Self::IndexedReadReuse { .. } | Self::LocalInputForward { .. } | Self::ReductionInputForward { .. } | Self::EliminatedValues { .. } | Self::EmptyMapRemoval { .. } | Self::IntegerDivisionResultPrune { .. } => None } }
    pub fn ir_json(&self) -> String {
        match self {
            Self::Syntax(site) => site.ir_json(),
            Self::Definition { origin, instance } => format!("{{\"kind\":\"definition_instance\",\"definition\":{},\"instance\":{}}}",
                origin.as_deref().map_or_else(|| "null".into(), SourceOrigin::ir_json), text(instance)),
            Self::IntegerDivisionPair { quotient, remainder } => format!("{{\"kind\":\"integer_division_inputs\",\"quotient\":{},\"remainder\":{}}}",
                quotient.ir_json(), remainder.ir_json()),
            Self::ExpressionReuse { retained, reused } => format!("{{\"kind\":\"expression_reuse\",\"retained\":{},\"reused\":[{}]}}",
                retained.ir_json(), reused.iter().map(ExpressionReuse::ir_json).collect::<Vec<_>>().join(",")),
            Self::LocalReadForward { retained, reads } => format!("{{\"kind\":\"local_read_forward\",\"result_index_base\":0,\"retained\":{},\"reads\":[{}]}}",
                retained.ir_json(), reads.iter().map(LocalReadForward::ir_json).collect::<Vec<_>>().join(",")),
            Self::IndexedStoreReadForward { retained, reads } => format!("{{\"kind\":\"indexed_store_read_forward\",\"result_index_base\":0,\"retained\":{},\"reads\":[{}]}}",
                retained.ir_json(), reads.iter().map(IndexedStoreReadForward::ir_json).collect::<Vec<_>>().join(",")),
            Self::IndexedReadReuse { retained, reused } => format!("{{\"kind\":\"indexed_read_reuse\",\"retained\":{},\"reused\":[{}]}}",
                retained.ir_json(), reused.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(",")),
            Self::LocalInputForward { retained, inputs } => format!("{{\"kind\":\"local_input_forward\",\"input_index_base\":0,\"retained\":{},\"inputs\":[{}]}}",
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json), inputs.iter().map(InputReadForward::ir_json).collect::<Vec<_>>().join(",")),
            Self::ReductionInputForward { retained, inputs } => format!("{{\"kind\":\"reduction_input_forward\",\"component_index_base\":0,\"retained\":{},\"inputs\":[{}]}}",
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json), inputs.iter().map(ReductionInputReadForward::ir_json).collect::<Vec<_>>().join(",")),
            Self::EliminatedValues { retained, values } => format!("{{\"kind\":\"eliminated_values\",\"retained\":{},\"values\":[{}]}}",
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json), values.iter().map(EliminatedValue::ir_json).collect::<Vec<_>>().join(",")),
            Self::EmptyMapRemoval { retained, stages } => format!("{{\"kind\":\"empty_map_removal\",\"retained\":{},\"stages\":[{}]}}",
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json), stages.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(",")),
            Self::IntegerDivisionResultPrune { retained, input_results, output_results } => format!("{{\"kind\":\"integer_division_result_prune\",\"retained\":{},\"input_results\":{},\"output_results\":{}}}",
                retained.ir_json(), division_results_json(input_results), division_results_json(output_results)),
        }
    }
}
#[derive(Clone, Debug)]
pub struct GeneratedOrigin {
    pub(crate) reason: GenerationReason,
    pub(crate) trigger: Option<GenerationAnchor>,
}
impl GeneratedOrigin {
    pub(crate) fn eliminated_values(origin: &mut Option<std::sync::Arc<Self>>, values: Vec<EliminatedValue>) {
        if !values.is_empty() {
            let retained = origin.take();
            *origin = Some(std::sync::Arc::new(Self {
                reason: GenerationReason::UnusedValueResults,
                trigger: Some(GenerationAnchor::EliminatedValues { retained, values }),
            }));
        }
    }
    pub(crate) fn remove_input_prefix(origin: &mut Option<std::sync::Arc<Self>>, count: usize) {
        let Some(mut shared) = origin.take() else { return; };
        match &mut std::sync::Arc::make_mut(&mut shared).trigger {
            Some(GenerationAnchor::LocalInputForward { retained, inputs }) => {
                Self::remove_input_prefix(retained, count);
                inputs.retain_mut(|input| {
                    if input.input_index < count { false }
                    else { input.input_index -= count; true }
                });
                if inputs.is_empty() { *origin = retained.take(); return; }
            }
            Some(GenerationAnchor::EliminatedValues { retained, .. }) => Self::remove_input_prefix(retained, count),
            _ => {}
        }
        *origin = Some(shared);
    }
    pub fn reason(&self) -> GenerationReason { self.reason }
    /// Returns a single syntax trigger; definitions and multi-input rewrites use their anchor.
    pub fn trigger(&self) -> Option<&SourceSite> { self.trigger.as_ref().and_then(GenerationAnchor::syntax) }
    pub fn anchor(&self) -> Option<&GenerationAnchor> { self.trigger.as_ref() }
    pub fn ir_json(&self) -> String {
        let reason = match self.reason {
            GenerationReason::ParallelIndex => "parallel_index", GenerationReason::ClearPrefix => "clear_prefix",
            GenerationReason::ApplicationEntry => "application_entry", GenerationReason::ApplicationEntryCall => "application_entry_call",
            GenerationReason::FrameErrorReceiver => "frame_error_receiver", GenerationReason::FrameErrorContext => "frame_error_context",
            GenerationReason::FrameCreate => "frame_create", GenerationReason::FrameReadiness => "frame_readiness",
            GenerationReason::FrameFailureExit => "frame_failure_exit", GenerationReason::FrameDestroy => "frame_destroy",
            GenerationReason::DatasetAllocate => "dataset_allocate",
            GenerationReason::DatasetInitializeCursor => "dataset_initialize_cursor",
            GenerationReason::DatasetInitializeCondition => "dataset_initialize_condition",
            GenerationReason::DatasetInitializeCall => "dataset_initialize_call",
            GenerationReason::DatasetInitializeWrite => "dataset_initialize_write",
            GenerationReason::DatasetInitializeAdvance => "dataset_initialize_advance",
            GenerationReason::DatasetInitializeLoop => "dataset_initialize_loop",
            GenerationReason::DatasetBindSlots => "dataset_bind_slots",
            GenerationReason::DatasetAssignSlots => "dataset_assign_slots",
            GenerationReason::DatasetRelease => "dataset_release",
            GenerationReason::DatasetDropCursor => "dataset_drop_cursor",
            GenerationReason::DatasetDropCondition => "dataset_drop_condition",
            GenerationReason::DatasetDropIndex => "dataset_drop_index",
            GenerationReason::DatasetDropElementRead => "dataset_drop_element_read",
            GenerationReason::DatasetDropAdvance => "dataset_drop_advance",
            GenerationReason::DatasetDropLoop => "dataset_drop_loop",
            GenerationReason::UserOwnedDrop => "user_owned_drop",
            GenerationReason::ConditionalDropBranch => "conditional_drop_branch",
            GenerationReason::ConditionalDropCondition => "conditional_drop_condition",
            GenerationReason::VariantDropBranch => "variant_drop_branch",
            GenerationReason::VariantDropCondition => "variant_drop_condition",
            GenerationReason::DatasetResizeFill => "dataset_resize_fill",
            GenerationReason::DatasetResizeCall => "dataset_resize_call",
            GenerationReason::DatasetAddress => "dataset_address",
            GenerationReason::DatasetStorageAddress => "dataset_storage_address",
            GenerationReason::DatasetRootAddress => "dataset_root_address",
            GenerationReason::DatasetOffsetAddress => "dataset_offset_address",
            GenerationReason::DatasetWriteAddress => "dataset_write_address",
            GenerationReason::DatasetAccessLayout => "dataset_access_layout",
            GenerationReason::DatasetFieldLayout => "dataset_field_layout",
            GenerationReason::DatasetProjectionAddress => "dataset_projection_address",
            GenerationReason::DatasetUpload => "dataset_upload",
            GenerationReason::DatasetDownload => "dataset_download",
            GenerationReason::ApplicationDomainCall => "application_domain_call",
            GenerationReason::DomainEntry => "domain_entry",
            GenerationReason::DomainCallLiteral => "domain_call_literal",
            GenerationReason::DomainPartCall => "domain_part_call",
            GenerationReason::DatasetCallArgument => "dataset_call_argument",
            GenerationReason::DatasetBorrowStrideCast => "dataset_borrow_stride_cast",
            GenerationReason::ArrayParameterStore => "array_parameter_store",
            GenerationReason::IntegerDivisionPair => "integer_division_pair",
            GenerationReason::ExpressionReuse => "expression_reuse",
            GenerationReason::LocalReadForward => "local_read_forward",
            GenerationReason::IndexedStoreReadForward => "indexed_store_read_forward",
            GenerationReason::IndexedReadReuse => "indexed_read_reuse",
            GenerationReason::UnusedValueResults => "unused_value_results",
            GenerationReason::EmptyMapRemoval => "empty_map_removal",
            GenerationReason::IntegerDivisionResultPrune => "integer_division_result_prune",
        };
        format!("{{\"kind\":\"generated_origin\",\"reason\":{},\"trigger\":{}}}", text(reason),
            self.anchor().map_or_else(|| "null".into(), GenerationAnchor::ir_json))
    }
}
impl super::Stage {
    pub fn generated_origin(&self) -> Option<&GeneratedOrigin> { self.generated_origin.as_deref() }
}
impl super::ValueCode {
    pub fn generated_origin(&self) -> Option<&GeneratedOrigin> { self.generated_origin.as_deref() }
}
impl super::Program {
    pub fn generated_origin(&self) -> Option<&GeneratedOrigin> { self.generated_origin.as_deref() }
}
