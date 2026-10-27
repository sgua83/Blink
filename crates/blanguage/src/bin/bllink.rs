use std::{path::PathBuf, process::ExitCode};
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
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" {
        println!("Blink linker\nbllink <input.obj>... -o <output.exe|output.dll>");
        return Ok(());
    }
    let mut inputs = Vec::new();
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-o" {
            i += 1;
            output = Some(PathBuf::from(args.get(i).ok_or("-o 缺少输出路径")?));
        } else {
            inputs.push(PathBuf::from(&args[i]));
        }
        i += 1;
    }
    let output = output.ok_or("用法：bllink <input.obj>... -o <output.exe|output.dll>")?;
    blanguage::linker::link_inputs(&inputs, &output)?;
    println!("linked {} object(s) -> {}", inputs.len(), output.display());
    Ok(())
}
