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
        println!("Blink linker\nbllink <input.obj> -o <output.exe|output.dll>");
        return Ok(());
    }
    let input = PathBuf::from(&args[0]);
    if args.len() != 3 || args[1] != "-o" {
        return Err("用法：bllink <input.obj> -o <output.exe|output.dll>".into());
    }
    let output = PathBuf::from(&args[2]);
    blanguage::linker::link(&input, &output)?;
    println!("linked {} -> {}", input.display(), output.display());
    Ok(())
}
