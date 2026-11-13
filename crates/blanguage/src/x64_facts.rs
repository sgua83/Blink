//! A file/range adapter; it does not infer instruction boundaries from a container format.
use super::{Argument, CompileError};
use std::{ffi::OsString, fmt::Write, path::PathBuf};

pub(super) const USAGE: &str = "用法：bc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]";

#[repr(C)]
struct X64CommandLine {
    error: CompileError,
    error_index: usize,
    ip: u64,
    offset: u64,
    length: u64,
    has_length: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::parse_x64_arguments"]
    fn parse_x64_arguments(arguments: *const Argument, count: usize) -> X64CommandLine;
}

pub(super) fn print(args: &[OsString], arguments: &[Argument]) -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(args.get(1).ok_or(USAGE)?).canonicalize()?;
    let line = unsafe { parse_x64_arguments(arguments.as_ptr(), arguments.len()) };
    line.error.result(args, line.error_index)?;
    let raw = std::fs::read(&path)?;
    let offset = usize::try_from(line.offset)?;
    let length = if line.has_length != 0 { usize::try_from(line.length)? }
        else { raw.len().checked_sub(offset).ok_or("指令范围起点超出文件")? };
    let end = offset.checked_add(length).ok_or("指令范围溢出")?;
    let bytes = raw.get(offset..end).ok_or("指令范围超出文件")?;
    let ip = line.ip;
    let decode = blanguage::native::instructions_json(bytes, ip.wrapping_add(offset as u64))?;
    let mut out = String::from("{\"kind\":\"x64_query\",");
    crate::json::path(&mut out, "source_path", &path);
    write!(&mut out, ",\"file_base_ip\":\"0x{ip:016x}\",\"offset_bytes\":{offset},\"length_bytes\":{length},\"decode\":{decode}}}").unwrap();
    println!("{out}"); Ok(())
}
