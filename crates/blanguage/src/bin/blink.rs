#[path = "../process_arguments.rs"]
mod process_arguments;
use std::{ffi::{OsString, c_void}, io::Write, path::Path, process::ExitCode, ptr, slice, str};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[repr(C)]
struct LinkArgument {
    bytes: *const u8,
    length: usize,
    text: u64,
}

#[repr(C)]
struct LinkCommandHost {
    context: *mut c_void,
    append_input: unsafe extern "C" fn(*mut c_void, usize),
    diagnostic: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    help: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    version: unsafe extern "C" fn(*mut c_void),
    link: unsafe extern "C" fn(*mut c_void, usize) -> u32,
    archive: unsafe extern "C" fn(*mut c_void, usize) -> u32,
    host_archive: unsafe extern "C" fn(*mut c_void, usize) -> u32,
    stack_report: unsafe extern "C" fn(*mut c_void, usize, u64, u64) -> u32,
    complete: unsafe extern "C" fn(*mut c_void, usize, *const u8, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::run_link_command"]
    fn run_link_command(arguments: *const LinkArgument, count: usize, host: LinkCommandHost);
}

struct LinkInvocation<'a> {
    args: &'a [OsString],
    paths: Vec<&'a Path>,
    result: Result,
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

fn run() -> Result {
    let args = process_arguments::read();
    let arguments: Vec<_> = args.iter().map(|argument| {
        let bytes = argument.as_encoded_bytes();
        LinkArgument { bytes: bytes.as_ptr(), length: bytes.len(), text: u64::from(argument.to_str().is_some()) }
    }).collect();
    let mut invocation = LinkInvocation { args: &args, paths: Vec::new(), result: Ok(()) };
    let host = LinkCommandHost {
        context: ptr::from_mut(&mut invocation).cast(),
        append_input,
        diagnostic,
        help,
        version,
        link,
        archive,
        host_archive,
        stack_report,
        complete,
    };
    // B consumes the borrowed argument bytes and returns before their owner drops.
    unsafe { run_link_command(arguments.as_ptr(), arguments.len(), host); }
    invocation.result
}

unsafe extern "C" fn append_input(context: *mut c_void, index: usize) {
    let invocation = unsafe { &mut *context.cast::<LinkInvocation<'_>>() };
    invocation.paths.push(Path::new(&invocation.args[index]));
}

unsafe extern "C" fn diagnostic(context: *mut c_void, text: *const u8, length: usize) {
    let invocation = unsafe { &mut *context.cast::<LinkInvocation<'_>>() };
    // The message is a complete B UTF-8 literal borrowed for this call.
    let message = unsafe { str::from_utf8_unchecked(slice::from_raw_parts(text, length)) };
    invocation.result = Err(message.to_owned().into());
}

unsafe extern "C" fn help(context: *mut c_void, text: *const u8, length: usize) {
    let invocation = unsafe { &mut *context.cast::<LinkInvocation<'_>>() };
    let message = unsafe { slice::from_raw_parts(text, length) };
    invocation.result = std::io::stdout().lock().write_all(message).map_err(Into::into);
}

unsafe extern "C" fn version(context: *mut c_void) {
    let invocation = unsafe { &mut *context.cast::<LinkInvocation<'_>>() };
    invocation.result = writeln!(std::io::stdout().lock(), "Blink linker {}", include_str!("../../../../VERSION").trim())
        .map_err(Into::into);
}

unsafe fn produce(
    context: *mut c_void,
    output: usize,
    product: impl FnOnce(&[&Path], &Path) -> Result,
) -> u32 {
    let invocation = unsafe { &mut *context.cast::<LinkInvocation<'_>>() };
    let path = Path::new(&invocation.args[output]);
    invocation.result = product(&invocation.paths, path);
    u32::from(invocation.result.is_ok())
}

unsafe extern "C" fn link(context: *mut c_void, output: usize) -> u32 {
    unsafe { produce(context, output, |inputs, path| blanguage::linker::link_inputs(inputs, path)) }
}

unsafe extern "C" fn archive(context: *mut c_void, output: usize) -> u32 {
    unsafe { produce(context, output, |inputs, path| blanguage::object::archive(inputs, path)) }
}

unsafe extern "C" fn host_archive(context: *mut c_void, output: usize) -> u32 {
    unsafe { produce(context, output, |inputs, path| blanguage::linker::host_archive(inputs, path)) }
}

unsafe extern "C" fn stack_report(context: *mut c_void, output: usize, budget: u64, has_budget: u64) -> u32 {
    let budget = (has_budget != 0).then_some(budget);
    unsafe { produce(context, output, |inputs, path| blanguage::linker::write_stack_report(inputs, path, budget)) }
}

unsafe extern "C" fn complete(context: *mut c_void, output: usize, text: *const u8, length: usize) {
    let invocation = unsafe { &mut *context.cast::<LinkInvocation<'_>>() };
    let label = unsafe { str::from_utf8_unchecked(slice::from_raw_parts(text, length)) };
    let path = Path::new(&invocation.args[output]);
    invocation.result = writeln!(std::io::stdout().lock(), "{label} {} input(s) -> {}", invocation.paths.len(), path.display())
        .map_err(Into::into);
}
