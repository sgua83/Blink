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
    if args.len() == 1 && args[0] == "--version" {
        println!("Blink linker {}", include_str!("../../../../VERSION").trim());
        return Ok(());
    }
    if args.is_empty() || args[0] == "--help" {
        println!(
            "Blink linker\nblink --version\nblink <input.obj|library.lib>... -o <output.exe|output.dll>\nblink --archive <module.obj|library.lib>... -o <library.lib>\nblink --stack-report <application.obj|library.lib>... -o <report.txt> [--stack-budget bytes]"
        );
        return Ok(());
    }
    let mut inputs = Vec::new();
    let mut archive = false;
    let mut stack_report = false;
    let mut stack_budget = None;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--archive" {
            archive = true;
        } else if args[i] == "--stack-report" {
            stack_report = true;
        } else if args[i] == "--stack-budget" {
            i += 1;
            stack_budget = Some(args.get(i).ok_or("--stack-budget缺少字节数")?.parse::<u64>()?);
        } else if args[i] == "-o" {
            i += 1;
            output = Some(PathBuf::from(args.get(i).ok_or("-o 缺少输出路径")?));
        } else {
            inputs.push(PathBuf::from(&args[i]));
        }
        i += 1;
    }
    if stack_budget.is_some() && !stack_report { return Err("--stack-budget只用于--stack-report命令".into()); }
    let output = output.ok_or(
        "用法：blink <input.obj|library.lib>... -o <output.exe|output.dll>；归档使用 --archive",
    )?;
    if stack_report {
        if archive { return Err("--stack-report与--archive是不同产物命令".into()); }
        let report = blanguage::linker::stack_report_inputs(&inputs, stack_budget)?;
        let path = std::path::absolute(&output)?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, report)?;
        println!("stack report {} input(s) -> {}", inputs.len(), output.display());
    } else if archive {
        blanguage::object::archive(&inputs, &output)?;
        println!("archived {} input(s) -> {}", inputs.len(), output.display());
    } else {
        blanguage::linker::link_inputs(&inputs, &output)?;
        println!("linked {} input(s) -> {}", inputs.len(), output.display());
    }
    Ok(())
}
