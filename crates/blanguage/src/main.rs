mod export;
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
        println!(
            "Blink compiler\nblc -c <module.b> -o <output.obj> [-I module-root] [--entry main | --program name]\nblc check <module.b> [module-root] [program<args>]\nblc build <module.b> <output-directory> [module-root] [program<args>]\nLink separately: bllink <input.obj> -o <output.exe|output.dll>"
        );
        return Ok(());
    }
    let command = &args[0];
    let path = PathBuf::from(args.get(1).ok_or("缺少 .b 文件")?);
    if command == "-c" {
        let mut root = path.parent().unwrap().to_path_buf();
        let mut output = None;
        let mut entry = "main";
        let mut program = None;
        let mut i = 2;
        while i < args.len() {
            let value = args.get(i + 1).ok_or("编译选项缺少参数")?;
            match args[i].as_str() {
                "-o" => output = Some(PathBuf::from(value)),
                "-I" => root = PathBuf::from(value),
                "--entry" => entry = value,
                "--program" => program = Some(value.as_str()),
                other => return Err(format!("未知编译选项 {other}").into()),
            }
            i += 2;
        }
        let output = output.ok_or("缺少 -o 目标文件路径")?;
        let object = if let Some(entry) = program {
            let compilation = blanguage::compile_entry(&path, &root, entry)?;
            let program = compilation
                .programs
                .first()
                .ok_or("DLL 编译需要无前置条件的具体 program")?;
            let prefix = output
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("目标文件名不是 UTF-8")?;
            blanguage::dll::compile_object(program, prefix)?
        } else {
            blanguage::compile_application(&path, &root, entry)?.compile()?
        };
        object.write(&output)?;
        println!("object {} -> {}", object.name(), output.display());
        return Ok(());
    }
    if !matches!(command.as_str(), "check" | "build") {
        return Err(format!("未知命令 {command}；编译使用 blc -c，链接使用 bllink").into());
    }
    let has_output = command == "build";
    let root = if has_output { args.get(3) } else { args.get(2) }
        .map(PathBuf::from)
        .unwrap_or_else(|| path.parent().unwrap().into());
    let selected = args.get(if has_output { 4 } else { 3 });
    let compilation = if let Some(entry) = selected {
        blanguage::compile_entry(&path, &root, entry)?
    } else if command == "check" {
        blanguage::check(&path, &root)?
    } else {
        blanguage::compile(&path, &root)?
    };
    if !compilation.open_generics.is_empty() {
        println!(
            "open const declarations (body checked on instantiation): {}",
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
    match command.as_str() {
        "check" => {
            if programs.is_empty() {
                println!("library {} checked", path.display());
            }
        }
        "build" => {
            let out = PathBuf::from(args.get(2).ok_or("缺少输出目录")?);
            std::fs::create_dir_all(&out)?;
            for (index, p) in programs.iter().enumerate() {
                // 类型的限定名保留在 layout；导出文件不使用 Windows 禁止的尖括号。
                let stem = if let Some((base, _)) = p.name().split_once('<') {
                    format!(
                        "{}.instance-{index}",
                        base.replace("::", "_").replace('.', "_")
                    )
                } else {
                    p.name().replace("::", "_").replace('.', "_")
                };
                export::program(p, &out, &stem)?;
            }
        }
        _ => return Err(format!("未知命令 {command}").into()),
    }
    Ok(())
}
