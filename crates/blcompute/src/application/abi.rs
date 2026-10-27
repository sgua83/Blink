//! 原生模块共用的实际 Frame C ABI；资源初始化数据仍由编译产物生产。
use super::{Frame, ProgramImage};
pub struct ModuleResources {
    pub types: Vec<blanguage::ir::Ty>,
    pub programs: Vec<ProgramImage>,
}
#[cfg(windows)]
mod native {
    use super::*;
    fn fail(error: Box<dyn std::error::Error + Send + Sync>) -> ! {
        eprintln!("{error}");
        std::process::exit(1)
    }
    unsafe fn finish<T>(frame: *mut Frame, result: crate::Result<T>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => {
                drop(unsafe { Box::from_raw(frame) });
                fail(error)
            }
        }
    }
    /// # Safety
    /// frame 是本 ABI 创建且仍由调用者独占持有的实际 Frame。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_destroy(frame: *mut Frame) {
        drop(unsafe { Box::from_raw(frame) });
    }
    /// # Safety
    /// frame／handle 来自当前仍持有该存储的 Frame，释放遵守最后消费者。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_release(frame: *mut Frame, handle: u32) {
        unsafe { &mut *frame }.release(handle);
    }
    /// # Safety
    /// frame／handle 仍有效且实际资源可以按声明上传。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_upload(frame: *mut Frame, handle: u32) {
        let result = unsafe { &mut *frame }.upload(handle);
        unsafe { finish(frame, result) }
    }
    /// # Safety
    /// frame／handle 仍有效且实际资源可以按声明下载。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_download(frame: *mut Frame, handle: u32) {
        let result = unsafe { &mut *frame }.download(handle);
        unsafe { finish(frame, result) }
    }
    /// # Safety
    /// 对应存储、访问、范围与指针期限由真实调用者遵守。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_address(frame: *mut Frame, handle: u32, write: u32) -> *mut u32 {
        let result = unsafe { (&mut *frame).address(handle, write != 0) };
        unsafe { finish(frame, result) }
    }
}
