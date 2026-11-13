//! 构建输入只交接已消费的声明与实际实现；COFF和入口映像共用同一编码。
use super::{ByteField, hash, Result};
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
impl Declaration {
    fn fields(&self) -> (u32, &str, Option<(&str, &[String])>) {
        match self {
            Self::Constant(name) => (0, name, None),
            Self::Record(name) => (1, name, None),
            Self::TypeSet(name) => (2, name, None),
            Self::External(name) => (3, name, None),
            Self::ValueName { name, module, imports } => (4, name, Some((module, imports))),
            Self::RecordName { name, module, imports } => (5, name, Some((module, imports))),
            Self::TypeSetName { name, module, imports } => (6, name, Some((module, imports))),
            Self::Implementation(name) => (7, name, None),
        }
    }
}

const MAGIC: &[u8; 8] = b"BLBO\x0c\0\0\0";
/// The original build request, not a target inferred from a file extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Application(String),
    Module(Vec<String>),
    Library { entry: String, prefix: String },
    Entry { entry: String, target: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceInput {
    pub module: String,
    /// Native OsStr encoding, preserved without lossy Unicode replacement.
    pub path: Vec<u8>,
    pub scope: String,
    /// Hash of the source portion named by scope, not necessarily the full file.
    pub fingerprint: [u8; 32],
}
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
#[repr(C)]
struct BuildSymbol {
    name: ByteField,
    contract: ByteField,
    declaration: ByteField,
    bound: u32,
    requirement: u32,
}

#[repr(u32)]
enum BuildReferenceKind {
    Missing,
    Source,
    Query,
    Implementation,
}

#[repr(u32)]
enum NameQuery {
    Missing,
    Unique,
    Ambiguous,
}

#[repr(C)]
struct BuildReference {
    name: ByteField,
    module_name: ByteField,
    value: ByteField,
    imports: *const ByteField,
    import_count: usize,
    tag: u32,
    kind: BuildReferenceKind,
    execution: u32,
    resolution: NameQuery,
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_digest::sha256_reference_symbols"]
    fn sha256_reference_symbols(
        digest: &mut hash::Digest, symbols: *const BuildSymbol, count: usize, reference_count: usize,
    );
    #[link_name = "bootstrap.build_digest::sha256_reference"]
    fn sha256_reference(digest: &mut hash::Digest, row: BuildReference);
}

fn build_symbol(declarations: &crate::Declarations, imported: Option<&crate::module::Imports>, symbol: &str)
    -> BuildSymbol
{
    let empty = &[][..];
    let mut result = BuildSymbol {
        name: symbol.as_bytes().into(), contract: empty.into(), declaration: empty.into(),
        bound: 0, requirement: 0,
    };
    // Instances and domain entry points use the original declaration name.
    let end = symbol.find(['<', '#']).unwrap_or(symbol.len());
    let name = &symbol[..end];
    let declaration = declarations.functions.get(name);
    let input = imported.and_then(|imports| imports.modules.values().find_map(|input| {
        input.definitions.get(symbol).map(|contract| (input, contract))
    }));
    if let (Some(declaration), Some((input, contract))) = (declaration, input) {
        let span = declaration.function.span();
        result.contract = contract.as_slice().into();
        result.declaration = declaration.source.text[span.start..span.end].as_bytes().into();
        result.bound = 1;
        result.requirement = match input.requirements.get(name) {
            None => 0,
            Some(crate::execution::Requirement::Invocation) => 1,
            Some(crate::execution::Requirement::Domain) => 2,
            Some(crate::execution::Requirement::Host) => 3,
        };
    }
    result
}

fn digest_reference(
    digest: &mut hash::Digest, declarations: &crate::Declarations, entry: &Declaration,
    import_fields: &mut Vec<ByteField>,
) {
    use crate::frontend::NameResolution;
    let (tag, name, query) = entry.fields();
    let mut row = BuildReference {
        name: name.as_bytes().into(),
        module_name: (&[][..]).into(),
        value: (&[][..]).into(),
        imports: std::ptr::null(),
        import_count: 0,
        tag,
        kind: BuildReferenceKind::Missing,
        execution: 0,
        resolution: NameQuery::Missing,
    };
    if let Some((module, imports)) = query {
        import_fields.clear();
        import_fields.extend(imports.iter().map(|name| ByteField::from(name.as_bytes())));
        row.kind = BuildReferenceKind::Query;
        row.module_name = module.as_bytes().into();
        row.imports = import_fields.as_ptr();
        row.import_count = import_fields.len();
        let resolved = if matches!(entry, Declaration::TypeSetName { .. }) {
            declarations.resolve_type_set(name, module, imports)
                .map_or(NameResolution::Missing, NameResolution::Unique)
        } else {
            declarations.resolve(name, module, imports, matches!(entry, Declaration::ValueName { .. }))
        };
        row.resolution = match &resolved {
            NameResolution::Missing => NameQuery::Missing,
            NameResolution::Unique(name) => {
                row.value = name.as_bytes().into();
                NameQuery::Unique
            }
            NameResolution::Ambiguous => NameQuery::Ambiguous,
        };
        unsafe { sha256_reference(digest, row) };
        return;
    }
    let input = match entry {
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
                row.execution = definition.execution as u32;
                (&definition.source, definition.function.span())
            }),
        Declaration::ValueName { .. } | Declaration::RecordName { .. } | Declaration::TypeSetName { .. } => unreachable!(),
    };
    if let Some((source, span)) = input {
        row.value = source.text[span.start..span.end].as_bytes().into();
        row.kind = if matches!(entry, Declaration::Implementation(_)) {
            BuildReferenceKind::Implementation
        } else {
            BuildReferenceKind::Source
        };
    }
    unsafe { sha256_reference(digest, row) };
}

pub(super) fn references<'a>(declarations: &crate::Declarations, imported: Option<&crate::module::Imports>,
    symbols: impl IntoIterator<Item = &'a str>, used: &[Declaration],
) -> [u8; 32] {
    let symbols: Vec<_> = symbols.into_iter()
        .map(|symbol| build_symbol(declarations, imported, symbol)).collect();
    let mut digest = hash::Digest::new();
    unsafe { sha256_reference_symbols(&mut digest, symbols.as_ptr(), symbols.len(), used.len()) };
    let mut import_fields = Vec::new();
    for entry in used {
        digest_reference(&mut digest, declarations, entry, &mut import_fields);
    }
    digest.finish()
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
        sources.push(SourceInput {
            module: name.clone(), path: source.path.as_os_str().as_encoded_bytes().to_vec(),
            scope: if !partial { "full_source" } else if implementation {
                "preamble_and_function_signatures"
            } else { "preamble" }.into(),
            fingerprint: hash::fields(&fields),
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
#[repr(C)]
struct ManifestCursor {
    input: ByteField,
    position: usize,
}

#[repr(C)]
struct ManifestReadWord {
    value: u64,
    error: u32,
}

#[repr(C)]
struct ManifestReadField {
    value: ByteField,
    error: u32,
}

#[repr(u32)]
enum ManifestFieldKind {
    Bytes,
    Text,
    Digest,
    OptionalDigest,
}

unsafe extern "C" {
    #[link_name = "bootstrap.metadata_read::manifest_body"]
    fn manifest_body(input: ByteField, magic: u64) -> ByteField;
    #[link_name = "bootstrap.metadata_read::manifest_number"]
    fn manifest_number(cursor: &mut ManifestCursor, wide: u32) -> ManifestReadWord;
    #[link_name = "bootstrap.metadata_read::manifest_field"]
    fn manifest_field(cursor: &mut ManifestCursor, kind: ManifestFieldKind) -> ManifestReadField;
}

struct ManifestReader<'a> {
    cursor: ManifestCursor,
    input: std::marker::PhantomData<&'a [u8]>,
}

impl<'a> ManifestReader<'a> {
    fn new(input: &'a [u8]) -> Option<Self> {
        let input = unsafe { manifest_body(input.into(), u64::from_le_bytes(*MAGIC)) };
        (!input.bytes.is_null()).then_some(Self {
            cursor: ManifestCursor { input, position: 0 },
            input: std::marker::PhantomData,
        })
    }

    fn check(error: u32) -> std::io::Result<()> {
        match error {
            0 => Ok(()),
            9 => Err(Error::other("模块接口长度溢出")),
            10 => Err(Error::other("模块布尔值编码错误")),
            _ => Err(Error::other("模块接口被截断")),
        }
    }

    fn slice(&self, bytes: ByteField) -> &'a [u8] {
        // Successful fields borrow the same original snapshot as the cursor.
        unsafe { std::slice::from_raw_parts(bytes.bytes, bytes.length) }
    }

    fn word(&mut self) -> std::io::Result<u32> {
        let row = unsafe { manifest_number(&mut self.cursor, 0) };
        Self::check(row.error)?;
        Ok(row.value as u32)
    }

    fn bytes(&mut self) -> std::io::Result<Vec<u8>> {
        let row = unsafe { manifest_field(&mut self.cursor, ManifestFieldKind::Bytes) };
        Self::check(row.error)?;
        Ok(self.slice(row.value).to_vec())
    }

    fn text(&mut self) -> std::io::Result<String> {
        let row = unsafe { manifest_field(&mut self.cursor, ManifestFieldKind::Text) };
        Self::check(row.error)?;
        std::str::from_utf8(self.slice(row.value)).map(str::to_owned).map_err(Error::other)
    }

    fn list<T>(&mut self, mut read: impl FnMut(&mut Self) -> std::io::Result<T>) -> std::io::Result<Vec<T>> {
        let count = unsafe { manifest_number(&mut self.cursor, 1) };
        Self::check(count.error)?;
        (0..count.value).map(|_| read(self)).collect()
    }

    fn digest(&mut self, kind: ManifestFieldKind) -> std::io::Result<ByteField> {
        let row = unsafe { manifest_field(&mut self.cursor, kind) };
        Self::check(row.error)?;
        Ok(row.value)
    }

    fn required_digest(&mut self) -> std::io::Result<[u8; 32]> {
        let bytes = self.digest(ManifestFieldKind::Digest)?;
        Ok(self.slice(bytes).try_into().unwrap())
    }

    fn native_digest(&mut self) -> std::io::Result<Option<[u8; 32]>> {
        let bytes = self.digest(ManifestFieldKind::OptionalDigest)?;
        Ok((!bytes.bytes.is_null()).then(|| self.slice(bytes).try_into().unwrap()))
    }

    fn declaration(&mut self) -> std::io::Result<Declaration> {
        Ok(match self.word()? {
            0 => Declaration::Constant(self.text()?),
            1 => Declaration::Record(self.text()?),
            2 => Declaration::TypeSet(self.text()?),
            3 => Declaration::External(self.text()?),
            4 => Declaration::ValueName { name: self.text()?, module: self.text()?, imports: self.list(Self::text)? },
            5 => Declaration::RecordName { name: self.text()?, module: self.text()?, imports: self.list(Self::text)? },
            6 => Declaration::TypeSetName { name: self.text()?, module: self.text()?, imports: self.list(Self::text)? },
            7 => Declaration::Implementation(self.text()?),
            _ => return Err(Error::other("模块枚举编码错误：Declaration")),
        })
    }

    fn selection(&mut self) -> std::io::Result<Selection> {
        Ok(match self.word()? {
            0 => Selection::Application(self.text()?),
            1 => Selection::Module(self.list(Self::text)?),
            2 => Selection::Library { entry: self.text()?, prefix: self.text()? },
            3 => Selection::Entry { entry: self.text()?, target: self.text()? },
            _ => return Err(Error::other("模块枚举编码错误：Selection")),
        })
    }

    fn source(&mut self) -> std::io::Result<SourceInput> {
        Ok(SourceInput {
            module: self.text()?,
            path: self.bytes()?,
            scope: self.text()?,
            fingerprint: self.required_digest()?,
        })
    }

    fn inputs(mut self) -> std::io::Result<Inputs> {
        let source = SourceInputs {
            path: self.bytes()?,
            root: self.bytes()?,
            working_directory: self.bytes()?,
            module: self.text()?,
            compiler: self.text()?,
            max_parameter_bytes: self.word()?,
            selection: self.selection()?,
            sources: self.list(Self::source)?,
            requirements: self.list(|reader| Ok((reader.text()?, reader.text()?)))?,
        };
        let inputs = Inputs {
            source,
            imports: self.required_digest()?,
            native: self.native_digest()?,
            declarations: self.list(Self::declaration)?,
        };
        if self.cursor.position != self.cursor.input.length {
            return Err(Error::other("模块接口有未消费内容"));
        }
        Ok(inputs)
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<Option<Inputs>> {
    Ok(ManifestReader::new(bytes).map(ManifestReader::inputs).transpose()?)
}
#[repr(C)]
struct ManifestSelection {
    first: ByteField,
    second: ByteField,
    instances: *const ByteField,
    instance_count: usize,
    tag: u32,
}

#[repr(C)]
struct ManifestSource {
    module_name: ByteField,
    path: ByteField,
    scope: ByteField,
    fingerprint: *const u8,
}

#[repr(C)]
struct ManifestRequirement {
    name: ByteField,
    kind: ByteField,
}

#[repr(C)]
struct ManifestDeclaration {
    name: ByteField,
    module_name: ByteField,
    import_start: usize,
    import_count: usize,
    tag: u32,
}

#[repr(C)]
struct BuildManifest {
    path: ByteField,
    root: ByteField,
    working_directory: ByteField,
    module_name: ByteField,
    compiler: ByteField,
    max_parameter_bytes: u32,
    selection: ManifestSelection,
    sources: *const ManifestSource,
    source_count: usize,
    requirements: *const ManifestRequirement,
    requirement_count: usize,
    imports_digest: *const u8,
    native_digest: *const u8,
    declarations: *const ManifestDeclaration,
    declaration_count: usize,
    declaration_imports: *const ByteField,
}

unsafe extern "C" {
    #[link_name = "bootstrap.metadata::build_manifest_length"]
    fn build_manifest_length(input: &BuildManifest) -> usize;
    #[link_name = "bootstrap.metadata::encode_build_manifest"]
    fn encode_build_manifest(magic: u64, input: &BuildManifest, output: *mut u8) -> usize;
}

impl Inputs {
    pub(super) fn encode(&self) -> Vec<u8> {
        let field = |text: &str| ByteField::from(text.as_bytes());
        let source = &self.source;
        let (tag, first, second, instances) = match &source.selection {
            Selection::Application(entry) => (0, entry.as_str(), "", &[][..]),
            Selection::Module(instances) => (1, "", "", instances.as_slice()),
            Selection::Library { entry, prefix } => (2, entry.as_str(), prefix.as_str(), &[][..]),
            Selection::Entry { entry, target } => (3, entry.as_str(), target.as_str(), &[][..]),
        };
        let instances: Vec<_> = instances.iter().map(|name| field(name)).collect();
        let selection = ManifestSelection {
            first: field(first), second: field(second), instances: instances.as_ptr(),
            instance_count: instances.len(), tag,
        };
        let sources: Vec<_> = source.sources.iter().map(|source| ManifestSource {
            module_name: field(&source.module), path: source.path.as_slice().into(),
            scope: field(&source.scope), fingerprint: source.fingerprint.as_ptr(),
        }).collect();
        let requirements: Vec<_> = source.requirements.iter().map(|(name, kind)| {
            ManifestRequirement { name: field(name), kind: field(kind) }
        }).collect();
        let mut imports = Vec::new();
        let declarations: Vec<_> = self.declarations.iter().map(|declaration| {
            let (tag, name, query) = declaration.fields();
            let (module, names) = query.unwrap_or(("", &[]));
            let start = imports.len();
            imports.extend(names.iter().map(|name| field(name)));
            ManifestDeclaration {
                name: field(name), module_name: field(module), import_start: start,
                import_count: names.len(), tag,
            }
        }).collect();
        let input = BuildManifest {
            path: source.path.as_slice().into(), root: source.root.as_slice().into(),
            working_directory: source.working_directory.as_slice().into(),
            module_name: field(&source.module), compiler: field(&source.compiler),
            max_parameter_bytes: source.max_parameter_bytes, selection,
            sources: sources.as_ptr(), source_count: sources.len(),
            requirements: requirements.as_ptr(), requirement_count: requirements.len(),
            imports_digest: self.imports.as_ptr(),
            native_digest: self.native.as_ref().map_or(std::ptr::null(), |digest| digest.as_ptr()),
            declarations: declarations.as_ptr(), declaration_count: declarations.len(),
            declaration_imports: imports.as_ptr(),
        };
        let magic = u64::from_le_bytes(*MAGIC);
        // The same borrowed fields stay live through measurement and the final write.
        unsafe {
            let length = build_manifest_length(&input);
            let mut output = Vec::with_capacity(length);
            let written = encode_build_manifest(magic, &input, output.as_mut_ptr());
            output.set_len(written);
            output
        }
    }
}
