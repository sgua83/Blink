//! 正式COFF对象的构建输入与最终产物复用；链接端不参与。
use super::{hash, publish, Result};
use crate::{module::codec::{Decode, Encode, record}, object::Object};
use std::{fs, io::{ErrorKind, Error}, path::{Path, PathBuf}};
/// 与现有三个-c分支相同的产物选择。
#[derive(Clone, Copy)]
pub enum ObjectKind<'a> {
    Module(&'a [String]),
    Application(&'a str),
    Library { entry: &'a str, prefix: &'a str },
}
pub struct ObjectCompilation {
    pub object: Object,
    pub artifact: PathBuf,
    pub reused: bool,
}
const MAGIC: &[u8; 8] = b"BLBO\x01\0\0\0";
struct Inputs {
    source: [u8; 32],
    native: Option<[u8; 32]>,
}
record!(Inputs { source, native });
fn error(value: impl ToString) -> Error { Error::other(value.to_string()) }
pub fn compile_object(
    path: &Path, root: &Path, dependencies: &[Object], kind: ObjectKind<'_>, output: &Path,
) -> Result<ObjectCompilation> {
    crate::Compiler::available()?.build_object(path, root, dependencies, kind, output)
}
impl crate::Compiler {
    pub fn build_object(
        &self, path: &Path, root: &Path, dependencies: &[Object], kind: ObjectKind<'_>, output: &Path,
    ) -> Result<ObjectCompilation> {
        self.execute(|worker| compile(worker, path, root, dependencies, kind, output))
            .map_err(|value| match value {
                crate::parallel::Error::Work(error) => error,
                crate::parallel::Error::Resource(value) => Box::new(value) as Box<dyn std::error::Error + Send + Sync>,
            })
    }
}
fn compile(
    worker: &crate::compiler::Worker, path: &Path, root: &Path, dependencies: &[Object],
    kind: ObjectKind<'_>, output: &Path,
) -> Result<ObjectCompilation> {
    let source_path = path;
    let source_root = root;
    // .h是模块来源；纯类型模块不依赖伪造的.b。
    let path = path.with_extension("h").canonicalize()?;
    let root = root.canonicalize()?;
    let artifact = std::path::absolute(output)?;
    let imports = if matches!(kind, ObjectKind::Library { .. }) { None }
        else { Some(crate::module::Imports::new(dependencies).map_err(|value| crate::ast::Diagnostic {
            source: crate::ast::Source { path: source_path.into(), text: "".into() },
            span: crate::ast::Span::default(), code: "B270", message: value.to_string(),
        })?) };
    let imported = imports.as_ref().filter(|_| !dependencies.is_empty());
    let (world, module) = crate::read_world(source_path, source_root, imported, worker)?;
    let mut bytes = Vec::new();
    for value in [path.as_os_str().as_encoded_bytes(), root.as_os_str().as_encoded_bytes(),
        env!("BLINK_COMPILER_ID").as_bytes()] { hash::field(&mut bytes, value); }
    match kind {
        ObjectKind::Module(instances) => {
            hash::field(&mut bytes, b"module");
            hash::field(&mut bytes, &instances.bytes());
        }
        ObjectKind::Application(entry) => {
            hash::field(&mut bytes, b"application"); hash::field(&mut bytes, entry.as_bytes());
        }
        ObjectKind::Library { entry, prefix } => {
            hash::field(&mut bytes, b"library"); hash::field(&mut bytes, entry.as_bytes());
            hash::field(&mut bytes, prefix.as_bytes());
        }
    }
    for (name, source) in world.interfaces.iter().chain(&world.sources) {
        hash::field(&mut bytes, name.as_bytes());
        hash::field(&mut bytes, source.path.as_os_str().as_encoded_bytes());
        hash::field(&mut bytes, source.text.as_bytes());
    }
    if let Some(imported) = imported {
        for name in world.interfaces.keys() {
            if let Some(input) = imported.modules.get(name) {
                hash::field(&mut bytes, name.as_bytes());
                hash::field(&mut bytes, &input.definitions.bytes());
                hash::field(&mut bytes, &input.requirements.len().to_le_bytes());
                for (name, requirement) in &input.requirements {
                    hash::field(&mut bytes, name.as_bytes());
                    hash::field(&mut bytes, &[match requirement {
                        crate::execution::Requirement::Invocation => 0,
                        crate::execution::Requirement::Domain => 1,
                        crate::execution::Requirement::Host => 2,
                    }]);
                }
            }
        }
    }
    let source = hash::digest(&bytes);
    let runtime = !matches!(kind, ObjectKind::Module(_));
    match fs::read(&artifact) {
        Ok(bytes) => {
            let reader = crate::object::ObjectReader::new(&bytes).map_err(error)?;
            if let Some(stored) = reader.build_inputs().map_err(error)? {
                let stored = Inputs::from_bytes(stored.strip_prefix(MAGIC)
                    .ok_or_else(|| Error::other("不支持的目标文件构建信息"))?).map_err(error)?;
                if stored.source == source {
                    let native = stored.native.map(|_| crate::object::native_inputs(runtime)).transpose()?;
                    if stored.native == native {
                        let object = reader.object().map_err(error)?;
                        return Ok(ObjectCompilation { object, artifact, reused: true });
                    }
                }
            }
        }
        Err(value) if value.kind() == ErrorKind::NotFound => {}
        Err(value) => return Err(value.into()),
    }
    let object = match kind {
        ObjectKind::Module(instances) => {
            crate::validate_types(&world)?;
            crate::check::compile_native_module(&world, &module, instances)?.compile_in(worker).map_err(error)?
        }
        ObjectKind::Application(entry) => {
            crate::compile_world(world, module, false, Some((crate::EntryKind::Application, entry)))?
                .applications.remove(0).compile_in(worker).map_err(error)?
        }
        ObjectKind::Library { entry, prefix } => {
            let compilation = crate::compile_world(world, module, true, Some((crate::EntryKind::Domain, entry)))?;
            let program = compilation.programs.first().ok_or_else(|| Error::other("DLL 编译需要无前置条件的具体 program"))?;
            crate::dll::compile_in(worker, program, prefix).map_err(error)?
        }
    };
    let native = if object.archive.is_some() { Some(crate::object::native_inputs(runtime)?) } else { None };
    let mut stored = MAGIC.to_vec(); stored.extend(Inputs { source, native }.bytes());
    let bytes = object.build_bytes(&stored).map_err(error)?;
    fs::create_dir_all(artifact.parent().unwrap())?;
    publish(&artifact, &bytes)?;
    Ok(ObjectCompilation { object, artifact, reused: false })
}
