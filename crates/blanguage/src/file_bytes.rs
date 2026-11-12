use crate::syntax;
use std::{
    fs::File,
    io,
    ops::Deref,
    os::windows::io::{AsRawHandle, RawHandle},
};

#[repr(C)]
pub(crate) struct FileBytes {
    data: *mut u8,
    length: usize,
    error: syntax::wire::StorageError,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ast_storage::read_source"]
    fn read_source(file: RawHandle) -> FileBytes;
    #[link_name = "bootstrap.ast_storage::release_source"]
    fn release_source(data: *mut u8) -> i32;
}

// ReadFile finishes before the buffer is shared. Every consumer only borrows
// its initialized bytes, and the last owner releases it on the Windows heap.
unsafe impl Send for FileBytes {}
unsafe impl Sync for FileBytes {}

impl FileBytes {
    pub(crate) fn read(file: &File) -> Result<Self, Box<dyn std::error::Error>> {
        const SOURCE_READ_ERROR: u32 = 4;
        let bytes = unsafe { read_source(file.as_raw_handle()) };
        match bytes.error.kind {
            0 => Ok(bytes),
            SOURCE_READ_ERROR => Err(io::Error::from_raw_os_error(bytes.error.native_code as i32).into()),
            kind => Err(syntax::StorageFailure { kind, native_code: bytes.error.native_code }.into()),
        }
    }
}

impl Deref for FileBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        // A successful B read owns exactly this initialized byte range.
        unsafe { std::slice::from_raw_parts(self.data, self.length) }
    }
}

impl Drop for FileBytes {
    fn drop(&mut self) {
        if !self.data.is_null() && unsafe { release_source(self.data) } == 0 {
            panic!("无法释放 B 文件缓冲");
        }
    }
}
