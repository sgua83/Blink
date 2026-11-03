//! CPU代码映像由编译器与执行器共用；x64发射只属于compiler角色。
mod image;
pub use image::*;
#[cfg(feature="compiler")]
mod codegen;
#[cfg(feature="compiler")]
pub use codegen::*;
#[cfg(feature="compiler")]
pub(crate) use codegen::{emit_application,emit_function,unwind_info,program};
