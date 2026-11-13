//! 构建输入的 SHA-256；不参与资源 ABI 或运行期所有权。
#[repr(C)]
pub(crate) struct Digest {
    words: [u32; 8],
    tail: [u64; 16],
    pending: u64,
    length: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_digest::sha256_start"]
    fn sha256_start() -> Digest;
    #[link_name = "bootstrap.build_digest::sha256_update"]
    fn sha256_update(state: &mut Digest, input: *const u8, length: usize);
    #[link_name = "bootstrap.build_digest::digest_field"]
    fn digest_field(state: &mut Digest, field: super::ByteField);
    #[link_name = "bootstrap.build_digest::sha256_finish"]
    fn sha256_finish(state: &mut Digest, output: *mut u8);
    #[link_name = "bootstrap.build_digest::sha256"]
    fn sha256(input: *const u8, length: usize, output: *mut u8);
    #[link_name = "bootstrap.build_digest::sha256_fields"]
    fn sha256_fields(fields: *const super::ByteField, count: usize, output: *mut u8);
}

impl Digest {
    pub(crate) fn new() -> Self {
        unsafe { sha256_start() }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        unsafe { sha256_update(self, bytes.as_ptr(), bytes.len()); }
    }

    pub(crate) fn field(&mut self, bytes: &[u8]) {
        unsafe { digest_field(self, bytes.into()) }
    }

    pub(crate) fn finish(mut self) -> [u8; 32] {
        let mut output = std::mem::MaybeUninit::<[u8; 32]>::uninit();
        unsafe {
            sha256_finish(&mut self, output.as_mut_ptr().cast());
            output.assume_init()
        }
    }
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
