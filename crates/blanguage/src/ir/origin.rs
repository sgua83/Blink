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
pub enum SourceSyntax { Statement, Expression, Place }
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
}
#[derive(Clone, Debug)]
pub enum GenerationAnchor {
    Syntax(std::sync::Arc<SourceSite>),
    Definition { origin: Option<std::sync::Arc<SourceOrigin>>, instance: std::sync::Arc<str> },
}
impl GenerationAnchor {
    pub fn syntax(&self) -> Option<&SourceSite> { match self { Self::Syntax(site) => Some(site), Self::Definition { .. } => None } }
    pub fn origin(&self) -> Option<&SourceOrigin> { match self { Self::Syntax(site) => Some(site.origin()), Self::Definition { origin, .. } => origin.as_deref() } }
    pub fn instance(&self) -> Option<&str> { match self { Self::Syntax(site) => site.instance(), Self::Definition { instance, .. } => Some(instance) } }
    pub fn ir_json(&self) -> String {
        match self {
            Self::Syntax(site) => site.ir_json(),
            Self::Definition { origin, instance } => format!("{{\"kind\":\"definition_instance\",\"definition\":{},\"instance\":{}}}",
                origin.as_deref().map_or_else(|| "null".into(), SourceOrigin::ir_json), text(instance)),
        }
    }
}
#[derive(Clone, Debug)]
pub struct GeneratedOrigin {
    pub(crate) reason: GenerationReason,
    pub(crate) trigger: Option<GenerationAnchor>,
}
impl GeneratedOrigin {
    pub fn reason(&self) -> GenerationReason { self.reason }
    /// Returns only an actual syntax trigger; implicit entry points use a definition anchor.
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
