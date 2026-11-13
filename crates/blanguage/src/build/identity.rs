//! 构建输入只交接已消费的声明与实际实现；COFF和入口映像共用同一编码。
use super::{ByteField, hash, Result};
use crate::module::codec::{Decode, Encode, record, variants};
use std::{collections::BTreeMap, io::Error, path::Path};
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
impl SourceInputs {
    pub(super) fn same_code(&self, other: &Self) -> bool {
        self.path == other.path && self.root == other.root
            && self.module == other.module && self.compiler == other.compiler
            && self.max_parameter_bytes == other.max_parameter_bytes
            && self.selection == other.selection && self.sources == other.sources
            && self.requirements == other.requirements
    }
}
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
                let span = declaration.function.span();
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
            Declaration::ValueName { name, module, imports } | Declaration::RecordName { name, module, imports } => {
                match declarations.resolve(name, module, imports, matches!(entry, Declaration::ValueName { .. })) {
                    crate::frontend::NameResolution::Missing => hash::field(&mut bytes, &None::<String>.bytes()),
                    crate::frontend::NameResolution::Unique(name) => hash::field(&mut bytes, &Some(name).bytes()),
                    crate::frontend::NameResolution::Ambiguous => hash::field(&mut bytes, &[2]),
                }
                continue;
            }
            Declaration::TypeSetName { name, module, imports } => {
                hash::field(&mut bytes, &declarations.resolve_type_set(name, module, imports).bytes());
                continue;
            }
            Declaration::Constant(key) => declarations.functions.get(key)
                .filter(|definition| definition.function.kind() == crate::ast::FunctionKind::Constant)
                .map(|definition| (&definition.source, definition.function.span())),
            Declaration::Record(key) => declarations.records.get(key)
                .map(|(record, source, ..)| (source, record.span())),
            Declaration::TypeSet(key) => declarations.type_sets.get(key)
                .map(|(set, source, ..)| (source, set.span())),
            Declaration::External(key) => declarations.functions.get(key)
                .filter(|definition| definition.function.external().is_some())
                .map(|definition| (&definition.source, definition.function.span())),
            Declaration::Implementation(key) => declarations.functions.get(key)
                .filter(|definition| definition.function.body().is_some())
                .map(|definition| {
                    hash::field(&mut bytes, &[definition.execution as u8]);
                    (&definition.source, definition.function.span())
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

struct SourceRegions<'a> {
    preamble_end: Option<usize>,
    include_signatures: bool,
    signatures: Vec<(&'a str, std::ops::Range<usize>)>,
}

impl<'a> SourceRegions<'a> {
    fn for_sources(declarations: &'a crate::Declarations, paths: impl Iterator<Item = (&'a Path, bool)>)
        -> BTreeMap<&'a Path, Self>
    {
        let mut regions = BTreeMap::<&Path, Self>::new();
        for (path, implementation) in paths {
            let region = regions.entry(path).or_insert_with(|| Self {
                preamble_end: None, include_signatures: false, signatures: Vec::new(),
            });
            region.include_signatures |= implementation;
        }
        for (name, definition) in &declarations.functions {
            if let Some(region) = regions.get_mut(definition.source.path.as_path()) {
                let start = definition.function.span().start;
                region.include(start);
                if region.include_signatures && definition.function.kind() != crate::ast::FunctionKind::Constant
                    && definition.function.external().is_none()
                {
                    region.signatures.push((name, start..definition.function.signature_end()));
                }
            }
        }
        for (record, owner, ..) in declarations.records.values() {
            if let Some(region) = regions.get_mut(owner.path.as_path()) {
                region.include(record.span().start);
            }
        }
        for (set, owner, ..) in declarations.type_sets.values() {
            if let Some(region) = regions.get_mut(owner.path.as_path()) {
                region.include(set.span().start);
            }
        }
        regions
    }

    fn include(&mut self, start: usize) {
        self.preamble_end = Some(self.preamble_end.map_or(start, |end| end.min(start)));
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.metadata::encode_build_fields"]
    fn encode_build_fields(fields: *const ByteField, count: usize, output: *mut u8);
}

fn push_field(fields: &mut Vec<ByteField>, value: &str) {
    fields.push(value.as_bytes().into());
}

pub(super) fn sources(path: &std::path::Path, root: &std::path::Path,
    selection: Selection, max_parameter_bytes: u32, world: &crate::World,
    module: &str, exports: bool,
) -> Result<SourceInputs> {
    let source_rows = || world.interfaces.iter().map(|(name, module)| (name, &module.source, false))
        .chain(world.sources.iter().filter(|(name, source)| source.path != world.interfaces[*name].source.path)
            .map(|(name, source)| (name, source, true)));
    let regions = SourceRegions::for_sources(&world.declarations, source_rows()
        .filter(|(name, _, implementation)| *name != module || *implementation)
        .map(|(_, source, implementation)| (source.path.as_path(), implementation)));
    let mut sources = Vec::new();
    let mut fields = Vec::<ByteField>::new();
    let mut bytes = Vec::<u8>::new();
    for (name, source, implementation) in source_rows() {
        let partial = name != module || implementation;
        fields.clear();
        if partial {
            let region = &regions[source.path.as_path()];
            let end = region.preamble_end.unwrap_or(source.text.len());
            push_field(&mut fields, source.text[..end].trim_end());
            if implementation {
                // Every ordinary signature is checked, including unused open templates.
                // Body dependencies remain in the actual consumed-declaration fingerprint.
                for (name, range) in &region.signatures {
                    push_field(&mut fields, name);
                    push_field(&mut fields, source.text[range.clone()].trim_end());
                }
            }
        } else {
            push_field(&mut fields, &source.text);
        }
        let length = fields.iter().map(|field| size_of::<u64>() + field.length).sum();
        bytes.clear();
        bytes.reserve(length);
        unsafe {
            encode_build_fields(fields.as_ptr(), fields.len(), bytes.as_mut_ptr());
            bytes.set_len(length);
        }
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
