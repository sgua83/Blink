//! Selects original compilation consumers, then prints their actual IR values.
use crate::json::string;
use std::{fmt::Write, path::PathBuf};

pub fn print(compiler: &blanguage::Compiler, args: &[std::ffi::OsString]) -> Result<(), Box<dyn std::error::Error>> {
    let usage = "用法：bc ir <module.b> [-I root] [--module | --program name | --entry name] [--function qualified-name | --stage index] [--import object|archive] [--instance name]";
    let path = PathBuf::from(args.get(1).ok_or(usage)?).canonicalize()?;
    let mut root = path.parent().unwrap().to_path_buf();
    let mut mode = None;
    let mut function = None;
    let mut stage = None;
    let mut import_paths = Vec::new();
    let mut instances = Vec::new();
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--module" {
            if mode.replace(("module", "")).is_some() { return Err("IR编译模式只能指定一次".into()); }
            i += 1; continue;
        }
        let value = args.get(i + 1).ok_or(usage)?;
        match args[i].to_str().ok_or(usage)? {
            "-I" => root = PathBuf::from(value),
            "--entry" | "--program" => {
                if mode.replace((if args[i] == "--entry" { "application" } else { "program" }, value.to_str().ok_or(usage)?)).is_some() {
                    return Err("IR编译模式只能指定一次".into());
                }
            }
            "--function" => {
                if function.replace(value.to_str().ok_or(usage)?).is_some() { return Err("--function不能重复指定".into()); }
            }
            "--stage" => {
                if stage.replace(value.to_str().ok_or(usage)?.parse::<usize>()?).is_some() { return Err("--stage不能重复指定".into()); }
            }
            "--import" => import_paths.push(PathBuf::from(value).canonicalize()?),
            "--instance" => instances.push(value.to_str().ok_or(usage)?.to_owned()),
            other => return Err(format!("未知IR选项 {other}；{usage}").into()),
        }
        i += 2;
    }
    let (mode, entry) = mode.unwrap_or(("module", ""));
    if function.is_some() && (stage.is_some() || mode == "program") {
        return Err("--function用于普通应用／模块，--stage用于--program".into());
    }
    if stage.is_some() && mode != "program" { return Err("--stage需要--program".into()); }
    if !instances.is_empty() && mode != "module" { return Err("--instance用于模块；入口实例直接写在--entry／--program值中".into()); }
    if !import_paths.is_empty() && mode == "program" { return Err("此并行入口编译API不接受已编译导入；可用--module查询模块的函数和资源程序".into()); }
    let root = root.canonicalize()?;
    let mut imports = Vec::new();
    for path in &import_paths { imports.extend(blanguage::object::Input::read(path)?.objects()?); }
    let (phase, value) = match mode {
        "application" => {
            let application = compiler.compile_application_with_modules(&path, &root, entry, &imports)?;
            let value = if let Some(function) = function {
                application.function_ir_json(function).ok_or_else(|| format!("本次应用没有保留普通函数 {function}"))?
            } else { application.ir_json() };
            ("finalized_application_ir", value)
        }
        "program" => {
            let compilation = compiler.compile_entry(&path, &root, entry)?;
            let program = compilation.programs.iter().map(|p| &**p)
                .chain(compilation.templates.iter().map(|p| &**p)).next().ok_or("本次入口没有并行IR")?;
            let value = if let Some(stage) = stage {
                program.stages().get(stage).ok_or_else(|| format!("本次入口只有{}个优化后阶段，没有阶段{stage}", program.stages().len()))?.ir_json()
            } else { program.ir_json() };
            ("optimized_program_ir", value)
        }
        _ => {
            let module = compiler.compile_native_module(&path, &root, &imports, &instances)?;
            let value = if let Some(function) = function {
                module.function_ir_json(function).ok_or_else(|| format!("本次模块没有保留普通函数 {function}"))?
            } else { module.ir_json() };
            ("finalized_module_ir", value)
        }
    };
    let mut out = String::from("{\"kind\":\"ir_query\",\"phase\":"); string(&mut out, phase);
    out.push(','); crate::json::path(&mut out, "source_path", &path);
    out.push(','); crate::json::path(&mut out, "module_root", &root);
    out.push_str(",\"mode\":"); string(&mut out, mode);
    out.push_str(",\"entry\":"); if entry.is_empty() { out.push_str("null"); } else { string(&mut out, entry); }
    out.push_str(",\"function\":"); if let Some(function) = function { string(&mut out, function); } else { out.push_str("null"); }
    out.push_str(",\"stage\":"); if let Some(stage) = stage { write!(&mut out, "{stage}").unwrap(); } else { out.push_str("null"); }
    out.push_str(",\"imports\":[");
    for (i, path) in import_paths.iter().enumerate() {
        if i != 0 { out.push(','); }
        if let Some(path) = path.to_str() { string(&mut out, path); } else { out.push_str("null"); }
    }
    out.push_str("],\"imports_encoded_bytes\":[");
    for (i, path) in import_paths.iter().enumerate() {
        if i != 0 { out.push(','); }
        crate::json::bytes(&mut out, path.as_os_str().as_encoded_bytes());
    }
    out.push_str("],\"instances\":[");
    for (i, value) in instances.iter().enumerate() { if i != 0 { out.push(','); } string(&mut out, value); }
    write!(&mut out, "],\"ir\":{value}}}").unwrap();
    println!("{out}");
    Ok(())
}
