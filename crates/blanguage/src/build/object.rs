//! 正式COFF对象的构建输入与最终产物复用；链接端不参与。
use super::{hash, publish, Result};
use crate::{module::codec::{Decode, Encode, record, variants}, object::Object};
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
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Declaration {
    Constant(String),
    Record(String),
    TypeSet(String),
    External(String),
    ValueName { name: String, module: String, imports: Vec<String> },
    RecordName { name: String, module: String, imports: Vec<String> },
    TypeSetName { name: String, module: String, imports: Vec<String> },
}
variants!(Declaration { 0 => Constant(name), 1 => Record(name), 2 => TypeSet(name), 3 => External(name), 4 => ValueName { name, module, imports }, 5 => RecordName { name, module, imports }, 6 => TypeSetName { name, module, imports } });
const MAGIC: &[u8; 8] = b"BLBO\x09\0\0\0";
struct Inputs {
    source: [u8; 32],
    imports: [u8; 32],
    native: Option<[u8; 32]>,
    declarations: Vec<Declaration>,
}
record!(Inputs { source, imports, native, declarations });
fn error(value: impl ToString) -> Error { Error::other(value.to_string()) }
fn import_identity(declarations: &crate::Declarations, imported: Option<&crate::module::Imports>, object: &Object, used: &[Declaration]) -> [u8; 32] {
    let mut bytes = Vec::new();
    for symbol in object.imports.keys() {
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

fn source_identity(bytes: &mut Vec<u8>, declarations: &crate::Declarations, source: &crate::ast::Source, imported: bool) {
    if !imported {
        hash::field(bytes, source.text.as_bytes());
        return;
    }
    let mut spans = declarations.functions.values()
        .filter(|definition| definition.source.path == source.path
            && (definition.compiled.is_some() || definition.function.external.is_some()
                || definition.function.kind == crate::ast::FunctionKind::Constant
                    && !definition.function.name.contains("::")))
        .map(|definition| definition.function.span).collect::<Vec<_>>();
    // 声明名称由实际查询绑定交接，正文由原内容键交接。
    for (record, ..) in declarations.records.values().filter(|(_, owner, ..)| owner.path == source.path) {
        spans.push(record.span);
    }
    for (set, ..) in declarations.type_sets.values().filter(|(_, owner, ..)| owner.path == source.path) {
        spans.push(set.span);
    }
    // 模块与import在声明之前；声明间／后的空白注释不产生额外身份。
    let end = spans.iter().map(|span| span.start).min().unwrap_or(source.text.len());
    hash::field(bytes, source.text[..end].trim_end().as_bytes());
}

pub fn compile_object(
    path: &Path, root: &Path, dependencies: &[Object], kind: ObjectKind<'_>, output: &Path,
) -> Result<ObjectCompilation> {
    crate::Compiler::available()?.build_object(path, root, dependencies, kind, output)
}
impl crate::Compiler {
    pub fn build_object(
        &self, path: &Path, root: &Path, dependencies: &[Object], kind: ObjectKind<'_>, output: &Path,
    ) -> Result<ObjectCompilation> {
        self.execute(|worker| compile(worker, path, root, dependencies, kind, output, None))
            .map_err(|value| match value {
                crate::parallel::Error::Work(error) => error,
                crate::parallel::Error::Resource(value) => Box::new(value) as Box<dyn std::error::Error + Send + Sync>,
            })
    }
}
pub(super) fn compile<'a>(
    worker: &crate::compiler::Worker, path: &Path, root: &Path,
    dependencies: impl IntoIterator<Item = &'a Object>,
    kind: ObjectKind<'_>, output: &Path, inputs: Option<&super::Inputs>,
) -> Result<ObjectCompilation> {
    let source_path = path;
    let source_root = root;
    // .h是模块来源；纯类型模块不依赖伪造的.b。
    let path = if inputs.is_some() { path.with_extension("h") } else { path.with_extension("h").canonicalize()? };
    let root = if inputs.is_some() { root.into() } else { root.canonicalize()? };
    let artifact = std::path::absolute(output)?;
    let imports = if matches!(kind, ObjectKind::Library { .. }) { None }
        else { Some(crate::module::Imports::new(dependencies).map_err(|value| crate::ast::Diagnostic {
            source: crate::ast::Source { path: source_path.into(), text: "".into() },
            span: crate::ast::Span::default(), code: "B270", message: value.to_string(),
        })?) };
    let imported = imports.as_ref().filter(|imports| !imports.modules.is_empty());
    let (world, module) = crate::read_world(source_path, source_root, imported, worker, inputs,
        Some(std::cell::RefCell::new(std::collections::BTreeSet::new())))?;
    let definitions = &world.declarations;
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
        source_identity(&mut bytes, definitions, source, name != &module && imported.is_some_and(|imports| imports.modules.contains_key(name)));
    }
    let source = hash::digest(&bytes);
    let runtime = !matches!(kind, ObjectKind::Module(_));
    match fs::read(&artifact) {
        Ok(bytes) => {
            let reader = crate::object::ObjectReader::new(&bytes).map_err(error)?;
            if let Some(stored) = reader.build_inputs().map_err(error)?
                .and_then(|stored| stored.strip_prefix(MAGIC))
            {
                let stored = Inputs::from_bytes(stored).map_err(error)?;
                if stored.source == source {
                    let object = reader.object().map_err(error)?;
                    if stored.imports == import_identity(definitions, imported, &object, &stored.declarations) {
                        let native = stored.native.map(|_| crate::object::native_inputs(runtime)).transpose()?;
                        if stored.native == native {
                            return Ok(ObjectCompilation { object, artifact, reused: true });
                        }
                    }
                }
            }
        }
        Err(value) if value.kind() == ErrorKind::NotFound => {}
        Err(value) => return Err(value.into()),
    }
    let object = match kind {
        ObjectKind::Module(instances) => {
            crate::check::compile_native_module(&world, &module, instances)?.compile_in(worker).map_err(error)?
        }
        ObjectKind::Application(entry) => {
            crate::compile_world(&world, module, false, Some((crate::EntryKind::Application, entry)))?
                .applications.remove(0).compile_in(worker).map_err(error)?
        }
        ObjectKind::Library { entry, prefix } => {
            let compilation = crate::compile_world(&world, module, true, Some((crate::EntryKind::Domain, entry)))?;
            let program = compilation.programs.first().ok_or_else(|| Error::other("DLL 编译需要无前置条件的具体 program"))?;
            crate::dll::compile_in(worker, program, prefix).map_err(error)?
        }
    };
    let native = if object.archive.is_some() { Some(crate::object::native_inputs(runtime)?) } else { None };
    let declarations = world.build_declarations.as_ref().unwrap().borrow().iter().cloned().collect::<Vec<_>>();
    let imports = import_identity(definitions, imported, &object, &declarations);
    let mut stored = MAGIC.to_vec(); stored.extend(Inputs { source, imports, native, declarations }.bytes());
    let bytes = object.build_bytes(&stored).map_err(error)?;
    fs::create_dir_all(artifact.parent().unwrap())?;
    publish(&artifact, &bytes)?;
    Ok(ObjectCompilation { object, artifact, reused: false })
}
