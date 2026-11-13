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
use crate::{file_bytes::FileBytes, module::codec::Encode};
use crate::{ExecutableProgram, TemplateProgram, ast::Target, resource};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
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
        self.execute(|worker| compile_in(worker, path, root, entry, target, output, dependencies, &[], None))
            .map_err(|error| match error {
                crate::parallel::Error::Work(error) => error,
                crate::parallel::Error::Resource(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
            })
    }
}
fn compile_in(
    worker: &crate::compiler::Worker, path: &Path, root: &Path, entry: &str,
    target: Target, output: &Path, dependencies: Vec<crate::object::Input>, borrowed: &[&crate::object::Object], inputs: Option<&Inputs>,
) -> Result<Compilation> {
    if target == Target::Compute {
        return Err("构建映像必须选择实际CPU或GPU后端".into());
    }
    let path = if inputs.is_some() { path.into() } else { path.canonicalize()? };
    let root = if inputs.is_some() { root.into() } else { root.canonicalize()? };
    let mut imports = crate::module::Imports::inputs(&dependencies).map_err(|e| std::io::Error::other(e.to_string()))?;
    for object in borrowed { imports.object(object).map_err(|e| std::io::Error::other(e.to_string()))?; }
    let imported = if dependencies.is_empty() && borrowed.is_empty() { None } else { Some(&imports) };
    let (world, module) = crate::read_world(&path, &root, imported, &[], worker, inputs,
        Some(std::cell::RefCell::new(std::collections::BTreeSet::new())))?;
    let mut request = Vec::new();
    for value in [
        path.as_os_str().as_encoded_bytes(),
        root.as_os_str().as_encoded_bytes(),
        entry.as_bytes(),
    ] {
        hash::field(&mut request, value);
    }
    hash::field(
        &mut request,
        if target == Target::Cpu {
            b"cpu"
        } else {
            b"gpu"
        },
    );
    let artifact = output.join(format!("{}.blbi", hash::hex(&hash::digest(&request))));
    let source = identity::sources(&path, &root,
        Selection::Entry { entry: entry.into(), target: if target == Target::Cpu { "cpu" } else { "gpu" }.into() },
        worker.compiler().max_parameter_bytes(), &world, &module, false)?;
    match FileBytes::read_path(&artifact) {
        Ok(bytes) => {
            let (identity, stored_image) = decode(&bytes)?;
            if let Some(inputs) = identity::decode(identity)? && inputs.source.same_code(&source) {
                let mut image = unsafe { resource::from_compiler_output(stored_image) }
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                if inputs.imports == references(&world.declarations, imported, &image, &inputs.declarations) {
                    if imported.is_some() {
                        link_inputs(&mut image, dependencies, borrowed, &imports)?;
                        let encoded = encode_image(&image);
                        if stored_image != encoded {
                            write_artifact(&artifact, identity, &encoded)?;
                        }
                    }
                    return Ok(images(image, artifact, true));
                }
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let compilation =
        crate::compile_world(&world, module, true, Some((crate::EntryKind::Domain, entry)))?;
    let compiled_images = compilation
        .programs
        .into_iter()
        .map(|p| TemplateProgram(p.0))
        .chain(compilation.templates)
        .map(|p| {
            if target == Target::Cpu {
                crate::native::program::compile_in(worker, p).map(resource::Image::Cpu)
            } else {
                crate::spirv::program::compile_in(worker, p).map(resource::Image::Gpu)
            }
            .map_err(std::io::Error::other)
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut image = resource::Module {
        types: Vec::new(),
        images: compiled_images,
    };
    let declarations = used_declarations(&world);
    let imports_identity = references(&world.declarations, imported, &image, &declarations);
    if imported.is_some() { link_inputs(&mut image, dependencies, borrowed, &imports)?; }
    let identity = identity::Inputs { source, imports: imports_identity, native: None, declarations }.encode();
    let encoded = encode_image(&image);
    crate::file_bytes::create_dir_all(output)?;
    write_artifact(&artifact, &identity, &encoded)?;
    Ok(images(image, artifact, false))
}
fn used_declarations(world: &crate::frontend::World) -> Vec<Declaration> {
    let mut used = world.build_declarations.as_ref().unwrap().take();
    let mut constants = used.iter().filter_map(|declaration| match declaration {
        Declaration::Constant(key) => Some(key.clone()),
        _ => None,
    }).collect::<Vec<_>>();
    while let Some(key) = constants.pop() {
        if let Some(value) = world.functions[&key].constant.get() {
            for dependency in &value.inputs {
                if used.insert(dependency.clone()) && let Declaration::Constant(key) = dependency {
                    constants.push(key.clone());
                }
            }
        }
    }
    used.into_iter().collect()
}

fn references(
    declarations: &crate::Declarations, imported: Option<&crate::module::Imports>,
    image: &resource::Module, used: &[Declaration],
) -> [u8; 32] {
    let symbols = image.images.iter().filter_map(|image| match image {
        resource::Image::Cpu(image) => Some(image), _ => None,
    }).flat_map(|image| crate::native::program::module_symbols(image).1);
    identity::references(declarations, imported, symbols, used)
}
fn link_inputs(
    image: &mut resource::Module, mut inputs: Vec<crate::object::Input>, borrowed: &[&crate::object::Object], imports: &crate::module::Imports,
) -> Result<()> {
    let contracts = imports.modules.values().flat_map(|module| module.definitions.iter())
        .map(|(name, contract)| (name.as_str(), contract)).collect();
    // A selected entry produces one CPU or GPU image; its nested resources stay owned by it.
    for image in &mut image.images {
        if let resource::Image::Cpu(image) = image {
            crate::native::program::link_inputs(image, std::mem::take(&mut inputs), borrowed, &contracts)
                .map_err(std::io::Error::other)?;
        }
    }
    Ok(())
}
fn encode_image(image: &resource::Module) -> Vec<u8> {
    let mut encoded = resource::ABI.to_vec();
    encoded.extend(image.bytes());
    encoded
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
    #[link_name = "bootstrap.metadata::encode_build_image"]
    fn encode_build_image(magic: u64, identity: ByteField, image: ByteField, output: *mut u8);
    #[link_name = "bootstrap.metadata_read::read_build_image"]
    fn read_build_image(input: ByteField, magic: u64) -> BuildImage;
}

fn write_artifact(path: &Path, identity: &[u8], image: &[u8]) -> Result<()> {
    let length = 3 * size_of::<u64>() + identity.len() + image.len();
    let mut bytes = Vec::with_capacity(length);
    unsafe {
        encode_build_image(BUILD_IMAGE_MAGIC, identity.into(), image.into(), bytes.as_mut_ptr());
        bytes.set_len(length);
    }
    publish(path, &bytes)
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
    let bytes = FileBytes::read_path(path)?;
    Ok(decode(&bytes)?.1.to_vec())
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
            // 同一编译映像的空前置合同；不构造IR或重新生成机器码。
            programs.push(match image {
                resource::Image::Cpu(image) => {
                    let mut image = image;
                    let module_code = image.module_code.take();
                    let (plan, code, functions,resources) = image.into_parts();
                    let mut image = unsafe { crate::native::ProgramImage::from_compiler_output(plan, code, functions,resources) };
                    image.module_code = module_code;
                    resource::Image::Cpu(image)
                }
                resource::Image::Gpu(image) => {
                    let (plan, code) = image.into_parts();
                    resource::Image::Gpu(unsafe {
                        crate::spirv::ProgramImage::from_compiler_output(plan, code)
                    })
                }
            });
        }
    }
    Compilation {
        programs,
        templates,
        artifact,
        reused,
    }
}
fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!("{}.{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let extension: Vec<_> = name.encode_utf16().collect();
    let temporary = crate::source_path::with_extension_units(path, &extension);
    Ok(crate::file_bytes::publish(path, &temporary, bytes)?)
}
