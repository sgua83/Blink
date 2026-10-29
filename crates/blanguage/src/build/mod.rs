//! 正式入口构建：由实际源码闭包产生原生映像，执行端装入最终代码。
pub(crate) mod hash;
mod object;
pub use object::{ObjectCompilation, ObjectKind, compile_object};
use crate::module::codec::{Decode, Encode, record};
use crate::{ExecutableProgram, TemplateProgram, ast::Target, resource};
use std::{
    fs,
    io::{ErrorKind, Write},
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
    pub reused: bool,
}
const MAGIC: &[u8; 8] = b"BLBI\x01\0\0\0";
struct Artifact {
    identity: Vec<u8>,
    image: Vec<u8>,
}
record!(Artifact { identity, image });

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
impl crate::Compiler {
    pub fn build_entry(
        &self, path: &Path, root: &Path, entry: &str, target: Target, output: &Path,
    ) -> Result<Compilation> {
        self.execute(|worker| compile_in(worker, path, root, entry, target, output))
            .map_err(|error| match error {
                crate::parallel::Error::Work(error) => error,
                crate::parallel::Error::Resource(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
            })
    }
}
fn compile_in(
    worker: &crate::compiler::Worker, path: &Path, root: &Path, entry: &str,
    target: Target, output: &Path,
) -> Result<Compilation> {
    if target == Target::Compute {
        return Err("构建映像必须选择实际CPU或GPU后端".into());
    }
    let path = path.canonicalize()?;
    let root = root.canonicalize()?;
    let (world, module) = crate::read_world(&path, &root, None, worker)?;
    let mut request = Vec::new();
    for value in [
        path.to_str().ok_or("入口路径不是UTF-8")?,
        root.to_str().ok_or("模块根不是UTF-8")?,
        entry,
    ] {
        hash::field(&mut request, value.as_bytes());
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
    let mut inputs = request;
    hash::field(&mut inputs, env!("BLINK_COMPILER_ID").as_bytes());
    for (name, source) in world.interfaces.iter().chain(&world.sources) {
        hash::field(&mut inputs, name.as_bytes());
        hash::field(
            &mut inputs,
            source.path.to_str().ok_or("源码路径不是UTF-8")?.as_bytes(),
        );
        hash::field(&mut inputs, source.text.as_bytes());
    }
    let identity = hash::digest(&inputs).to_vec();
    match fs::read(&artifact) {
        Ok(bytes) => {
            let stored = decode(&bytes)?;
            if stored.identity == identity {
                return stored.compilation(artifact, true);
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let compilation =
        crate::compile_world(world, module, true, Some((crate::EntryKind::Domain, entry)))?;
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
    let image = resource::Module {
        types: Vec::new(),
        images: compiled_images,
    };
    let mut encoded = resource::ABI.to_vec();
    encoded.extend(image.bytes());
    let stored = Artifact {
        identity,
        image: encoded,
    };
    fs::create_dir_all(output)?;
    let mut bytes = MAGIC.to_vec();
    bytes.extend(stored.bytes());
    publish(&artifact, &bytes)?;
    Ok(images(image, artifact, false))
}
fn decode(bytes: &[u8]) -> Result<Artifact> {
    let bytes = bytes.strip_prefix(MAGIC).ok_or("不支持的入口构建映像")?;
    Artifact::from_bytes(bytes).map_err(|e| std::io::Error::other(e.to_string()).into())
}
/// 独立执行可只装入产物，不读取B源码或调用编译器后端。
///
/// # Safety
/// 文件必须是可信编译器产生且未修改的完整原生映像，保持资源ABI及真实前缀合同。
pub unsafe fn load(path: &Path) -> Result<Compilation> {
    let stored = decode(&fs::read(path)?)?;
    stored.compilation(path.into(), true)
}
impl Artifact {
    fn compilation(self, artifact: PathBuf, reused: bool) -> Result<Compilation> {
        let module = unsafe { resource::from_compiler_output(&self.image) }
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(images(module, artifact, reused))
    }
}
fn images(module: resource::Module, artifact: PathBuf, reused: bool) -> Compilation {
    let mut programs = Vec::new();
    let mut templates = Vec::new();
    for image in module.images {
        if !image.prefix_requirements.is_empty() {
            templates.push(image);
        } else {
            // 同一编译映像的空前置合同；不构造IR或重新生成机器码。
            programs.push(match image {
                resource::Image::Cpu(image) => {
                    let (plan, code) = image.into_parts();
                    resource::Image::Cpu(unsafe {
                        crate::native::ProgramImage::from_compiler_output(plan, code)
                    })
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
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::File::create_new(&temporary)?;
    let result = file.write_all(bytes);
    drop(file);
    if let Err(error) = result.and_then(|_| fs::rename(&temporary, path)) {
        fs::remove_file(&temporary).map_err(|cleanup| {
            std::io::Error::other(format!("{error}；删除未完成产物：{cleanup}"))
        })?;
        return Err(error.into());
    }
    Ok(())
}
