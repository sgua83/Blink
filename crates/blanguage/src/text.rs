//! Synchronously write ordinary B text into the caller-owned String.
unsafe extern "C" {
    #[link_name = "blink.text_core::write_hexadecimal"]
    fn write_hexadecimal(input: *const u8, count: u64, output: *mut u8);
}

pub(crate) fn hexadecimal(out: &mut String, value: &[u8]) {
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
}
