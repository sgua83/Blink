//! A selected static type/field query through the existing compiler owner.
use crate::json::string;
use std::{fmt::Write, path::PathBuf};

pub fn print(compiler: &blanguage::Compiler, args: &[std::ffi::OsString]) -> Result<(), Box<dyn std::error::Error>> {
    let usage = "用法：bc layout <module.h|module.b> <type> [-I module-root] [--field path]";
    let path = PathBuf::from(args.get(1).ok_or(usage)?).canonicalize()?;
    let request = args.get(2).ok_or(usage)?.to_str().ok_or(usage)?;
    let mut root = path.parent().unwrap().to_path_buf();
    let mut field = None;
    let mut i = 3;
    while i < args.len() {
        let value = args.get(i + 1).ok_or(usage)?;
        match args[i].to_str().ok_or(usage)? {
            "-I" => root = PathBuf::from(value),
            "--field" => field = Some(value.to_str().ok_or(usage)?),
            other => return Err(format!("未知布局选项 {other}；{usage}").into()),
        }
        i += 2;
    }
    let root = root.canonicalize()?;
    let (root_type, offset, layout) = compiler.type_layout_json(&path, &root, request, field)?;
    let mut out = String::from("{\"kind\":\"type_layout\",\"phase\":\"checked_type\",");
    crate::json::path(&mut out, "source_path", &path);
    out.push(','); crate::json::path(&mut out, "module_root", &root);
    out.push_str(",\"request\":"); string(&mut out, request);
    out.push_str(",\"root_type\":"); string(&mut out, &root_type);
    out.push_str(",\"field\":");
    if let Some(field) = field { string(&mut out, field); }
    else { out.push_str("null"); }
    write!(&mut out, ",\"offset_bytes\":{offset},\"layout\":{} }}", layout).unwrap();
    println!("{out}");
    Ok(())
}
