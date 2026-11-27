//! 本次OS动态库加载引用；符号及代码消费者共同借用其实际寿命。
use crate::Result;
use std::ffi::{CStr, c_char, c_void};

#[repr(C)]
struct NativeLibraryOpen {
    value: *mut c_void,
    os_code: u32,
    allocation_failed: u32,
}

#[repr(C)]
struct NativeLibrarySymbol {
    value: *const (),
    os_code: u32,
}

unsafe extern "C" {
    #[link_name = "blink.windows.cpu_image::open_native_library"]
    fn open_native_library(name: *const u8, length: usize) -> NativeLibraryOpen;
    #[link_name = "blink.windows.cpu_image::native_library_symbol"]
    fn native_library_symbol(module: *mut c_void, nul_name: *const c_char) -> NativeLibrarySymbol;
    #[link_name = "blink.windows.cpu_image::release_native_library"]
    fn release_native_library(module: *mut c_void);
}

pub(crate) struct Library(*mut c_void);
impl Library {
    pub(crate) fn load(name: &str) -> Result<Self> {
        let opened = unsafe { open_native_library(name.as_ptr(), name.len()) };
        if opened.value.is_null() {
            return Err(std::io::Error::from_raw_os_error(opened.os_code as i32).into());
        }
        Ok(Self(opened.value))
    }
    pub(crate) fn symbol(&self, name: &CStr) -> Result<*const ()> {
        let symbol = unsafe { native_library_symbol(self.0, name.as_ptr()) };
        if symbol.value.is_null() {
            return Err(std::io::Error::from_raw_os_error(symbol.os_code as i32).into());
        }
        Ok(symbol.value)
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe { release_native_library(self.0); }
    }
}
