//! 原生模块共用的实际 Frame C ABI；资源初始化数据仍由编译产物生产。
use super::Frame;
#[cfg(windows)]
mod native {
    use super::*;
    use crate::Value;
    use std::ffi::c_void;
    type ErrorReceiver = unsafe extern "C" fn(*mut c_void, *const u8, usize);
    unsafe fn finish<T>(result: crate::Result<T>, on_error: ErrorReceiver, context: *mut c_void) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                let message = error.to_string();
                unsafe { on_error(context, message.as_ptr(), message.len()) };
                None
            }
        }
    }
    /// # Safety
    /// frame 属于本运行库，ty 是已绑定模块的实际类型索引；调用遵守独占所有权。
    /// output 可写一个 u32；接收函数及 context 保持到调用返回，消息只借用到接收函数返回。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_allocate(
        frame: *mut Frame, ty: u32, count: u32, output: *mut u32,
        on_error: ErrorReceiver, context: *mut c_void,
    ) -> u32 {
        let result = unsafe { &mut *frame }.allocate(ty, count);
        match unsafe { finish(result, on_error, context) } {
            Some(value) => { unsafe { output.write(value) }; 1 }
            None => 0,
        }
    }
    /// # Safety
    /// frame／handle 是唯一存储所有者；fill 提供该 Ty 的有效普通值分量，
    /// 原地址与借用消费者已经结束，调用期间填充值存储保持有效。
    /// output 可写一个 u32；接收函数及 context 保持到调用返回，消息只借用到接收函数返回。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_resize(
        frame: *mut Frame, handle: u32, count: u32, fill: *const u32, output: *mut u32,
        on_error: ErrorReceiver, context: *mut c_void,
    ) -> u32 {
        let result = unsafe { (&mut *frame).resize(handle, count, fill) };
        match unsafe { finish(result, on_error, context) } {
            Some(value) => { unsafe { output.write(value) }; 1 }
            None => 0,
        }
    }
    /// # Safety
    /// frame／program 来自本次已绑定资源映像；data和values按该程序接口交付有效的
    /// 句柄和C布局标量数组。空数组同样具有有效对齐地址，借用保持到同步真实完成。
    /// 接收函数及 context 保持到调用返回，消息只借用到接收函数返回。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_run(
        frame: *mut Frame,
        program: u32,
        data: *const blanguage::resource::DatasetBinding,
        values: *const Value,
        on_error: ErrorReceiver,
        context: *mut c_void,
    ) -> u32 {
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
        u32::from(unsafe { finish(result, on_error, context) }.is_some())
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
    /// frame／handle 仍有效且调用遵守独占所有权；原操作按实际驻留状态交回结果。
    /// 接收函数及 context 保持到调用返回，消息只借用到接收函数返回。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_upload(
        frame: *mut Frame, handle: u32, on_error: ErrorReceiver, context: *mut c_void,
    ) -> u32 {
        let result = unsafe { &mut *frame }.upload(handle);
        u32::from(unsafe { finish(result, on_error, context) }.is_some())
    }
    /// # Safety
    /// frame／handle 仍有效且调用遵守独占所有权；原操作按实际驻留状态交回结果。
    /// 接收函数及 context 保持到调用返回，消息只借用到接收函数返回。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_download(
        frame: *mut Frame, handle: u32, on_error: ErrorReceiver, context: *mut c_void,
    ) -> u32 {
        let result = unsafe { &mut *frame }.download(handle);
        u32::from(unsafe { finish(result, on_error, context) }.is_some())
    }
    /// # Safety
    /// 对应存储、访问、范围与指针期限由真实调用者遵守。output 可写一个原生指针；
    /// 接收函数及 context 保持到调用返回，消息只借用到接收函数返回。
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn blink_address(
        frame: *mut Frame, handle: u32, offset: u64, bytes: u32, output: *mut *mut u32,
        on_error: ErrorReceiver, context: *mut c_void,
    ) -> u32 {
        let result = unsafe { (&mut *frame).address(handle, offset, bytes) };
        match unsafe { finish(result, on_error, context) } {
            Some(value) => { unsafe { output.write(value) }; 1 }
            None => 0,
        }
    }
}
