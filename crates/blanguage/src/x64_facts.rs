//! A file/range adapter; it does not infer instruction boundaries from a container format.
use super::{Argument, CompileError};
use std::{ffi::OsString, fmt::Write, path::Path};

#[derive(Clone, Copy)]
#[repr(C)]
struct X64CommandLine {
    error: CompileError,
    error_index: usize,
    ip: u64,
    offset: u64,
    length: u64,
    has_length: u64,
}

#[repr(C)]
struct X64Range {
    end: usize,
    start_ip: u64,
    error: CompileError,
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::parse_x64_arguments"]
    fn parse_x64_arguments(arguments: *const Argument, count: usize) -> X64CommandLine;
    #[link_name = "bootstrap.cli::resolve_x64_range"]
    fn resolve_x64_range(line: X64CommandLine, file_size: usize) -> X64Range;
}

pub(super) fn print(args: &[OsString], arguments: &[Argument]) -> Result<(), Box<dyn std::error::Error>> {
    let path = blanguage::canonicalize_source_path(Path::new(args.get(1).ok_or_else(|| CompileError::X64_USAGE.message(args, 0))?))?;
    let line = unsafe { parse_x64_arguments(arguments.as_ptr(), arguments.len()) };
    line.error.result(args, line.error_index)?;
    let raw = std::fs::read(&path)?;
    let range = unsafe { resolve_x64_range(line, raw.len()) };
    range.error.result(args, line.error_index)?;
    let offset = line.offset as usize;
    // On success B returns an end in offset..=raw.len() for this same buffer.
    let bytes = unsafe { raw.get_unchecked(offset..range.end) };
    let length = bytes.len();
    let ip = line.ip;
    let decode = blanguage::native::instructions_json(bytes, range.start_ip)?;
    let mut out = String::from("{\"kind\":\"x64_query\",");
    crate::json::path(&mut out, "source_path", &path);
    write!(&mut out, ",\"file_base_ip\":\"0x{ip:016x}\",\"offset_bytes\":{offset},\"length_bytes\":{length},\"decode\":{decode}}}").unwrap();
    println!("{out}"); Ok(())
}
