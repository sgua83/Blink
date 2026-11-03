//! 构建输入只交接已消费的声明与实际实现；COFF和入口映像共用同一编码。
use super::{hash, Result};
use crate::module::codec::{Decode, Encode, record, variants};
use std::io::Error;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Declaration {
    Constant(String),
    Record(String),
    TypeSet(String),
    External(String),
    Implementation(String),
    ValueName { name: String, module: String, imports: Vec<String> },
    RecordName { name: String, module: String, imports: Vec<String> },
    TypeSetName { name: String, module: String, imports: Vec<String> },
}
variants!(Declaration { 0 => Constant(name), 1 => Record(name), 2 => TypeSet(name), 3 => External(name), 4 => ValueName { name, module, imports }, 5 => RecordName { name, module, imports }, 6 => TypeSetName { name, module, imports }, 7 => Implementation(name) });
const MAGIC: &[u8; 8] = b"BLBO\x0c\0\0\0";
/// The original build request, not a target inferred from a file extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Application(String),
    Module(Vec<String>),
    Library { entry: String, prefix: String },
    Entry { entry: String, target: String },
}
variants!(Selection { 0 => Application(entry), 1 => Module(instances), 2 => Library { entry, prefix }, 3 => Entry { entry, target } });
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceInput {
    pub module: String,
    /// Native OsStr encoding, preserved without lossy Unicode replacement.
    pub path: Vec<u8>,
    pub scope: String,
    /// Hash of the source portion named by scope, not necessarily the full file.
    pub fingerprint: [u8; 32],
}
record!(SourceInput { module, path, scope, fingerprint });
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceInputs {
    pub path: Vec<u8>,
    pub root: Vec<u8>,
    pub working_directory: Vec<u8>,
    pub module: String,
    pub compiler: String,
    pub max_parameter_bytes: u32,
    pub selection: Selection,
    pub sources: Vec<SourceInput>,
    pub requirements: Vec<(String, String)>,
}
record!(SourceInputs { path, root, working_directory, module, compiler, max_parameter_bytes, selection, sources, requirements });
pub struct Inputs {
    pub source: SourceInputs,
    pub imports: [u8; 32],
    pub native: Option<[u8; 32]>,
    pub declarations: Vec<Declaration>,
}
record!(Inputs { source, imports, native, declarations });
pub(super) fn references<'a>(declarations: &crate::Declarations, imported: Option<&crate::module::Imports>, symbols: impl IntoIterator<Item = &'a str>, used: &[Declaration]) -> [u8; 32] {
    let mut bytes = Vec::new();
    for symbol in symbols {
        hash::field(&mut bytes, symbol.as_bytes());
        // 实例与域入口复用原源声明，名字规则来自generics::key／domain::symbol。
        let end = symbol.find(['<', '#']).unwrap_or(symbol.len());
        let declaration = declarations.functions.get(&symbol[..end]);
        let input = imported.and_then(|imports| imports.modules.values()
            .find(|input| input.definitions.contains_key(symbol)));
        match (input, declaration) {
            (Some(input), Some(declaration)) => {
                hash::field(&mut bytes, &[1]);
                hash::field(&mut bytes, &input.definitions[symbol]);
                let span = declaration.function.span;
                hash::field(&mut bytes, declaration.source.text[span.start..span.end].as_bytes());
                match input.requirements.get(&symbol[..end]) {
                    Some(requirement) => hash::field(&mut bytes, &[1, match requirement {
                        crate::execution::Requirement::Invocation => 0,
                        crate::execution::Requirement::Domain => 1,
                        crate::execution::Requirement::Host => 2,
                    }]),
                    None => hash::field(&mut bytes, &[0]),
                }
            }
            _ => hash::field(&mut bytes, &[0]),
        }
    }
    hash::field(&mut bytes, &(used.len() as u64).to_le_bytes());
    for entry in used {
        hash::field(&mut bytes, &entry.bytes());
        let input = match entry {
            Declaration::ValueName { name, module, imports } => {
                hash::field(&mut bytes, &declarations.resolve(name, module, imports, true).bytes());
                continue;
            }
            Declaration::RecordName { name, module, imports } => {
                hash::field(&mut bytes, &declarations.resolve(name, module, imports, false).bytes());
                continue;
            }
            Declaration::TypeSetName { name, module, imports } => {
                hash::field(&mut bytes, &declarations.resolve_type_set(name, module, imports).bytes());
                continue;
            }
            Declaration::Constant(key) => declarations.functions.get(key)
                .filter(|definition| definition.function.kind == crate::ast::FunctionKind::Constant)
                .map(|definition| (&definition.source, definition.function.span)),
            Declaration::Record(key) => declarations.records.get(key)
                .map(|(record, source, ..)| (source, record.span)),
            Declaration::TypeSet(key) => declarations.type_sets.get(key)
                .map(|(set, source, ..)| (source, set.span)),
            Declaration::External(key) => declarations.functions.get(key)
                .filter(|definition| definition.function.external.is_some())
                .map(|definition| (&definition.source, definition.function.span)),
            Declaration::Implementation(key) => declarations.functions.get(key)
                .filter(|definition| definition.function.body.is_some())
                .map(|definition| {
                    hash::field(&mut bytes, &[definition.execution as u8]);
                    (&definition.source, definition.function.span)
                }),
        };
        match input {
            Some((source, span)) => {
                hash::field(&mut bytes, &[1]);
                hash::field(&mut bytes, source.text[span.start..span.end].as_bytes());
            }
            None => hash::field(&mut bytes, &[0]),
        }
    }
    hash::digest(&bytes)
}

fn source_identity(bytes: &mut Vec<u8>, declarations: &crate::Declarations, source: &crate::ast::Source,
    partial: bool, signatures: bool,
) {
    if !partial {
        hash::field(bytes, source.text.as_bytes());
        return;
    }
    let mut spans = declarations.functions.values()
        .filter(|definition| definition.source.path == source.path)
        .map(|definition| definition.function.span).collect::<Vec<_>>();
    for (record, ..) in declarations.records.values().filter(|(_, owner, ..)| owner.path == source.path) {
        spans.push(record.span);
    }
    for (set, ..) in declarations.type_sets.values().filter(|(_, owner, ..)| owner.path == source.path) {
        spans.push(set.span);
    }
    let end = spans.iter().map(|span| span.start).min().unwrap_or(source.text.len());
    hash::field(bytes, source.text[..end].trim_end().as_bytes());
    if signatures {
        // Every source declaration is checked, including unused open templates;
        // its implementation is consumed only by actual instances or body checks.
        for (name, definition) in declarations.functions.iter().filter(|(_, definition)| {
            definition.source.path == source.path && definition.function.kind != crate::ast::FunctionKind::Constant
                && definition.function.external.is_none()
        }) {
            let function = &definition.function;
            let parameters = function.params.iter().map(|parameter|
                (&parameter.name, &parameter.ty, parameter.access)).collect::<Vec<_>>();
            let contracts = function.contracts.iter().map(|contract|
                (contract.kind, &contract.predicate)).collect::<std::collections::BTreeSet<_>>();
            let signature = format!("{:?}", (function.kind, function.c_abi, &function.types,
                &function.values, &function.result, function.target, parameters, contracts));
            hash::field(bytes, name.as_bytes());
            hash::field(bytes, signature.as_bytes());
        }
    }
}
pub(super) fn sources(path: &std::path::Path, root: &std::path::Path,
    selection: Selection, max_parameter_bytes: u32, world: &crate::World,
    module: &str, exports: bool,
) -> Result<SourceInputs> {
    let mut sources = Vec::new();
    for (name, source) in world.interfaces.iter().map(|(name, module)| (name, &module.source))
        .chain(world.sources.iter().filter(|(name, source)| source.path != world.interfaces[*name].source.path))
    {
        let implementation = source.path != world.interfaces[name].source.path;
        let partial = name != module || implementation;
        let mut bytes = Vec::new();
        source_identity(&mut bytes, &world.declarations, source, partial, implementation);
        sources.push(SourceInput {
            module: name.clone(), path: source.path.as_os_str().as_encoded_bytes().to_vec(),
            scope: if !partial { "full_source" } else if implementation {
                "preamble_and_function_signatures"
            } else { "preamble" }.into(),
            fingerprint: hash::digest(&bytes),
        });
    }
    let requirements = if exports {
        // A module owns the inferred participation contract of every public declaration,
        // including open templates whose concrete machine code is not exported here.
        world.functions.iter().filter(|(_, definition)| definition.module == module && definition.public)
            .map(|(name, definition)| (name.clone(), match definition.execution {
                crate::execution::Requirement::Invocation => "invocation",
                crate::execution::Requirement::Domain => "domain",
                crate::execution::Requirement::Host => "host",
            }.into())).collect()
    } else { Vec::new() };
    Ok(SourceInputs {
        path: path.as_os_str().as_encoded_bytes().to_vec(),
        root: root.as_os_str().as_encoded_bytes().to_vec(),
        working_directory: std::env::current_dir()?.as_os_str().as_encoded_bytes().to_vec(),
        module: module.into(),
        compiler: env!("BLINK_COMPILER_ID").into(), max_parameter_bytes,
        selection, sources, requirements,
    })
}
pub(super) fn decode(bytes: &[u8]) -> Result<Option<Inputs>> {
    bytes.strip_prefix(MAGIC).map(Inputs::from_bytes).transpose()
        .map_err(|value| Error::other(value.to_string()).into())
}
impl Inputs {
    pub(super) fn encode(&self) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec(); bytes.extend(self.bytes()); bytes
    }
}
