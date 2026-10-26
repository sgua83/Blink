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
    if args.is_empty() {
        println!(
            "Blink compiler\nblc check <module.b> [module-root] [program<args>]\nblc build <module.b> <output-directory> [module-root] [program<args>]\nblc build-dll <module.b> <output.dll> [module-root] [program<args>]\nblc build-exe <module.b> <output.exe> [module-root] [entry=main]"
        );
        return Ok(());
    }
    let command = &args[0];
    let path = PathBuf::from(args.get(1).ok_or("缺少 .b 文件")?);
    if command == "build-exe" {
        let root = args
            .get(3)
            .map(PathBuf::from)
            .unwrap_or_else(|| path.parent().unwrap().into());
        let entry = args.get(4).map(String::as_str).unwrap_or("main");
        let application = blanguage::compile_application(&path, &root, entry)?;
        let output = PathBuf::from(args.get(2).ok_or("缺少 .exe 输出路径")?);
        application.build(&output)?;
        println!("application {} -> {}", application.name(), output.display());
        return Ok(());
    }
    let has_output = matches!(command.as_str(), "build" | "build-dll");
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
        "build-dll" => {
            if compilation.programs.len() != 1 || !compilation.templates.is_empty() {
                return Err(
                    "build-dll 需要一个无前置条件的具体 program；请在命令末尾选择入口".into(),
                );
            }
            blanguage::dll::build(
                &compilation.programs[0],
                &PathBuf::from(args.get(2).ok_or("缺少 DLL 输出路径")?),
            )?;
        }
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
