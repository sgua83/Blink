//! Selects original compilation consumers, then prints their actual IR values.
use super::{Argument, CompileError, EntryMode, OperandSink, Operands, parsed_text};
use crate::json::string;
use std::{ffi::OsString, fmt::Write, path::Path};

#[repr(C)]
struct IrCommandLine {
    mode: EntryMode,
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
    fn parse_ir_arguments(arguments: *const Argument, count: usize, operands: OperandSink) -> IrCommandLine;
}

pub(super) fn print(
    compiler: &blanguage::Compiler, args: &[OsString], arguments: &[Argument],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = blanguage::canonicalize_source_path(Path::new(args.get(1).ok_or_else(|| CompileError::IR_USAGE.message(args, 0))?))?;
    let mut operands = Operands::default();
    let line = unsafe { parse_ir_arguments(arguments.as_ptr(), arguments.len(), operands.sink()) };
    // Preserve path errors from imports encountered before the reported syntax error.
    let import_paths = operands.imports.iter()
        .map(|&index| blanguage::canonicalize_source_path(Path::new(&args[index])))
        .collect::<std::io::Result<Vec<_>>>()?;
    line.error.result(args, line.error_index)?;
    let instances = operands.instances(args);
    let text = |index: usize| unsafe { parsed_text(&args[index]) };
    let explicit_root;
    let root: &Path = match args.get(line.root_index) {
        Some(root) => {
            explicit_root = blanguage::canonicalize_source_path(Path::new(root))?;
            &explicit_root
        }
        None => path.parent().ok_or_else(|| CompileError::IR_USAGE.message(args, 0))?,
    };
    let entry = if line.entry_index == args.len() { "" } else { text(line.entry_index) };
    let function = (line.function_index != args.len()).then(|| text(line.function_index));
    let stage = (line.has_stage != 0).then_some(line.stage);
    let mut imports = Vec::new();
    for path in &import_paths { imports.extend(blanguage::module::CompiledInterface::read(path)?); }
    let (mode, phase, value) = match line.mode {
        EntryMode::Application => {
            let application = compiler.compile_application_with_interfaces(&path, root, entry, imports)?;
            let value = if let Some(function) = function {
                application.function_ir_json(function).ok_or_else(|| format!("本次应用没有保留普通函数 {function}"))?
            } else { application.ir_json() };
            ("application", "finalized_application_ir", value)
        }
        EntryMode::Program => {
            let compilation = compiler.compile_entry_with_interfaces(&path, root, entry, imports)?;
            let program = compilation.programs.iter().map(|p| &**p)
                .chain(compilation.templates.iter().map(|p| &**p)).next().ok_or("本次入口没有并行IR")?;
            let value = if let Some(stage) = stage {
                program.stages().get(stage).ok_or_else(|| format!("本次入口只有{}个优化后阶段，没有阶段{stage}", program.stages().len()))?.ir_json()
            } else { program.ir_json() };
            ("program", "optimized_program_ir", value)
        }
        EntryMode::Module => {
            let module = compiler.compile_native_module_with_interfaces(&path, root, imports, &instances)?;
            let value = if let Some(function) = function {
                module.function_ir_json(function).ok_or_else(|| format!("本次模块没有保留普通函数 {function}"))?
            } else { module.ir_json() };
            ("module", "finalized_module_ir", value)
        }
    };
    let mut out = String::from("{\"kind\":\"ir_query\",\"phase\":"); string(&mut out, phase);
    out.push(','); crate::json::path(&mut out, "source_path", &path);
    out.push(','); crate::json::path(&mut out, "module_root", root);
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
