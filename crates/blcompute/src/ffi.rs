//! 编译器生成 DLL 的公共 C ABI 执行边界；原生 Rust API 不经过此层。
use crate::{
    Dataset, Result, Value,
    cpu::CpuProgram,
    error,
    vulkan::{Device, GpuProgram},
};
use blanguage::{
    ExecutableProgram,
    ir::{ExecutionPlan as Program, Scalar},
};
use std::ffi::{c_char, c_void};

/// 指针指向 count 个编译器声明的完整记录；记录的字段与布局来自生成的头文件。
pub struct Buffer {
    pub data: *mut c_void,
    pub count: u32,
}
pub enum LibraryProgram {
    Cpu(CpuProgram),
    Gpu { device: Device, program: GpuProgram },
}
impl LibraryProgram {
    pub fn new(program: ExecutableProgram, backend: u32) -> Result<Self> {
        match backend {
            0 => Ok(Self::Cpu(CpuProgram::new(program)?)),
            1 => {
                let device = Device::for_program(&program)?;
                let program = device.compile(program)?;
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
    /// 每个非空指针须指向 count 个有效记录，read/inout 字段须已初始化；输出须可写。
    /// 可写区间不能与其他参数重叠。调用期间宿主不得访问这些存储；同一 handle
    /// 的全部调用与释放必须在创建线程串行执行。生成的 Rust 包装器编码这些借用约束。
    pub unsafe fn run(&mut self, buffers: &[Buffer], values: &[Value]) -> Result<()> {
        let p = self.program();
        if buffers.len() != p.buffers().len() {
            return Err(error("DLL 数据绑定数量与编译接口不一致"));
        }
        let mut datasets = Vec::new();
        for (i, (buffer, layout)) in buffers.iter().zip(p.buffers()).enumerate() {
            if buffer.count != 0 && buffer.data.is_null() {
                return Err(error(format!("{} 的非空数据指针为空", layout.name)));
            }
            let mut data = Dataset::new(p, i, buffer.count)?;
            if layout.access.reads() {
                let words = data.words_mut();
                for record in 0..buffer.count as usize {
                    // 只读真实字段，不读取 C/C++ 结构体未初始化的填充字节。
                    for &(_, offset) in &layout.ty.components {
                        let byte = record * layout.stride as usize + offset as usize;
                        words[byte / 4] = unsafe {
                            buffer
                                .data
                                .cast::<u8>()
                                .add(byte)
                                .cast::<u32>()
                                .read_unaligned()
                        };
                    }
                }
            }
            datasets.push(data);
        }
        let outputs = match self {
            Self::Cpu(p) => {
                p.run(&mut datasets, values)?;
                datasets
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| p.program().buffers()[*i].access.writes())
                    .collect::<Vec<_>>()
            }
            Self::Gpu { device, program } => {
                let uploaded = datasets
                    .into_iter()
                    .map(|d| device.upload(d))
                    .collect::<Result<Vec<_>>>()?;
                let completed = program.submit(uploaded, values)?.wait()?;
                completed
                    .datasets()
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| program.program().buffers()[*i].access.writes())
                    .map(|(i, d)| Ok((i, device.download(d)?)))
                    .collect::<Result<Vec<_>>>()?
            }
        };
        for (i, data) in outputs {
            let buffer = &buffers[i];
            let layout = &self.program().buffers()[i];
            for record in 0..buffer.count as usize {
                for &(_, offset) in &layout.ty.components {
                    let byte = record * layout.stride as usize + offset as usize;
                    unsafe {
                        buffer
                            .data
                            .cast::<u8>()
                            .add(byte)
                            .cast::<u32>()
                            .write_unaligned(data.words()[byte / 4]);
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
            let bits = unsafe {
                data.cast::<u8>()
                    .add(offset as usize)
                    .cast::<u32>()
                    .read_unaligned()
            };
            match scalar {
                Scalar::F32 => Value::F32(f32::from_bits(bits)),
                Scalar::U32 => Value::U32(bits),
                Scalar::I32 => Value::I32(bits as i32),
                Scalar::Bool => Value::Bool(bits != 0),
            }
        })
        .collect()
}

/// 写入调用方拥有的 UTF-8 错误缓冲区。0 成功，-1 失败；容量包含结尾 NUL。
///
/// # Safety
/// capacity > 0 时 message 必须指向 capacity 个可写字节；可用 null + 0 放弃文本。
pub unsafe fn status(result: Result<()>, message: *mut c_char, capacity: usize) -> i32 {
    let (code, text) = match result {
        Ok(()) => (0, String::new()),
        Err(e) => (-1, e.to_string()),
    };
    if capacity != 0 {
        let mut end = text.len().min(capacity - 1);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), message.cast(), end);
            *message.add(end) = 0;
        }
    }
    code
}
