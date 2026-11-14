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

unsafe extern "C" {
    #[link_name = "blink.text_core::write_hexadecimal"]
    fn write_hexadecimal(input: *const u8, count: u64, output: *mut u8);
}

pub(crate) fn bytes(out: &mut String, value: &[u8]) {
    out.push('"');
    let start = out.len();
    // String owns and checks the complete encoded extent. Saturation sends an
    // unrepresentable request through that same capacity check.
    out.reserve(value.len().saturating_mul(2));
    unsafe {
        let bytes = out.as_mut_vec();
        write_hexadecimal(value.as_ptr(), value.len() as u64, bytes.as_mut_ptr().add(start));
        // The synchronous B encoder initialized exactly two ASCII bytes per input.
        bytes.set_len(start + value.len() * 2);
    }
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
