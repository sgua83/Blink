//! JSON text escaping shared by actual compiler query outputs.
use std::fmt::Write;

pub(crate) fn string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
}

pub(crate) fn bytes(out: &mut String, value: &[u8]) {
    out.push('"');
    for byte in value { write!(out, "{byte:02x}").unwrap(); }
    out.push('"');
}

pub(crate) fn path(out: &mut String, name: &str, value: &std::path::Path) {
    string(out, name);
    out.push(':');
    if let Some(value) = value.to_str() { string(out, value); }
    else { out.push_str("null"); }
    out.push(',');
    string(out, &format!("{name}_encoded_bytes"));
    out.push(':');
    bytes(out, value.as_os_str().as_encoded_bytes());
}
