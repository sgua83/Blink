//! 构建输入的 SHA-256；不参与资源 ABI 或运行期所有权。
unsafe extern "C" {
    #[link_name = "bootstrap.build_digest::sha256"]
    fn sha256(input: *const u8, length: usize, output: *mut u8);
    #[link_name = "bootstrap.build_digest::sha256_fields"]
    fn sha256_fields(fields: *const super::ByteField, count: usize, output: *mut u8);
}

pub fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut output = std::mem::MaybeUninit::<[u8; 32]>::uninit();
    unsafe {
        sha256(bytes.as_ptr(), bytes.len(), output.as_mut_ptr().cast());
        output.assume_init()
    }
}
pub(super) fn fields(fields: &[super::ByteField]) -> [u8; 32] {
    let mut output = std::mem::MaybeUninit::<[u8; 32]>::uninit();
    unsafe {
        sha256_fields(fields.as_ptr(), fields.len(), output.as_mut_ptr().cast());
        output.assume_init()
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn field(target: &mut Vec<u8>, bytes: &[u8]) {
    target.extend((bytes.len() as u64).to_le_bytes());
    target.extend(bytes);
}
