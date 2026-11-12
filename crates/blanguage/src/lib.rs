//! BLanguage .h/.b 前端、类型和访问检查、并行 IR 与原生代码生成。
#[cfg(feature = "compiler")]
pub mod application;
pub mod ast;
#[cfg(feature = "compiler")]
mod syntax;
#[cfg(feature = "compiler")]
pub mod build;
#[cfg(feature = "compiler")]
pub mod compiler;
#[cfg(feature = "compiler")]
pub use compiler::{Compiler, Processor};
#[cfg(feature = "compiler")]
mod check;
#[cfg(feature = "compiler")]
pub mod dll;
#[cfg(feature = "compiler")]
mod execution;
#[cfg(feature = "compiler")]
mod function_types;
#[cfg(feature = "compiler")]
mod generics;
pub mod ir;
mod json;
#[cfg(feature = "compiler")]
mod lexer;
#[cfg(feature = "compiler")]
mod names;
#[cfg(feature = "compiler")]
pub mod linker;
#[cfg(feature = "compiler")]
pub mod module;
pub mod native;
#[cfg(feature = "compiler")]
pub mod object;
#[cfg(feature = "compiler")]
mod optimize;
#[cfg(feature = "compiler")]
mod parser;
#[cfg(feature = "compiler")]
mod parallel;
pub mod resource;
pub mod spirv;
#[cfg(feature = "compiler")]
mod builtin_types;
#[cfg(feature = "compiler")]
mod type_sets;
#[cfg(feature = "compiler")]
mod value_parameters;

#[cfg(feature = "compiler")]
use ast::*;

#[derive(Clone, Debug)]
pub struct ExecutableProgram(ir::Program);
impl std::ops::Deref for ExecutableProgram {
    type Target = ir::Program;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
/// 接口条件已检查的程序；只能以执行结果持有的域关系绑定，不能作为裸数据入口。
#[derive(Clone, Debug)]
pub struct TemplateProgram(ir::Program);
impl std::ops::Deref for TemplateProgram {
    type Target = ir::Program;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
#[cfg(feature = "compiler")]
mod file_bytes;
#[cfg(feature = "compiler")]
mod frontend;
#[cfg(feature = "compiler")]
pub use frontend::{Compilation, compile, compile_with_modules, compile_entry, compile_entry_with_modules,
    compile_application, compile_application_with_modules, check};
#[cfg(feature = "compiler")]
pub(crate) use frontend::*;
mod codec;
