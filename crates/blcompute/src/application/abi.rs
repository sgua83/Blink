//! 原生模块共用的实际 Frame C ABI；资源初始化数据仍由编译产物生产。
use super::Frame;
#[cfg(windows)]
mod native {
    use super::*;
    use crate::Value;
    use std::ffi::c_void;

    // 表示只由本运行库解释；跨模块交接不透明指针，不交接Rust容器。
    struct NativeError { message: String }
    fn finish<T>(result: crate::Result<T>, output: impl FnOnce(T)) -> *mut c_void {
        match result {
            Ok(value) => { output(value); std::ptr::null_mut() }
            Err(error) => Box::into_raw(Box::new(NativeError { message: error.to_string() })).cast(),
        }
    }
    /// # Safety
    /// error是本运行库交回且仍由调用者唯一持有的非空错误；bytes可写一个usize。
    /// 返回文本只借用到该错误销毁；不会借用Frame。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_error_message(error: *const c_void, bytes: *mut usize) -> *const u8 {
        let error = unsafe { &*error.cast::<NativeError>() };
        unsafe { bytes.write(error.message.len()) };
        error.message.as_ptr()
    }
    /// # Safety
    /// error是本运行库交回且仍由调用者唯一持有的非空错误；结束所有文本借用后恰好消费一次。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_error_destroy(error: *mut c_void) {
        drop(unsafe { Box::from_raw(error.cast::<NativeError>()) });
    }
    /// # Safety
    /// primary和secondary是本运行库交回的两个独立、唯一拥有且无活动文本借用的错误。
    /// 同次消费二者并交回一个拥有错误；原primary分配继续承载全部实际错误文本。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_error_join(primary:*mut c_void,secondary:*mut c_void)->*mut c_void {
        let other=unsafe { Box::from_raw(secondary.cast::<NativeError>()) };
        let error=unsafe { &mut *primary.cast::<NativeError>() };
        error.message.push(char::from(10));
        error.message.push_str(&other.message);
        primary
    }
    /// # Safety
    /// frame属于本运行库，ty是已绑定模块的实际类型索引；调用遵守独占所有权。
    /// output可写一个u32，仅成功时初始化。返回空指针表示成功，非空错误交由调用者消费。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_allocate(
        output: *mut u32, frame: *mut Frame, ty: u32, count: u32,
    ) -> *mut c_void {
        let result = unsafe { &mut *frame }.allocate(ty, count);
        finish(result, |value| unsafe { output.write(value) })
    }
    /// # Safety
    /// frame／handle是唯一存储所有者；fill提供该Ty的有效普通值分量，
    /// 原地址与借用消费者已经结束，调用期间填充值存储保持有效。
    /// output只在成功时初始化；非空返回错误由调用者消费。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_resize(
        output: *mut u32, frame: *mut Frame, handle: u32, count: u32, fill: *const u32,
    ) -> *mut c_void {
        let result = unsafe { (&mut *frame).resize(handle, count, fill) };
        finish(result, |value| unsafe { output.write(value) })
    }
    /// # Safety
    /// frame／program来自本次已绑定资源映像；data和values按该程序接口交付有效的
    /// 句柄和C布局标量数组。空数组同样具有有效对齐地址，借用保持到同步真实完成。
    /// 返回空指针表示成功；非空错误由调用者消费。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_run(
        frame: *mut Frame, program: u32,
        data: *const blanguage::resource::DatasetBinding, values: *const Value,
    ) -> *mut c_void {
        let plan = unsafe { &*frame }.definitions.programs[program as usize].layout();
        let data = unsafe { std::slice::from_raw_parts(data, plan.buffers().len()) };
        let count = plan.uniforms().iter().map(|value| value.ty.components.len()).sum();
        let values = unsafe { std::slice::from_raw_parts(values, count) };
        let result = unsafe { &mut *frame }.run(program, data, values);
        finish(result, |()| {})
    }
    /// # Safety
    /// frame是本ABI创建且仍由调用者独占持有的实际Frame。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_destroy(frame: *mut Frame) {
        drop(unsafe { Box::from_raw(frame) });
    }
    /// # Safety
    /// frame／handle来自当前仍持有该存储的Frame，释放遵守最后消费者。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_release(frame: *mut Frame, handle: u32) {
        unsafe { &mut *frame }.release(handle);
    }
    /// # Safety
    /// frame／handle仍有效且调用遵守独占所有权；原操作按实际驻留状态交回结果。
    /// 返回空指针表示成功；非空错误由调用者消费。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_upload(frame: *mut Frame, handle: u32) -> *mut c_void {
        let result = unsafe { &mut *frame }.upload(handle);
        finish(result, |()| {})
    }
    /// # Safety
    /// frame／handle仍有效且调用遵守独占所有权；原操作按实际驻留状态交回结果。
    /// 返回空指针表示成功；非空错误由调用者消费。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_download(frame: *mut Frame, handle: u32) -> *mut c_void {
        let result = unsafe { &mut *frame }.download(handle);
        finish(result, |()| {})
    }
    /// # Safety
    /// frame／handle是存活的原资源，调用者保证真实目标位置、只读／可写权限、
    /// 写入范围及最后消费者前的存活；直接交回原存储指针，不创建错误结果。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_address(
        frame: *mut Frame, handle: u32,
        offset: u64, bytes: u32, stride: u64, count: u32,
    ) -> *mut u32 {
        unsafe { (&mut *frame).address(handle, offset, bytes, stride, count) }
    }
    /// 借用原存储地址；不授予CPU访问权或延长存储寿命。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_storage_address(frame: *mut Frame, handle: u32) -> u64 {
        unsafe { (&mut *frame).storage_address(handle) }
    }
}
