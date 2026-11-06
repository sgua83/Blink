//! 算法包装交接同次编译的执行布局及已链接CPU入口；GPU交接原完整映像。
use crate::{ExecutableProgram, ast::Target, ir::Scalar};

fn code_declarations(names: &[String]) -> String {
    names.iter().enumerate().map(|(index, name)|
        format!("#[link_name={name:?}] fn range_{index}();")).collect::<Vec<_>>().join("\n")
}
fn module_factory(prefix: &str, image: &[u8], names: &[String]) -> String {
    let declarations = code_declarations(names);
    let entries = (0..names.len()).map(|index| format!("range_{index}")).collect::<Vec<_>>().join(",");
    let count = names.len();
    let abi = include_str!("../resource/module_image.rs");
    format!(r#"{abi}
static IMAGE: &[u8] = &{image:?};
unsafe extern "C" {{ {declarations} }}
static CODE: [unsafe extern "C" fn(); {count}] = [{entries}];
#[unsafe(no_mangle)]
pub extern "C" fn {prefix}_resources() -> ModuleImage {{
    ModuleImage {{ data: IMAGE.as_ptr(), bytes: IMAGE.len(), code: CODE.as_ptr().cast(), codes: CODE.len() }}
}}
"#)
}
pub(super) fn constructor(
    p: &ExecutableProgram, prefix: &str, worker: &crate::compiler::Worker,
) -> Result<(String, Vec<(String, crate::native::NativeCode)>, String, Option<crate::object::ResourceBinding>), Box<dyn std::error::Error>> {
    let mut functions = Vec::new();
    let mut factory = String::new();
    let mut resources = None;
    let cpu = if p.target() == Target::Gpu {
        "return Err(std::io::Error::other(\"GPU 专属程序不能提交给 CPU\").into())".into()
    } else {
        let image = crate::native::program::compile_in(worker, p.clone()).map_err(std::io::Error::other)?;
        let mut index = 0;
        let mut image = image.map_code_with(&mut |name, code| {
            let current = index; index += 1;
            let name = name.map(str::to_owned).unwrap_or_else(|| format!("__blink_range_{prefix}_{current}"));
            functions.push((name, code)); current
        });
        let module = image.resources.take();
        let bytes = crate::resource::linked_output(&image);
        let names = functions.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
        if let Some(module) = module {
            let resource_prefix = crate::application::resource_prefix(p.name());
            factory = module_factory(&resource_prefix, &crate::resource::linked_output(&module), &names);
            resources = Some(crate::object::ResourceBinding {
                prefix: resource_prefix, types: u32::try_from(module.types.len())?,
                programs: u32::try_from(module.images.len())?,
                abi: crate::resource::ABI.to_vec(), code: names,
            });
            format!(r#"{{
                use blanguage::resource::ModuleFactory;
                unsafe extern "C" {{ static __blink_module_count: usize; static __blink_module_factories: ModuleFactory; }}
                let code = unsafe {{ std::slice::from_raw_parts(CODE.as_ptr().cast::<*const u8>(), CODE.len()) }};
                let image = unsafe {{ NativeProgramImage::<ExecutableProgram,*const u8>::from_linked_compiler_output_bytes(&{bytes:?}, code) }}?;
                let factories = unsafe {{ std::slice::from_raw_parts(std::ptr::addr_of!(__blink_module_factories), __blink_module_count) }};
                LibraryProgram::Cpu(unsafe {{ CpuProgram::from_linked_modules(image, factories) }}?)
            }}"#)
        } else {
            let declarations = code_declarations(&names);
            let addresses = (0..index).map(|index| format!("range_{index} as *const u8")).collect::<Vec<_>>().join(",");
            format!("{{ unsafe extern \"C\" {{ {declarations} }} let code=[{addresses}];let image=unsafe {{NativeProgramImage::<ExecutableProgram,*const u8>::from_linked_compiler_output_bytes(&{bytes:?},&code)}}?; LibraryProgram::Cpu(unsafe {{CpuProgram::from_linked(image)}}?) }}")
        }
    };
    let gpu = if p.target() == Target::Cpu {
        "return Err(std::io::Error::other(\"CPU 专属程序不能提交给 GPU\").into())".into()
    } else {
        let image = crate::spirv::program::compile_in(worker, p.clone()).map_err(std::io::Error::other)?;
        format!("{{ let image = unsafe {{ ShaderProgramImage::<ExecutableProgram>::from_compiler_output_bytes(&{:?}) }}?; let device = Device::for_program(image.program())?; let program = device.load(image)?; LibraryProgram::Gpu {{ device, program }} }}", image.compiler_output_bytes())
    };
    Ok((format!("{{ match backend {{0 => {{ {cpu} }}, 1 => {{ {gpu} }}, _ => return Err(std::io::Error::other(\"backend 必须为 CPU(0) 或 GPU(1)\").into()),}} }}"), functions, factory, resources))
}

pub(crate) fn application_source(
    resources: &crate::application::Resources,
    prefix: &str,
    root: bool,
    image: &[u8],
    code: &[(String, crate::native::NativeCode)],
) -> Result<String, Box<dyn std::error::Error>> {
    use std::fmt::Write;
    let mut functions = String::new();
    functions.push_str(r#"
unsafe extern "C" {
    fn blink_allocate(output:*mut u32,frame:*mut c_void,ty:u32,count:u32)->*mut c_void;
    fn blink_run(frame:*mut c_void,program:u32,data:*const DatasetBinding,words:*const UniformValue)->*mut c_void;
    fn blink_complete_field(frame:*mut c_void,data:u32,offset:u32,intervals:*const ElementInterval,interval_count:usize,limits:*const u32,limit_count:usize);
    fn blink_error_message(error:*const c_void,bytes:*mut usize)->*const u8;
    fn blink_error_destroy(error:*mut c_void);
    fn blink_destroy(frame:*mut c_void);
}
"#);
    if root {
        functions.push_str(r#"
use blcompute::application::Frame;
unsafe extern "C" { static __blink_module_count: usize; static __blink_module_factories: blanguage::resource::ModuleFactory; }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_application_error(_context: *mut c_void, data: *const u8, bytes: usize) {
    let message = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(data, bytes)) };
    eprintln!("{message}");
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_application_failed(frame: *mut c_void, error: *mut c_void) -> ! {
    let mut bytes = 0;
    let message = unsafe { blink_error_message(error, &mut bytes) };
    unsafe { blink_application_error(std::ptr::null_mut(), message, bytes) };
    unsafe { blink_error_destroy(error) };
    unsafe { blink_destroy(frame) };
    std::process::exit(1)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_create(on_error: unsafe extern "C" fn(*mut c_void, *const u8, usize), context: *mut c_void) -> *mut c_void {
    let factories = unsafe { std::slice::from_raw_parts(std::ptr::addr_of!(__blink_module_factories), __blink_module_count) };
    match unsafe { Frame::from_linked_modules(factories) } {
        Ok(frame) => Box::into_raw(Box::new(frame)).cast(),
        Err(error) => {
            let message = error.to_string();
            unsafe { on_error(context, message.as_ptr(), message.len()) };
            std::ptr::null_mut()
        }
    }
}
"#);
    }
    if !resources.types.is_empty() {
        writeln!(
            functions,
            "unsafe extern \"C\" {{ static {prefix}_type_base:u32; }}"
        )?;
    }
    if !resources.programs.is_empty() {
        writeln!(
            functions,
            "unsafe extern \"C\" {{ static {prefix}_program_base:u32; }}"
        )?;
    }
    for index in 0..resources.types.len() {
        writeln!(
            functions,
            "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {prefix}_allocate_{index}(output:*mut u32,frame:*mut c_void,count:u32)->*mut c_void {{ unsafe {{ blink_allocate(output,frame,{prefix}_type_base+{index},count) }} }}"
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
        writeln!(
            functions,
            "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {prefix}_run_{index}({})->*mut c_void {{ let data=[{}];let words=[{}];unsafe {{ blink_run(frame,{prefix}_program_base+{index},data.as_ptr(),words.as_ptr()) }} }}",
            params.join(","),
            handles.join(","),
            words.join(",")
        )?;
    }
    for (index, fields) in resources.completions.iter().enumerate() {
        let counts = fields.iter().flat_map(|field| &field.limits).map(|(parameter, _)| parameter + 1).max().unwrap_or(0);
        let params = (0..counts).map(|i| format!(",count_{i}:u32")).collect::<String>();
        writeln!(functions, "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {prefix}_complete_{index}(frame:*mut c_void,data:u32{params}) {{")?;
        for field in fields {
            let offset = field.offset;
            let values = field.intervals.iter().map(|(lower, upper)| format!("ElementInterval{{lower:{lower},upper:{upper}}}")).collect::<Vec<_>>().join(",");
            let limit_count = field.limits.len();
            let limits = field.limits.iter().map(|(parameter, distance)| match *distance {
                0 => format!("count_{parameter}.checked_add(1)"),
                1 => format!("Some(count_{parameter})"),
                distance => format!("Some(count_{parameter}.saturating_sub({}))", distance - 1),
            }).collect::<Vec<_>>().join(",");
            writeln!(functions, "{{ static INTERVALS:[ElementInterval;{}]=[{values}];let bounds:[Option<u32>;{limit_count}]=[{limits}];let mut limits=[0u32;{limit_count}];let mut count=0;for limit in bounds.into_iter().flatten(){{limits[count]=limit;count+=1;}}unsafe {{ blink_complete_field(frame,data,{offset},INTERVALS.as_ptr(),INTERVALS.len(),limits.as_ptr(),count) }} }}", field.intervals.len())?;
        }
        writeln!(functions, "}}")?;
    }
    let names = code.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
    Ok(include_str!("../application/runtime.rs.in")
        .replace("@FACTORY@", &module_factory(prefix, image, &names))
        .replace("@FUNCTIONS@", &functions))
}
