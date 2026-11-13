//! 本次OS动态库加载引用；符号及代码消费者共同借用其实际寿命。
use crate::Result;
use std::ffi::{CStr, c_char, c_void};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *const c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

pub(crate) struct Library(*mut c_void);
impl Library {
    pub(crate) fn load(name: &str) -> Result<Self> {
        let name: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
        let handle = unsafe { LoadLibraryW(name.as_ptr()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(handle))
    }
    pub(crate) fn symbol(&self, name: &CStr) -> Result<*const ()> {
        let address = unsafe { GetProcAddress(self.0, name.as_ptr()) };
        if address.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(address.cast())
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.0); }
    }
}
