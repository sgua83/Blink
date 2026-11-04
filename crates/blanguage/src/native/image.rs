//! 已编译CPU映像与范围ABI；编译器和执行器共享同一份代码及布局。
use crate::ir::{ExecutionPlan,Scalar};

/// CPU范围入口的同一绑定表：原地址和实际记录步长。纹理槽只消费地址。
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BufferAddress {
    pub address: *mut u32,
    pub stride: usize,
}
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
}
#[derive(Clone, Copy, Debug)]
pub enum RangeResult {
    None,
    Values(usize),
    UnsignedSum,
}
impl RangeResult {
    pub fn words(self) -> usize {
        match self {
            Self::None => 0,
            Self::Values(n) => n,
            Self::UnsignedSum => 1,
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
}
impl<P, C> ProgramImage<P, C> {
    /// 恢复本版本编译器输出的原生代码与执行布局。
    ///
    /// # Safety
    /// 必须来自本版本编译器的同一个P入口，携带完整机器码／重定位或同源原生代码绑定、
    /// 入口 ABI、调度及前缀关系；不得由宿主输入构造或伪造条件模板的证明。
    #[doc(hidden)]
    pub unsafe fn from_compiler_output(program: ExecutionPlan, stages: Vec<StageCode<C>>, functions: Vec<(String, C)>) -> Self {
        Self {
            program,
            stages,
            functions,
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
    pub fn code(&self) -> impl Iterator<Item = &C> {
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
    pub fn into_parts(self) -> (ExecutionPlan, Vec<StageCode<C>>, Vec<(String, C)>) {
        (self.program, self.stages, self.functions)
    }

    /// 同一编译映像只改变代码引用的表示；执行布局及入口类别保持。
    pub(crate) fn map_code<D>(self, mut code: impl FnMut(C) -> D) -> ProgramImage<P, D> {
        let stages = self.stages.into_iter().map(|stage| match stage {
            StageCode::Element { main, continuation, result } => {
                let main = code(main);
                let continuation = continuation.map(|continuation| match continuation {
                    ContinuationCode::ReductionTail(c) => ContinuationCode::ReductionTail(code(c)),
                    ContinuationCode::ScanAdd(c) => ContinuationCode::ScanAdd(code(c)),
                    ContinuationCode::CompactScatter(c) => ContinuationCode::CompactScatter(code(c)),
                    ContinuationCode::SegmentedFinish(c) => ContinuationCode::SegmentedFinish(code(c)),
                    ContinuationCode::Scan { prefix, apply, source, target } =>
                        ContinuationCode::Scan { prefix: code(prefix), apply: code(apply), source, target },
                });
                StageCode::Element { main, continuation, result }
            }
            StageCode::Sort(c) => StageCode::Sort(SortCode {
                source: c.source, target: c.target,
                initialize: c.initialize.into_iter().map(&mut code).collect(),
                histogram: code(c.histogram), scatter: code(c.scatter), gather: code(c.gather),
            }),
        }).collect();
        let functions = self.functions.into_iter().map(|(name, function)| (name, code(function))).collect();
        ProgramImage { program: self.program, marker: std::marker::PhantomData, stages, functions }
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
    pub(crate) fn code_mut(&mut self) -> impl Iterator<Item = &mut NativeCode> {
        self.stages.iter_mut().flat_map(|stage| {
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
        }).chain(self.functions.iter_mut().map(|(_, code)| code))
    }
}
