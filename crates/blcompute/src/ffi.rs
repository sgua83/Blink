//! 编译器生成 DLL 的公共 C ABI 执行边界；原生 Rust API 不经过此层。
use crate::{
    Dataset, Result, Value,
    cpu::{CpuDataset, CpuProgram},
    error,
    vulkan::{Device, GpuProgram},
};
use blanguage::{
    ir::{ExecutionPlan as Program, Scalar},
};
use blanguage::{
    ExecutableProgram,
    resource::{ModuleFactory, ModuleImage},
    spirv::ProgramImage as ShaderProgramImage,
};
use std::ffi::{c_char, c_void};

pub use blanguage::resource::LibraryBuffer as Buffer;

pub enum LibraryProgram {
    Cpu(CpuProgram),
    Gpu { device: Device, program: GpuProgram },
}
impl LibraryProgram {
    #[cfg(feature="compiler")]
    pub fn new(program: ExecutableProgram, backend: u32) -> Result<Self> {
        match backend {
            0 => Ok(Self::Cpu(CpuProgram::new(program)?)),
            1 => {
                let image = blanguage::spirv::compile_program(program).map_err(error)?;
                let device = Device::for_program(image.program())?;
                let program = device.load(image)?;
                Ok(Self::Gpu { device, program })
            }
            _ => Err(error("backend 必须为 CPU(0) 或 GPU(1)")),
        }
    }
    pub fn program(&self) -> &Program {
        match self {
            Self::Cpu(p) => p.program(),
            Self::Gpu { program, .. } => program.program(),
        }
    }
    /// 一次同步调用拥有本次数据；返回前 GPU 已完成且输出已交还调用方。
    ///
    /// # Safety
    /// 有载荷且数量非零时，指针须指向 count 个有效记录；零尺寸记录允许 NULL。
    /// read/inout 字段须已初始化，输出须可写。
    /// 可写区间不能与其他参数重叠。调用期间宿主不得访问这些存储；同一 handle
    /// 的全部调用与释放必须在创建线程串行执行。生成的 Rust 包装器编码这些借用约束。
    pub unsafe fn run(&mut self, buffers: &[Buffer], values: &[Value]) -> Result<()> {
        let p = self.program();
        if buffers.len() != p.buffers().len() {
            return Err(error("DLL 数据绑定数量与编译接口不一致"));
        }
        let mut datasets = Vec::new();
        for (i, (buffer, layout)) in buffers.iter().zip(p.buffers()).enumerate() {
            if layout.ty.size != 0 && buffer.count != 0 && buffer.data.is_null() {
                return Err(error(format!("{} 的非空数据指针为空", layout.name)));
            }
            let mut data = Dataset::new(p, i, buffer.count)?;
            if layout.access.reads() {
                let words = data.words_mut();
                for record in 0..buffer.count as usize {
                    // 只读真实字段，不读取 C/C++ 结构体未初始化的填充字节。
                    for &(scalar, offset) in &layout.ty.components {
                        let byte = record * layout.stride as usize + offset as usize;
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                buffer.data.cast::<u8>().add(byte),
                                words.as_mut_ptr().cast::<u8>().add(byte),
                                scalar.size() as usize,
                            );
                        }
                    }
                }
            }
            datasets.push(data);
        }
        let outputs: Vec<(usize, CpuDataset)> = match self {
            Self::Cpu(p) => {
                p.run(&mut datasets, values)?;
                datasets
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| p.program().buffers()[*i].access.writes())
                    .map(|(i, data)| (i, data.into()))
                    .collect::<Vec<_>>()
            }
            Self::Gpu { device, program } => {
                let datasets = datasets.into_iter().map(Into::into).collect::<Vec<_>>();
                let uploaded = device.upload_domains(datasets)?;
                let completed = program.submit(uploaded, values)?.wait().map_err(|(error, _)| error)?;
                let (output_slots, datasets): (Vec<_>, Vec<_>) = completed
                    .into_datasets()
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| program.program().buffers()[*i].access.writes())
                    .unzip();
                let outputs = device.download_domains(datasets.into())?;
                output_slots.into_iter().zip(outputs.into_datasets()).collect()
            }
        };
        for (i, data) in outputs {
            let buffer = &buffers[i];
            let layout = &self.program().buffers()[i];
            for record in 0..buffer.count as usize {
                for &(scalar, offset) in &layout.ty.components {
                    let byte = record * layout.stride as usize + offset as usize;
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            data.words().as_ptr().cast::<u8>().add(byte),
                            buffer.data.cast::<u8>().add(byte),
                            scalar.size() as usize,
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// 将一个已打包 uniform 的真实字段转换为原生 Rust 调用参数，跳过布局填充。
///
/// # Safety
/// data 指向生成接口所声明的、字段已初始化的 uniform 类型。
pub unsafe fn uniform_values(data: *const c_void, ty: &blanguage::ir::Ty) -> Vec<Value> {
    ty.components
        .iter()
        .map(|&(scalar, offset)| {
            let address = unsafe { data.cast::<u8>().add(offset as usize) };
            let bits = match scalar.size() {
                1 => u64::from(unsafe { address.read() }),
                2 => u64::from(unsafe { address.cast::<u16>().read_unaligned() }),
                4 => u64::from(unsafe { address.cast::<u32>().read_unaligned() }),
                _ => unsafe { address.cast::<u64>().read_unaligned() },
            };
            match scalar {
                Scalar::F32 => Value::F32(f32::from_bits(bits as u32)), Scalar::F64 => Value::F64(f64::from_bits(bits)),
                Scalar::U32 => Value::U32(bits as u32),
                Scalar::I32 => Value::I32(bits as i32),
                Scalar::Bool => Value::Bool(bits != 0),
                Scalar::U64 => Value::U64(bits),
                Scalar::I64 => Value::I64(bits as i64),
                Scalar::U8 => Value::U8(bits as u8),
                Scalar::I8 => Value::I8(bits as i8),
                Scalar::U16 => Value::U16(bits as u16),
                Scalar::I16 => Value::I16(bits as i16),
            }
        })
        .collect()
}

/// 交回0成功或-1失败；可选UTF-8缓冲容量包含结尾NUL。
/// 容量0不生产或写入文本，容量1只写NUL；其余按完整字符截断。
///
/// # Safety
/// capacity > 0 时 message 必须指向 capacity 个可写字节；可用 null + 0 放弃文本。
pub unsafe fn status(result: Result<()>, message: *mut c_char, capacity: usize) -> i32 {
    let code = if result.is_ok() { 0 } else { -1 };
    if capacity > 1 {
        let text = match result {
            Ok(()) => String::new(),
            Err(e) => e.to_string(),
        };
        let mut end = text.len().min(capacity - 1);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), message.cast(), end);
            *message.add(end) = 0;
        }
    } else if capacity == 1 {
        unsafe { *message = 0; }
    }
    code
}

/// # Safety
/// The selected image, code addresses and factory table come from the linked DLL.
/// The output and error buffer are valid according to its generated C interface.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_library_create(
    backend: u32,
    cpu: *const ModuleImage,
    gpu: *const ModuleImage,
    factories: *const ModuleFactory,
    factory_count: usize,
    out: *mut *mut LibraryProgram,
    message: *mut c_char,
    capacity: usize,
) -> i32 {
    let result = (|| {
        let out = unsafe { out.as_mut() }.ok_or_else(|| error("null output handle"))?;
        *out = std::ptr::null_mut();
        let handle = match backend {
            0 => {
                let cpu = unsafe { cpu.as_ref() }.ok_or_else(|| error("GPU 专属程序不能提交给 CPU"))?;
                let bytes = unsafe { std::slice::from_raw_parts(cpu.data, cpu.bytes) };
                let code = unsafe { std::slice::from_raw_parts(cpu.code, cpu.codes) };
                let image = unsafe { blanguage::resource::decode_linked_program::<ExecutableProgram>(bytes) }?;
                let factories = (!factories.is_null()).then(|| unsafe {
                    std::slice::from_raw_parts(factories, factory_count)
                });
                LibraryProgram::Cpu(unsafe { CpuProgram::from_linked_indices(image, code, factories) }?)
            }
            1 => {
                let gpu = unsafe { gpu.as_ref() }.ok_or_else(|| error("CPU 专属程序不能提交给 GPU"))?;
                let bytes = unsafe { std::slice::from_raw_parts(gpu.data, gpu.bytes) };
                let image = unsafe { ShaderProgramImage::<ExecutableProgram>::from_compiler_output_bytes(bytes) }?;
                let device = Device::for_program(image.program())?;
                let program = device.load(image)?;
                LibraryProgram::Gpu { device, program }
            }
            _ => return Err(error("backend 必须为 CPU(0) 或 GPU(1)")),
        };
        *out = Box::into_raw(Box::new(handle));
        Ok(())
    })();
    unsafe { status(result, message, capacity) }
}

/// # Safety
/// The generated wrapper supplies one buffer per dataset and one pointer per
/// nonempty uniform in the program's order. The original call owns those borrows.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_library_run(
    handle: *mut LibraryProgram,
    buffers: *const Buffer,
    uniforms: *const *const c_void,
    message: *mut c_char,
    capacity: usize,
) -> i32 {
    let result = (|| {
        let handle = unsafe { handle.as_mut() }.ok_or_else(|| error("null handle"))?;
        let program = handle.program();
        let buffers = unsafe { std::slice::from_raw_parts(buffers, program.buffers().len()) };
        let mut values = Vec::new();
        for (index, uniform) in program.uniforms().iter().filter(|u| u.ty.size != 0).enumerate() {
            let data = unsafe { *uniforms.add(index) };
            if data.is_null() {
                return Err(error(format!("null uniform: {}", uniform.name)));
            }
            values.extend(unsafe { uniform_values(data, &uniform.ty) });
        }
        unsafe { handle.run(buffers, &values) }
    })();
    unsafe { status(result, message, capacity) }
}

/// # Safety
/// A nonnull handle is the owned result of creation and has no remaining callers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blink_library_destroy(handle: *mut LibraryProgram) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}
