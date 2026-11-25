//! 正式入口构建：复用实际源码与公开合同编译，原链接器消费当前模块代码。
pub(crate) mod hash;
mod object;
mod identity;
mod inputs;
mod graph;
mod observations;
pub use observations::{code_facts, image_code_facts, source_facts, image_source_facts};
pub use graph::{Dependency, Output, Unit, UnitKind, compile_graph};
pub(crate) use inputs::Inputs;
pub use identity::{Declaration, Inputs as InputFacts, Selection, SourceInput, SourceInputs};
pub use object::{ObjectCompilation, ObjectKind, compile_object};
use crate::{file_bytes::FileBytes, native::program::ModuleLink};
use crate::{ExecutableProgram, TemplateProgram, ast::Target, resource};
use std::{
    collections::BTreeSet,
    ffi::c_void,
    io::ErrorKind,
    path::{Path, PathBuf},
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub type Program = resource::Image<ExecutableProgram>;
pub type Template = resource::Image<TemplateProgram>;
pub struct Compilation {
    pub programs: Vec<Program>,
    pub templates: Vec<Template>,
    pub artifact: PathBuf,
    /// 根代码是否复用；当前模块代码仍由原链接器消费。
    pub reused: bool,
}
const BUILD_IMAGE_MAGIC: u64 = u64::from_le_bytes(*b"BLBI\x01\0\0\0");

/// output 是调用方拥有的生成目录；按本次实际装入的源码和编译器实现复用入口。
pub fn compile_entry(
    path: &Path,
    root: &Path,
    entry: &str,
    target: Target,
    output: &Path,
) -> Result<Compilation> {
    crate::Compiler::available()?.build_entry(path, root, entry, target, output)
}
/// 编译后模块由本次调用拥有；借用公开接口后将原代码交给链接器。
pub fn compile_entry_with_inputs(
    path: &Path, root: &Path, entry: &str, target: Target, output: &Path,
    inputs: Vec<crate::object::Input>,
) -> Result<Compilation> {
    crate::Compiler::available()?.build_entry_with_inputs(path, root, entry, target, output, inputs)
}
impl crate::Compiler {
    pub fn build_entry(
        &self, path: &Path, root: &Path, entry: &str, target: Target, output: &Path,
    ) -> Result<Compilation> {
        self.build_entry_with_inputs(path, root, entry, target, output, Vec::new())
    }
    pub fn build_entry_with_inputs(
        &self, path: &Path, root: &Path, entry: &str, target: Target, output: &Path,
        dependencies: Vec<crate::object::Input>,
    ) -> Result<Compilation> {
        self.execute(|worker| compile_in(worker, path, root, entry, target, EntryOutput::Directory(output), dependencies, &[], None, None))
            .map_err(|error| match error {
                crate::parallel::Error::Work(error) => error,
                crate::parallel::Error::Resource(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
            })
    }
}
struct Artifact<'a> {
    path: &'a Path,
    replace_existing: bool,
}

enum EntryOutput<'a> {
    Directory(&'a Path),
    Artifact(Artifact<'a>),
}

fn entry_backend(target: Target) -> Result<&'static str> {
    match target {
        Target::Cpu => Ok("cpu"),
        Target::Gpu => Ok("gpu"),
        Target::Compute => Err("构建映像必须选择实际CPU或GPU后端".into()),
    }
}

fn entry_artifact(path: &Path, root: &Path, entry: &str, target: &str, output: &Path) -> PathBuf {
    let request = [
        path.as_os_str().as_encoded_bytes().into(),
        root.as_os_str().as_encoded_bytes().into(),
        entry.as_bytes().into(),
        target.as_bytes().into(),
    ];
    output.join(format!("{}.blbi", hash::hex(&hash::fields(&request))))
}

fn compile_in(
    worker: &crate::compiler::Worker, path: &Path, root: &Path, entry: &str,
    target: Target, output: EntryOutput<'_>, dependencies: Vec<crate::object::Input>, borrowed: &[&crate::object::Object], inputs: Option<&Inputs>,
    entry_checks: Option<&crate::frontend::EntryChecks>,
) -> Result<Compilation> {
    let target_name = entry_backend(target)?;
    let path = if inputs.is_some() { path.into() } else { crate::source_path::canonicalize(path)? };
    let root = if inputs.is_some() { root.into() } else { crate::source_path::canonicalize(root)? };
    let mut imports = crate::module::Imports::inputs(&dependencies).map_err(|e| std::io::Error::other(e.to_string()))?;
    for object in borrowed { imports.object(object).map_err(|e| std::io::Error::other(e.to_string()))?; }
    let imported = if dependencies.is_empty() && borrowed.is_empty() { None } else { Some(&imports) };
    let (world, module, _) = crate::read_world(&path, &root, imported, &[], worker, inputs,
        Some(std::cell::RefCell::new(std::collections::BTreeSet::new())))?;
    let (artifact, replace_existing) = match output {
        EntryOutput::Directory(output) => (entry_artifact(&path, &root, entry, target_name, output), true),
        EntryOutput::Artifact(output) => (output.path.to_owned(), output.replace_existing),
    };
    let source = identity::sources(&path, &root,
        Selection::Entry { entry: entry.into(), target: target_name.into() },
        worker.compiler().max_parameter_bytes(), &world, &module, false)?;
    match replace_existing.then(|| FileBytes::read_path(&artifact)) {
        Some(Ok(bytes)) => {
            let (identity, stored_image) = decode(&bytes)?;
            if let Some(inputs) = identity::match_source(identity, &source)? {
                let mut image = unsafe { resource::from_compiler_output(stored_image) }
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                let link = entry_link(&mut image);
                if inputs.imports == references(&world.declarations, imported, link.as_ref(), inputs.declarations.iter()) {
                    if imported.is_some() && let Some(link) = link
                        && link_entry(link, dependencies, borrowed, &world.declarations)?
                    {
                        let encoded = resource::compiler_output(&image);
                        write_artifact(&artifact, replace_existing, identity, &encoded)?;
                    }
                    return Ok(images(image, artifact, true));
                }
            }
        }
        Some(Err(error)) if error.kind() != ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let compilation =
        crate::compile_world(&world, module, true, Some((crate::EntryKind::Domain, entry)), entry_checks)?;
    let compiled_images = compilation
        .programs
        .into_iter()
        .map(|p| TemplateProgram(p.0))
        .chain(compilation.templates)
        .map(|p| {
            if target == Target::Cpu {
                crate::native::program::compile_in(worker, p).map(resource::Image::Cpu)
            } else {
                crate::spirv::program::compile_in(worker, &p).map(resource::Image::Gpu)
            }
            .map_err(std::io::Error::other)
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut image = resource::Module {
        types: Vec::new(),
        images: compiled_images,
    };
    let direct = world.build_declarations.as_ref().unwrap().borrow();
    let declarations = used_declarations(&world, &direct);
    let link = entry_link(&mut image);
    let imports_identity = references(&world.declarations, imported, link.as_ref(), declarations.iter().copied());
    if imported.is_some() && let Some(link) = link {
        link_entry(link, dependencies, borrowed, &world.declarations)?;
    }
    let identity = identity::encode(&source, &imports_identity, None, &declarations);
    let encoded = resource::compiler_output(&image);
    crate::file_bytes::create_dir_all(artifact.parent().unwrap())?;
    write_artifact(&artifact, replace_existing, &identity, &encoded)?;
    Ok(images(image, artifact, false))
}
#[repr(C)]
struct DeclarationHost {
    next: unsafe extern "C" fn(*mut c_void) -> *const c_void,
    retain: unsafe extern "C" fn(*mut c_void, *const c_void) -> bool,
    expand: unsafe extern "C" fn(*mut c_void, *const c_void),
}

unsafe extern "C" {
    #[link_name = "bootstrap.build_digest::complete_declarations"]
    fn complete_declarations(context: *mut c_void, host: DeclarationHost);
}

struct DeclarationClosure<'a> {
    world: &'a crate::frontend::World,
    pending: Vec<std::collections::btree_set::Iter<'a, Declaration>>,
    used: BTreeSet<&'a Declaration>,
}

fn constant_inputs<'a>(world: &'a crate::frontend::World, declaration: &Declaration) -> Option<&'a BTreeSet<Declaration>> {
    let Declaration::Constant(key) = declaration else { return None; };
    world.functions[key].constant.get().map(|value| &value.inputs).filter(|inputs| !inputs.is_empty())
}

unsafe extern "C" fn next_declaration(context: *mut c_void) -> *const c_void {
    let closure = unsafe { &mut *context.cast::<DeclarationClosure<'_>>() };
    while let Some(inputs) = closure.pending.last_mut() {
        if let Some(declaration) = inputs.next() {
            return ptr::from_ref(declaration).cast();
        }
        closure.pending.pop();
    }
    ptr::null()
}

unsafe extern "C" fn retain_declaration(context: *mut c_void, declaration: *const c_void) -> bool {
    let closure = unsafe { &mut *context.cast::<DeclarationClosure<'_>>() };
    // The cached inputs remain owned by the world for this closure.
    let declaration = unsafe { &*declaration.cast::<Declaration>() };
    closure.used.insert(declaration)
}

unsafe extern "C" fn expand_declaration(context: *mut c_void, declaration: *const c_void) {
    let closure = unsafe { &mut *context.cast::<DeclarationClosure<'_>>() };
    let declaration = unsafe { &*declaration.cast::<Declaration>() };
    if let Some(inputs) = constant_inputs(closure.world, declaration) {
        closure.pending.push(inputs.iter());
    }
}

fn used_declarations<'a>(
    world: &'a crate::frontend::World, direct: &'a BTreeSet<Declaration>,
) -> Vec<&'a Declaration> {
    let pending = direct.iter().filter_map(|declaration| constant_inputs(world, declaration).map(BTreeSet::iter)).collect();
    let used = direct.iter().collect();
    let mut closure = DeclarationClosure { world, pending, used };
    unsafe {
        complete_declarations(ptr::from_mut(&mut closure).cast(), DeclarationHost {
            next: next_declaration, retain: retain_declaration, expand: expand_declaration,
        });
    }
    closure.used.into_iter().collect()
}

fn entry_link(image: &mut resource::Module) -> Option<ModuleLink<'_>> {
    // The selected entry produces one root; module_link visits its nested CPU resources.
    image.images.iter_mut().find_map(|image| match image {
        resource::Image::Cpu(image) => Some(image.module_link()),
        resource::Image::Gpu(_) => None,
    })
}

fn references<'a>(
    declarations: &crate::Declarations, imported: Option<&crate::module::Imports>,
    link: Option<&ModuleLink<'_>>, used: impl ExactSizeIterator<Item = &'a Declaration>,
) -> [u8; 32] {
    let symbols = link.into_iter().flat_map(|link| link.external.iter().map(crate::names::ModuleName::as_str));
    identity::references(declarations, imported, symbols, used)
}

fn link_entry(
    link: ModuleLink<'_>, inputs: Vec<crate::object::Input>,
    borrowed: &[&crate::object::Object], declarations: &crate::Declarations,
) -> std::io::Result<bool> {
    crate::native::program::link_inputs(link, inputs, borrowed, |names| {
        crate::linker::bind_root_contracts(names, |name| {
            declarations.compiled_function(name).map(|(_, _, contract)| contract)
        })
    }).map_err(std::io::Error::other)
}

#[repr(C)]
struct ByteField {
    bytes: *const u8,
    length: usize,
}

impl From<&[u8]> for ByteField {
    fn from(bytes: &[u8]) -> Self {
        Self { bytes: bytes.as_ptr(), length: bytes.len() }
    }
}

#[repr(u32)]
#[expect(dead_code, reason = "the B decoder produces every status")]
enum BuildImageStatus {
    Read = 0,
    Unsupported = 1,
    Truncated = 2,
    Trailing = 3,
}

#[repr(C)]
struct BuildImage {
    identity: ByteField,
    image: ByteField,
    status: BuildImageStatus,
}

unsafe extern "C" {
    #[link_name = "bootstrap.metadata::build_image_header"]
    fn build_image_header(magic: u64, identity_bytes: u64, image_bytes: u64, output: *mut u8);
    #[link_name = "bootstrap.metadata_read::read_build_image"]
    fn read_build_image(input: ByteField, magic: u64) -> BuildImage;
}

fn write_artifact(path: &Path, replace_existing: bool, identity: &[u8], image: &[u8]) -> Result<()> {
    let mut header = [0u8; 3 * size_of::<u64>()];
    unsafe {
        build_image_header(BUILD_IMAGE_MAGIC, identity.len() as u64, image.len() as u64, header.as_mut_ptr());
    }
    publish(path, replace_existing, [&header[..16], identity, &header[16..], image])
}

fn read_envelope(bytes: &[u8]) -> Result<Option<(&[u8], &[u8])>> {
    let fields = unsafe { read_build_image(bytes.into(), BUILD_IMAGE_MAGIC) };
    match fields.status {
        BuildImageStatus::Read => {
            // B returns two ranges borrowed from this same input snapshot.
            let slice = |field: ByteField| unsafe {
                std::slice::from_raw_parts(field.bytes, field.length)
            };
            Ok(Some((slice(fields.identity), slice(fields.image))))
        }
        BuildImageStatus::Unsupported => Ok(None),
        BuildImageStatus::Truncated => Err(std::io::Error::other("模块接口被截断").into()),
        BuildImageStatus::Trailing => Err(std::io::Error::other("模块接口有未消费内容").into()),
    }
}

fn decode(bytes: &[u8]) -> Result<(&[u8], &[u8])> {
    read_envelope(bytes)?.ok_or_else(|| "不支持的入口构建映像".into())
}

/// Reads the request and consumed source identities stored in an actual artifact.
/// No source files, compiler workers, native code or device are opened.
pub fn input_facts(path: &Path) -> Result<InputFacts> {
    decode_input_facts(&FileBytes::read_path(path)?)
}

/// Returns the original build facts and SHA-256 of the same artifact bytes.
pub fn input_facts_with_sha256(path: &Path) -> Result<(InputFacts, String)> {
    let bytes = FileBytes::read_path(path)?;
    let facts = decode_input_facts(&bytes)?;
    Ok((facts, hash::hex(&hash::digest(&bytes))))
}

fn decode_input_facts(bytes: &[u8]) -> Result<InputFacts> {
    let identity = match read_envelope(bytes)? {
        Some((identity, _)) => identity,
        None => crate::object::ObjectReader::new(bytes)
            .and_then(|reader| reader.build_inputs())
            .map_err(|error| std::io::Error::other(error.to_string()))?
            .ok_or("该对象未保存构建输入")?,
    };
    identity::decode(identity)?
        .ok_or_else(|| "产物未保存当前结构化构建输入；不能从旧摘要还原".into())
}
/// Extracts the original resource-ABI image from a build artifact.
/// Does not interpret build inputs, recompile, load code, or change the source artifact.
/// Code trust and execution contracts remain with the resource decoder and runtime.
pub fn execution_image(path: &Path) -> Result<Vec<u8>> {
    with_execution_image(path, |image| Ok(image.to_vec()))
}

/// Delivers the compiler tool's decoded image to its new output file.
#[doc(hidden)]
pub fn write_execution_image(path: &Path, output: &Path) -> Result<usize> {
    with_execution_image(path, |image| {
        crate::file_bytes::create_new(output, image)?;
        Ok(image.len())
    })
}

fn with_execution_image<T>(path: &Path, consume: impl FnOnce(&[u8]) -> Result<T>) -> Result<T> {
    let bytes = FileBytes::read_path(path)?;
    consume(decode(&bytes)?.1)
}
/// 独立执行可只装入产物，不读取B源码或调用编译器后端。
///
/// # Safety
/// 文件必须是可信编译器产生且未修改的完整原生映像，保持资源ABI及真实前缀合同。
pub unsafe fn load(path: &Path) -> Result<Compilation> {
    unsafe { load_bytes(path, &FileBytes::read_path(path)?) }
}

/// Loads an execution artifact and returns the SHA-256 of the exact BLBI bytes
/// used to decode it. The digest identifies that read, even if the path is reused.
///
/// # Safety
/// The file must satisfy the same trust and execution contracts as [load].
pub unsafe fn load_with_sha256(path: &Path) -> Result<(Compilation, String)> {
    let bytes = FileBytes::read_path(path)?;
    let compilation = unsafe { load_bytes(path, &bytes) }?;
    Ok((compilation, hash::hex(&hash::digest(&bytes))))
}

unsafe fn load_bytes(path: &Path, bytes: &[u8]) -> Result<Compilation> {
    let (_, image) = decode(bytes)?;
    let module = unsafe { resource::from_compiler_output(image) }
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(images(module, path.into(), true))
}
fn images(module: resource::Module, artifact: PathBuf, reused: bool) -> Compilation {
    let mut programs = Vec::new();
    let mut templates = Vec::new();
    for image in module.images {
        if crate::ir::ProgramInterface::requires_bindings(&image) {
            templates.push(image);
        } else {
            // These images have no binding requirements.
            programs.push(unsafe { match image {
                resource::Image::Cpu(image) => resource::Image::Cpu(image.with_entry_type()),
                resource::Image::Gpu(image) => resource::Image::Gpu(image.with_entry_type()),
            } });
        }
    }
    Compilation {
        programs,
        templates,
        artifact,
        reused,
    }
}
fn publish<const N: usize>(path: &Path, replace_existing: bool, parts: [&[u8]; N]) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!(".blink-{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let temporary = path.with_file_name(name);
    Ok(crate::file_bytes::publish(path, &temporary, parts, replace_existing)?)
}
