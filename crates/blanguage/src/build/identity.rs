//! 构建输入只交接已消费的声明与实际实现；COFF和入口映像共用同一编码。
use super::{ByteField, hash, Result};
use std::{collections::BTreeMap, ffi::c_void, io::Error, ptr};
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
    imports: *const c_void,
    import_count: usize,
    tag: u32,
    kind: BuildReferenceKind,
    execution: u32,
    resolution: NameQuery,
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_digest::sha256_reference_symbol"]
    fn sha256_reference_symbol(digest: &mut hash::Digest, symbol: BuildSymbol);
    #[link_name = "bootstrap.build_digest::sha256_reference_count"]
    fn sha256_reference_count(digest: &mut hash::Digest, count: usize);
    #[link_name = "bootstrap.build_digest::sha256_reference"]
    fn sha256_reference(digest: &mut hash::Digest, row: BuildReference, name: NameField);
}

type NameField = unsafe extern "C" fn(*const c_void, usize) -> ByteField;

unsafe extern "C" fn name_field(names: *const c_void, index: usize) -> ByteField {
    let name = unsafe { &*names.cast::<String>().add(index) };
    name.as_bytes().into()
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
        row.kind = BuildReferenceKind::Query;
        row.module_name = module.as_bytes().into();
        row.imports = imports.as_ptr().cast();
        row.import_count = imports.len();
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
        unsafe { sha256_reference(digest, row, name_field) };
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
    unsafe { sha256_reference(digest, row, name_field) };
}

pub(super) fn references<'a>(declarations: &crate::Declarations, imported: Option<&crate::module::Imports>,
    symbols: impl IntoIterator<Item = &'a str>, used: &[Declaration],
) -> [u8; 32] {
    let mut digest = hash::Digest::new();
    for symbol in symbols {
        let row = build_symbol(declarations, imported, symbol);
        unsafe { sha256_reference_symbol(&mut digest, row) };
    }
    unsafe { sha256_reference_count(&mut digest, used.len()) };
    for entry in used {
        digest_reference(&mut digest, declarations, entry);
    }
    digest.finish()
}

struct SourceRegion {
    preamble_end: usize,
    include_signatures: bool,
    signatures: Vec<SourceSignature>,
}

#[repr(C)]
struct SourceSignature {
    name: ByteField,
    signature: ByteField,
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_digest::record_source_declaration"]
    fn record_source_declaration(
        preamble_end: &mut usize, start: usize, implementation: bool, ordinary: bool,
    ) -> bool;
    #[link_name = "bootstrap.build_digest::sha256_source"]
    fn sha256_source(
        prefix: ByteField, signatures: *const SourceSignature, count: usize, output: &mut [u8; 32],
    );
}

pub(super) fn sources(path: &std::path::Path, root: &std::path::Path,
    selection: Selection, max_parameter_bytes: u32, world: &crate::World,
    module: &str, exports: bool,
) -> Result<SourceInputs> {
    let source_rows = || world.interfaces.iter().map(|(name, module)| (name, &module.source, false))
        .chain(world.sources.iter().filter(|(name, source)| source.path != world.interfaces[*name].source.path)
            .map(|(name, source)| (name, source, true)));
    let mut paths = BTreeMap::new();
    let mut regions = Vec::new();
    for (name, source, implementation) in source_rows() {
        if name == module && !implementation { continue; }
        let index = *paths.entry(source.path.as_path()).or_insert_with(|| {
            let index = regions.len();
            regions.push(SourceRegion {
                preamble_end: usize::MAX, include_signatures: false, signatures: Vec::new(),
            });
            index
        });
        regions[index].include_signatures |= implementation;
    }

    let mut record = |source: &crate::ast::Source, start, signature: Option<(&str, &crate::syntax::Function)>| {
        if let Some(&index) = paths.get(source.path.as_path()) {
            let region = &mut regions[index];
            let keep = unsafe {
                record_source_declaration(&mut region.preamble_end, start, region.include_signatures, signature.is_some())
            };
            if let Some((name, function)) = signature.filter(|_| keep) {
                let end = function.signature_end();
                region.signatures.push(SourceSignature {
                    name: name.as_bytes().into(),
                    signature: source.text[start..end].trim_end().as_bytes().into(),
                });
            }
        }
    };
    for (name, definition) in &world.declarations.functions {
        let ordinary = definition.function.kind() != crate::ast::FunctionKind::Constant
            && definition.function.external().is_none();
        record(&definition.source, definition.function.span().start,
            ordinary.then_some((name.as_str(), &definition.function)));
    }
    for (record_type, source, ..) in world.declarations.records.values() {
        record(source, record_type.span().start, None);
    }
    for (set, source, ..) in world.declarations.type_sets.values() {
        record(source, set.span().start, None);
    }

    let mut sources = Vec::new();
    for (name, source, implementation) in source_rows() {
        let partial = name != module || implementation;
        let (scope, prefix, signatures) = if partial {
            let region = &regions[paths[source.path.as_path()]];
            let end = if region.preamble_end == usize::MAX { source.text.len() } else { region.preamble_end };
            let scope = if implementation { "preamble_and_function_signatures" } else { "preamble" };
            let signatures = if implementation { region.signatures.as_slice() } else { &[] };
            (scope, source.text[..end].trim_end(), signatures)
        } else {
            ("full_source", &*source.text, &[][..])
        };
        // Source text and declaration names remain owned by the world for this call.
        let mut fingerprint = [0; 32];
        unsafe {
            sha256_source(prefix.as_bytes().into(), signatures.as_ptr(), signatures.len(), &mut fingerprint);
        }
        sources.push(SourceInput {
            module: name.clone(), path: source.path.as_os_str().as_encoded_bytes().to_vec(),
            scope: scope.into(), fingerprint,
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
    instances: *const c_void,
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
    imports: *const c_void,
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
    context: *const c_void,
    source_count: usize,
    requirement_count: usize,
    imports_digest: *const u8,
    native_digest: *const u8,
    declaration_count: usize,
}

#[repr(C)]
struct ManifestHost {
    name: NameField,
    source: unsafe extern "C" fn(*const c_void, usize) -> ManifestSource,
    requirement: unsafe extern "C" fn(*const c_void, usize) -> ManifestRequirement,
    declaration: unsafe extern "C" fn(*const c_void, usize) -> ManifestDeclaration,
}

unsafe extern "C" fn manifest_source(context: *const c_void, index: usize) -> ManifestSource {
    let input = unsafe { &*context.cast::<Inputs>() };
    let source = &input.source.sources[index];
    ManifestSource {
        module_name: source.module.as_bytes().into(),
        path: source.path.as_slice().into(),
        scope: source.scope.as_bytes().into(),
        fingerprint: source.fingerprint.as_ptr(),
    }
}

unsafe extern "C" fn manifest_requirement(context: *const c_void, index: usize) -> ManifestRequirement {
    let input = unsafe { &*context.cast::<Inputs>() };
    let (name, kind) = &input.source.requirements[index];
    ManifestRequirement { name: name.as_bytes().into(), kind: kind.as_bytes().into() }
}

unsafe extern "C" fn manifest_declaration(context: *const c_void, index: usize) -> ManifestDeclaration {
    let input = unsafe { &*context.cast::<Inputs>() };
    let (tag, name, query) = input.declarations[index].fields();
    let (module, names) = query.unwrap_or(("", &[]));
    ManifestDeclaration {
        name: name.as_bytes().into(), module_name: module.as_bytes().into(),
        imports: names.as_ptr().cast(), import_count: names.len(), tag,
    }
}

const MANIFEST_HOST: ManifestHost = ManifestHost {
    name: name_field, source: manifest_source,
    requirement: manifest_requirement, declaration: manifest_declaration,
};

unsafe extern "C" {
    #[link_name = "bootstrap.metadata::build_manifest_length"]
    fn build_manifest_length(input: &BuildManifest, host: ManifestHost) -> usize;
    #[link_name = "bootstrap.metadata::encode_build_manifest"]
    fn encode_build_manifest(magic: u64, input: &BuildManifest, host: ManifestHost, output: *mut u8) -> usize;
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
        let selection = ManifestSelection {
            first: field(first), second: field(second), instances: instances.as_ptr().cast(),
            instance_count: instances.len(), tag,
        };
        let input = BuildManifest {
            path: source.path.as_slice().into(), root: source.root.as_slice().into(),
            working_directory: source.working_directory.as_slice().into(),
            module_name: field(&source.module), compiler: field(&source.compiler),
            max_parameter_bytes: source.max_parameter_bytes, selection,
            context: ptr::from_ref(self).cast(),
            source_count: source.sources.len(), requirement_count: source.requirements.len(),
            imports_digest: self.imports.as_ptr(),
            native_digest: self.native.as_ref().map_or(ptr::null(), |digest| digest.as_ptr()),
            declaration_count: self.declarations.len(),
        };
        let magic = u64::from_le_bytes(*MAGIC);
        // The same borrowed fields stay live through measurement and the final write.
        unsafe {
            let length = build_manifest_length(&input, MANIFEST_HOST);
            let mut output = Vec::with_capacity(length);
            let written = encode_build_manifest(magic, &input, MANIFEST_HOST, output.as_mut_ptr());
            output.set_len(written);
            output
        }
    }
}
