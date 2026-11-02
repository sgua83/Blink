//! 原生包装交接同一次真实编译的映像字节，不重建类型或阶段对象源码。
use crate::{ExecutableProgram, ast::Target, ir::Scalar};

pub(super) fn constructor(p: &ExecutableProgram, worker: &crate::compiler::Worker) -> Result<(String, Vec<crate::object::stack::Frame>), Box<dyn std::error::Error>> {
    let mut stack = Vec::new();
    let cpu = if p.target() == Target::Gpu {
        "return Err(std::io::Error::other(\"GPU 专属程序不能提交给 CPU\").into())".into()
    } else {
        let image = crate::native::program::compile_in(worker, p.clone()).map_err(std::io::Error::other)?;
        stack = crate::object::stack::compute(p.name(), image.stages());
        format!(
            "LibraryProgram::Cpu(CpuProgram::from_image(unsafe {{ NativeProgramImage::<ExecutableProgram>::from_compiler_output_bytes(&{:?}) }}?)?)",
            image.compiler_output_bytes()
        )
    };
    let gpu = if p.target() == Target::Cpu {
        "return Err(std::io::Error::other(\"CPU 专属程序不能提交给 GPU\").into())".into()
    } else {
        let image = crate::spirv::program::compile_in(worker, p.clone()).map_err(std::io::Error::other)?;
        format!(
            "{{ let image = unsafe {{ ShaderProgramImage::<ExecutableProgram>::from_compiler_output_bytes(&{:?}) }}?; let device = Device::for_program(image.program())?; let program = device.load(image)?; LibraryProgram::Gpu {{ device, program }} }}",
            image.compiler_output_bytes()
        )
    };
    Ok((format!(
        "match backend {{0 => {{ {cpu} }}, 1 => {{ {gpu} }}, _ => return Err(std::io::Error::other(\"backend 必须为 CPU(0) 或 GPU(1)\").into()),}}"
    ), stack))
}

pub(crate) fn application_source(
    resources: &crate::application::Resources,
    prefix: &str,
    root: bool,
    image: &[u8],
) -> Result<String, Box<dyn std::error::Error>> {
    use std::fmt::Write;
    let mut functions = String::new();
    if root {
        functions.push_str(r#"
use blcompute::application::{Frame, ProgramImage};
type Factory = unsafe extern "C" fn() -> ModuleImage;
unsafe extern "C" { static __blink_module_count: usize; static __blink_module_factories: Factory; }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_application_error(_context: *mut c_void, data: *const u8, bytes: usize) {
    let message = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(data, bytes)) };
    eprintln!("{message}");
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_create(on_error: unsafe extern "C" fn(*mut c_void, *const u8, usize), context: *mut c_void) -> *mut c_void {
    let factories = unsafe { std::slice::from_raw_parts(std::ptr::addr_of!(__blink_module_factories), __blink_module_count) };
    let mut types = Vec::new();
    let mut programs = Vec::new();
    for factory in factories {
        let image = unsafe { factory() };
        let bytes = unsafe { std::slice::from_raw_parts(image.data, image.bytes) };
        let module = match unsafe { blanguage::resource::from_compiler_output(bytes) } {
            Ok(module) => module,
            Err(error) => {
                let message = error.to_string();
                unsafe { on_error(context, message.as_ptr(), message.len()) };
                return std::ptr::null_mut();
            }
        };
        types.extend(module.types);
        programs.extend(module.images.into_iter().map(|image| match image {
            blanguage::resource::Image::Cpu(image) => ProgramImage::Cpu(image),
            blanguage::resource::Image::Gpu(image) => ProgramImage::Gpu(image),
        }));
    }
    Box::into_raw(Box::new(Frame::new(types, programs))).cast()
}
"#);
    }
    if !resources.types.is_empty() {
        writeln!(
            functions,
            "unsafe extern \"C\" {{ static {prefix}_type_base:u32; fn blink_allocate(frame:*mut c_void,ty:u32,count:u32)->u32; }}"
        )?;
    }
    if !resources.programs.is_empty() {
        writeln!(
            functions,
            "unsafe extern \"C\" {{ static {prefix}_program_base:u32; fn blink_run(frame:*mut c_void,program:u32,data:*const DatasetBinding,words:*const UniformValue); }}"
        )?;
    }
    for index in 0..resources.types.len() {
        writeln!(
            functions,
            "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {prefix}_allocate_{index}(frame:*mut c_void,count:u32)->u32 {{ unsafe {{ blink_allocate(frame,{prefix}_type_base+{index},count) }} }}"
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
            "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {prefix}_run_{index}({}) {{ let data=[{}];let words=[{}];unsafe {{ blink_run(frame,{prefix}_program_base+{index},data.as_ptr(),words.as_ptr()) }} }}",
            params.join(","),
            handles.join(","),
            words.join(",")
        )?;
    }
    Ok(include_str!("../application/runtime.rs.in")
        .replace("@PREFIX@", prefix)
        .replace("@IMAGE@", &format!("{image:?}"))
        .replace("@FUNCTIONS@", &functions))
}
