//! Windows x64 应用编译为 COFF 目标文件；PE 链接由独立 linker 模块负责。
pub(crate) use crate::resource::RUNTIME_LIBRARY;
use crate::{
    ast::Target,
    ir::{Program, Ty, ValueCode},
    native,
    object::{self, Kind, Object},
};
use std::path::Path;
mod tasks;
pub(crate) use tasks::bind_tasks;
pub(crate) mod factory;
pub struct Application {
    pub(crate) compiler: crate::Compiler,
    pub(crate) name: String,
    pub(crate) body: ValueCode,
    pub(crate) functions: Vec<crate::ir::FunctionBody>,
    pub(crate) imports: std::collections::BTreeMap<String, Vec<u8>>,
    pub(crate) resources: Option<Resources>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Resources {
    pub types: Vec<Ty>,
    pub programs: Vec<(Target, Program)>,
    pub completions: Vec<Vec<ElementCompletion>>,
    pub task_starts: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementCompletion {
    pub offset: u32,
    pub intervals: Vec<(u64, u64)>,
    pub known_bits: (u64, u64),
    pub limits: Vec<(usize, u32)>,
}
impl Application {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn compile(&self) -> Result<Object, Box<dyn std::error::Error>> {
        self.compiler.execute(|worker| self.compile_in(worker).map_err(|error| error.to_string()))
            .map_err(|error| std::io::Error::other(error.to_string()).into())
    }
    pub(crate) fn compile_in(&self, worker: &crate::compiler::Worker) -> Result<Object, Box<dyn std::error::Error>> {
        use std::borrow::Cow;
        let mut resources = self.resources.as_ref().map(Cow::Borrowed);
        let mut body = Cow::Borrowed(&self.body);
        let mut ordinary = Cow::Borrowed(self.functions.as_slice());
        let has_tasks = self.resources.as_ref().is_some_and(|resources| resources.task_starts);
        if has_tasks {
            bind_tasks(&self.name, Some(body.to_mut()), &self.functions, ordinary.to_mut(),
                &self.imports, resources.as_mut().map(Cow::to_mut)).map_err(std::io::Error::other)?;
        }
        let code = native::emit_application(&body).map_err(std::io::Error::other)?;
        let mut stack_bytes = code.stack_bytes;
        let mut functions = crate::parallel::try_map(worker, ordinary.iter().collect(), |_| (),
            |_, function| native::emit_function(function).map(|code| (function.name.clone(), code)))
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let (compiled_resources, resource_code, resource_imports) = compile_resources(&self.name, resources.as_deref(), true, worker, has_tasks)?;
        let resource_names: Vec<_> = resource_code.iter().map(|(name, _)| name.clone()).collect();
        functions.extend(resource_code);
        let mut imports = self.imports.clone();
        imports.extend(resource_imports);
        for (_, function) in &functions {
            stack_bytes = stack_bytes.max(function.stack_bytes);
        }
        let (sections, positions, stack) =
            object::application_code(code, functions, &imports, &resource_prefix(&self.name), &self.name)?;
        let symbols = std::iter::once("main".to_owned()).chain(resource_names).map(|name| object::Symbol {
            section: positions[&name].0, value: positions[&name].1,
            name, contract: Vec::new(), function: true,
        }).collect();
        let (archive, factory) = compiled_resources.unzip();
        let mut object = Object {
            name: self.name.clone(),
            kind: Kind::Application { stack_bytes },
            sections,
            stack,
            symbols,
            imports,
            archive,
        };
        if let Some(factory) = factory {
            factory.emit(&mut object, &positions);
        }
        Ok(object)
    }
    pub fn write_object(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        self.compile()?.write(path)
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.native_resources::module_prefix"]
    fn module_resource_prefix(name: *const u8, length: usize, output: *mut u8) -> usize;
}
pub(crate) fn resource_prefix(name: &str) -> String {
    let length = unsafe { module_resource_prefix(name.as_ptr(), name.len(), std::ptr::null_mut()) };
    let mut output = Vec::with_capacity(length);
    unsafe {
        module_resource_prefix(name.as_ptr(), name.len(), output.as_mut_ptr());
        output.set_len(length);
        String::from_utf8_unchecked(output)
    }
}
pub(crate) fn compile_resources(
    name: &str,
    resources: Option<&Resources>,
    root: bool,
    worker: &crate::compiler::Worker,
    has_tasks: bool,
) -> Result<(Option<(object::Archive, factory::ResourceFactory)>, Vec<(String, native::NativeCode)>, std::collections::BTreeMap<String, Vec<u8>>), Box<dyn std::error::Error>> {
    Ok(if let Some(resources) = resources {
        let prefix = resource_prefix(name);
        let (image, code, imports) = crate::resource::compile(resources, worker)?;
        let code: Vec<_> = code.into_iter().enumerate()
            .map(|(index, code)| (format!("__blink_range_{prefix}_{index}"), code)).collect();
        let types = u32::try_from(resources.types.len())?;
        let native = factory::wrappers(&prefix, resources, types, root, has_tasks)?;
        let mut exports = if root {
            [
                "blink_destroy",
                "blink_release",
                "blink_upload",
                "blink_download",
                "blink_address",
                "blink_storage_address",
                "blink_allocate",
                "blink_resize",
                "blink_run",
                "blink_complete_field",
                "blink_error_message",
                "blink_error_destroy",
                "blink_error_join",
                "blink_error_consume",
                "blink_error_consumed",
                "blink_task_start",
                "blink_task_submit",
                "blink_task_wait",
                "blink_application_failed",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        exports.extend((0..resources.types.len()).map(|i| format!("{prefix}_allocate_{i}")));
        exports.extend((0..resources.programs.len()).map(|i| format!("{prefix}_run_{i}")));
        exports.extend((0..resources.completions.len()).map(|i| format!("{prefix}_complete_{i}")));
        if root {
            exports.extend(["blink_create".into(), "blink_application_error".into()]);
        }
        if has_tasks { exports.push(format!("{prefix}_task_start")); }
        exports.push(format!("{prefix}_resources"));
        let archive = object::compile_archive(native, exports, root, worker)?;
        let factory = factory::ResourceFactory {
            binding: object::ResourceBinding {
                prefix,
                types,
                programs: u32::try_from(resources.programs.len())?,
                abi: crate::resource::ABI.to_vec(),
                code: code.iter().map(|(name, _)| name.clone()).collect(),
            },
            image,
            ranges_symbol: None,
        };
        (Some((archive, factory)), code, imports)
    } else {
        (None, Vec::new(), std::collections::BTreeMap::new())
    })
}
