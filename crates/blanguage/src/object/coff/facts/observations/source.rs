//! Source identity and coordinates in optional observation material, without source text.
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
pub(super) struct Range { span: Span, start: Option<Position>, end: Option<Position> }
record!(Range { span, start, end });
impl Range {
    pub(super) fn capture(source: Option<&str>, span: Span) -> Self {
        Self { span, start: source.map(|text| Position::capture(text, span.start)),
            end: source.map(|text| Position::capture(text, span.end)) }
    }
    pub(super) fn fields(&self) -> Vec<(&'static str, String)> {
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
pub(super) struct Definition {
    module: String, name: String, path: Option<String>, digest: [u8; 32],
    bytes: usize, range: Range, encoding: PathEncoding,
}
record!(Definition { module, name, path, digest, bytes, range, encoding });
impl Definition {
    pub(super) fn capture(origin: &ir::SourceOrigin) -> Self {
        Self::capture_source(origin.source(), origin.module(), origin.definition(), origin.span())
    }
    fn capture_source(source: &crate::ast::Source, module: &str, name: &str, span: Span) -> Self {
        Self { module: module.into(), name: name.into(),
            path: source.path.to_str().map(str::to_owned), digest: hash::digest(source.text.as_bytes()),
            bytes: source.text.len(), range: Range::capture(Some(&source.text), span),
            encoding: PathEncoding::capture(&source.path) }
    }
    pub(super) fn json(&self) -> String { object(&self.fields("source_definition", "definition")) }
    fn fields(&self, kind: &str, name: &'static str) -> Vec<(&'static str, String)> {
        let mut fields = vec![("kind", text(kind)), ("module", text(&self.module)),
            (name, text(&self.name)), ("source_path", optional(self.path.as_deref(), text)),
            ("source_sha256", text(&hash::hex(&self.digest))), ("source_bytes", self.bytes.to_string())];
        fields.extend(self.range.fields()); fields.push(self.encoding.field()); fields
    }
}
struct Inline { definition: Option<Definition>, instance: Option<String>, range: Range }
record!(Inline { definition, instance, range });
impl Inline {
    fn capture(site: &ir::InlineSite) -> Self {
        Self { definition: site.origin().map(Definition::capture),
            instance: site.instance().map(str::to_owned), range: Range::capture(site.origin().map(|o| o.source().text.as_ref()), site.span()) }
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
            instance: site.instance().map(str::to_owned), range: Range::capture(site.origin().map(|o| o.source().text.as_ref()), site.span()),
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
pub(super) struct Site {
    definition: Definition, instance: Option<String>, syntax: ir::SourceSyntax,
    inlined_at: Vec<Inline>, domain_path: Vec<Domain>, range: Range,
}
record!(Site { definition, instance, syntax, inlined_at, domain_path, range });
variants!(ir::SourceSyntax { 0=>Statement, 1=>Expression, 2=>Place, 3=>Parameter });
impl Site {
    pub(super) fn capture(site: &ir::SourceSite) -> Self {
        Self { definition: Definition::capture(site.origin()), instance: site.instance().map(str::to_owned),
            syntax: site.syntax(), inlined_at: site.inlined_at().iter().map(Inline::capture).collect(),
            domain_path: site.domain_path().iter().map(Domain::capture).collect(),
            range: Range::capture(Some(&site.origin().source().text), site.span()) }
    }
    pub(super) fn json(&self) -> String {
        let syntax = match self.syntax { ir::SourceSyntax::Statement => "statement", ir::SourceSyntax::Expression => "expression",
            ir::SourceSyntax::Place => "place", ir::SourceSyntax::Parameter => "parameter" };
        let mut fields = vec![("kind", text("source_syntax")), ("syntax", text(syntax)),
            ("definition", self.definition.json()), ("instance", optional(self.instance.as_deref(), text)),
            ("inlined_at_order", text("outermost_first")), ("inlined_at", array(self.inlined_at.iter().map(Inline::json))),
            ("domain_path_order", text("outermost_first")), ("domain_path", array(self.domain_path.iter().map(Domain::json)))];
        fields.extend(self.range.fields()); object(&fields)
    }
}
pub(super) fn optional<T: ?Sized>(value: Option<&T>, json: impl FnOnce(&T) -> String) -> String {
    value.map_or_else(|| "null".into(), json)
}

pub(super) struct Declaration { value: Definition, representation: ir::DeclarationRepresentation }
record!(Declaration { value, representation });
variants!(ir::DeclarationRepresentation { 0=>Source, 1=>CompiledInterface });
impl Declaration {
    fn capture(origin: &ir::SourceDeclaration) -> Self {
        Self { value: Definition::capture_source(origin.source(), origin.module(), origin.declaration(), origin.span()),
            representation: origin.representation() }
    }
    pub(super) fn json(&self) -> String {
        let mut fields = self.value.fields("source_declaration", "declaration");
        let representation = match self.representation {
            ir::DeclarationRepresentation::Source => "source",
            ir::DeclarationRepresentation::CompiledInterface => "compiled_interface",
        };
        fields.insert(3, ("representation", text(representation))); object(&fields)
    }
}
impl Encode for ir::SourceOrigin {
    fn encode(&self, writer: &mut Writer) { Definition::capture(self).encode(writer); }
}
impl Encode for ir::SourceSite {
    fn encode(&self, writer: &mut Writer) { Site::capture(self).encode(writer); }
}
impl Encode for ir::SourceDeclaration {
    fn encode(&self, writer: &mut Writer) { Declaration::capture(self).encode(writer); }
}
