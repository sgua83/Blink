/// 原生静态模块工厂与运行库共用的 C 布局。
#[repr(C)]
pub struct ModuleImage {
    pub data: *const u8,
    pub bytes: usize,
    pub code: *const *const u8,
    pub codes: usize,
}
pub type ModuleFactory = unsafe extern "C" fn() -> ModuleImage;
