//! Selects original compilation consumers, then prints their actual IR values.
use super::{Argument, CompileError};
use crate::json::string;
use std::{ffi::OsString, fmt::Write, path::PathBuf};

pub(super) const USAGE: &str = "用法：bc ir <module.b> [-I root] [--module | --program name | --entry name] [--function qualified-name | --stage index] [--import object|archive] [--instance name]";

#[repr(u32)]
enum IrMode {
    Module,
    Application,
    Program,
}

#[repr(C)]
struct IrCommandLine {
    mode: IrMode,
    error: CompileError,
    error_index: usize,
    root_index: usize,
    entry_index: usize,
    function_index: usize,
    stage: usize,
    has_stage: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::parse_ir_arguments"]
    fn parse_ir_arguments(arguments: *const Argument, count: usize, roles: *mut u8) -> IrCommandLine;
}

pub(super) fn print(
    compiler: &blanguage::Compiler, args: &[OsString], arguments: &[Argument], roles: &mut [u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(args.get(1).ok_or(USAGE)?).canonicalize()?;
    let line = unsafe { parse_ir_arguments(arguments.as_ptr(), arguments.len(), roles.as_mut_ptr()) };
    // Every returned text index has been checked by the parser against the original argument.
    let text = |index: usize| unsafe { std::str::from_utf8_unchecked(args[index].as_encoded_bytes()) };
    let mut import_paths = Vec::new();
    let mut instances = Vec::new();
    // Preserve path errors from imports encountered before the reported syntax error.
    for (index, (&role, arg)) in roles.iter().zip(args).enumerate() {
        match role {
            1 => import_paths.push(PathBuf::from(arg).canonicalize()?),
            2 => instances.push(text(index).to_owned()),
            _ => {}
        }
    }
    line.error.result(args, line.error_index)?;
    let root = match args.get(line.root_index) {
        Some(root) => PathBuf::from(root).canonicalize()?,
        None => path.parent().unwrap().canonicalize()?,
    };
    let entry = if line.entry_index == args.len() { "" } else { text(line.entry_index) };
    let function = (line.function_index != args.len()).then(|| text(line.function_index));
    let stage = (line.has_stage != 0).then_some(line.stage);
    let mut imports = Vec::new();
    for path in &import_paths { imports.extend(blanguage::object::Input::read(path)?.objects()?); }
    let (mode, phase, value) = match line.mode {
        IrMode::Application => {
            let application = compiler.compile_application_with_modules(&path, &root, entry, &imports)?;
            let value = if let Some(function) = function {
                application.function_ir_json(function).ok_or_else(|| format!("本次应用没有保留普通函数 {function}"))?
            } else { application.ir_json() };
            ("application", "finalized_application_ir", value)
        }
        IrMode::Program => {
            let compilation = compiler.compile_entry(&path, &root, entry)?;
            let program = compilation.programs.iter().map(|p| &**p)
                .chain(compilation.templates.iter().map(|p| &**p)).next().ok_or("本次入口没有并行IR")?;
            let value = if let Some(stage) = stage {
                program.stages().get(stage).ok_or_else(|| format!("本次入口只有{}个优化后阶段，没有阶段{stage}", program.stages().len()))?.ir_json()
            } else { program.ir_json() };
            ("program", "optimized_program_ir", value)
        }
        IrMode::Module => {
            let module = compiler.compile_native_module(&path, &root, &imports, &instances)?;
            let value = if let Some(function) = function {
                module.function_ir_json(function).ok_or_else(|| format!("本次模块没有保留普通函数 {function}"))?
            } else { module.ir_json() };
            ("module", "finalized_module_ir", value)
        }
    };
    let mut out = String::from("{\"kind\":\"ir_query\",\"phase\":"); string(&mut out, phase);
    out.push(','); crate::json::path(&mut out, "source_path", &path);
    out.push(','); crate::json::path(&mut out, "module_root", &root);
    out.push_str(",\"mode\":"); string(&mut out, mode);
    out.push_str(",\"entry\":"); if entry.is_empty() { out.push_str("null"); } else { string(&mut out, entry); }
    out.push_str(",\"function\":"); if let Some(function) = function { string(&mut out, function); } else { out.push_str("null"); }
    out.push_str(",\"stage\":"); if let Some(stage) = stage { write!(&mut out, "{stage}").unwrap(); } else { out.push_str("null"); }
    out.push_str(",\"imports\":[");
    for (i, path) in import_paths.iter().enumerate() {
        if i != 0 { out.push(','); }
        if let Some(path) = path.to_str() { string(&mut out, path); } else { out.push_str("null"); }
    }
    out.push_str("],\"imports_encoded_bytes\":[");
    for (i, path) in import_paths.iter().enumerate() {
        if i != 0 { out.push(','); }
        crate::json::bytes(&mut out, path.as_os_str().as_encoded_bytes());
    }
    out.push_str("],\"instances\":[");
    for (i, value) in instances.iter().enumerate() { if i != 0 { out.push(','); } string(&mut out, value); }
    write!(&mut out, "],\"ir\":{value}}}").unwrap();
    println!("{out}");
    Ok(())
}
