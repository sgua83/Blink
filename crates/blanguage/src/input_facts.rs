//! AI query of the artifact's original input record; no Compiler or source loading.
use blanguage::build::{Declaration, InputFacts, Selection};
use std::{fmt::Write, path::Path};
use crate::json::string;
fn bytes(out: &mut String, value: &[u8]) {
    out.push('"');
    for byte in value { write!(out, "{byte:02x}").unwrap(); }
    out.push('"');
}
fn path(out: &mut String, value: &[u8]) {
    out.push_str("{\"text\":");
    match std::str::from_utf8(value) {
        Ok(value) => string(out, value),
        Err(_) => out.push_str("null"),
    }
    out.push_str(",\"encoded_bytes\":");
    bytes(out, value);
    out.push('}');
}
fn encode(input: &InputFacts) -> String {
    let value = &input.source;
    let mut out = String::from("{\"kind\":\"build_inputs\",\"path_encoding\":\"rust_os_str_encoded_bytes\",\"input\":");
    path(&mut out, &value.path);
    out.push_str(",\"root\":"); path(&mut out, &value.root);
    out.push_str(",\"working_directory\":"); path(&mut out, &value.working_directory);
    out.push_str(",\"module\":"); string(&mut out, &value.module);
    out.push_str(",\"compiler\":"); string(&mut out, &value.compiler);
    write!(&mut out, ",\"max_parameter_bytes\":{},\"selection\":{{", value.max_parameter_bytes).unwrap();
    match &value.selection {
        Selection::Application(entry) => {
            out.push_str("\"kind\":\"application\",\"entry\":"); string(&mut out, entry);
        }
        Selection::Module(instances) => {
            out.push_str("\"kind\":\"module\",\"instances\":[");
            for (index, instance) in instances.iter().enumerate() {
                if index != 0 { out.push(','); }
                string(&mut out, instance);
            }
            out.push(']');
        }
        Selection::Library { entry, prefix } => {
            out.push_str("\"kind\":\"algorithm_library\",\"entry\":"); string(&mut out, entry);
            out.push_str(",\"prefix\":"); string(&mut out, prefix);
        }
        Selection::Entry { entry, target } => {
            out.push_str("\"kind\":\"entry_image\",\"entry\":"); string(&mut out, entry);
            out.push_str(",\"target\":"); string(&mut out, target);
        }
    }
    out.push_str("},\"sources\":[");
    for (index, source) in value.sources.iter().enumerate() {
        if index != 0 { out.push(','); }
        out.push_str("{\"module\":"); string(&mut out, &source.module);
        out.push_str(",\"path\":"); path(&mut out, &source.path);
        out.push_str(",\"scope\":"); string(&mut out, &source.scope);
        out.push_str(",\"fingerprint\":"); bytes(&mut out, &source.fingerprint);
        out.push('}');
    }
    out.push_str("],\"public_execution_requirements\":[");
    for (index, (name, requirement)) in value.requirements.iter().enumerate() {
        if index != 0 { out.push(','); }
        out.push_str("{\"name\":"); string(&mut out, name);
        out.push_str(",\"requirement\":"); string(&mut out, requirement);
        out.push('}');
    }
    out.push_str("],\"imports_fingerprint\":"); bytes(&mut out, &input.imports);
    out.push_str(",\"native_fingerprint\":");
    match &input.native {
        Some(value) => bytes(&mut out, value),
        None => out.push_str("null"),
    }
    out.push_str(",\"declarations\":[");
    for (index, declaration) in input.declarations.iter().enumerate() {
        if index != 0 { out.push(','); }
        let (kind, name, context) = match declaration {
            Declaration::Constant(name) => ("constant", name, None),
            Declaration::Record(name) => ("record", name, None),
            Declaration::TypeSet(name) => ("type_set", name, None),
            Declaration::External(name) => ("external", name, None),
            Declaration::Implementation(name) => ("implementation", name, None),
            Declaration::ValueName { name, module, imports } => ("value_name", name, Some((module, imports))),
            Declaration::RecordName { name, module, imports } => ("record_name", name, Some((module, imports))),
            Declaration::TypeSetName { name, module, imports } => ("type_set_name", name, Some((module, imports))),
        };
        out.push_str("{\"kind\":"); string(&mut out, kind);
        out.push_str(",\"name\":"); string(&mut out, name);
        if let Some((module, imports)) = context {
            out.push_str(",\"module\":"); string(&mut out, module);
            out.push_str(",\"imports\":[");
            for (index, import) in imports.iter().enumerate() {
                if index != 0 { out.push(','); }
                string(&mut out, import);
            }
            out.push(']');
        }
        out.push('}');
    }
    out.push_str("]}");
    out
}
pub fn print(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let value = blanguage::build::input_facts(path)
        .map_err(|error| -> Box<dyn std::error::Error> { error })?;
    println!("{}", encode(&value));
    Ok(())
}
