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

use std::{ffi::{OsStr, OsString, c_void}, io::Write, num::NonZeroUsize, path::Path, process::ExitCode};

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
#[repr(transparent)]
struct CompileError(u32);

#[derive(Clone, Copy, PartialEq)]
#[repr(u32)]
enum EntryMode {
    Module,
    Application,
    Program,
}

#[repr(u32)]
#[expect(dead_code, reason = "B produces the selected compiler resource discriminant")]
enum CompilerResources {
    Available,
    Workers,
    Processors,
    Inherited,
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
    resources: CompilerResources,
    jobs: usize,
    has_parameter_limit: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::parse_compile_arguments"]
    fn parse_compile_arguments(
        arguments: *const Argument,
        count: usize,
        operands: OperandSink,
        inherited: u32,
    ) -> CompileCommandLine;
}

#[repr(C)]
struct CompileDiagnosticHost {
    context: *mut c_void,
    text: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    argument: unsafe extern "C" fn(*mut c_void, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::render_compile_error"]
    fn render_compile_error(error: CompileError, index: usize, host: CompileDiagnosticHost);
    #[link_name = "bootstrap.cli::write_compile_help"]
    fn write_compile_help(context: *mut c_void, emit: unsafe extern "C" fn(*mut c_void, *const u8, usize));
}

struct CompileDiagnostic<'a> {
    args: &'a [OsString],
    message: String,
}

unsafe extern "C" fn append_compile_text(context: *mut c_void, data: *const u8, length: usize) {
    let output = unsafe { &mut *context.cast::<CompileDiagnostic<'_>>() };
    // The B command module borrows a UTF-8 literal for this call.
    let text = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(data, length)) };
    output.message.push_str(text);
}

unsafe extern "C" fn append_compile_argument(context: *mut c_void, index: usize) {
    let output = unsafe { &mut *context.cast::<CompileDiagnostic<'_>>() };
    output.message.push_str(&output.args[index].to_string_lossy());
}

unsafe extern "C" fn print_compile_help(_: *mut c_void, data: *const u8, length: usize) {
    // The B command module borrows a UTF-8 literal for this call.
    let text = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(data, length)) };
    print!("{text}");
}

impl CompileError {
    const IR_USAGE: Self = Self(26);
    const LAYOUT_USAGE: Self = Self(35);
    const X64_USAGE: Self = Self(37);

    fn message(self, args: &[OsString], index: usize) -> Box<dyn std::error::Error> {
        let mut output = CompileDiagnostic { args, message: String::new() };
        unsafe {
            render_compile_error(self, index, CompileDiagnosticHost {
                context: std::ptr::from_mut(&mut output).cast(),
                text: append_compile_text,
                argument: append_compile_argument,
            });
        }
        output.message.into()
    }

    fn result(self, args: &[OsString], index: usize) -> Result<(), Box<dyn std::error::Error>> {
        if self.0 == 0 { return Ok(()); }
        Err(self.message(args, index))
    }
}

impl CompileCommandLine {
    fn compiler(&self, processors: Vec<blanguage::Processor>) -> std::io::Result<blanguage::Compiler> {
        let compiler = match self.resources {
            CompilerResources::Available => blanguage::Compiler::available()?,
            CompilerResources::Workers => {
                // B selects Workers only after parsing a nonzero count.
                blanguage::Compiler::with_workers(unsafe { NonZeroUsize::new_unchecked(self.jobs) })
            }
            CompilerResources::Processors => blanguage::Compiler::on_processors(processors)?,
            CompilerResources::Inherited => blanguage::Compiler::bootstrap_worker()?,
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
    let inherited = std::env::var_os(worker_protocol::BOOTSTRAP_JOBSERVER).as_deref() == Some(OsStr::new("1"));
    let mut operands = Operands::default();
    // The original argument bytes and operand receiver stay live through parsing.
    let line = unsafe {
        parse_compile_arguments(arguments.as_ptr(), args.len(), operands.sink(), u32::from(inherited))
    };
    line.prefix_error.result(&args, line.error_index)?;
    match line.command {
        CompileCommand::Version => {
            println!("Blink compiler {}", include_str!("../../../VERSION").trim());
            return Ok(());
        }
        CompileCommand::Help => {
            unsafe { write_compile_help(std::ptr::null_mut(), print_compile_help); }
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
    let path = Path::new(&args[line.source_index]);
    let root = args.get(line.root_index).map(Path::new).unwrap_or_else(|| path.parent().unwrap());
    let mut program = ProgramCommand { compiler: &compiler, args: &args, path, root, compilation: None, error: None };
    let host = ProgramCommandHost {
        context: std::ptr::from_mut(&mut program).cast(),
        compile: compile_programs,
        report_generics: report_open_generics,
        describe: describe_program,
        prepare_output: prepare_output_directory,
        export_program: export_requested_program,
        report_library: report_checked_library,
    };
    unsafe { run_program_command(line.command, args.len(), line.entry_index, line.output_index, host); }
    program.error.map_or(Ok(()), Err)
}

#[repr(u32)]
#[expect(dead_code, reason = "the B command module supplies the ABI discriminants")]
enum ProgramRequest {
    Check,
    All,
    Entry,
}

#[repr(C)]
struct ProgramResults {
    count: usize,
    generics: u32,
    success: u32,
}

#[repr(C)]
struct ProgramCommandHost {
    context: *mut c_void,
    compile: unsafe extern "C" fn(*mut c_void, ProgramRequest, usize) -> ProgramResults,
    report_generics: unsafe extern "C" fn(*mut c_void) -> u32,
    describe: unsafe extern "C" fn(*mut c_void, usize) -> u32,
    prepare_output: unsafe extern "C" fn(*mut c_void, usize) -> u32,
    export_program: unsafe extern "C" fn(*mut c_void, usize, usize) -> u32,
    report_library: unsafe extern "C" fn(*mut c_void),
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::run_program_command"]
    fn run_program_command(command: CompileCommand, arguments: usize, entry: usize, output: usize, host: ProgramCommandHost);
}

struct ProgramCommand<'a> {
    compiler: &'a blanguage::Compiler,
    args: &'a [OsString],
    path: &'a Path,
    root: &'a Path,
    compilation: Option<blanguage::Compilation>,
    error: Option<Box<dyn std::error::Error>>,
}

impl ProgramCommand<'_> {
    fn program(&self, index: usize) -> &blanguage::ir::Program {
        let compilation = self.compilation.as_ref().unwrap();
        if index < compilation.programs.len() {
            &compilation.programs[index]
        } else {
            &compilation.templates[index - compilation.programs.len()]
        }
    }

    fn report(&mut self, text: std::fmt::Arguments<'_>) -> u32 {
        self.record(std::io::stdout().lock().write_fmt(text).map_err(Into::into))
    }

    fn record(&mut self, result: Result<(), Box<dyn std::error::Error>>) -> u32 {
        match result {
            Ok(()) => 1,
            Err(error) => {
                self.error = Some(error);
                0
            }
        }
    }
}

unsafe extern "C" fn compile_programs(
    context: *mut c_void,
    request: ProgramRequest,
    entry: usize,
) -> ProgramResults {
    let command = unsafe { &mut *context.cast::<ProgramCommand<'_>>() };
    let result = match request {
        ProgramRequest::Check => command.compiler.check(command.path, command.root),
        ProgramRequest::All => command.compiler.compile(command.path, command.root),
        ProgramRequest::Entry => {
            let entry = unsafe { parsed_text(&command.args[entry]) };
            command.compiler.compile_entry(command.path, command.root, entry)
        }
    };
    match result {
        Ok(compilation) => {
            let count = compilation.programs.len() + compilation.templates.len();
            let generics = u32::from(!compilation.open_generics.is_empty());
            command.compilation = Some(compilation);
            ProgramResults { count, generics, success: 1 }
        }
        Err(error) => {
            command.error = Some(error.into());
            ProgramResults { count: 0, generics: 0, success: 0 }
        }
    }
}

unsafe extern "C" fn report_open_generics(context: *mut c_void) -> u32 {
    let command = unsafe { &mut *context.cast::<ProgramCommand<'_>>() };
    let names = command.compilation.as_ref().unwrap().open_generics.join(", ");
    command.report(format_args!("open generic declarations (body checked on instantiation): {names}\n"))
}

unsafe extern "C" fn describe_program(context: *mut c_void, index: usize) -> u32 {
    let command = unsafe { &mut *context.cast::<ProgramCommand<'_>>() };
    let description = command.program(index).describe();
    command.report(format_args!("{description}"))
}

unsafe extern "C" fn prepare_output_directory(context: *mut c_void, output: usize) -> u32 {
    let command = unsafe { &mut *context.cast::<ProgramCommand<'_>>() };
    let result = command.args.get(output).ok_or_else(|| "缺少输出目录".into())
        .and_then(|path| std::fs::create_dir_all(path).map_err(Into::into));
    command.record(result)
}

unsafe extern "C" fn export_requested_program(context: *mut c_void, output: usize, index: usize) -> u32 {
    let command = unsafe { &mut *context.cast::<ProgramCommand<'_>>() };
    let result = export::program(command.compiler, command.program(index), Path::new(&command.args[output]), index);
    command.record(result)
}

unsafe extern "C" fn report_checked_library(context: *mut c_void) {
    let command = unsafe { &mut *context.cast::<ProgramCommand<'_>>() };
    let path = command.path;
    command.report(format_args!("library {} checked\n", path.display()));
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
