mod process_arguments;
mod export;
mod input_facts;
mod image_export;
mod code_facts;
mod ir_facts;
mod json;
mod text;
mod layout_facts;
mod x64_facts;
mod spirv_facts;

use std::{ffi::{OsStr, OsString, c_void}, num::NonZeroUsize, path::{Path, PathBuf}, process::ExitCode};

#[repr(C)]
struct Argument {
    bytes: *const u8,
    length: u64,
    text: u64,
}

#[repr(C)]
struct OperandSink {
    context: *mut c_void,
    import_path: unsafe extern "C" fn(*mut c_void, usize),
    instance_name: unsafe extern "C" fn(*mut c_void, usize),
    processor: unsafe extern "C" fn(*mut c_void, u16, u8),
}

#[derive(Default)]
struct Operands {
    imports: Vec<usize>,
    instances: Vec<usize>,
    processors: Vec<blanguage::Processor>,
}

impl Operands {
    fn sink(&mut self) -> OperandSink {
        OperandSink {
            context: std::ptr::from_mut(self).cast(),
            import_path: collect_import,
            instance_name: collect_instance,
            processor: collect_processor,
        }
    }

    fn instances(&self, args: &[OsString]) -> Vec<String> {
        self.instances.iter().map(|&index| {
            // The parser emits instance operands only after checking their text flag.
            unsafe { parsed_text(&args[index]) }.to_owned()
        }).collect()
    }
}

unsafe extern "C" fn collect_import(context: *mut c_void, index: usize) {
    unsafe { &mut *context.cast::<Operands>() }.imports.push(index);
}

unsafe extern "C" fn collect_instance(context: *mut c_void, index: usize) {
    unsafe { &mut *context.cast::<Operands>() }.instances.push(index);
}

unsafe extern "C" fn collect_processor(context: *mut c_void, group: u16, number: u8) {
    unsafe { &mut *context.cast::<Operands>() }.processors.push(blanguage::Processor { group, number });
}

/// The index must name an operand whose original OsString was marked as UTF-8 by B's caller.
unsafe fn parsed_text(arg: &OsStr) -> &str {
    unsafe { std::str::from_utf8_unchecked(arg.as_encoded_bytes()) }
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
    IrStageMode, IrInstanceMode,
    LayoutUsage = 35, LayoutOption, X64Usage, X64Option, X64Repeated, ImageUsage, SpirvUsage,
}

#[derive(Clone, Copy, PartialEq)]
#[repr(u32)]
enum EntryMode {
    Module,
    Application,
    Program,
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
    report_index: usize,
    budget: u64,
    has_budget: u64,
    mode: EntryMode,
    jobs: usize,
    has_parameter_limit: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::parse_compile_arguments"]
    fn parse_compile_arguments(
        arguments: *const Argument,
        count: usize,
        operands: OperandSink,
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
    fn compiler(&self, processors: Vec<blanguage::Processor>) -> std::io::Result<blanguage::Compiler> {
        let compiler = if std::env::var_os(worker_protocol::BOOTSTRAP_JOBSERVER).as_deref() == Some(std::ffi::OsStr::new("1")) {
            blanguage::Compiler::bootstrap_worker()?
        } else if !processors.is_empty() {
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
mod worker_protocol;
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
    let mut operands = Operands::default();
    // The original argument bytes and operand receiver stay live through parsing.
    let line = unsafe {
        parse_compile_arguments(arguments.as_ptr(), args.len(), operands.sink())
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
            println!("{}", worker_protocol::BOOTSTRAP_JOBSERVER_HELP);
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

    let compiler = line.compiler(std::mem::take(&mut operands.processors))?;
    match line.command {
        CompileCommand::Layout => return layout_facts::print(
            &compiler, &args[line.command_index..], &arguments[line.command_index..],
        ),
        CompileCommand::Ir => return ir_facts::print(
            &compiler, &args[line.command_index..], &arguments[line.command_index..],
        ),
        CompileCommand::Object => return build_object(&compiler, &args, operands, &line),
        _ => line.error.result(&args, line.error_index)?,
    }
    let path = PathBuf::from(&args[line.source_index]);
    let root = args.get(line.root_index).map(PathBuf::from).unwrap_or_else(|| path.parent().unwrap().into());
    let has_output = matches!(line.command, CompileCommand::Build);
    let compilation = if let Some(entry) = args.get(line.entry_index) {
        compiler.compile_entry(&path, &root, unsafe { parsed_text(entry) })?
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
    let programs = compilation.programs.iter().map(|p| &**p)
        .chain(compilation.templates.iter().map(|p| &**p));
    for p in programs.clone() {
        print!("{}", p.describe());
    }
    if has_output {
        let out = PathBuf::from(args.get(line.output_index).ok_or("缺少输出目录")?);
        std::fs::create_dir_all(&out)?;
        for (index, p) in programs.enumerate() {
            export::program(&compiler, p, &out, index)?;
        }
    } else if compilation.programs.is_empty() && compilation.templates.is_empty() {
        println!("library {} checked", path.display());
    }
    Ok(())
}

type ObjectCommandResult = Result<(), Box<dyn std::error::Error>>;

#[repr(C)]
struct ObjectCommandHost {
    import_interface: unsafe extern "C" fn(*mut c_void, usize) -> u32,
    diagnostic: unsafe extern "C" fn(*mut c_void, CompileError, usize),
    compile: unsafe extern "C" fn(*mut c_void, *const CompileCommandLine),
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::run_object_command"]
    fn run_object_command(line: *const CompileCommandLine, imports: *const usize,
        count: usize, context: *mut c_void, host: ObjectCommandHost);

    #[link_name = "bootstrap.cli::finish_object_command"]
    fn finish_object_command(line: *const CompileCommandLine, arguments: usize, context: *mut c_void,
        report: unsafe extern "C" fn(*mut c_void, usize, u64, u64) -> u32,
        complete: unsafe extern "C" fn(*mut c_void));
}

struct ObjectCommand<'a> {
    compiler: &'a blanguage::Compiler,
    args: &'a [OsString],
    operands: &'a Operands,
    imports: Vec<blanguage::module::CompiledInterface>,
    result: ObjectCommandResult,
}

fn build_object(
    compiler: &blanguage::Compiler,
    args: &[OsString],
    operands: Operands,
    line: &CompileCommandLine,
) -> ObjectCommandResult {
    let mut command = ObjectCommand { compiler, args, operands: &operands, imports: Vec::new(), result: Ok(()) };
    let host = ObjectCommandHost { import_interface, diagnostic: object_diagnostic, compile: compile_requested_object };
    unsafe {
        run_object_command(line, operands.imports.as_ptr(), operands.imports.len(),
            std::ptr::from_mut(&mut command).cast(), host);
    }
    command.result
}

unsafe extern "C" fn import_interface(context: *mut c_void, index: usize) -> u32 {
    let command = unsafe { &mut *context.cast::<ObjectCommand<'_>>() };
    match blanguage::module::CompiledInterface::read(Path::new(&command.args[index])) {
        Ok(interfaces) => {
            command.imports.extend(interfaces);
            1
        }
        Err(error) => {
            command.result = Err(error.into());
            0
        }
    }
}

unsafe extern "C" fn object_diagnostic(context: *mut c_void, error: CompileError, index: usize) {
    let command = unsafe { &mut *context.cast::<ObjectCommand<'_>>() };
    command.result = error.result(command.args, index);
}

unsafe extern "C" fn compile_requested_object(context: *mut c_void, line: *const CompileCommandLine) {
    let command = unsafe { &mut *context.cast::<ObjectCommand<'_>>() };
    command.result = command.compile(unsafe { &*line });
}

impl ObjectCommand<'_> {
    fn compile(&mut self, line: &CompileCommandLine) -> ObjectCommandResult {
        let path = Path::new(&self.args[line.source_index]);
        let root = self.args.get(line.root_index).map(Path::new)
            .or_else(|| path.parent().filter(|parent| !parent.as_os_str().is_empty()))
            .unwrap_or_else(|| Path::new("."));
        let output = Path::new(&self.args[line.output_index]);
        use blanguage::build::ObjectKind;
        let instances;
        let kind = match line.mode {
            EntryMode::Module => {
                instances = self.operands.instances(self.args);
                ObjectKind::Module(&instances)
            }
            EntryMode::Program => {
                let entry = unsafe { parsed_text(&self.args[line.entry_index]) };
                let prefix = output.file_stem().and_then(|value| value.to_str()).ok_or("目标文件名不是UTF-8")?;
                ObjectKind::Library { entry, prefix }
            }
            EntryMode::Application => {
                let entry = self.args.get(line.entry_index)
                    .map(|entry| unsafe { parsed_text(entry) })
                    .unwrap_or("main");
                ObjectKind::Application(entry)
            }
        };
        let imports = std::mem::take(&mut self.imports);
        let compiled = self.compiler.build_object_with_interfaces(path, root, imports, kind, output)
            .map_err(|error| -> Box<dyn std::error::Error> { error })?;
        let mut completion = ObjectCompletion { args: self.args, output, compiled, result: Ok(()) };
        unsafe {
            finish_object_command(line, self.args.len(), std::ptr::from_mut(&mut completion).cast(),
                write_object_report, announce_object);
        }
        completion.result
    }
}

struct ObjectCompletion<'a> {
    args: &'a [OsString],
    output: &'a Path,
    compiled: blanguage::build::ObjectCompilation,
    result: ObjectCommandResult,
}

impl ObjectCompletion<'_> {
    fn report(&self, path: &Path, budget: Option<u64>) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        #[cfg(windows)]
        let _object_input = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new().read(true).share_mode(1).open(&self.compiled.artifact)?
        };
        std::fs::write(path, self.compiled.object.stack_report(budget))
    }
}

unsafe extern "C" fn write_object_report(context: *mut c_void, index: usize, budget: u64, has_budget: u64) -> u32 {
    let output = unsafe { &mut *context.cast::<ObjectCompletion<'_>>() };
    output.result = output.report(Path::new(&output.args[index]), (has_budget != 0).then_some(budget))
        .map_err(Into::into);
    u32::from(output.result.is_ok())
}

unsafe extern "C" fn announce_object(context: *mut c_void) {
    use std::io::Write;
    let output = unsafe { &mut *context.cast::<ObjectCompletion<'_>>() };
    output.result = writeln!(std::io::stdout().lock(), "object {} -> {}{}", output.compiled.object.name(),
        output.output.display(), if output.compiled.reused { " (reused)" } else { "" }).map_err(Into::into);
}
