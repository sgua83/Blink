mod process_arguments;
mod export;
mod input_facts;
mod image_export;
mod code_facts;
mod ir_facts;
mod json;
mod layout_facts;
mod x64_facts;
mod spirv_facts;

use std::{ffi::OsString, num::NonZeroUsize, path::{Path, PathBuf}, process::ExitCode};

#[repr(C)]
struct Argument {
    bytes: *const u8,
    length: u64,
    text: u64,
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum CompileCommand {
    Unknown, Version, Help, Inputs, X64, Image, Code, Spirv, Layout, Ir, Object, Check, Build,
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum CompileError {
    None, MissingResourceOperand, ResourceText, DuplicateJobs, DuplicateCpus,
    DuplicateParameterLimit, EmptyNumber, InvalidNumber, Overflow, ZeroJobs,
    CpuSyntax, ResourceConflict, InputsUsage, CommandText, MissingSource,
    UnknownCommand, MissingOptionOperand, OptionText, UnknownOption, BudgetText,
    EntryText, InstanceText, BudgetWithoutReport, MissingOutput, CodeUsage, ObjectCodeUsage,
    IrUsage, IrModeRepeated, IrFunctionRepeated, IrStageRepeated, IrOption, IrFunctionMode,
    IrStageMode, IrInstanceMode, IrImportMode,
    LayoutUsage, LayoutOption, X64Usage, X64Option, X64Repeated, ImageUsage, SpirvUsage,
}

#[repr(C)]
struct CompileCommandLine {
    command: CompileCommand,
    prefix_error: CompileError,
    error: CompileError,
    max_parameter_bytes: u32,
    command_index: usize,
    error_index: usize,
    source_index: usize,
    output_index: usize,
    root_index: usize,
    entry_index: usize,
    program_index: usize,
    report_index: usize,
    budget: u64,
    has_budget: u64,
    as_module: u64,
    jobs: usize,
    cpu_count: usize,
    has_parameter_limit: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::parse_compile_arguments"]
    fn parse_compile_arguments(
        arguments: *const Argument,
        count: usize,
        roles: *mut u8,
        cpus: *mut u32,
    ) -> CompileCommandLine;
}

impl CompileError {
    fn result(self, args: &[OsString], index: usize) -> Result<(), Box<dyn std::error::Error>> {
        let message = match self {
            Self::None => return Ok(()),
            Self::MissingResourceOperand => return Err(format!("{}缺少参数", args[index].to_string_lossy()).into()),
            Self::ResourceText => "编译资源参数不是UTF-8文本",
            Self::DuplicateJobs => "--jobs不能重复指定",
            Self::DuplicateCpus => "--cpus不能重复指定",
            Self::DuplicateParameterLimit => "--max-parameter-bytes不能重复指定",
            Self::EmptyNumber => "cannot parse integer from empty string",
            Self::InvalidNumber => "invalid digit found in string",
            Self::Overflow => "number too large to fit in target type",
            Self::ZeroJobs => "number would be zero for non-zero type",
            Self::CpuSyntax => "CPU写为组:编号",
            Self::ResourceConflict => "--jobs与--cpus选择一种编译资源声明",
            Self::InputsUsage => "用法：bc inputs <compiled.obj|entry.blbi>",
            Self::CommandText => "命令不是UTF-8文本",
            Self::MissingSource => "缺少 .b 文件",
            Self::UnknownCommand => return Err(format!(
                "未知命令 {}；编译使用 bc -c，链接使用 blink", args[index].to_string_lossy()
            ).into()),
            Self::MissingOptionOperand => "编译选项缺少参数",
            Self::OptionText => "编译选项不是UTF-8文本",
            Self::UnknownOption => return Err(format!("未知编译选项 {}", args[index].to_string_lossy()).into()),
            Self::BudgetText => "栈预算不是UTF-8文本",
            Self::EntryText => "入口不是UTF-8文本",
            Self::InstanceText => "实例不是UTF-8文本",
            Self::BudgetWithoutReport => "--stack-budget需要--stack-report输出",
            Self::MissingOutput => "缺少 -o 目标文件路径",
            Self::CodeUsage => "usage: bc code <entry.blbi> [--observations <ranges.blcr>] [--source-range start:end] | bc code --image <deployment.blri> [--observations <ranges.blcr>] [--source-range start:end]",
            Self::ObjectCodeUsage => "usage: bc code --object <compiled.obj> [--observations <ranges.blcf>] [--section number | --function exact-symbol] [--source-range start:end]",
            Self::IrUsage => ir_facts::USAGE,
            Self::IrModeRepeated => "IR编译模式只能指定一次",
            Self::IrFunctionRepeated => "--function不能重复指定",
            Self::IrStageRepeated => "--stage不能重复指定",
            Self::IrOption => return Err(format!("未知IR选项 {}；{}", args[index].to_string_lossy(), ir_facts::USAGE).into()),
            Self::IrFunctionMode => "--function用于普通应用／模块，--stage用于--program",
            Self::IrStageMode => "--stage需要--program",
            Self::IrInstanceMode => "--instance用于模块；入口实例直接写在--entry／--program值中",
            Self::IrImportMode => "此并行入口编译API不接受已编译导入；可用--module查询模块的函数和资源程序",
            Self::LayoutUsage => layout_facts::USAGE,
            Self::LayoutOption => return Err(format!("未知布局选项 {}；{}", args[index].to_string_lossy(), layout_facts::USAGE).into()),
            Self::X64Usage => x64_facts::USAGE,
            Self::X64Option => return Err(format!("未知x64查询选项 {}；{}", args[index].to_string_lossy(), x64_facts::USAGE).into()),
            Self::X64Repeated => return Err(format!("{}不能重复指定", args[index].to_string_lossy()).into()),
            Self::ImageUsage => "用法：bc image <entry.blbi> -o <deployment.blri>",
            Self::SpirvUsage => "usage: bc spirv <module.spv>",
        };
        Err(message.into())
    }
}

impl CompileCommandLine {
    fn compiler(&self, cpus: &[u32]) -> std::io::Result<blanguage::Compiler> {
        let compiler = if self.cpu_count != 0 {
            let processors = cpus[..self.cpu_count].iter().map(|&cpu| blanguage::Processor {
                group: (cpu >> 8) as u16,
                number: cpu as u8,
            }).collect();
            blanguage::Compiler::on_processors(processors)?
        } else if let Some(jobs) = NonZeroUsize::new(self.jobs) {
            blanguage::Compiler::with_workers(jobs)
        } else {
            blanguage::Compiler::available()?
        };
        Ok(if self.has_parameter_limit != 0 {
            compiler.with_max_parameter_bytes(self.max_parameter_bytes)
        } else {
            compiler
        })
    }
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = process_arguments::read();
    let arguments: Vec<_> = args.iter().map(|arg| {
        let bytes = arg.as_encoded_bytes();
        Argument { bytes: bytes.as_ptr(), length: bytes.len() as u64, text: u64::from(arg.to_str().is_some()) }
    }).collect();
    let longest = arguments.iter().map(|arg| arg.length as usize).max().unwrap_or(0);
    // The smallest processor spelling is g:n, with one comma between entries.
    let mut cpus = vec![0; (longest + 1) / 4];
    let mut roles = vec![0; args.len()];
    // B borrows the original argument bytes and writes only within the two buffers.
    let line = unsafe {
        parse_compile_arguments(arguments.as_ptr(), args.len(), roles.as_mut_ptr(), cpus.as_mut_ptr())
    };
    line.prefix_error.result(&args, line.error_index)?;
    match line.command {
        CompileCommand::Version => {
            println!("Blink compiler {}", include_str!("../../../VERSION").trim());
            return Ok(());
        }
        CompileCommand::Help => {
            println!(
            "Blink compiler\nbc --version\nbc inputs <compiled.obj|entry.blbi>\nbc image <entry.blbi> -o <deployment.blri>\nbc code <entry.blbi> [--observations <ranges.blcr>]\nbc code --image <deployment.blri> [--observations <ranges.blcr>]\nbc code --object <compiled.obj> [--observations <ranges.blcf>] [--section number | --function exact-symbol]\nbc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]\nbc spirv <module.spv>\nbc ir <module.b> [-I root] [--module | --program name | --entry name] [--function qualified-name | --stage index] [--import object|archive] [--instance name]\nbc layout <module.h|module.b> <type> [-I module-root] [--field path]\nGlobal compile options before command: [--jobs N | --cpus group:number,...] [--max-parameter-bytes N]\nbc -c <module.b> -o <output.obj> [-I module-root] [--entry main | --program name | --module] [--import module.obj|library.lib] [--instance function<args>] [--stack-report report.txt [--stack-budget bytes]]\nbc check <module.b> [module-root] [program<args>]\nbc build <module.b> <output-directory> [module-root] [program<args>]\nLink separately: blink <input.obj> -o <output.exe|output.dll>"
        );
            return Ok(());
        }
        CompileCommand::Inputs => {
            line.error.result(&args, line.error_index)?;
            return input_facts::print(Path::new(&args[line.source_index]));
        }
        CompileCommand::X64 => return x64_facts::print(&args, &arguments),
        CompileCommand::Image => {
            line.error.result(&args, line.error_index)?;
            return image_export::export(Path::new(&args[line.source_index]), Path::new(&args[line.output_index]));
        }
        CompileCommand::Code => return code_facts::print(&args, &arguments),
        CompileCommand::Spirv => {
            line.error.result(&args, line.error_index)?;
            return spirv_facts::print(Path::new(&args[line.source_index]));
        }
        _ => {}
    }

    let compiler = line.compiler(&cpus)?;
    match line.command {
        CompileCommand::Layout => return layout_facts::print(
            &compiler, &args[line.command_index..], &arguments[line.command_index..],
        ),
        CompileCommand::Ir => return ir_facts::print(
            &compiler, &args[line.command_index..], &arguments[line.command_index..], &mut roles[line.command_index..],
        ),
        CompileCommand::Object => return build_object(&compiler, &args, &roles, &line),
        _ => line.error.result(&args, line.error_index)?,
    }
    let path = PathBuf::from(&args[line.source_index]);
    let root = args.get(line.root_index).map(PathBuf::from).unwrap_or_else(|| path.parent().unwrap().into());
    let has_output = matches!(line.command, CompileCommand::Build);
    let compilation = if let Some(entry) = args.get(line.entry_index) {
        compiler.compile_entry(&path, &root, entry.to_str().ok_or("入口不是UTF-8文本")?)?
    } else if has_output {
        compiler.compile(&path, &root)?
    } else {
        compiler.check(&path, &root)?
    };
    if !compilation.open_generics.is_empty() {
        println!(
            "open generic declarations (body checked on instantiation): {}",
            compilation.open_generics.join(", ")
        );
    }
    let programs: Vec<&blanguage::ir::Program> = compilation
        .programs
        .iter()
        .map(|p| &**p)
        .chain(compilation.templates.iter().map(|p| &**p))
        .collect();
    for p in &programs {
        print!("{}", p.describe());
    }
    if has_output {
        let out = PathBuf::from(args.get(line.output_index).ok_or("缺少输出目录")?);
        std::fs::create_dir_all(&out)?;
        for (index, p) in programs.iter().enumerate() {
            // 类型的限定名保留在 layout；导出文件不使用 Windows 禁止的尖括号。
            let stem = if let Some((base, _)) = p.name().split_once('<') {
                format!("{}.instance-{index}", base.replace("::", "_").replace('.', "_"))
            } else {
                p.name().replace("::", "_").replace('.', "_")
            };
            export::program(&compiler, p, &out, &stem)?;
        }
    } else if programs.is_empty() {
        println!("library {} checked", path.display());
    }
    Ok(())
}

fn build_object(
    compiler: &blanguage::Compiler,
    args: &[OsString],
    roles: &[u8],
    line: &CompileCommandLine,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(&args[line.source_index]);
    let parent = path.parent().unwrap();
    let root = args.get(line.root_index).map(PathBuf::from).unwrap_or_else(|| {
        if parent.as_os_str().is_empty() { PathBuf::from(".") } else { parent.to_path_buf() }
    });
    let mut imports = Vec::new();
    let mut instances = Vec::new();
    // Imports encountered before a syntax error still perform their original I/O first.
    for (arg, role) in args.iter().zip(roles) {
        match role {
            1 => imports.extend(blanguage::object::Input::read(Path::new(arg))?.objects()?),
            2 => instances.push(arg.to_str().ok_or("实例不是UTF-8文本")?.to_owned()),
            _ => {}
        }
    }
    line.error.result(args, line.error_index)?;
    let output = PathBuf::from(&args[line.output_index]);
    let kind = if line.as_module != 0 {
        blanguage::build::ObjectKind::Module(&instances)
    } else if let Some(entry) = args.get(line.program_index) {
        let entry = entry.to_str().ok_or("入口不是UTF-8文本")?;
        let prefix = output.file_stem().and_then(|value| value.to_str()).ok_or("目标文件名不是UTF-8")?;
        blanguage::build::ObjectKind::Library { entry, prefix }
    } else {
        let entry = args.get(line.entry_index).and_then(|entry| entry.to_str()).unwrap_or("main");
        blanguage::build::ObjectKind::Application(entry)
    };
    let compiled = compiler.build_object(&path, &root, &imports, kind, &output)
        .map_err(|error| -> Box<dyn std::error::Error> { error })?;
    if let Some(path) = args.get(line.report_index).map(PathBuf::from) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        #[cfg(windows)]
        let _object_input = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new().read(true).share_mode(1).open(&compiled.artifact)?
        };
        let budget = (line.has_budget != 0).then_some(line.budget);
        std::fs::write(path, compiled.object.stack_report(budget))?;
    }
    println!("object {} -> {}{}", compiled.object.name(), output.display(),
        if compiled.reused { " (reused)" } else { "" });
    Ok(())
}
