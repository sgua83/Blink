mod export;
mod input_facts;
mod image_export;
mod code_facts;
mod ir_facts;
mod json;
mod layout_facts;
mod x64_facts;
mod spirv_facts;
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
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("Blink compiler {}", include_str!("../../../VERSION").trim());
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "inputs") {
        if args.len() != 2 { return Err("用法：bc inputs <compiled.obj|entry.blbi>".into()); }
        return input_facts::print(std::path::Path::new(&args[1]));
    }
    if args.first().is_some_and(|arg| arg == "x64") { return x64_facts::print(&args); }
    if args.first().is_some_and(|arg| arg == "image") { return image_export::export(&args); }
    if args.first().is_some_and(|arg| arg == "code") { return code_facts::print(&args); }
    if args.first().is_some_and(|arg| arg == "spirv") { return spirv_facts::print(&args); }
    let mut jobs = None;
    let mut cpus = None;
    let mut max_parameter_bytes = None;
    while args.first().is_some_and(|arg| matches!(arg.as_str(), "--jobs" | "--cpus" | "--max-parameter-bytes")) {
        let option = args.remove(0);
        if args.is_empty() { return Err(format!("{option}缺少参数").into()); }
        let value = args.remove(0);
        if option == "--jobs" {
            if jobs.is_some() { return Err("--jobs不能重复指定".into()); }
            jobs = Some(value.parse::<std::num::NonZeroUsize>()?);
        } else if option == "--cpus" {
            if cpus.is_some() { return Err("--cpus不能重复指定".into()); }
            cpus = Some(value.split(',').map(|cpu| {
                let (group, number) = cpu.split_once(':').ok_or("CPU写为组:编号")?;
                Ok::<_, Box<dyn std::error::Error>>(blanguage::Processor {
                    group: group.parse()?, number: number.parse()?,
                })
            }).collect::<Result<Vec<_>, _>>()?);
        } else {
            if max_parameter_bytes.is_some() { return Err("--max-parameter-bytes不能重复指定".into()); }
            max_parameter_bytes = Some(value.parse::<u32>()?);
        }
    }
    if jobs.is_some() && cpus.is_some() { return Err("--jobs与--cpus选择一种编译资源声明".into()); }
    if args.is_empty() || args[0] == "--help" {
        println!(
            "Blink compiler\nbc --version\nbc inputs <compiled.obj|entry.blbi>\nbc image <entry.blbi> -o <deployment.blri>\nbc code <entry.blbi> --observations <ranges.blcr>\nbc code --image <deployment.blri> --observations <ranges.blcr>\nbc code --object <compiled.obj> [--observations <ranges.blcf>] [--section number | --function exact-symbol]\nbc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]\nbc spirv <module.spv>\nbc ir <module.b> [-I root] [--module | --program name | --entry name] [--function qualified-name | --stage index] [--import object|archive] [--instance name]\nbc layout <module.h|module.b> <type> [-I module-root] [--field path]\nGlobal compile options before command: [--jobs N | --cpus group:number,...] [--max-parameter-bytes N]\nbc -c <module.b> -o <output.obj> [-I module-root] [--entry main | --program name | --module] [--import module.obj|library.lib] [--instance function<args>] [--stack-report report.txt [--stack-budget bytes]]\nbc check <module.b> [module-root] [program<args>]\nbc build <module.b> <output-directory> [module-root] [program<args>]\nLink separately: blink <input.obj> -o <output.exe|output.dll>"
        );
        return Ok(());
    }
    let compiler = if let Some(cpus) = cpus {
        blanguage::Compiler::on_processors(cpus)?
    } else if let Some(jobs) = jobs {
        blanguage::Compiler::with_workers(jobs)
    } else {
        blanguage::Compiler::available()?
    };
    let compiler = if let Some(bytes) = max_parameter_bytes { compiler.with_max_parameter_bytes(bytes) } else { compiler };
    let command = &args[0];
    if command == "layout" { return layout_facts::print(&compiler, &args); }
    if command == "ir" { return ir_facts::print(&compiler, &args); }
    let path = PathBuf::from(args.get(1).ok_or("缺少 .b 文件")?);
    if command == "-c" {
        let parent = path.parent().unwrap();
        let mut root = if parent.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            parent.to_path_buf()
        };
        let mut output = None;
        let mut stack_report = None;
        let mut stack_budget = None;
        let mut entry = "main";
        let mut program = None;
        let mut module = false;
        let mut imports = Vec::new();
        let mut instances = Vec::new();
        let mut i = 2;
        while i < args.len() {
            if args[i] == "--module" {
                module = true;
                i += 1;
                continue;
            }
            let value = args.get(i + 1).ok_or("编译选项缺少参数")?;
            match args[i].as_str() {
                "-o" => output = Some(PathBuf::from(value)),
                "--stack-report" => stack_report = Some(PathBuf::from(value)),
                "--stack-budget" => stack_budget = Some(value.parse::<u64>()?),
                "-I" => root = PathBuf::from(value),
                "--entry" => entry = value,
                "--program" => program = Some(value.as_str()),
                "--import" => imports
                    .extend(blanguage::object::Input::read(&PathBuf::from(value))?.objects()?),
                "--instance" => instances.push(value.clone()),
                other => return Err(format!("未知编译选项 {other}").into()),
            }
            i += 2;
        }
        if stack_budget.is_some() && stack_report.is_none() { return Err("--stack-budget需要--stack-report输出".into()); }
        let output = output.ok_or("缺少 -o 目标文件路径")?;
        let kind = if module {
            blanguage::build::ObjectKind::Module(&instances)
        } else if let Some(entry) = program {
            let prefix = output.file_stem().and_then(|value| value.to_str())
                .ok_or("目标文件名不是UTF-8")?;
            blanguage::build::ObjectKind::Library { entry, prefix }
        } else {
            blanguage::build::ObjectKind::Application(entry)
        };
        let compiled = compiler.build_object(&path, &root, &imports, kind, &output)
            .map_err(|error| -> Box<dyn std::error::Error> { error })?;
        if let Some(path) = stack_report {
            let path = std::path::absolute(path)?;
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(path, compiled.object.stack_report(stack_budget))?;
        }
        println!("object {} -> {}{}", compiled.object.name(), output.display(),
            if compiled.reused { " (reused)" } else { "" });
        return Ok(());
    }
    if !matches!(command.as_str(), "check" | "build") {
        return Err(format!("未知命令 {command}；编译使用 bc -c，链接使用 blink").into());
    }
    let has_output = command == "build";
    let root = if has_output { args.get(3) } else { args.get(2) }
        .map(PathBuf::from)
        .unwrap_or_else(|| path.parent().unwrap().into());
    let selected = args.get(if has_output { 4 } else { 3 });
    let compilation = if let Some(entry) = selected {
        compiler.compile_entry(&path, &root, entry)?
    } else if command == "check" {
        compiler.check(&path, &root)?
    } else {
        compiler.compile(&path, &root)?
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
                export::program(&compiler, p, &out, &stem)?;
            }
        }
        _ => return Err(format!("未知命令 {command}").into()),
    }
    Ok(())
}
