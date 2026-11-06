//! 模块资源 ABI：标量值与实际字节布局；不传递 Rust 容器表示。
use crate::codec::{self, Decode, Encode, Reader, Writer, record, variants};
use crate::{TemplateProgram, ir::*, native, spirv};
#[cfg(feature = "compiler")]
use crate::ast::Target;

// C 入口签名或本文件的字段／标签编码改变时，改变这一实际 ABI 身份。
pub const ABI: &[u8; 8] = b"BLRI\x1f\0\0\0";
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
pub struct Module<C = native::NativeCode> {
    pub types: Vec<Ty>,
    pub images: Vec<Image<TemplateProgram, C>>,
}
record!(Module { types, images });
record!(Module<usize> { types, images });
variants!(Image { 0=>Cpu(image), 1=>Gpu(image) });
variants!(Image<TemplateProgram, usize> { 2=>Cpu(image), 1=>Gpu(image) });

#[cfg(feature = "compiler")]
pub(crate) fn compile( resources: &crate::application::Resources, worker: &crate::compiler::Worker) -> codec::Result<(Vec<u8>, Vec<native::NativeCode>, std::collections::BTreeMap<String, Vec<u8>>)> {
    let jobs = |cpu| resources.programs.iter().enumerate()
        .filter(|(_, (target, _))| (*target == Target::Cpu) == cpu)
        .flat_map(|(program, (_, code))| (0..code.stages().len()).map(move |stage| (program, stage)))
        .collect();
    let cpu = crate::parallel::try_map(worker, jobs(true), |_| (), |_, (program, stage)| {
        native::program::compile_stage(&resources.programs[program].1, stage)
    }).map_err(|error| std::io::Error::other(error.to_string()))?;
    let gpu = crate::parallel::try_map(worker, jobs(false), |_| (), |_, (program, stage)| {
        spirv::program::compile_stage(&resources.programs[program].1, stage)
    }).map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut cpu = cpu.into_iter();
    let mut gpu = gpu.into_iter();
    let mut code = Vec::new();
    let images = resources.programs.iter().map(|(target, program)| {
        let mut plan = ExecutionPlan::from_program(program);
        if *target == Target::Cpu {
            let stages: Vec<_> = program.stages().iter().map(|_| cpu.next().unwrap()).collect();
            let image = unsafe { native::ProgramImage::from_compiler_output(plan, stages, Vec::new()) };
            Image::Cpu(image.map_code(|entry| { let index = code.len(); code.push(entry); index }))
        } else {
            let stages = plan.stages.iter_mut().map(|stage| {
                let (code, bytes) = gpu.next().unwrap();
                stage.shared_bytes = bytes;
                code
            }).collect();
            Image::Gpu(unsafe { spirv::ProgramImage::from_compiler_output(plan, stages) })
        }
    }).collect();
    let bytes = compiler_output(&Module { types: resources.types.clone(), images });
    let imports = resources.programs.iter().flat_map(|(_, program)| program.imports.clone()).collect();
    Ok((bytes, code, imports))
}
fn compiler_output<T: Encode>(value: &T) -> Vec<u8> {
    let mut bytes = ABI.to_vec();
    bytes.extend(value.bytes());
    bytes
}
fn decode_compiler_output<T: Decode>(bytes: &[u8]) -> codec::Result<T> {
    let bytes = bytes.strip_prefix(ABI).ok_or("不支持的模块资源 ABI")?;
    T::from_bytes(bytes)
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

/// 恢复所属原生模块的静态布局并绑定同次链接的CPU入口；不分配代码VM。
///
/// # Safety
/// bytes与code必须来自同一可信模块工厂，索引对应原范围ABI；所属模块及依赖
/// 必须保持到最后CPU工作和程序消费者结束。地址表不转移代码所有权。
#[doc(hidden)]
pub unsafe fn from_linked_compiler_output(bytes: &[u8], code: &[*const u8]) -> codec::Result<Module<*const u8>> {
    let module: Module<usize> = decode_compiler_output(bytes)?;
    let images = module.images.into_iter().map(|image| match image {
        Image::Cpu(image) => Image::Cpu(image.map_code(|index| code[index])),
        Image::Gpu(image) => Image::Gpu(image),
    }).collect();
    Ok(Module { types: module.types, images })
}

macro_rules! image_output {
    ($backend:ident) => {
        impl<P> $backend::ProgramImage<P> {
            pub(crate) fn compiler_output_bytes(&self) -> Vec<u8> { compiler_output(self) }
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
impl<P, C: Encode> Encode for native::ProgramImage<P, C> {
    fn encode(&self, writer: &mut Writer) {
        self.program().encode(writer);
        self.stages().encode(writer);
        self.functions().encode(writer);
    }
}
impl<P, C: Decode> Decode for native::ProgramImage<P, C> {
    fn decode(reader: &mut Reader<'_>) -> codec::Result<Self> {
        let program = ExecutionPlan::decode(reader)?;
        let stages = Vec::<native::StageCode<C>>::decode(reader)?;
        let functions = Vec::<(String, C)>::decode(reader)?;
        Ok(unsafe { Self::from_compiler_output(program, stages, functions) })
    }
}
impl<P> Encode for spirv::ProgramImage<P> {
    fn encode(&self, writer: &mut Writer) {
        self.program().encode(writer);
        self.stages().encode(writer);
    }
}
impl<P> Decode for spirv::ProgramImage<P> {
    fn decode(reader: &mut Reader<'_>) -> codec::Result<Self> {
        let program = ExecutionPlan::decode(reader)?;
        let stages = Vec::<spirv::StageCode>::decode(reader)?;
        Ok(unsafe { Self::from_compiler_output(program, stages) })
    }
}
record!(crate::ast::ExternalName { library, symbol });
variants!(crate::ast::TextureDimension { 2=>D2, 3=>D3 });
record!(TextureBinding { name, count, dimension });
record!(Reduction {
    source,
    source_word,
    target,
    target_word
});
record!(ReductionSpec {
    fields,
    ty,
    operator
});
record!(SegmentedReduction {
    values,
    head_word,
    count
});
record!(Scan {
    values,
    ty,
    operator,
    head_word
});
record!(Compaction {
    source,
    flags,
    flag_word,
    target,
    count,
    count_word
});
variants!(SortOrder {0=>Ascending,1=>Descending});
record!(Sorting {
    source,
    keys,
    target
});
variants!(Operation<ReductionLayout> {0=>Map,1=>Reduce(r),2=>CustomReduce(r),3=>CustomScan(s),4=>CustomSegmentedReduce(r),5=>SegmentedReduce(r),6=>ScanExclusive(s),7=>Compact(c),8=>StableSort(s)});
record!(PrefixCount { binding, word });
record!(PrefixRelation { domain, count });
record!(Selection { binding, word });
variants!(CollectiveOperator {0=>Sum,1=>Min,2=>Max,3=>BitAnd,4=>BitOr,5=>BitXor});
record!(ReductionLayout { fields, ty });
record!(CustomScan<ReductionLayout> { values, head_word });
record!(CustomSegmentedReduction<ReductionLayout> { values, head_word, count });
record!(SubgroupCapabilities {
    basic,
    vote,
    arithmetic,
    ballot,
    shuffle,
    shuffle_relative,
    extended_types
});
record!(DeviceRequirements {
    ray_query, float64, integer8, integer16, atomic64, shared_atomic64,
    sorting,
    maximal_reconvergence,
    texture_indexing,
    position_fetch,
    subgroup
});
record!(ProgramLayout {
    write_regions,
    name,
    target,
    buffers,
    accelerations,
    textures,
    uniforms,
    uniform_bytes,
    prefix_requirements,
    prefix_guarantees,
    length_requirements,
    requirements
});
record!(ExecutionStage {
    domain,
    prefix,
    selection,
    operation,
    reads,
    writes,
    inputs,
    tile_scope,
    width,
    shared_bytes,
    float32_preserve, float64_preserve
});
record!(ExecutionPlan {
    interface,
    stages,
    indirect_buffer,
    auxiliary_buffers
});
variants!(native::NativeSymbol {0=>Print(scalar),1=>External(name),2=>Function(name)});
record!(native::Relocation { offset, target });
// Observation ranges belong to the compiler result, not the deployed execution ABI.
impl Encode for native::NativeCode {
    fn encode(&self, writer: &mut Writer) {
        self.bytes.encode(writer);
        self.relocations.encode(writer);
        self.stack_bytes.encode(writer);
        self.calls.encode(writer);
        self.function_end.encode(writer);
        self.unwind.encode(writer);
    }
}
impl Decode for native::NativeCode {
    fn decode(reader: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(Self {
            bytes: Vec::decode(reader)?,
            relocations: Vec::decode(reader)?,
            stack_bytes: u32::decode(reader)?,
            calls: Vec::decode(reader)?,
            function_end: usize::decode(reader)?,
            unwind: Vec::decode(reader)?,
            #[cfg(feature = "compiler")]
            tail_regions: None,
            #[cfg(feature = "compiler")]
            tail_references: None,
            #[cfg(feature = "compiler")]
            operation_ranges: None,
            #[cfg(feature = "compiler")]
            function_abi: None,
        })
    }
}
variants!(native::RangeResult {0=>None,1=>Values(n)});
variants!(@<C> native::ContinuationCode<C> {0=>ReductionTail(code),1=>ScanAdd(code),2=>CompactScatter(code),3=>SegmentedFinish(code),4=>Scan{prefix,apply,source,target}});
record!(@<C> native::SortCode<C> {
    source,
    target,
    initialize,
    histogram,
    scatter,
    gather
});
variants!(@<C> native::StageCode<C> {0=>Element{main,continuation,result},1=>Sort(code)});
record!(spirv::ShaderCode { words, ranks });
record!(spirv::StageCode {
    main,
    tail,
    scan_add,
    finish,
    dispatch,
    selector,
    sort
});
