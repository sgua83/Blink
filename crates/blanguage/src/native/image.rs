//! 已编译CPU映像与范围ABI；编译器和执行器共享同一份代码及布局。
use crate::ir::{ExecutionPlan,Scalar};

/// CPU范围入口的同一绑定表：原地址和实际记录步长。纹理槽只消费地址。
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BufferAddress {
    pub address: *mut u32,
    pub stride: usize,
}
/// 同一CPU范围入口：空指针成功，非空是调用者唯一拥有的原资源错误；算法结果仅写入output。
/// buffers、uniforms及output由本次调用保持到入口完成；无算法结果时output不解引用。
pub type RangeEntry = unsafe extern "C" fn(
    *const BufferAddress, *const u32, u32, u32, *mut u32,
) -> *mut std::ffi::c_void;
#[derive(Clone, Debug)]
pub struct Relocation {
    pub offset: usize,
    pub target: NativeSymbol,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativeSymbol {
    Print(Scalar),
    External(crate::ast::ExternalName),
    Function(String),
}
impl std::fmt::Debug for NativeSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Print(v) => f.debug_tuple("Print").field(v).finish(),
            Self::Function(v) => f.debug_tuple("Function").field(v).finish(),
            Self::External(v) => f
                .debug_struct("External")
                .field("library", &v.library)
                .field("symbol", &v.symbol)
                .finish(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct NativeCode {
    pub bytes: Vec<u8>,
    pub relocations: Vec<Relocation>,
    pub stack_bytes: u32,
    pub calls: Vec<Option<usize>>,
    /// 主函数末端；不包括同映像的无帧探栈叶辅助与常量。
    pub function_end: usize,
    /// 发射器产生的Windows x64展开字节，与本映像一起交接。
    pub unwind: Vec<u8>,
    /// 仅compiler持有的尾部分区；执行编码不保留，主函数仍沿function_end。
    #[cfg(feature = "compiler")]
    pub tail_regions: Option<Vec<Region>>,
    /// 原尾部目标与实际rel32字段；执行编码不保留。
    #[cfg(feature = "compiler")]
    pub tail_references: Option<Vec<TailReference>>,
    /// 原IR操作的实际发射区间；仅compiler观察，执行编码不保留。
    #[cfg(feature = "compiler")]
    pub operation_ranges: Option<Vec<OperationRange>>,
    /// 同一次普通函数发射的ABI生成事实；执行编码不保留。
    #[cfg(feature = "compiler")]
    pub function_abi: Option<FunctionAbi>,
}
#[cfg(feature = "compiler")]
#[derive(Clone, Debug)]
pub struct TailReference {
    pub target: usize,
    pub rel32_fields: Vec<usize>,
}
#[cfg(feature = "compiler")]
#[derive(Clone, Debug)]
pub struct OperationRange {
    pub start: usize,
    pub end: usize,
    pub includes_nested_operations: bool,
    pub origin: crate::ir::OperationOrigin,
}
#[cfg(feature = "compiler")]
#[derive(Clone, Debug)]
pub struct FunctionAbi {
    pub definition: Option<crate::ir::SourceOrigin>,
    pub instance: Option<String>,
    pub ranges: Vec<AbiRange>,
}
#[cfg(feature = "compiler")]
#[derive(Clone, Debug)]
pub struct AbiRange {
    pub start: usize,
    pub end: usize,
    pub kind: AbiRangeKind,
}
#[cfg(feature = "compiler")]
#[derive(Clone, Debug)]
pub enum AbiRangeKind {
    HiddenResultHome { slot: usize },
    ParameterHome { slot: usize, parameter_index: usize, origin: Option<std::sync::Arc<crate::ir::ParameterOrigin>> },
    ParameterInput { slot: usize, parameter_index: usize, origin: Option<std::sync::Arc<crate::ir::ParameterOrigin>> },
    ScratchSave,
    ScratchRestore,
    FrameAllocate { bytes: u32 },
    FrameRelease { bytes: u32 },
    NonvolatileSave,
    NonvolatileRestore,
    ResultAddressSave { stack_offset: u32 },
    IndirectResult { stack_offset: u32 },
    FloatResult,
    IntegerResult,
    FallibleSuccessStatus,
    FunctionReturn,
}
#[derive(Clone, Copy, Debug)]
pub enum RangeResult {
    None,
    Values(usize),
}
impl RangeResult {
    pub fn words(self) -> usize {
        match self {
            Self::None => 0,
            Self::Values(n) => n,
        }
    }
}
#[derive(Clone, Debug)]
pub enum ContinuationCode<C = NativeCode> {
    ReductionTail(C),
    ScanAdd(C),
    CompactScatter(C),
    SegmentedFinish(C),
    Scan {
        prefix: C,
        apply: C,
        source: usize,
        target: usize,
    },
}
#[derive(Clone, Debug)]
pub struct SortCode<C = NativeCode> {
    pub source: usize,
    pub target: usize,
    pub initialize: Vec<C>,
    pub histogram: C,
    pub scatter: C,
    pub gather: C,
}
#[derive(Clone, Debug)]
pub enum StageCode<C = NativeCode> {
    Element {
        main: C,
        continuation: Option<ContinuationCode<C>>,
        result: RangeResult,
    },
    Sort(SortCode<C>),
}
#[derive(Clone, Debug)]
pub struct ProgramImage<P, C = NativeCode> {
    program: ExecutionPlan,
    marker: std::marker::PhantomData<fn() -> P>,
    stages: Vec<StageCode<C>>,
    functions: Vec<(String, C)>,
    pub(crate) resources: Option<crate::resource::Module<C>>,
    pub(crate) module_code: Option<super::ModuleCode>,
}
impl<P, C> ProgramImage<P, C> {
    /// 恢复本版本编译器输出的原生代码与执行布局。
    ///
    /// # Safety
    /// 必须来自本版本编译器的同一个P入口，携带完整机器码／重定位或同源原生代码绑定、
    /// 入口 ABI、调度及前缀关系；不得由宿主输入构造或伪造条件模板的证明。
    #[doc(hidden)]
    pub unsafe fn from_compiler_output(program: ExecutionPlan, stages: Vec<StageCode<C>>, functions: Vec<(String, C)>, resources: Option<crate::resource::Module<C>>) -> Self {
        Self {
            program,
            stages,
            functions,
            resources,
            module_code: None,
            marker: std::marker::PhantomData,
        }
    }
    pub fn program(&self) -> &ExecutionPlan {
        &self.program
    }
    pub fn stages(&self) -> &[StageCode<C>] {
        &self.stages
    }
    pub fn functions(&self) -> &[(String, C)] { &self.functions }
    pub fn module_code(&self) -> Option<&super::ModuleCode> { self.module_code.as_ref() }
    fn own_code(&self) -> impl Iterator<Item = &C> {
        self.stages.iter().flat_map(|stage| {
            let (initialize, entries): (&[C], [Option<&C>; 3]) = match stage {
                StageCode::Sort(code) => (&code.initialize, [Some(&code.histogram), Some(&code.scatter), Some(&code.gather)]),
                StageCode::Element { main, continuation, .. } => {
                    let (first, second) = match continuation {
                        None => (None, None),
                        Some(ContinuationCode::ReductionTail(code) | ContinuationCode::ScanAdd(code)
                            | ContinuationCode::CompactScatter(code) | ContinuationCode::SegmentedFinish(code)) => (Some(code), None),
                        Some(ContinuationCode::Scan { prefix, apply, .. }) => (Some(prefix), Some(apply)),
                    };
                    (&[], [Some(main), first, second])
                }
            };
            initialize.iter().chain(entries.into_iter().flatten())
        }).chain(self.functions.iter().map(|(_, code)| code))
    }
    pub fn code(&self) -> impl Iterator<Item = &C> {
        self.own_code().chain(self.resources.iter().flat_map(|module| &module.images)
            .filter_map(|image| match image { crate::resource::Image::Cpu(image) => Some(image), _ => None })
            .flat_map(|image| image.own_code()))
    }
    pub fn into_parts(self) -> (ExecutionPlan, Vec<StageCode<C>>, Vec<(String, C)>, Option<crate::resource::Module<C>>) {
        (self.program, self.stages, self.functions, self.resources)
    }

    /// 同一编译映像只改变代码引用的表示；执行布局及入口类别保持。
    pub(crate) fn map_code<D>(self, mut code: impl FnMut(C) -> D) -> ProgramImage<P, D> {
        self.map_code_with(&mut |_, value| code(value))
    }
    pub(crate) fn map_code_with<D, F: FnMut(Option<&str>, C) -> D>(self, code: &mut F) -> ProgramImage<P, D> {
        let stages = self.stages.into_iter().map(|stage| match stage {
            StageCode::Element { main, continuation, result } => {
                let main = code(None,main);
                let continuation = continuation.map(|continuation| match continuation {
                    ContinuationCode::ReductionTail(c) => ContinuationCode::ReductionTail(code(None,c)),
                    ContinuationCode::ScanAdd(c) => ContinuationCode::ScanAdd(code(None,c)),
                    ContinuationCode::CompactScatter(c) => ContinuationCode::CompactScatter(code(None,c)),
                    ContinuationCode::SegmentedFinish(c) => ContinuationCode::SegmentedFinish(code(None,c)),
                    ContinuationCode::Scan { prefix, apply, source, target } =>
                        ContinuationCode::Scan { prefix: code(None,prefix), apply: code(None,apply), source, target },
                });
                StageCode::Element { main, continuation, result }
            }
            StageCode::Sort(c) => StageCode::Sort(SortCode {
                source: c.source, target: c.target,
                initialize: c.initialize.into_iter().map(|value| code(None,value)).collect(),
                histogram: code(None,c.histogram), scatter: code(None,c.scatter), gather: code(None,c.gather),
            }),
        }).collect();
        let functions = self.functions.into_iter().map(|(name, function)| {let value=code(Some(&name),function);(name,value)}).collect();
        let resources=self.resources.map(|module|crate::resource::Module{
            types:module.types,
            images:module.images.into_iter().map(|image|match image{
                crate::resource::Image::Cpu(image)=>crate::resource::Image::Cpu(image.map_code_with(code)),
                crate::resource::Image::Gpu(image)=>crate::resource::Image::Gpu(image),
            }).collect(),
        });
        ProgramImage { program: self.program, marker: std::marker::PhantomData, stages, functions, resources, module_code: self.module_code }
    }

}

/// 原发射者写入的尾部分区；end是同一bytes中的排他末端。
#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub end: usize,
    pub kind: RegionKind,
}
#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionKind {
    StackProbe,
    Alignment,
    Literal,
}

#[cfg(feature = "compiler")]
impl<P> ProgramImage<P> {
    fn own_code_mut<'a>(stages: &'a mut [StageCode], functions: &'a mut [(String, NativeCode)]) -> impl Iterator<Item = &'a mut NativeCode> {
        stages.iter_mut().flat_map(|stage| {
            let (initialize, entries): (&mut [NativeCode], [Option<&mut NativeCode>; 3]) = match stage {
                StageCode::Sort(code) => (&mut code.initialize, [Some(&mut code.histogram), Some(&mut code.scatter), Some(&mut code.gather)]),
                StageCode::Element { main, continuation, .. } => {
                    let (first, second) = match continuation {
                        None => (None, None),
                        Some(ContinuationCode::ReductionTail(code) | ContinuationCode::ScanAdd(code)
                            | ContinuationCode::CompactScatter(code) | ContinuationCode::SegmentedFinish(code)) => (Some(code), None),
                        Some(ContinuationCode::Scan { prefix, apply, .. }) => (Some(prefix), Some(apply)),
                    };
                    (std::slice::from_mut(main), [first, second, None])
                }
            };
            initialize.iter_mut().chain(entries.into_iter().flatten())
        }).chain(functions.iter_mut().map(|(_, code)| code))
    }
    pub(crate) fn code_mut(&mut self) -> impl Iterator<Item = &mut NativeCode> {
        Self::own_code_mut(&mut self.stages, &mut self.functions).chain(self.resources.iter_mut()
            .flat_map(|module| &mut module.images)
            .filter_map(|image| match image { crate::resource::Image::Cpu(image) => Some(image), _ => None })
            .flat_map(|image| Self::own_code_mut(&mut image.stages, &mut image.functions)))
    }
}
