//! 算法包装交接同次编译的执行布局及已链接CPU入口；GPU交接原完整映像。
use crate::{ExecutableProgram, ast::Target, ir::Scalar};

pub(super) fn constructor(p: &ExecutableProgram, prefix: &str, worker: &crate::compiler::Worker) -> Result<(String, Vec<(String, crate::native::NativeCode)>), Box<dyn std::error::Error>> {
    use crate::native::{ContinuationCode, RangeResult, StageCode};
    let mut functions = Vec::new();
    let mut declarations = String::new();
    let cpu = if p.target() == Target::Gpu {
        "return Err(std::io::Error::other(\"GPU 专属程序不能提交给 CPU\").into())".into()
    } else {
        let image = crate::native::program::compile_in(worker, p.clone()).map_err(std::io::Error::other)?;
        let layout = image.program().compiler_output_bytes();
        let (_, stages, callees) = image.into_parts();
        let mut entry = |code: crate::native::NativeCode| {
            let index = functions.len();
            let name = format!("__blink_range_{prefix}_{index}");
            declarations.push_str(&format!("#[link_name={name:?}] static range_{index}: u8;\n"));
            functions.push((name, code));
            format!("std::ptr::addr_of!(range_{index})")
        };
        let result = |result| match result {
            RangeResult::None => "RangeResult::None".to_owned(),
            RangeResult::Values(words) => format!("RangeResult::Values({words})"),
        };
        let stages = stages.into_iter().map(|stage| match stage {
            StageCode::Element { main, continuation, result: returns } => {
                let main = entry(main);
                let continuation = match continuation {
                    None => "None".to_owned(),
                    Some(continuation) => {
                        let value = match continuation {
                            ContinuationCode::ReductionTail(code) => format!("ContinuationCode::ReductionTail({})", entry(code)),
                            ContinuationCode::ScanAdd(code) => format!("ContinuationCode::ScanAdd({})", entry(code)),
                            ContinuationCode::CompactScatter(code) => format!("ContinuationCode::CompactScatter({})", entry(code)),
                            ContinuationCode::SegmentedFinish(code) => format!("ContinuationCode::SegmentedFinish({})", entry(code)),
                            ContinuationCode::Scan { prefix, apply, source, target } => format!("ContinuationCode::Scan{{prefix:{},apply:{},source:{source},target:{target}}}", entry(prefix), entry(apply)),
                        };
                        format!("Some({value})")
                    }
                };
                format!("StageCode::Element{{main:{main},continuation:{continuation},result:{}}}", result(returns))
            }
            StageCode::Sort(code) => {
                let initialize = code.initialize.into_iter().map(&mut entry).collect::<Vec<_>>().join(",");
                format!("StageCode::Sort(SortCode{{source:{},target:{},initialize:vec![{initialize}],histogram:{},scatter:{},gather:{}}})", code.source, code.target, entry(code.histogram), entry(code.scatter), entry(code.gather))
            }
        }).collect::<Vec<_>>().join(",");
        functions.extend(callees);
        format!("{{ use blanguage::native::{{StageCode,ContinuationCode,SortCode,RangeResult}}; LibraryProgram::Cpu(unsafe {{ CpuProgram::from_linked(NativeProgramImage::<ExecutableProgram,*const u8>::from_compiler_output(blanguage::ir::ExecutionPlan::from_compiler_output_bytes(&{layout:?})?,vec![{stages}],Vec::new())) }}?) }}")
    };
    let gpu = if p.target() == Target::Cpu {
        "return Err(std::io::Error::other(\"CPU 专属程序不能提交给 GPU\").into())".into()
    } else {
        let image = crate::spirv::program::compile_in(worker, p.clone()).map_err(std::io::Error::other)?;
        format!("{{ let image = unsafe {{ ShaderProgramImage::<ExecutableProgram>::from_compiler_output_bytes(&{:?}) }}?; let device = Device::for_program(image.program())?; let program = device.load(image)?; LibraryProgram::Gpu {{ device, program }} }}", image.compiler_output_bytes())
    };
    let declarations = format!("unsafe extern \"C\" {{\n{declarations}}}\n");
    Ok((format!("{{ {declarations} match backend {{0 => {{ {cpu} }}, 1 => {{ {gpu} }}, _ => return Err(std::io::Error::other(\"backend 必须为 CPU(0) 或 GPU(1)\").into()),}} }}"), functions))
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
    fn blink_resize(output:*mut u32,frame:*mut c_void,handle:u32,count:u32,fill:*const u32)->*mut c_void;
    fn blink_run(frame:*mut c_void,program:u32,data:*const DatasetBinding,words:*const UniformValue)->*mut c_void;
    fn blink_upload(frame:*mut c_void,handle:u32)->*mut c_void;
    fn blink_download(frame:*mut c_void,handle:u32)->*mut c_void;
    fn blink_address(output:*mut *mut u32,frame:*mut c_void,handle:u32,offset:u64,bytes:u32,stride:u64,count:u32)->*mut c_void;
    fn blink_error_message(error:*const c_void,bytes:*mut usize)->*const u8;
    fn blink_error_destroy(error:*mut c_void);
    fn blink_destroy(frame:*mut c_void);
}
"#);
    if !root {
        functions.push_str(r#"
unsafe extern "C" {
    fn blink_application_error(context:*mut c_void,data:*const u8,bytes:usize);
    fn blink_application_failed(frame:*mut c_void,error:*mut c_void)->!;
}
"#);
    }
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
pub unsafe extern "C" fn blink_application_failed(frame: *mut c_void, error: *mut c_void) -> ! {
    let mut bytes = 0;
    let message = unsafe { blink_error_message(error, &mut bytes) };
    unsafe { blink_application_error(std::ptr::null_mut(), message, bytes) };
    unsafe { blink_error_destroy(error) };
    unsafe { blink_destroy(frame) };
    std::process::exit(1)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_application_resize(frame:*mut c_void,handle:u32,count:u32,fill:*const u32)->u32 {
    let mut output=0;
    let error = unsafe { blink_resize(&mut output,frame,handle,count,fill) };
    if !error.is_null() {
        unsafe { blink_application_failed(frame,error) }
    }
    output
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_application_upload(frame:*mut c_void,handle:u32) {
    let error = unsafe { blink_upload(frame,handle) };
    if !error.is_null() {
        unsafe { blink_application_failed(frame,error) }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_application_download(frame:*mut c_void,handle:u32) {
    let error = unsafe { blink_download(frame,handle) };
    if !error.is_null() {
        unsafe { blink_application_failed(frame,error) }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_application_address(frame:*mut c_void,handle:u32,offset:u64,bytes:u32,stride:u64,count:u32)->*mut u32 {
    let mut output=std::ptr::null_mut();
    let error = unsafe { blink_address(&mut output,frame,handle,offset,bytes,stride,count) };
    if !error.is_null() {
        unsafe { blink_application_failed(frame,error) }
    }
    output
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_create(on_error: unsafe extern "C" fn(*mut c_void, *const u8, usize), context: *mut c_void) -> *mut c_void {
    let factories = unsafe { std::slice::from_raw_parts(std::ptr::addr_of!(__blink_module_factories), __blink_module_count) };
    let mut types = Vec::new();
    let mut programs = Vec::new();
    for factory in factories {
        let image = unsafe { factory() };
        let bytes = unsafe { std::slice::from_raw_parts(image.data, image.bytes) };
        let code = unsafe { std::slice::from_raw_parts(image.code, image.codes) };
        let module = match unsafe { blanguage::resource::from_linked_compiler_output(bytes, code) } {
            Ok(module) => module,
            Err(error) => {
                let message = error.to_string();
                unsafe { on_error(context, message.as_ptr(), message.len()) };
                return std::ptr::null_mut();
            }
        };
        types.extend(module.types);
        programs.extend(module.images.into_iter().map(|image| match image {
            blanguage::resource::Image::Cpu(image) => ProgramImage::CpuLinked(image),
            blanguage::resource::Image::Gpu(image) => ProgramImage::Gpu(image),
        }));
    }
    match Frame::new(types, programs) {
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
            "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {prefix}_allocate_{index}(frame:*mut c_void,count:u32)->u32 {{ let mut output=0; let error=unsafe {{ blink_allocate(&mut output,frame,{prefix}_type_base+{index},count) }}; if !error.is_null() {{ unsafe {{ blink_application_failed(frame,error) }} }} output }}"
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
            "#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn {prefix}_run_{index}({}) {{ let data=[{}];let words=[{}];let error=unsafe {{ blink_run(frame,{prefix}_program_base+{index},data.as_ptr(),words.as_ptr()) }};if !error.is_null() {{ unsafe {{ blink_application_failed(frame,error) }} }} }}",
            params.join(","),
            handles.join(","),
            words.join(",")
        )?;
    }
    Ok(include_str!("../application/runtime.rs.in")
        .replace("@PREFIX@", prefix)
        .replace("@IMAGE@", &format!("{image:?}"))
        .replace("@CODE_COUNT@", &code.len().to_string())
        .replace("@CODE_DECLARATIONS@", &code.iter().enumerate().map(|(index, (name, _))|
            format!("#[link_name={name:?}] fn range_{index}();")).collect::<Vec<_>>().join("\n"))
        .replace("@CODE_ENTRIES@", &(0..code.len()).map(|index| format!("range_{index}")).collect::<Vec<_>>().join(","))
        .replace("@FUNCTIONS@", &functions))
}
