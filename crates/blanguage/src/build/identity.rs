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
    CompiledContract { name: String, provider: String },
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
            Self::CompiledContract { name, provider } => (8, name, Some((provider, &[]))),
        }
    }
}

const MAGIC: &[u8; 8] = b"BLBO\x0d\0\0\0";
const PREVIOUS_MAGIC: &[u8; 8] = b"BLBO\x0c\0\0\0";
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
pub struct Inputs {
    pub source: SourceInputs,
    pub imports: [u8; 32],
    pub native: Option<[u8; 32]>,
    pub declarations: Vec<Declaration>,
}
#[repr(C)]
struct CompiledSymbol {
    name: ByteField,
    provider: ByteField,
    contract: ByteField,
    declaration: ByteField,
}

#[repr(C)]
struct ReferenceKey {
    name: ByteField,
    module_name: ByteField,
    imports: *const c_void,
    import_count: usize,
    tag: u32,
}

#[repr(C)]
struct FunctionReference {
    declaration: ByteField,
    constant: u32,
    external: u32,
    implementation: u32,
    execution: u32,
}

#[repr(u32)]
#[allow(dead_code)] // The B identity producer chooses the actual namespace.
enum ReferenceQuery { Value, Record, TypeSet }

#[repr(u32)]
enum NameQuery { Missing, Unique, Ambiguous }

#[repr(C)]
struct ReferenceHost {
    symbol_stream: *mut c_void,
    declaration_stream: *mut c_void,
    context: *const c_void,
    next_symbol: unsafe extern "C" fn(*mut c_void) -> ByteField,
    next_declaration: unsafe extern "C" fn(*mut c_void) -> ReferenceKey,
    name: NameField,
    symbol: unsafe extern "C" fn(*const c_void, ByteField) -> CompiledSymbol,
    requirement: unsafe extern "C" fn(*const c_void, ByteField, ByteField) -> u32,
    function: unsafe extern "C" fn(*const c_void, ByteField) -> FunctionReference,
    record: unsafe extern "C" fn(*const c_void, ByteField) -> ByteField,
    type_set: unsafe extern "C" fn(*const c_void, ByteField) -> ByteField,
    contract: unsafe extern "C" fn(*const c_void, ByteField, ByteField) -> ByteField,
    query: unsafe extern "C" fn(*const c_void, *mut hash::Digest, ReferenceKey, ReferenceQuery),
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_digest::dependency_digest"]
    fn dependency_digest(declaration_count: usize, imported: u32, host: ReferenceHost, output: &mut [u8; 32]);
    #[link_name = "bootstrap.build_digest::digest_name_resolution"]
    fn digest_name_resolution(state: *mut hash::Digest, resolution: NameQuery, name: ByteField);
}

type NameField = unsafe extern "C" fn(*const c_void, usize) -> ByteField;

unsafe extern "C" fn name_field(names: *const c_void, index: usize) -> ByteField {
    let name = unsafe { &*names.cast::<String>().add(index) };
    name.as_bytes().into()
}

struct ReferenceSources<'a> {
    declarations: &'a crate::Declarations,
    imported: Option<&'a crate::module::Imports>,
}

const ABSENT: ByteField = ByteField { bytes: ptr::null(), length: 0 };

unsafe fn reference_text(field: &ByteField) -> &str {
    unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(field.bytes, field.length)) }
}

unsafe extern "C" fn next_symbol<'a, I: Iterator<Item = &'a str>>(stream: *mut c_void) -> ByteField {
    unsafe { &mut *stream.cast::<I>() }.next().map_or(ABSENT, |name| name.as_bytes().into())
}

unsafe extern "C" fn next_declaration<'a, I: Iterator<Item = &'a Declaration>>(stream: *mut c_void) -> ReferenceKey {
    match unsafe { &mut *stream.cast::<I>() }.next() {
        Some(entry) => {
            let (tag, name, scope) = entry.fields();
            let (module_name, imports, import_count) = scope.map_or((ABSENT, ptr::null(), 0), |(module, imports)| {
                (module.as_bytes().into(), imports.as_ptr().cast(), imports.len())
            });
            ReferenceKey { name: name.as_bytes().into(), module_name, imports, import_count, tag }
        }
        None => ReferenceKey { name: ABSENT, module_name: ABSENT, imports: ptr::null(), import_count: 0, tag: 0 },
    }
}

unsafe extern "C" fn reference_symbol(context: *const c_void, symbol: ByteField) -> CompiledSymbol {
    let source = unsafe { &*context.cast::<ReferenceSources<'_>>() };
    let symbol = unsafe { reference_text(&symbol) };
    match source.declarations.compiled_function(symbol) {
        Some((name, definition, contract)) => {
            let span = definition.function.span();
            CompiledSymbol {
                name: name.as_bytes().into(),
                provider: definition.module.as_bytes().into(),
                contract: contract.into(),
                declaration: definition.source.text[span.start..span.end].as_bytes().into(),
            }
        }
        None => CompiledSymbol { name: ABSENT, provider: ABSENT, contract: ABSENT, declaration: ABSENT },
    }
}

unsafe extern "C" fn reference_requirement(context: *const c_void, provider: ByteField, name: ByteField) -> u32 {
    let source = unsafe { &*context.cast::<ReferenceSources<'_>>() };
    let (provider, name) = unsafe { (reference_text(&provider), reference_text(&name)) };
    source.imported.and_then(|imports| imports.modules.get(provider))
        .and_then(|module| module.requirements.get(name))
        .map_or(0, |requirement| *requirement as u32 + 1)
}

unsafe extern "C" fn reference_function(context: *const c_void, name: ByteField) -> FunctionReference {
    let source = unsafe { &*context.cast::<ReferenceSources<'_>>() };
    let name = unsafe { reference_text(&name) };
    match source.declarations.functions.get(name) {
        Some(definition) => {
            let function = &definition.function;
            let span = function.span();
            FunctionReference {
                declaration: definition.source.text[span.start..span.end].as_bytes().into(),
                constant: u32::from(function.kind() == crate::ast::FunctionKind::Constant),
                external: u32::from(function.external().is_some()),
                implementation: u32::from(function.body().is_some()),
                execution: definition.execution as u32,
            }
        }
        None => FunctionReference { declaration: ABSENT, constant: 0, external: 0, implementation: 0, execution: 0 },
    }
}

unsafe extern "C" fn reference_record(context: *const c_void, name: ByteField) -> ByteField {
    let source = unsafe { &*context.cast::<ReferenceSources<'_>>() };
    let name = unsafe { reference_text(&name) };
    source.declarations.records.get(name).map_or(ABSENT, |(record, source, ..)| {
        let span = record.span();
        source.text[span.start..span.end].as_bytes().into()
    })
}

unsafe extern "C" fn reference_type_set(context: *const c_void, name: ByteField) -> ByteField {
    let source = unsafe { &*context.cast::<ReferenceSources<'_>>() };
    let name = unsafe { reference_text(&name) };
    source.declarations.type_sets.get(name).map_or(ABSENT, |(set, source, ..)| {
        let span = set.span();
        source.text[span.start..span.end].as_bytes().into()
    })
}

unsafe extern "C" fn reference_contract(context: *const c_void, provider: ByteField, name: ByteField) -> ByteField {
    let source = unsafe { &*context.cast::<ReferenceSources<'_>>() };
    let (provider, name) = unsafe { (reference_text(&provider), reference_text(&name)) };
    source.imported.and_then(|imports| imports.modules.get(provider))
        .and_then(|module| module.definitions.get(name))
        .map_or(ABSENT, |bytes| bytes.as_slice().into())
}

unsafe extern "C" fn reference_query(context: *const c_void, digest: *mut hash::Digest, key: ReferenceKey, query: ReferenceQuery) {
    use crate::frontend::NameResolution;
    let source = unsafe { &*context.cast::<ReferenceSources<'_>>() };
    let (name, module) = unsafe { (reference_text(&key.name), reference_text(&key.module_name)) };
    let imports = unsafe { std::slice::from_raw_parts(key.imports.cast::<String>(), key.import_count) };
    let declarations = source.declarations;
    let result = match query {
        ReferenceQuery::Value => declarations.resolve(name, module, imports, true),
        ReferenceQuery::Record => declarations.resolve(name, module, imports, false),
        ReferenceQuery::TypeSet => declarations.resolve_type_set(name, module, imports)
            .map_or(NameResolution::Missing, NameResolution::Unique),
    };
    let (resolution, name) = match &result {
        NameResolution::Missing => (NameQuery::Missing, ABSENT),
        NameResolution::Unique(name) => (NameQuery::Unique, name.as_bytes().into()),
        NameResolution::Ambiguous => (NameQuery::Ambiguous, ABSENT),
    };
    unsafe { digest_name_resolution(digest, resolution, name) }
}

fn reference_host<'symbol, 'declaration, S, D>(sources: &ReferenceSources<'_>, symbols: &mut S, used: &mut D)
    -> ReferenceHost
where
    S: Iterator<Item = &'symbol str>,
    D: Iterator<Item = &'declaration Declaration>,
{
    ReferenceHost {
        symbol_stream: ptr::from_mut(symbols).cast(),
        declaration_stream: ptr::from_mut(used).cast(),
        context: ptr::from_ref(sources).cast(),
        next_symbol: next_symbol::<S>,
        next_declaration: next_declaration::<D>,
        name: name_field,
        symbol: reference_symbol,
        requirement: reference_requirement,
        function: reference_function,
        record: reference_record,
        type_set: reference_type_set,
        contract: reference_contract,
        query: reference_query,
    }
}

pub(super) fn references<'symbol, 'declaration, S, D>(
    declarations: &crate::Declarations, imported: Option<&crate::module::Imports>, symbols: S, mut used: D,
) -> [u8; 32]
where
    S: IntoIterator<Item = &'symbol str>,
    D: ExactSizeIterator<Item = &'declaration Declaration>,
{
    let sources = ReferenceSources { declarations, imported };
    let mut symbols = symbols.into_iter();
    let host = reference_host(&sources, &mut symbols, &mut used);
    let mut digest = [0; 32];
    unsafe { dependency_digest(used.len(), u32::from(imported.is_some()), host, &mut digest) };
    digest
}

#[repr(C)]
struct ObjectReuse {
    imports: *const [u8; 32],
    runtime: *const [u8; 32],
    context: *mut c_void,
    load_runtime: unsafe extern "C" fn(*mut c_void, &mut [u8; 32]) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_digest::matches_dependency_reuse"]
    fn matches_dependency_reuse(declarations: usize, imported: u32, host: ReferenceHost, expected: &[u8; 32]) -> bool;
    #[link_name = "bootstrap.build_digest::matches_object_reuse"]
    fn matches_object_reuse(declarations: usize, imported: u32, host: ReferenceHost, reuse: ObjectReuse) -> u32;
}

struct RuntimeDigest<'a> {
    load: &'a mut dyn FnMut() -> Result<[u8; 32]>,
    error: Option<Box<dyn std::error::Error + Send + Sync>>,
}

unsafe extern "C" fn load_runtime_digest(context: *mut c_void, digest: &mut [u8; 32]) -> u32 {
    let runtime = unsafe { &mut *context.cast::<RuntimeDigest<'_>>() };
    match (runtime.load)() {
        Ok(value) => {
            *digest = value;
            1
        }
        Err(error) => {
            runtime.error = Some(error);
            0
        }
    }
}

impl ReuseInputs<'_> {
    pub(super) fn matches_entry<'symbol>(
        &self, declarations: &crate::Declarations, imported: Option<&crate::module::Imports>,
        symbols: impl Iterator<Item = &'symbol str>,
    ) -> bool {
        let sources = ReferenceSources { declarations, imported };
        let mut symbols = symbols;
        let mut used = self.declarations.iter();
        let host = reference_host(&sources, &mut symbols, &mut used);
        unsafe { matches_dependency_reuse(used.len(), u32::from(imported.is_some()), host, self.imports) }
    }

    pub(super) fn matches_object<'symbol>(
        &self, declarations: &crate::Declarations, imported: Option<&crate::module::Imports>,
        symbols: impl IntoIterator<Item = &'symbol str>, mut load_runtime: impl FnMut() -> Result<[u8; 32]>,
    ) -> Result<bool> {
        let sources = ReferenceSources { declarations, imported };
        let mut symbols = symbols.into_iter();
        let mut used = self.declarations.iter();
        let host = reference_host(&sources, &mut symbols, &mut used);
        let mut runtime = RuntimeDigest { load: &mut load_runtime, error: None };
        let reuse = ObjectReuse {
            imports: self.imports,
            runtime: self.native.map_or(ptr::null(), ptr::from_ref),
            context: ptr::from_mut(&mut runtime).cast(),
            load_runtime: load_runtime_digest,
        };
        let matches = unsafe { matches_object_reuse(used.len(), u32::from(imported.is_some()), host, reuse) };
        runtime.error.map_or(Ok(matches != 0), Err)
    }
}

#[repr(C)]
struct SourceExtent {
    preamble_end: usize,
    include_signatures: u32,
}

// Only the extent prefix is shared with B. The host owns the borrowed signature list.
#[repr(C)]
struct SourceRegion {
    extent: SourceExtent,
    signatures: Vec<SourceSignature>,
}

#[repr(C)]
struct SourceSignature {
    name: ByteField,
    signature: ByteField,
}

#[repr(C)]
struct SignatureSpan {
    signatures: *const SourceSignature,
    count: usize,
}

#[repr(C)]
struct SourceFile {
    source: *const c_void,
    name: ByteField,
    text: ByteField,
    root: u32,
    implementation: u32,
    shares_interface: u32,
}

#[repr(C)]
struct SourceDefinition {
    source: *const c_void,
    function: *const c_void,
    name: ByteField,
    start: usize,
    constant: u32,
    external: u32,
}

impl SourceDefinition {
    fn declaration(source: &crate::ast::Source, start: usize) -> Self {
        Self { source: ptr::from_ref(source).cast(), function: ptr::null(), name: ABSENT,
            start, constant: 0, external: 0 }
    }
}

#[repr(C)]
struct SourceRequirement {
    name: ByteField,
    public: u32,
    root: u32,
    execution: u32,
}

#[repr(u32)]
#[allow(dead_code)] // The B source capture selects the projected range.
enum SourceScope { Full, Preamble, Signatures }

#[repr(C)]
struct SourceHost {
    output: *mut c_void,
    files: *mut c_void,
    declarations: *mut c_void,
    requirements: *mut c_void,
    next_file: unsafe extern "C" fn(*mut c_void) -> SourceFile,
    rewind_files: unsafe extern "C" fn(*mut c_void),
    next_definition: unsafe extern "C" fn(*mut c_void) -> SourceDefinition,
    next_requirement: unsafe extern "C" fn(*mut c_void) -> SourceRequirement,
    register_region: unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void,
    region: unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void,
    signature: unsafe extern "C" fn(*mut c_void, SourceDefinition),
    prefix: unsafe extern "C" fn(*const c_void, usize) -> ByteField,
    signatures: unsafe extern "C" fn(*const c_void) -> SignatureSpan,
    emit_source: unsafe extern "C" fn(*mut c_void, SourceFile, SourceScope, *const [u8; 32]),
    emit_requirement: unsafe extern "C" fn(*mut c_void, ByteField, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_digest::capture_sources"]
    fn capture_sources(host: SourceHost, exports: u32);
}

struct SourceFiles<'a> {
    world: &'a crate::World,
    module: &'a str,
    interfaces: crate::frontend::declarations::Iter<'a, crate::ModuleDeclaration>,
    implementations: crate::frontend::declarations::Iter<'a, crate::ModuleDeclaration>,
}

impl<'a> SourceFiles<'a> {
    fn new(world: &'a crate::World, module: &'a str) -> Self {
        Self { world, module, interfaces: world.modules.iter(), implementations: world.modules.iter() }
    }
}

fn source_file(name: &str, source: &crate::ast::Source, root: bool, implementation: bool, shares_interface: bool) -> SourceFile {
    SourceFile {
        source: ptr::from_ref(source).cast(), name: name.as_bytes().into(), text: source.text.as_bytes().into(),
        root: u32::from(root), implementation: u32::from(implementation), shares_interface: u32::from(shares_interface),
    }
}

unsafe extern "C" fn next_source_file(context: *mut c_void) -> SourceFile {
    let stream = unsafe { &mut *context.cast::<SourceFiles<'_>>() };
    if let Some((name, declaration)) = stream.interfaces.next() {
        return source_file(name, &declaration.interface.source, name == stream.module, false, false);
    }
    if let Some((name, declaration)) = stream.implementations.next() {
        let source = declaration.source();
        let same_path = source.path == declaration.interface.source.path;
        return source_file(name, source, name == stream.module, true, same_path);
    }
    SourceFile { source: ptr::null(), name: ABSENT, text: ABSENT, root: 0, implementation: 0, shares_interface: 0 }
}

unsafe extern "C" fn rewind_source_files(context: *mut c_void) {
    let rows = unsafe { &mut *context.cast::<SourceFiles<'_>>() };
    rows.interfaces = rows.world.modules.iter();
    rows.implementations = rows.world.modules.iter();
}

unsafe extern "C" fn next_source_definition<I: Iterator<Item = SourceDefinition>>(context: *mut c_void) -> SourceDefinition {
    unsafe { &mut *context.cast::<I>() }.next().unwrap_or(SourceDefinition {
        source: ptr::null(), function: ptr::null(), name: ABSENT, start: 0, constant: 0, external: 0,
    })
}

unsafe extern "C" fn next_source_requirement<I: Iterator<Item = SourceRequirement>>(context: *mut c_void) -> SourceRequirement {
    unsafe { &mut *context.cast::<I>() }.next().unwrap_or(SourceRequirement { name: ABSENT, public: 0, root: 0, execution: 0 })
}

#[derive(Default)]
struct CapturedSources<'a> {
    regions: BTreeMap<&'a std::path::Path, SourceRegion>,
    sources: Vec<SourceInput>,
    requirements: Vec<(String, String)>,
}

unsafe extern "C" fn register_source_region(context: *mut c_void, source: *const c_void) -> *mut c_void {
    let output = unsafe { &mut *context.cast::<CapturedSources<'_>>() };
    let path = unsafe { &*source.cast::<crate::ast::Source>() }.path.as_path();
    let region = output.regions.entry(path).or_insert_with(|| SourceRegion {
        extent: SourceExtent { preamble_end: usize::MAX, include_signatures: 0 },
        signatures: Vec::new(),
    });
    ptr::from_mut(region).cast()
}

unsafe extern "C" fn find_source_region(context: *mut c_void, source: *const c_void) -> *mut c_void {
    let output = unsafe { &mut *context.cast::<CapturedSources<'_>>() };
    let path = unsafe { &*source.cast::<crate::ast::Source>() }.path.as_path();
    output.regions.get_mut(path).map_or(ptr::null_mut(), |region| ptr::from_mut(region).cast())
}

unsafe extern "C" fn keep_source_signature(region: *mut c_void, declaration: SourceDefinition) {
    let region = unsafe { &mut *region.cast::<SourceRegion>() };
    let source = unsafe { &*declaration.source.cast::<crate::ast::Source>() };
    let function = unsafe { &*declaration.function.cast::<crate::syntax::Function>() };
    let signature = source.text[declaration.start..function.signature_end()].trim_end().as_bytes().into();
    region.signatures.push(SourceSignature { name: declaration.name, signature });
}

unsafe extern "C" fn source_prefix(source: *const c_void, end: usize) -> ByteField {
    let source = unsafe { &*source.cast::<crate::ast::Source>() };
    source.text[..end].trim_end().as_bytes().into()
}

unsafe extern "C" fn source_signatures(region: *const c_void) -> SignatureSpan {
    let region = unsafe { &*region.cast::<SourceRegion>() };
    SignatureSpan { signatures: region.signatures.as_ptr(), count: region.signatures.len() }
}

unsafe extern "C" fn emit_source(context: *mut c_void, file: SourceFile, scope: SourceScope, fingerprint: *const [u8; 32]) {
    let out = unsafe { &mut *context.cast::<CapturedSources<'_>>() };
    let source = unsafe { &*file.source.cast::<crate::ast::Source>() };
    let scope = match scope {
        SourceScope::Full => "full_source",
        SourceScope::Preamble => "preamble",
        SourceScope::Signatures => "preamble_and_function_signatures",
    };
    out.sources.push(SourceInput {
        module: unsafe { reference_text(&file.name) }.into(),
        path: source.path.as_os_str().as_encoded_bytes().to_vec(),
        scope: scope.into(),
        fingerprint: unsafe { *fingerprint },
    });
}

unsafe extern "C" fn emit_source_requirement(context: *mut c_void, name: ByteField, execution: u32) {
    let out = unsafe { &mut *context.cast::<CapturedSources<'_>>() };
    let name = unsafe { reference_text(&name) };
    let label = ["invocation", "domain", "host"][execution as usize];
    out.requirements.push((name.into(), label.into()));
}

fn capture<'a, I, R>(world: &'a crate::World, module: &str, mut declarations: I, mut requirements: R, exports: bool) -> CapturedSources<'a>
where
    I: Iterator<Item = SourceDefinition>,
    R: Iterator<Item = SourceRequirement>,
{
    let mut files = SourceFiles::new(world, module);
    let mut output = CapturedSources::default();
    let host = SourceHost {
        output: ptr::from_mut(&mut output).cast(),
        files: ptr::from_mut(&mut files).cast(),
        declarations: ptr::from_mut(&mut declarations).cast(),
        requirements: ptr::from_mut(&mut requirements).cast(),
        next_file: next_source_file,
        rewind_files: rewind_source_files,
        next_definition: next_source_definition::<I>,
        next_requirement: next_source_requirement::<R>,
        register_region: register_source_region,
        region: find_source_region,
        signature: keep_source_signature,
        prefix: source_prefix,
        signatures: source_signatures,
        emit_source,
        emit_requirement: emit_source_requirement,
    };
    unsafe { capture_sources(host, u32::from(exports)) };
    output
}

pub(super) fn sources(path: &std::path::Path, root: &std::path::Path,
    selection: Selection, max_parameter_bytes: u32, world: &crate::World,
    module: &str, exports: bool,
) -> Result<SourceInputs> {
    let declarations = world.declarations.functions.iter().map(|(name, definition)| {
        let function = &definition.function;
        SourceDefinition {
            source: ptr::from_ref(&definition.source).cast(),
            function: ptr::from_ref(function).cast(),
            name: name.as_bytes().into(),
            start: function.span().start,
            constant: u32::from(function.kind() == crate::ast::FunctionKind::Constant),
            external: u32::from(function.external().is_some()),
        }
    }).chain(world.declarations.records.values().map(|(record, source, ..)| {
        SourceDefinition::declaration(source, record.span().start)
    })).chain(world.declarations.type_sets.values().map(|(set, source, ..)| {
        SourceDefinition::declaration(source, set.span().start)
    }));
    let requirements = world.functions.iter().map(|(name, definition)| SourceRequirement {
        name: name.as_bytes().into(),
        public: u32::from(definition.public), root: u32::from(definition.module == module),
        execution: definition.execution as u32,
    });
    let CapturedSources { sources, requirements, .. } = capture(world, module, declarations, requirements, exports);
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
struct ManifestIdentity {
    path: ByteField,
    root: ByteField,
    working_directory: ByteField,
    module_name: ByteField,
    compiler: ByteField,
    max_parameter_bytes: u32,
}

#[repr(C)]
struct ManifestVariant {
    first: ByteField,
    second: ByteField,
    tag: u32,
}

#[repr(C)]
struct ManifestReadResult {
    identity: ManifestIdentity,
    imports_digest: *const u8,
    native_digest: *const u8,
    same_source: u32,
    status: u32,
}

#[repr(C)]
struct ManifestExpected {
    identity: ManifestIdentity,
    selection: ManifestSelection,
    context: *const c_void,
    source_count: usize,
    requirement_count: usize,
}

#[repr(C)]
struct ManifestWriter {
    output: *mut c_void,
    append: unsafe extern "C" fn(*mut c_void, ByteField),
}

#[repr(C)]
struct ManifestReceiver {
    context: *mut c_void,
    valid_text: unsafe extern "C" fn(*mut c_void, ByteField, u32) -> u32,
    name: unsafe extern "C" fn(*mut c_void, ByteField),
    selection: unsafe extern "C" fn(*mut c_void, ManifestVariant),
    source: unsafe extern "C" fn(*mut c_void, ManifestSource),
    requirement: unsafe extern "C" fn(*mut c_void, ManifestRequirement),
    declaration: unsafe extern "C" fn(*mut c_void, ManifestVariant),
    current_name: NameField,
    current_source: unsafe extern "C" fn(*const c_void, usize) -> ManifestSource,
    current_requirement: unsafe extern "C" fn(*const c_void, usize) -> ManifestRequirement,
    json: ManifestWriter,
    artifact_digest: ByteField,
}

unsafe extern "C" {
    #[link_name = "bootstrap.metadata_read::read_manifest"]
    fn read_manifest(
        input: ByteField, magic: u64, receiver: ManifestReceiver, expected: *const ManifestExpected,
    ) -> ManifestReadResult;
}

#[derive(Default)]
struct ManifestProjection {
    selection: Option<Selection>,
    sources: Vec<SourceInput>,
    requirements: Vec<(String, String)>,
    declarations: Vec<Declaration>,
    names: Vec<String>,
    invalid_text: Option<std::str::Utf8Error>,
}

unsafe fn own_manifest_bytes(field: ByteField) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(field.bytes, field.length) }.to_vec()
}

unsafe fn own_manifest_text(field: ByteField) -> String {
    // B passes these exact immutable ranges through validate_manifest_text before projection.
    unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(field.bytes, field.length)) }.into()
}

unsafe extern "C" fn validate_manifest_text(context: *mut c_void, field: ByteField, required: u32) -> u32 {
    let bytes = unsafe { std::slice::from_raw_parts(field.bytes, field.length) };
    match std::str::from_utf8(bytes) {
        Ok(_) => 1,
        Err(error) => {
            if required != 0 {
                unsafe { &mut *context.cast::<ManifestProjection>() }.invalid_text = Some(error);
            }
            0
        }
    }
}

unsafe extern "C" fn append_manifest_json(output: *mut c_void, field: ByteField) {
    let output = unsafe { &mut *output.cast::<Vec<u8>>() };
    output.extend_from_slice(unsafe { std::slice::from_raw_parts(field.bytes, field.length) });
}

unsafe extern "C" fn collect_manifest_name(context: *mut c_void, field: ByteField) {
    unsafe { &mut *context.cast::<ManifestProjection>() }.names.push(unsafe { own_manifest_text(field) });
}

unsafe extern "C" fn collect_manifest_selection(context: *mut c_void, variant: ManifestVariant) {
    let output = unsafe { &mut *context.cast::<ManifestProjection>() };
    output.selection = Some(match variant.tag {
        0 => Selection::Application(unsafe { own_manifest_text(variant.first) }),
        1 => Selection::Module(std::mem::take(&mut output.names)),
        2 => Selection::Library {
            entry: unsafe { own_manifest_text(variant.first) },
            prefix: unsafe { own_manifest_text(variant.second) },
        },
        _ => Selection::Entry {
            entry: unsafe { own_manifest_text(variant.first) },
            target: unsafe { own_manifest_text(variant.second) },
        },
    });
}

unsafe extern "C" fn collect_manifest_source(context: *mut c_void, row: ManifestSource) {
    unsafe { &mut *context.cast::<ManifestProjection>() }.sources.push(SourceInput {
        module: unsafe { own_manifest_text(row.module_name) },
        path: unsafe { own_manifest_bytes(row.path) },
        scope: unsafe { own_manifest_text(row.scope) },
        fingerprint: unsafe { *row.fingerprint.cast::<[u8; 32]>() },
    });
}

unsafe extern "C" fn collect_manifest_requirement(context: *mut c_void, row: ManifestRequirement) {
    unsafe { &mut *context.cast::<ManifestProjection>() }.requirements.push((
        unsafe { own_manifest_text(row.name) },
        unsafe { own_manifest_text(row.kind) },
    ));
}

unsafe extern "C" fn collect_manifest_declaration(context: *mut c_void, variant: ManifestVariant) {
    let output = unsafe { &mut *context.cast::<ManifestProjection>() };
    let name = unsafe { own_manifest_text(variant.first) };
    let declaration = match variant.tag {
        0 => Declaration::Constant(name),
        1 => Declaration::Record(name),
        2 => Declaration::TypeSet(name),
        3 => Declaration::External(name),
        7 => Declaration::Implementation(name),
        8 => Declaration::CompiledContract { name, provider: unsafe { own_manifest_text(variant.second) } },
        tag => {
            let module = unsafe { own_manifest_text(variant.second) };
            let imports = std::mem::take(&mut output.names);
            match tag {
                4 => Declaration::ValueName { name, module, imports },
                5 => Declaration::RecordName { name, module, imports },
                _ => Declaration::TypeSetName { name, module, imports },
            }
        }
    };
    output.declarations.push(declaration);
}

fn read_manifest_fields(
    bytes: &[u8], projection: &mut ManifestProjection, expected: Option<&ManifestExpected>,
    json: Option<(&mut Vec<u8>, &[u8; 32])>,
) -> Result<Option<ManifestReadResult>> {
    let (json, digest) = json.map_or((ptr::null_mut(), &[][..]), |(out, digest)| {
        (ptr::from_mut(out).cast(), &digest[..])
    });
    let receiver = ManifestReceiver {
        context: ptr::from_mut(projection).cast(),
        valid_text: validate_manifest_text,
        name: collect_manifest_name,
        selection: collect_manifest_selection,
        source: collect_manifest_source,
        requirement: collect_manifest_requirement,
        declaration: collect_manifest_declaration,
        current_name: name_field,
        current_source: manifest_source,
        current_requirement: manifest_requirement,
        json: ManifestWriter { output: json, append: append_manifest_json },
        artifact_digest: digest.into(),
    };
    // All incoming fields borrow the original bytes and live through this B call.
    let magic = if expected.is_none() && bytes.starts_with(PREVIOUS_MAGIC) { PREVIOUS_MAGIC } else { MAGIC };
    let expected = expected.map_or(ptr::null(), ptr::from_ref);
    let manifest = unsafe { read_manifest(bytes.into(), u64::from_le_bytes(*magic), receiver, expected) };
    match manifest.status {
        0 => {}
        1 => return Ok(None),
        8 => return Err(Error::other("模块接口被截断").into()),
        9 => return Err(Error::other("模块接口长度溢出").into()),
        10 => return Err(Error::other("模块布尔值编码错误").into()),
        11 => return Err(Error::other(projection.invalid_text.unwrap()).into()),
        12 => return Err(Error::other("模块枚举编码错误：Selection").into()),
        13 => return Err(Error::other("模块枚举编码错误：Declaration").into()),
        _ => return Err(Error::other("模块接口有未消费内容").into()),
    }
    Ok(Some(manifest))
}

pub(super) fn decode(bytes: &[u8]) -> Result<Option<Inputs>> {
    let mut projection = ManifestProjection::default();
    let Some(manifest) = read_manifest_fields(bytes, &mut projection, None, None)? else { return Ok(None); };
    let identity = manifest.identity;
    Ok(Some(Inputs {
        source: SourceInputs {
            path: unsafe { own_manifest_bytes(identity.path) },
            root: unsafe { own_manifest_bytes(identity.root) },
            working_directory: unsafe { own_manifest_bytes(identity.working_directory) },
            module: unsafe { own_manifest_text(identity.module_name) },
            compiler: unsafe { own_manifest_text(identity.compiler) },
            max_parameter_bytes: identity.max_parameter_bytes,
            selection: projection.selection.unwrap(),
            sources: projection.sources,
            requirements: projection.requirements,
        },
        imports: unsafe { *manifest.imports_digest.cast::<[u8; 32]>() },
        native: unsafe { manifest.native_digest.cast::<[u8; 32]>().as_ref() }.copied(),
        declarations: projection.declarations,
    }))
}

pub(super) fn decode_json(bytes: &[u8], digest: &[u8; 32]) -> Result<Option<String>> {
    let mut output = Vec::new();
    if read_manifest_fields(bytes, &mut ManifestProjection::default(), None, Some((&mut output, digest)))?.is_none() {
        return Ok(None);
    }
    // B emits ASCII syntax and only borrows text after UTF-8 validation.
    Ok(Some(unsafe { String::from_utf8_unchecked(output) }))
}

pub(super) struct ReuseInputs<'a> {
    imports: &'a [u8; 32],
    native: Option<&'a [u8; 32]>,
    declarations: Vec<Declaration>,
}

pub(super) fn match_source<'a>(bytes: &'a [u8], source: &SourceInputs) -> Result<Option<ReuseInputs<'a>>> {
    let rows = ManifestRows { source, declarations: &[] };
    let expected = ManifestExpected {
        identity: ManifestIdentity::borrow(source),
        selection: ManifestSelection::borrow(&source.selection),
        context: ptr::from_ref(&rows).cast(),
        source_count: source.sources.len(),
        requirement_count: source.requirements.len(),
    };
    let mut projection = ManifestProjection::default();
    let Some(manifest) = read_manifest_fields(bytes, &mut projection, Some(&expected), None)?
        .filter(|manifest| manifest.same_source != 0)
    else { return Ok(None); };
    Ok(Some(ReuseInputs {
        imports: unsafe { &*manifest.imports_digest.cast::<[u8; 32]>() },
        native: unsafe { manifest.native_digest.cast::<[u8; 32]>().as_ref() },
        declarations: projection.declarations,
    }))
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

struct ManifestRows<'a> {
    source: &'a SourceInputs,
    declarations: &'a [&'a Declaration],
}

unsafe extern "C" fn manifest_source(context: *const c_void, index: usize) -> ManifestSource {
    let input = unsafe { &*context.cast::<ManifestRows<'_>>() };
    let source = &input.source.sources[index];
    ManifestSource {
        module_name: source.module.as_bytes().into(),
        path: source.path.as_slice().into(),
        scope: source.scope.as_bytes().into(),
        fingerprint: source.fingerprint.as_ptr(),
    }
}

unsafe extern "C" fn manifest_requirement(context: *const c_void, index: usize) -> ManifestRequirement {
    let input = unsafe { &*context.cast::<ManifestRows<'_>>() };
    let (name, kind) = &input.source.requirements[index];
    ManifestRequirement { name: name.as_bytes().into(), kind: kind.as_bytes().into() }
}

unsafe extern "C" fn manifest_declaration(context: *const c_void, index: usize) -> ManifestDeclaration {
    let input = unsafe { &*context.cast::<ManifestRows<'_>>() };
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

impl ManifestIdentity {
    fn borrow(source: &SourceInputs) -> Self {
        Self {
            path: source.path.as_slice().into(), root: source.root.as_slice().into(),
            working_directory: source.working_directory.as_slice().into(),
            module_name: source.module.as_bytes().into(), compiler: source.compiler.as_bytes().into(),
            max_parameter_bytes: source.max_parameter_bytes,
        }
    }
}

impl ManifestSelection {
    fn borrow(selection: &Selection) -> Self {
        let (tag, first, second, instances) = match selection {
            Selection::Application(entry) => (0, entry.as_str(), "", &[][..]),
            Selection::Module(instances) => (1, "", "", instances.as_slice()),
            Selection::Library { entry, prefix } => (2, entry.as_str(), prefix.as_str(), &[][..]),
            Selection::Entry { entry, target } => (3, entry.as_str(), target.as_str(), &[][..]),
        };
        Self {
            first: first.as_bytes().into(), second: second.as_bytes().into(),
            instances: instances.as_ptr().cast(), instance_count: instances.len(), tag,
        }
    }
}

pub(super) fn encode(
    source: &SourceInputs, imports: &[u8; 32], native: Option<&[u8; 32]>, declarations: &[&Declaration],
) -> Vec<u8> {
    let identity = ManifestIdentity::borrow(source);
    let rows = ManifestRows { source, declarations };
    let input = BuildManifest {
        path: identity.path, root: identity.root, working_directory: identity.working_directory,
        module_name: identity.module_name, compiler: identity.compiler,
        max_parameter_bytes: identity.max_parameter_bytes,
        selection: ManifestSelection::borrow(&source.selection),
        context: ptr::from_ref(&rows).cast(),
        source_count: source.sources.len(), requirement_count: source.requirements.len(),
        imports_digest: imports.as_ptr(),
        native_digest: native.map_or(ptr::null(), |digest| digest.as_ptr()),
        declaration_count: declarations.len(),
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
