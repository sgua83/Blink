/// One borrowed dataset argument in the algorithm DLL C ABI.
#[repr(C)]
pub struct LibraryBuffer {
    pub data: *mut core::ffi::c_void,
    pub count: u32,
}
