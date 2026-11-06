/// 同一静态模块工厂的C布局；声明也直接用于无编译器依赖的生成单元。
#[repr(C)]
pub struct ModuleImage {
    pub data: *const u8,
    pub bytes: usize,
    pub code: *const *const u8,
    pub codes: usize,
}
pub type ModuleFactory = unsafe extern "C" fn() -> ModuleImage;
