//! 算法包装交接同次编译的执行布局及已链接CPU入口；GPU交接原完整映像。
use crate::{ExecutableProgram, application::factory::ResourceFactory, ast::Target, ir::Scalar};

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

pub(crate) fn application_source(
    resources: &crate::application::Resources,
    prefix: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if resources.programs.is_empty() && resources.completions.is_empty() {
        return Ok(None);
    }
    use std::fmt::Write;
    let mut functions = String::new();
    functions.push_str(r#"
unsafe extern "C" {
    fn blink_run(frame:*mut c_void,program:u32,data:*const DatasetBinding,words:*const UniformValue,textures:*const *const c_void)->*mut c_void;
    fn blink_complete_field(frame:*mut c_void,data:u32,offset:u32,intervals:*const ElementInterval,interval_count:usize,limits:*const u32,limit_count:usize,known_mask:u64,known_value:u64);
}
"#);
    if !resources.programs.is_empty() {
        writeln!(
            functions,
            "unsafe extern \"C\" {{ static {prefix}_program_base:u32; }}"
        )?;
    }
    for (index, (_, program)) in resources.programs.iter().enumerate() {
        let mut params = vec!["frame:*mut c_void".to_owned()];
        let handles = (0..program.buffers.len())
            .map(|i| {
                let name = format!("data_{i}");
                params.push(format!("{name}:u32"));
                params.push(format!("offset_{i}:u32"));
                format!("DatasetBinding{{handle:{name},offset:offset_{i}}}")
            })
            .collect::<Vec<_>>();
        let mut words = Vec::new();
        for uniform in &program.uniforms {
            for (scalar, _) in &uniform.ty.components {
                let name = format!("value_{}", words.len());
                let ty = if *scalar == Scalar::Bool { "u32" } else { scalar.name() };
                params.push(format!("{name}:{ty}"));
                let bits = match scalar {
                    Scalar::F32 => format!("u64::from({name}.to_bits())"), Scalar::F64 => format!("{name}.to_bits()"),
                    Scalar::I32 => format!("u64::from({name} as u32)"),
                    Scalar::Bool => format!("u64::from({name} != 0)"),
                    Scalar::U32 => format!("u64::from({name})"),
                    Scalar::U64 => name,
                    Scalar::I64 => format!("{name} as u64"),
                    Scalar::U8 | Scalar::U16 => format!("u64::from({name})"),
                    Scalar::I8 => format!("u64::from({name} as u8)"),
                    Scalar::I16 => format!("u64::from({name} as u16)"),
                };
                words.push(format!(
                    "UniformValue{{kind:{},bits:{bits}}}",
                    crate::resource::scalar_tag(*scalar)
                ));
            }
        }
        let textures = (0..program.textures.len()).map(|i| {
            let name = format!("texture_{i}");
            params.push(format!("{name}:*const c_void"));
            name
        }).collect::<Vec<_>>();
        writeln!(
            functions,
            "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {prefix}_run_{index}({})->*mut c_void {{ let data=[{}];let words=[{}];let textures=[{}];unsafe {{ blink_run(frame,{prefix}_program_base.wrapping_add({index}),data.as_ptr(),words.as_ptr(),textures.as_ptr()) }} }}",
            params.join(","),
            handles.join(","),
            words.join(","),
            textures.join(",")
        )?;
    }
    for (index, fields) in resources.completions.iter().enumerate() {
        let counts = fields.iter().flat_map(|field| &field.limits).map(|(parameter, _)| parameter + 1).max().unwrap_or(0);
        let params = (0..counts).map(|i| format!(",count_{i}:u32")).collect::<String>();
        writeln!(functions, "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {prefix}_complete_{index}(frame:*mut c_void,data:u32,offset:u32{params}) {{")?;
        for field in fields {
            let offset = field.offset;
            let (known_mask,known_value) = field.known_bits;
            let values = field.intervals.iter().map(|(lower, upper)| format!("ElementInterval{{lower:{lower},upper:{upper}}}")).collect::<Vec<_>>().join(",");
            let limit_count = field.limits.len();
            writeln!(functions, "{{ static INTERVALS:[ElementInterval;{}]=[{values}];let mut limits=[0u32;{limit_count}];let mut count=0usize;", field.intervals.len())?;
            // At most one output per input limit fits the fixed C array.
            let append = "unsafe { limits.as_mut_ptr().add(count).write(limit); } count+=1;";
            for (parameter, distance) in &field.limits {
                if *distance == 0 {
                    writeln!(functions, "if let Some(limit)=count_{parameter}.checked_add(1) {{ {append} }}")?;
                } else {
                    writeln!(functions, "{{ let limit=count_{parameter}.saturating_sub({}); {append} }}", distance - 1)?;
                }
            }
            writeln!(functions, "unsafe {{ blink_complete_field(frame,data,offset.wrapping_add({offset}),INTERVALS.as_ptr(),INTERVALS.len(),limits.as_ptr(),count,{known_mask},{known_value}) }} }}")?;
        }
        writeln!(functions, "}}")?;
    }
    Ok(Some(include_str!("../application/runtime.rs.in").replace("@FUNCTIONS@", &functions)))
}
