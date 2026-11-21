//! 模块资源 ABI：标量值与实际字节布局；不传递 Rust 容器表示。
use crate::codec::{self, Decode, Encode, Reader, Writer};
use crate::{TemplateProgram, ir::*, native, spirv};
#[cfg(feature = "compiler")]
use crate::ast::Target;

// C 入口签名或本文件的字段／标签编码改变时，改变这一实际 ABI 身份。
pub const ABI: &[u8; 8] = b"BLRI\x34\0\0\0";
pub const RUNTIME_LIBRARY: &str = "@blink.application@";
// 非根资源静态包装声明的原C ABI，模块专属索引符号仍由原链接器绑定。
pub(crate) const MODULE_RUNTIME_SYMBOLS: &[&str] = &["blink_allocate", "blink_run", "blink_complete_field", "blink_task_start", "blink_task_wait"];
mod device_requirements;
mod program_layout;
mod execution_plan;
mod image_codec;
mod module_image;
mod library_buffer;
pub use library_buffer::LibraryBuffer;
pub use module_image::{ModuleImage, ModuleFactory};
/// 字段共同保证的闭区间，按原Scalar有序位编码；仅同步借用，不拥有存储。
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ElementInterval {
    pub lower: u64,
    pub upper: u64,
}
/// 根Frame中的存储引用；offset为记录内字节偏移，不交接另一份存储所有权。
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DatasetBinding {
    pub handle: u32,
    pub offset: u32,
}
pub const F32: u32 = 0;
pub const U32: u32 = 1;
pub const I32: u32 = 2;
pub const BOOL: u32 = 3;
pub const U64: u32 = 4;
pub const I64: u32 = 5;
pub const F64: u32 = 6;
pub const U8: u32 = 7;
pub const I8: u32 = 8;
pub const U16: u32 = 9;
pub const I16: u32 = 10;
pub(crate) const fn scalar_tag(scalar: Scalar) -> u32 {
    match scalar {
        Scalar::F32 => F32,
        Scalar::U32 => U32,
        Scalar::I32 => I32,
        Scalar::Bool => BOOL,
        Scalar::U64 => U64,
        Scalar::I64 => I64, Scalar::F64 => F64,
        Scalar::U8 => U8,
        Scalar::I8 => I8,
        Scalar::U16 => U16,
        Scalar::I16 => I16,
    }
}
#[derive(Clone, Debug)]
pub enum Image<P = TemplateProgram, C = native::NativeCode> {
    Cpu(native::ProgramImage<P, C>),
    Gpu(spirv::ProgramImage<P>),
}
impl<P, C> std::ops::Deref for Image<P, C> {
    type Target = ExecutionPlan;
    fn deref(&self) -> &ExecutionPlan {
        match self { Self::Cpu(image) => image.program(), Self::Gpu(image) => image.program() }
    }
}
impl<P, C> Image<P, C> {
    pub fn into_cpu(self) -> Result<native::ProgramImage<P, C>, &'static str> {
        match self { Self::Cpu(image) => Ok(image), Self::Gpu(_) => Err("GPU映像不能作为CPU机器码装入") }
    }
    pub fn into_gpu(self) -> Result<spirv::ProgramImage<P>, &'static str> {
        match self { Self::Gpu(image) => Ok(image), Self::Cpu(_) => Err("CPU映像不能作为SPIR-V装入") }
    }
}
#[derive(Clone, Debug)]
pub struct Module<C = native::NativeCode> {
    pub types: Vec<Ty>,
    pub images: Vec<Image<TemplateProgram, C>>,
}
#[cfg(feature = "compiler")]
fn compile_images<C>(
    resources: &crate::application::Resources,
    worker: &crate::compiler::Worker,
    mut cpu_stage: impl FnMut(native::StageCode) -> native::StageCode<C>,
) -> codec::Result<Vec<Image<TemplateProgram, C>>> {
    let requirements: Vec<_> = resources.programs.iter().map(|(_, program)| program.requirements()).collect();
    let cpu_jobs = resources.programs.iter().enumerate()
        .filter(|(_, (target, _))| *target == Target::Cpu)
        .flat_map(|(program, (_, code))| (0..code.stages().len()).map(move |stage| (program, stage)))
        .collect();
    let cpu = crate::parallel::try_map(worker, cpu_jobs, |_| (), |_, (program, stage)| {
        native::program::compile_stage(&resources.programs[program].1, stage)
    }).map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut gpu_jobs = Vec::new();
    for (program, (target, code)) in resources.programs.iter().enumerate() {
        if *target != Target::Cpu {
            // Keep a layout error at the original position of that program's first shader.
            match spirv::program::check_shader_pages(code) {
                Ok(()) => gpu_jobs.extend((0..code.stages().len()).map(|stage| Ok((program, stage)))),
                Err(error) => gpu_jobs.push(Err(error)),
            }
        }
    }
    let gpu = crate::parallel::try_map(worker, gpu_jobs, |_| (), |_, job| {
        let (program, stage) = job?;
        spirv::program::compile_stage(&resources.programs[program].1, stage, requirements[program])
    }).map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut cpu = cpu.into_iter();
    let mut gpu = gpu.into_iter();
    let images = resources.programs.iter().zip(requirements).map(|((target, program), requirements)| {
        let count = program.stages().len();
        if *target == Target::Cpu {
            let (layout, stages) = (0..count).map(|_| {
                let (layout, stage) = cpu.next().unwrap();
                (layout, cpu_stage(stage))
            }).unzip();
            let plan = ExecutionPlan::from_program(program, layout, requirements);
            let image = unsafe { native::ProgramImage::from_compiler_output(plan, stages, Vec::new(), None) };
            Image::Cpu(image)
        } else {
            let (layout, stages) = (0..count).map(|_| gpu.next().unwrap()).unzip();
            let plan = ExecutionPlan::from_program(program, layout, requirements);
            Image::Gpu(unsafe { spirv::ProgramImage::from_compiler_output(plan, stages) })
        }
    }).collect();
    Ok(images)
}

#[cfg(feature = "compiler")]
pub(crate) fn compile_image(
    resources: &crate::application::Resources,
    worker: &crate::compiler::Worker,
) -> codec::Result<Module> {
    let images = compile_images(resources, worker, std::convert::identity)?;
    Ok(Module { types: resources.types.clone(), images })
}

#[cfg(feature = "compiler")]
pub(crate) fn compile_linked(
    resources: &crate::application::Resources,
    worker: &crate::compiler::Worker,
    mut entry: impl FnMut(native::NativeCode) -> usize,
) -> codec::Result<Vec<u8>> {
    let images = compile_images(resources, worker, |stage| stage.map_code(&mut entry))?;
    Ok(image_codec::linked_module_bytes(&resources.types, &images))
}

#[cfg(feature = "compiler")]
pub(crate) fn compile(resources: &crate::application::Resources,worker:&crate::compiler::Worker)
    -> codec::Result<(Vec<u8>, Vec<native::NativeCode>, std::collections::BTreeMap<String, Vec<u8>>)> {
    let mut code = Vec::new();
    let bytes = compile_linked(resources, worker, |entry| {
        let index = code.len();
        code.push(entry);
        index
    })?;
    let imports=resources.programs.iter().flat_map(|(_,program)|program.imports.clone()).collect();
    Ok((bytes,code,imports))
}

pub(crate) fn compiler_output<T: Encode>(value: &T) -> Vec<u8> {
    let mut writer = Writer(ABI.to_vec());
    value.encode(&mut writer);
    writer.0
}
fn resource_payload(bytes: &[u8]) -> codec::Result<&[u8]> {
    bytes.strip_prefix(ABI).ok_or_else(|| "不支持的模块资源 ABI".into())
}
fn decode_compiler_output<T: Decode>(bytes: &[u8]) -> codec::Result<T> {
    T::from_bytes(resource_payload(bytes)?)
}
impl ExecutionPlan {
    pub(crate) fn compiler_output_bytes(&self) -> Vec<u8> { compiler_output(self) }
    /// 恢复同一编译产物的执行布局；代码由所属原生对象交接。
    ///
    /// # Safety
    /// bytes必须是本资源ABI的实际布局，与所绑定代码的类型、访问、阶段及入口类别完全一致。
    #[doc(hidden)]
    pub unsafe fn from_compiler_output_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        decode_compiler_output(bytes).map_err(|error|
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()))
    }
}
/// # Safety
/// bytes 必须是此资源 ABI 的实际编译产物，原机器码、布局、访问与前缀合同未被伪造。
/// 资源 ABI 不使用生成它的 Rust 容器布局或编译器源码身份。
pub unsafe fn from_compiler_output(bytes: &[u8]) -> codec::Result<Module> {
    decode_compiler_output(bytes)
}

/// 解码原模块工厂的布局和代码索引，供最终执行入口绑定同次链接的代码表。
///
/// # Safety
/// bytes必须来自可信模块工厂；绑定时使用该工厂同次链接的代码表。
#[doc(hidden)]
pub unsafe fn decode_linked_module(bytes: &[u8]) -> codec::Result<Module<usize>> {
    decode_compiler_output(bytes)
}

/// 按原工厂顺序将类型追加到构造中的目录，并交接每幅已解码的程序映像。
/// 解码失败优先于装入失败。调用者持有构造中的值，并在失败时释放它们。
///
/// # Safety
/// bytes必须来自可信模块工厂；绑定时使用该工厂同次链接的代码表。
#[doc(hidden)]
pub unsafe fn decode_linked_module_into<E>(
    bytes: &[u8],
    types: &mut Vec<Ty>,
    mut bind: impl FnMut(Image<TemplateProgram, usize>) -> Result<(), E>,
) -> codec::Result<Result<(), E>> {
    let mut reader = codec::Reader::new(resource_payload(bytes)?);
    let mut binding = Ok(());
    image_codec::read_linked_module(&mut reader, types, |image| {
        if binding.is_ok() {
            binding = bind(image);
        }
    })?;
    reader.finish()?;
    Ok(binding)
}

/// 解码同一原生程序的布局和已链接代码索引。
///
/// # Safety
/// bytes必须来自可信原生编译产物的同一P入口，绑定时使用同次链接的代码表。
#[doc(hidden)]
pub unsafe fn decode_linked_program<P>(bytes: &[u8]) -> std::io::Result<native::ProgramImage<P, usize>> {
    decode_compiler_output(bytes).map_err(|error|
        std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()))
}

/// 恢复所属原生模块的静态布局并绑定同次链接的CPU入口；不分配代码VM。
///
/// # Safety
/// bytes与code必须来自同一可信模块工厂，索引对应原范围ABI；所属模块及依赖
/// 必须保持到最后CPU工作和程序消费者结束。地址表不转移代码所有权。
#[doc(hidden)]
pub unsafe fn from_linked_compiler_output(bytes: &[u8], code: &[*const u8]) -> codec::Result<Module<*const u8>> {
    let module = unsafe { decode_linked_module(bytes) }?;
    let images = module.images.into_iter().map(|image| match image {
        Image::Cpu(image) => Image::Cpu(image.map_code(&mut |index| code[index])),
        Image::Gpu(image) => Image::Gpu(image),
    }).collect();
    Ok(Module { types: module.types, images })
}

macro_rules! image_output {
    ($backend:ident) => {
        impl<P> $backend::ProgramImage<P> {
            pub fn compiler_output_bytes(&self) -> Vec<u8> { compiler_output(self) }
            /// 从同一P入口的本版本编译产物恢复映像，不改变入口类别。
            ///
            /// # Safety
            /// bytes必须来自此ABI的同一P入口，代码、布局、访问、目标与前缀合同完整未改。
            /// 不得用宿主输入或另一个入口类别伪造已编译程序。
            #[doc(hidden)]
            pub unsafe fn from_compiler_output_bytes(bytes: &[u8]) -> std::io::Result<Self> {
                decode_compiler_output(bytes).map_err(|error|
                    std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()))
            }
        }
    };
}
image_output!(native);
image_output!(spirv);
// Observation ranges belong to the compiler result, not the deployed execution ABI.
impl<P> native::ProgramImage<P,*const u8>{
    /// # Safety
    /// bytes与code来自同一次原生编译及链接，索引、入口类别和完整资源目录一致；
    /// 原模块与依赖保持到最后真实代码及资源消费者完成。
    pub unsafe fn from_linked_compiler_output_bytes(bytes:&[u8],code:&[*const u8])->std::io::Result<Self>{
        let image = unsafe { decode_linked_program::<P>(bytes) }?;
        Ok(image.map_code(&mut |index|code[index]))
    }
}

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(crate::ast::TextureDimension);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{self, Native, Read as ReadCallback, Slot};
    #[repr(C)]
    struct Reader {
        d2: ReadCallback,
        d3: ReadCallback,
    }
    unsafe extern "C" fn build_d2(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureDimension>() = crate::ast::TextureDimension::D2;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_d3(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureDimension>() = crate::ast::TextureDimension::D3;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_read_texture_dimension"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for crate::ast::TextureDimension {
        fn empty() -> Self {
            Self::D2
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::D2 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_dimension_d2"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::D3 => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_dimension_d3"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        d2: build_d2,
                        d3: build_d3,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(crate::ast::TextureFormat);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{self, Native, Read as ReadCallback, Slot};
    #[repr(C)]
    struct Reader {
        rgba32float: ReadCallback,
        rgba8unorm: ReadCallback,
        rgba8srgb: ReadCallback,
        r8unorm: ReadCallback,
        rg16unorm: ReadCallback,
        rgb10a2unorm: ReadCallback,
        r11g11b10float: ReadCallback,
        r16float: ReadCallback,
    }
    unsafe extern "C" fn build_rgba32float(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureFormat>() =
                crate::ast::TextureFormat::Rgba32Float;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_rgba8unorm(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureFormat>() =
                crate::ast::TextureFormat::Rgba8Unorm;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_rgba8srgb(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureFormat>() = crate::ast::TextureFormat::Rgba8Srgb;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_r8unorm(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureFormat>() = crate::ast::TextureFormat::R8Unorm;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_rg16unorm(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureFormat>() = crate::ast::TextureFormat::Rg16Unorm;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_rgb10a2unorm(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureFormat>() =
                crate::ast::TextureFormat::Rgb10A2Unorm;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_r11g11b10float(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureFormat>() =
                crate::ast::TextureFormat::R11G11B10Float;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_r16float(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<crate::ast::TextureFormat>() =
                crate::ast::TextureFormat::R16Float;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_read_texture_format"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for crate::ast::TextureFormat {
        fn empty() -> Self {
            Self::Rgba32Float
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Rgba32Float => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_format_rgba32float"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Rgba8Unorm => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_format_rgba8unorm"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Rgba8Srgb => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_format_rgba8srgb"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::R8Unorm => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_format_r8unorm"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Rg16Unorm => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_format_rg16unorm"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Rgb10A2Unorm => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_format_rgb10a2unorm"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::R11G11B10Float => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_format_r11g11b10float"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::R16Float => {
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_texture_format_r16float"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        rgba32float: build_rgba32float,
                        rgba8unorm: build_rgba8unorm,
                        rgba8srgb: build_rgba8srgb,
                        r8unorm: build_r8unorm,
                        rg16unorm: build_rg16unorm,
                        rgb10a2unorm: build_rgb10a2unorm,
                        r11g11b10float: build_r11g11b10float,
                        r16float: build_r16float,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(TextureBinding);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        name: Slot,
        count: Slot,
        dimension: Slot,
        format: Slot,
    }
    #[repr(C)]
    struct Writer {
        name: WriteCallback,
        count: WriteCallback,
        dimension: WriteCallback,
        format: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        name: ReadCallback,
        count: ReadCallback,
        dimension: ReadCallback,
        format: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_write_texture_binding"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "codec.value_format::contract_read_texture_binding"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for TextureBinding {
        fn empty() -> Self {
            Self {
                name: <String as Native>::empty(),
                count: <Option<u32> as Native>::empty(),
                dimension: <crate::ast::TextureDimension as Native>::empty(),
                format: <Option<crate::ast::TextureFormat> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        name: contracts::address(&self.name).cast_mut(),
                        count: contracts::address(&self.count).cast_mut(),
                        dimension: contracts::address(&self.dimension).cast_mut(),
                        format: contracts::address(&self.format).cast_mut(),
                    },
                    Writer {
                        name: contracts::write_value::<String>,
                        count: contracts::write_value::<Option<u32>>,
                        dimension: contracts::write_value::<crate::ast::TextureDimension>,
                        format: contracts::write_value::<Option<crate::ast::TextureFormat>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        name: contracts::slot(&mut output.name),
                        count: contracts::slot(&mut output.count),
                        dimension: contracts::slot(&mut output.dimension),
                        format: contracts::slot(&mut output.format),
                    },
                    Reader {
                        name: contracts::read_value::<String>,
                        count: contracts::read_value::<Option<u32>>,
                        dimension: contracts::read_value::<crate::ast::TextureDimension>,
                        format: contracts::read_value::<Option<crate::ast::TextureFormat>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(PrefixCount);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        binding: Slot,
        word: Slot,
    }
    #[repr(C)]
    struct Writer {
        binding: WriteCallback,
        word: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        binding: ReadCallback,
        word: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_write_prefix_count"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "codec.value_format::contract_read_prefix_count"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for PrefixCount {
        fn empty() -> Self {
            Self {
                binding: <usize as Native>::empty(),
                word: <u32 as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        binding: contracts::address(&self.binding).cast_mut(),
                        word: contracts::address(&self.word).cast_mut(),
                    },
                    Writer {
                        binding: contracts::write_value::<usize>,
                        word: contracts::write_value::<u32>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        binding: contracts::slot(&mut output.binding),
                        word: contracts::slot(&mut output.word),
                    },
                    Reader {
                        binding: contracts::read_value::<usize>,
                        word: contracts::read_value::<u32>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(PrefixRelation);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        domain: Slot,
        domain_region: Slot,
        count: Slot,
    }
    #[repr(C)]
    struct Writer {
        domain: WriteCallback,
        domain_region: WriteCallback,
        count: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        domain: ReadCallback,
        domain_region: ReadCallback,
        count: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_write_prefix_relation"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "codec.value_format::contract_read_prefix_relation"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for PrefixRelation {
        fn empty() -> Self {
            Self {
                domain: <usize as Native>::empty(),
                domain_region: <Option<(u32, u32)> as Native>::empty(),
                count: <PrefixCount as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        domain: contracts::address(&self.domain).cast_mut(),
                        domain_region: contracts::address(&self.domain_region).cast_mut(),
                        count: contracts::address(&self.count).cast_mut(),
                    },
                    Writer {
                        domain: contracts::write_value::<usize>,
                        domain_region: contracts::write_value::<Option<(u32, u32)>>,
                        count: contracts::write_value::<PrefixCount>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        domain: contracts::slot(&mut output.domain),
                        domain_region: contracts::slot(&mut output.domain_region),
                        count: contracts::slot(&mut output.count),
                    },
                    Reader {
                        domain: contracts::read_value::<usize>,
                        domain_region: contracts::read_value::<Option<(u32, u32)>>,
                        count: contracts::read_value::<PrefixCount>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(ElementRange);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        binding: Slot,
        offset: Slot,
        scalar: Slot,
        intervals: Slot,
        known_bits: Slot,
    }
    #[repr(C)]
    struct Writer {
        binding: WriteCallback,
        offset: WriteCallback,
        scalar: WriteCallback,
        intervals: WriteCallback,
        known_bits: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        binding: ReadCallback,
        offset: ReadCallback,
        scalar: ReadCallback,
        intervals: ReadCallback,
        known_bits: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_write_element_range"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "codec.value_format::contract_read_element_range"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for ElementRange {
        fn empty() -> Self {
            Self {
                binding: <usize as Native>::empty(),
                offset: <u32 as Native>::empty(),
                scalar: <Scalar as Native>::empty(),
                intervals: <Vec<(Bits, Bits)> as Native>::empty(),
                known_bits: <(Bits, Bits) as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        binding: contracts::address(&self.binding).cast_mut(),
                        offset: contracts::address(&self.offset).cast_mut(),
                        scalar: contracts::address(&self.scalar).cast_mut(),
                        intervals: contracts::address(&self.intervals).cast_mut(),
                        known_bits: contracts::address(&self.known_bits).cast_mut(),
                    },
                    Writer {
                        binding: contracts::write_value::<usize>,
                        offset: contracts::write_value::<u32>,
                        scalar: contracts::write_value::<Scalar>,
                        intervals: contracts::write_value::<Vec<(Bits, Bits)>>,
                        known_bits: contracts::write_value::<(Bits, Bits)>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        binding: contracts::slot(&mut output.binding),
                        offset: contracts::slot(&mut output.offset),
                        scalar: contracts::slot(&mut output.scalar),
                        intervals: contracts::slot(&mut output.intervals),
                        known_bits: contracts::slot(&mut output.known_bits),
                    },
                    Reader {
                        binding: contracts::read_value::<usize>,
                        offset: contracts::read_value::<u32>,
                        scalar: contracts::read_value::<Scalar>,
                        intervals: contracts::read_value::<Vec<(Bits, Bits)>>,
                        known_bits: contracts::read_value::<(Bits, Bits)>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(ElementFact);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        range: ReadCallback,
        index: ReadCallback,
    }
    unsafe extern "C" fn build_range(context: Slot, destination: Slot) -> ReadStatus {
        let mut range = <ElementRange as Native>::empty();
        #[repr(C)]
        struct Fields {
            range: Slot,
        }
        #[repr(C)]
        struct Host {
            range: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "codec.value_format::contract_read_element_fact_range"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    range: contracts::slot(&mut range),
                },
                Host {
                    range: contracts::read_value::<ElementRange>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<ElementFact>() = ElementFact::Range(range);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_index(context: Slot, destination: Slot) -> ReadStatus {
        let mut binding = <usize as Native>::empty();
        let mut offset = <u32 as Native>::empty();
        let mut target = <usize as Native>::empty();
        #[repr(C)]
        struct Fields {
            binding: Slot,
            offset: Slot,
            target: Slot,
        }
        #[repr(C)]
        struct Host {
            binding: ReadCallback,
            offset: ReadCallback,
            target: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "codec.value_format::contract_read_element_fact_index"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    binding: contracts::slot(&mut binding),
                    offset: contracts::slot(&mut offset),
                    target: contracts::slot(&mut target),
                },
                Host {
                    binding: contracts::read_value::<usize>,
                    offset: contracts::read_value::<u32>,
                    target: contracts::read_value::<usize>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<ElementFact>() = ElementFact::Index {
                binding,
                offset,
                target,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "codec.value_format::contract_read_element_fact"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for ElementFact {
        fn empty() -> Self {
            Self::Index {
                binding: 0,
                offset: 0,
                target: 0,
            }
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Range(range) => {
                    #[repr(C)]
                    struct Fields {
                        range: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        range: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_element_fact_range"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                range: contracts::address(range).cast_mut(),
                            },
                            Host {
                                range: contracts::write_value::<ElementRange>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Index {
                    binding,
                    offset,
                    target,
                } => {
                    #[repr(C)]
                    struct Fields {
                        binding: Slot,
                        offset: Slot,
                        target: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        binding: WriteCallback,
                        offset: WriteCallback,
                        target: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "codec.value_format::contract_write_element_fact_index"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                binding: contracts::address(binding).cast_mut(),
                                offset: contracts::address(offset).cast_mut(),
                                target: contracts::address(target).cast_mut(),
                            },
                            Host {
                                binding: contracts::write_value::<usize>,
                                offset: contracts::write_value::<u32>,
                                target: contracts::write_value::<usize>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        range: build_range,
                        index: build_index,
                    },
                )
            }
        }
    }
};
