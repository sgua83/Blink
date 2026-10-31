//! 原生模块共用的实际 Frame C ABI；资源初始化数据仍由编译产物生产。
use super::Frame;
#[cfg(windows)]
mod native {
    use super::*;
    use crate::Value;
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
    /// frame 属于本运行库，ty 是已绑定模块的实际类型索引；调用遵守独占所有权。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_allocate(frame: *mut Frame, ty: u32, count: u32) -> u32 {
        let result = unsafe { &mut *frame }.allocate(ty, count);
        unsafe { finish(frame, result) }
    }
    /// # Safety
    /// frame／handle 是唯一存储所有者；fill 提供该 Ty 的有效普通值分量，
    /// 原地址与借用消费者已经结束，调用期间填充值存储保持有效。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_resize(frame: *mut Frame, handle: u32, count: u32, fill: *const u32) -> u32 {
        let result = unsafe { (&mut *frame).resize(handle, count, fill) };
        unsafe { finish(frame, result) }
    }
    /// # Safety
    /// frame／program 来自本次已绑定资源映像；data和values按该程序接口交付有效的
    /// 句柄和C布局标量数组。空数组同样具有有效对齐地址，借用保持到同步真实完成。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_run(
        frame: *mut Frame,
        program: u32,
        data: *const blanguage::resource::DatasetBinding,
        values: *const Value,
    ) {
        let plan = unsafe { &*frame }.programs[program as usize]
            .as_ref()
            .unwrap()
            .layout();
        let data = unsafe { std::slice::from_raw_parts(data, plan.buffers().len()) };
        let count = plan
            .uniforms()
            .iter()
            .map(|value| value.ty.components.len())
            .sum();
        let values = unsafe { std::slice::from_raw_parts(values, count) };
        let result = unsafe { &mut *frame }.run(program, data, values);
        unsafe { finish(frame, result) }
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
    pub unsafe extern "C" fn blink_address(frame: *mut Frame, handle: u32, offset: u64, bytes: u32) -> *mut u32 {
        let result = unsafe { (&mut *frame).address(handle, offset, bytes) };
        unsafe { finish(frame, result) }
    }
}
