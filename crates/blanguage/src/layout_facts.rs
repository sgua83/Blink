//! A selected static type/field query through the existing compiler owner.
use crate::json::string;
use std::{fmt::Write, path::PathBuf};

pub fn print(compiler: &blanguage::Compiler, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let usage = "用法：bc layout <module.h|module.b> <type> [-I module-root] [--field path]";
    let path = PathBuf::from(args.get(1).ok_or(usage)?).canonicalize()?;
    let request = args.get(2).ok_or(usage)?;
    let mut root = path.parent().unwrap().to_path_buf();
    let mut field = None;
    let mut i = 3;
    while i < args.len() {
        let value = args.get(i + 1).ok_or(usage)?;
        match args[i].as_str() {
            "-I" => root = PathBuf::from(value),
            "--field" => field = Some(value.as_str()),
            other => return Err(format!("未知布局选项 {other}；{usage}").into()),
        }
        i += 2;
    }
    let root = root.canonicalize()?;
    let value = compiler.type_layout(&path, &root, request)?;
    let (offset, selected) = if let Some(field) = field {
        value.field(field).ok_or_else(|| format!("类型 {} 没有可定位的静态字段路径 {field}", value.name))?
    } else { (0, &value) };
    let mut out = String::from("{\"kind\":\"type_layout\",\"phase\":\"checked_type\",\"source_path\":");
    string(&mut out, path.to_str().ok_or("源码路径不是UTF-8")?);
    out.push_str(",\"module_root\":"); string(&mut out, root.to_str().ok_or("模块根不是UTF-8")?);
    out.push_str(",\"request\":"); string(&mut out, request);
    out.push_str(",\"root_type\":"); string(&mut out, &value.name);
    out.push_str(",\"field\":");
    if let Some(field) = field { string(&mut out, field); }
    else { out.push_str("null"); }
    write!(&mut out, ",\"offset_bytes\":{offset},\"layout\":{} }}", selected.layout_json()).unwrap();
    println!("{out}");
    Ok(())
}
