//! SPIR-V编译入口与已编译GPU映像。
mod image;
pub use image::*;
#[cfg(feature = "compiler")]
mod codegen;
#[cfg(feature = "compiler")]
pub use codegen::*;
#[cfg(feature = "compiler")]
pub(crate) use codegen::program;
