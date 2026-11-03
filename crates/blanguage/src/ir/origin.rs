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
#[derive(Clone, Debug)]
pub struct SourceSite {
    pub(crate) origin: std::sync::Arc<SourceOrigin>,
    pub(crate) span: Span,
    pub(crate) syntax: SourceSyntax,
}
impl SourceSite {
    pub fn origin(&self) -> &SourceOrigin { &self.origin }
    pub fn span(&self) -> Span { self.span }
    pub fn syntax(&self) -> SourceSyntax { self.syntax }
    pub fn ir_json(&self) -> String {
        let syntax = match self.syntax {
            SourceSyntax::Statement => "statement", SourceSyntax::Expression => "expression",
            SourceSyntax::Place => "place",
        };
        format!("{{\"kind\":\"source_syntax\",\"syntax\":{},\"definition\":{},\"span_unit\":\"utf8_byte\",\"span_range\":\"half_open\",\"start_byte\":{},\"end_byte\":{},\"column_unit\":\"unicode_scalar\",\"position_base\":1,\"start\":{},\"end\":{}}}",
            text(syntax), self.origin.ir_json(), self.span.start, self.span.end,
            position(&self.origin.source.text, self.span.start), position(&self.origin.source.text, self.span.end))
    }
}
