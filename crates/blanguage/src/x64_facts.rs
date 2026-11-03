//! A file/range adapter; it does not infer instruction boundaries from a container format.
use crate::json::string;
use std::{path::PathBuf, fmt::Write};

fn integer(value: &str) -> Result<u64, std::num::ParseIntError> {
    if let Some(value) = value.strip_prefix("0x") { u64::from_str_radix(value, 16) }
    else { value.parse() }
}
pub fn print(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let usage = "用法：bc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]";
    let path = PathBuf::from(args.get(1).ok_or(usage)?).canonicalize()?;
    let mut ip = None; let mut offset = None; let mut length = None;
    let mut i = 2;
    while i < args.len() {
        let value = args.get(i + 1).ok_or(usage)?;
        let selected = match args[i].as_str() {
            "--ip" => &mut ip, "--offset" => &mut offset, "--length" => &mut length,
            other => return Err(format!("未知x64查询选项 {other}；{usage}").into()),
        };
        if selected.replace(integer(value)?).is_some() { return Err(format!("{}不能重复指定", args[i]).into()); }
        i += 2;
    }
    let raw = std::fs::read(&path)?;
    let offset = usize::try_from(offset.unwrap_or(0))?;
    let length = if let Some(length) = length { usize::try_from(length)? }
        else { raw.len().checked_sub(offset).ok_or("指令范围起点超出文件")? };
    let end = offset.checked_add(length).ok_or("指令范围溢出")?;
    let bytes = raw.get(offset..end).ok_or("指令范围超出文件")?;
    let ip = ip.unwrap_or(0);
    let decode = blanguage::native::instructions_json(bytes, ip.wrapping_add(offset as u64))?;
    let mut out = String::from("{\"kind\":\"x64_query\",\"source_path\":");
    string(&mut out, path.to_str().ok_or("指令文件路径不是UTF-8")?);
    write!(&mut out, ",\"file_base_ip\":\"0x{ip:016x}\",\"offset_bytes\":{offset},\"length_bytes\":{length},\"decode\":{decode}}}").unwrap();
    println!("{out}"); Ok(())
}
