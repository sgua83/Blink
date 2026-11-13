use std::{path::PathBuf, process::ExitCode};

#[repr(C)]
struct LinkArgument {
    bytes: *const u8,
    length: usize,
    text: u64,
}

#[repr(u32)]
enum LinkCommand {
    Link,
    Archive,
    HostArchive,
    StackReport,
    Help,
    Version,
}

#[repr(u32)]
enum LinkArgumentError {
    None,
    ProductConflict,
    MissingBudget,
    BudgetText,
    EmptyBudget,
    InvalidBudget,
    BudgetOverflow,
    MissingOutputOperand,
    BudgetWithoutReport,
    MissingOutput,
}

#[repr(C)]
struct LinkCommandLine {
    command: LinkCommand,
    error: LinkArgumentError,
    output_index: usize,
    input_count: usize,
    budget: u64,
    has_budget: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.link_cli::parse_arguments"]
    fn parse_arguments(
        arguments: *const LinkArgument,
        count: usize,
        input_indices: *mut usize,
    ) -> LinkCommandLine;
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
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let arguments = args.iter().map(|arg| {
        let bytes = arg.as_encoded_bytes();
        LinkArgument { bytes: bytes.as_ptr(), length: bytes.len(), text: u64::from(arg.to_str().is_some()) }
    }).collect::<Vec<_>>();
    let mut input_indices = vec![0; arguments.len()];
    // The original OsStrings and the index buffer stay live through parsing.
    let parsed = unsafe { parse_arguments(arguments.as_ptr(), arguments.len(), input_indices.as_mut_ptr()) };
    let error = match parsed.error {
        LinkArgumentError::None => None,
        LinkArgumentError::ProductConflict => Some("blink只能选择一种产物命令"),
        LinkArgumentError::MissingBudget => Some("--stack-budget缺少字节数"),
        LinkArgumentError::BudgetText => Some("--stack-budget需要十进制字节数"),
        LinkArgumentError::EmptyBudget => Some("cannot parse integer from empty string"),
        LinkArgumentError::InvalidBudget => Some("invalid digit found in string"),
        LinkArgumentError::BudgetOverflow => Some("number too large to fit in target type"),
        LinkArgumentError::MissingOutputOperand => Some("-o 缺少输出路径"),
        LinkArgumentError::BudgetWithoutReport => Some("--stack-budget只用于--stack-report命令"),
        LinkArgumentError::MissingOutput => Some(
            "用法：blink <input.obj|library.lib>... -o <output.exe|output.dll>；B接口库使用 --archive，原生宿主库使用 --host-archive",
        ),
    };
    if let Some(error) = error {
        return Err(error.into());
    }
    match parsed.command {
        LinkCommand::Version => {
            println!("Blink linker {}", include_str!("../../../../VERSION").trim());
            return Ok(());
        }
        LinkCommand::Help => {
            println!(
                "Blink linker\nblink --version\nblink <input.obj|library.lib>... -o <output.exe|output.dll>\nblink --archive <module.obj|library.lib>... -o <library.lib>\nblink --host-archive <module.obj|library.lib>... -o <host.lib>\nblink --stack-report <application.obj|library.lib>... -o <report.txt> [--stack-budget bytes]"
            );
            return Ok(());
        }
        _ => {}
    }
    let inputs = input_indices[..parsed.input_count].iter()
        .map(|&index| PathBuf::from(&args[index])).collect::<Vec<_>>();
    let output = PathBuf::from(&args[parsed.output_index]);
    let stack_budget = (parsed.has_budget != 0).then_some(parsed.budget);
    if matches!(parsed.command, LinkCommand::StackReport) {
        #[cfg(windows)]
        let _input_files = {
            use std::os::windows::fs::OpenOptionsExt;
            inputs.iter().map(|path|
                std::fs::OpenOptions::new().read(true).share_mode(1).open(path)
            ).collect::<std::io::Result<Vec<_>>>()?
        };
        let report = blanguage::linker::stack_report_inputs(&inputs, stack_budget)?;
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&output, report)?;
        println!("stack report {} input(s) -> {}", inputs.len(), output.display());
    } else if matches!(parsed.command, LinkCommand::Archive) {
        blanguage::object::archive(&inputs, &output)?;
        println!("archived {} input(s) -> {}", inputs.len(), output.display());
    } else if matches!(parsed.command, LinkCommand::HostArchive) {
        blanguage::linker::host_archive(&inputs, &output)?;
        println!("host archived {} input(s) -> {}", inputs.len(), output.display());
    } else {
        blanguage::linker::link_inputs(&inputs, &output)?;
        println!("linked {} input(s) -> {}", inputs.len(), output.display());
    }
    Ok(())
}
