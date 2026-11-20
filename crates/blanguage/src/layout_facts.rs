//! A selected static type/field query through the existing compiler owner.
use super::{Argument, CompileError};
use crate::json::string;
use std::{ffi::OsString, fmt::Write, path::Path};

pub(super) const USAGE: &str = "用法：bc layout <module.h|module.b> <type> [-I module-root] [--field path]";

#[repr(C)]
struct LayoutCommandLine {
    error: CompileError,
    error_index: usize,
    root_index: usize,
    field_index: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::parse_layout_arguments"]
    fn parse_layout_arguments(arguments: *const Argument, count: usize) -> LayoutCommandLine;
}

pub(super) fn print(
    compiler: &blanguage::Compiler, args: &[OsString], arguments: &[Argument],
) -> Result<(), Box<dyn std::error::Error>> {
    let path = blanguage::canonicalize_source_path(Path::new(args.get(1).ok_or(USAGE)?))?;
    let line = unsafe { parse_layout_arguments(arguments.as_ptr(), arguments.len()) };
    line.error.result(args, line.error_index)?;
    // The B parser has checked that the type operand and selected field are text.
    let text = |index: usize| unsafe { std::str::from_utf8_unchecked(args[index].as_encoded_bytes()) };
    let request = text(2);
    let field = (line.field_index != args.len()).then(|| text(line.field_index));
    let explicit_root;
    let root: &Path = match args.get(line.root_index) {
        Some(root) => {
            explicit_root = blanguage::canonicalize_source_path(Path::new(root))?;
            &explicit_root
        }
        None => path.parent().ok_or(USAGE)?,
    };
    let (root_type, offset, layout) = compiler.type_layout_json(&path, root, request, field)?;
    let mut out = String::from("{\"kind\":\"type_layout\",\"phase\":\"checked_type\",");
    crate::json::path(&mut out, "source_path", &path);
    out.push(','); crate::json::path(&mut out, "module_root", root);
    out.push_str(",\"request\":"); string(&mut out, request);
    out.push_str(",\"root_type\":"); string(&mut out, &root_type);
    out.push_str(",\"field\":");
    if let Some(field) = field { string(&mut out, field); }
    else { out.push_str("null"); }
    write!(&mut out, ",\"offset_bytes\":{offset},\"layout\":{} }}", layout).unwrap();
    println!("{out}");
    Ok(())
}
