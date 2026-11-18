//! 算法包装交接同次编译的执行布局及已链接CPU入口；GPU交接原完整映像。
use crate::{ExecutableProgram, application::factory::ResourceFactory, ast::Target};

fn code_declarations(names: &[String]) -> String {
    names.iter().enumerate().map(|(index, name)|
        format!("#[link_name={name:?}] fn range_{index}();")).collect::<Vec<_>>().join("\n")
}
pub(super) fn constructor(
    p: &ExecutableProgram, prefix: &str, worker: &crate::compiler::Worker,
) -> Result<(String, Vec<(String, crate::native::NativeCode)>, Option<ResourceFactory>), Box<dyn std::error::Error>> {
    use std::fmt::Write;
    let mut images = String::new();
    let mut functions = Vec::new();
    let mut resources = None;
    if p.target() == Target::Gpu {
        images.push_str("let cpu = core::ptr::null();\n");
    } else {
        let image = crate::native::program::compile_in(worker, p.clone()).map_err(std::io::Error::other)?;
        let mut image = image.map_code_with(&mut |name, code| {
            let index = functions.len();
            let name = name.map(str::to_owned).unwrap_or_else(|| format!("__blink_range_{prefix}_{index}"));
            functions.push((name, code));
            index
        });
        let module = image.resources.take();
        let bytes = crate::resource::linked_output(&image);
        let names: Vec<_> = functions.iter().map(|(name, _)| name.clone()).collect();
        let count = names.len();
        let code = if let Some(module) = module {
            let resource_prefix = crate::application::resource_prefix(p.name());
            let ranges = format!("{resource_prefix}_ranges");
            writeln!(images, "unsafe extern \"C\" {{ #[link_name={ranges:?}] static CODE: [usize; {count}]; }}")?;
            resources = Some(ResourceFactory {
                binding: crate::object::ResourceBinding {
                    prefix: resource_prefix, types: u32::try_from(module.types.len())?,
                    programs: u32::try_from(module.images.len())?,
                    abi: crate::resource::ABI.to_vec(), code: names,
                },
                image: crate::resource::linked_output(&module),
                ranges_symbol: Some(ranges),
            });
            "core::ptr::addr_of!(CODE).cast()"
        } else {
            let declarations = code_declarations(&names);
            let addresses = (0..count).map(|index| format!("range_{index} as *const u8")).collect::<Vec<_>>().join(",");
            writeln!(images, "unsafe extern \"C\" {{ {declarations} }}\nlet code: [*const u8; {count}] = [{addresses}];")?;
            "code.as_ptr()"
        };
        writeln!(images, "let bytes: &[u8] = &{bytes:?};\nlet cpu = &ModuleImage {{ data: bytes.as_ptr(), bytes: bytes.len(), code: {code}, codes: {count} }};")?;
    }
    if resources.is_some() {
        images.push_str(r#"
unsafe extern "C" {
    static __blink_module_count: usize;
    static __blink_module_factories: ModuleFactory;
}
let factories = core::ptr::addr_of!(__blink_module_factories);
let factory_count = unsafe { __blink_module_count };
"#);
    } else {
        images.push_str("let factories = core::ptr::null();\nlet factory_count = 0;\n");
    }
    if p.target() == Target::Cpu {
        images.push_str("let gpu = core::ptr::null();\n");
    } else {
        let image = crate::spirv::program::compile_in(worker, p.clone()).map_err(std::io::Error::other)?;
        let bytes = image.compiler_output_bytes();
        writeln!(images, "let bytes: &[u8] = &{bytes:?};\nlet gpu = &ModuleImage {{ data: bytes.as_ptr(), bytes: bytes.len(), code: core::ptr::null(), codes: 0 }};")?;
    }
    Ok((images, functions, resources))
}
