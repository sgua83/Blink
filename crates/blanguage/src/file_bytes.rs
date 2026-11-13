use crate::syntax;
use std::{
    fs::File,
    io,
    ops::Deref,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, RawHandle},
    },
    path::Path,
};

#[repr(C)]
pub(crate) struct FileBytes {
    data: *mut u8,
    length: usize,
    error: syntax::wire::StorageError,
}

#[repr(u32)]
enum HostWriteKind {
    Written,
    OpenError,
    WriteError,
    NoProgress,
}

#[repr(C)]
pub(crate) struct HostWriteResult {
    kind: HostWriteKind,
    status: u32,
}

impl HostWriteResult {
    pub(crate) fn created(&self) -> bool {
        !matches!(self.kind, HostWriteKind::OpenError)
    }

    pub(crate) fn into_result(self) -> io::Result<()> {
        match self.kind {
            HostWriteKind::Written => Ok(()),
            HostWriteKind::OpenError | HostWriteKind::WriteError => {
                Err(io::Error::from_raw_os_error(self.status as i32))
            }
            HostWriteKind::NoProgress => Err(io::Error::new(io::ErrorKind::WriteZero, "failed to write whole buffer")),
        }
    }
}

#[repr(C)]
struct HostPublishResult {
    written: HostWriteResult,
    replacement: u32,
    cleanup: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.host_sdk::publish_host_file"]
    fn publish_host_file(
        target: *const u16,
        target_units: usize,
        temporary: *const u16,
        temporary_units: usize,
        data: *const u8,
        bytes: usize,
    ) -> HostPublishResult;
    #[link_name = "blink.windows.path_core::create_output_directories"]
    fn create_output_directories(path: *const u16, units: usize) -> u32;
    #[link_name = "bootstrap.host_sdk::write_host_input"]
    fn write_host_input(path: *const u16, units: usize, data: *const u8, bytes: usize) -> HostWriteResult;
    #[link_name = "bootstrap.host_sdk::write_host_output"]
    fn write_host_output(path: *const u16, units: usize, data: *const u8, bytes: usize) -> HostWriteResult;
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
        unsafe { read_source(file.as_raw_handle()) }.into_result()
    }

    pub(crate) fn into_result(self) -> Result<Self, Box<dyn std::error::Error>> {
        const SOURCE_READ_ERROR: u32 = 4;
        match self.error.kind {
            0 => Ok(self),
            SOURCE_READ_ERROR => Err(io::Error::from_raw_os_error(self.error.native_code as i32).into()),
            kind => Err(syntax::StorageFailure { kind, native_code: self.error.native_code }.into()),
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

pub(crate) fn host_path(path: &Path) -> io::Result<Vec<u16>> {
    let mut units: Vec<_> = path.as_os_str().encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "file name contained an unexpected NUL byte"));
    }
    units.push(0);
    Ok(units)
}

pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> io::Result<HostWriteResult> {
    let path = host_path(path)?;
    Ok(unsafe { write_host_input(path.as_ptr(), path.len() - 1, bytes.as_ptr(), bytes.len()) })
}

pub(crate) fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let path = host_path(path)?;
    unsafe { write_host_output(path.as_ptr(), path.len() - 1, bytes.as_ptr(), bytes.len()) }.into_result()
}

pub(crate) fn create_dir_all(path: &Path) -> io::Result<()> {
    if path.parent().is_none() {
        return Ok(());
    }
    let units = host_path(path)?;
    let status = unsafe { create_output_directories(units.as_ptr(), units.len() - 1) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

pub(crate) fn publish(path: &Path, temporary: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = host_path(temporary)?;
    let path = host_path(path)?;
    let published = unsafe {
        publish_host_file(
            path.as_ptr(), path.len() - 1, temporary.as_ptr(), temporary.len() - 1,
            bytes.as_ptr(), bytes.len(),
        )
    };
    let result = published.written.into_result().and_then(|()| {
        if published.replacement == 0 { Ok(()) }
        else { Err(io::Error::from_raw_os_error(published.replacement as i32)) }
    });
    result.map_err(|error| {
        if published.cleanup == 0 { error }
        else {
            let cleanup = io::Error::from_raw_os_error(published.cleanup as i32);
            io::Error::other(format!("{error}；删除未完成产物：{cleanup}"))
        }
    })
}
