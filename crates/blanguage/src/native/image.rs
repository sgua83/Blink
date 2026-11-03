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
}
impl<P, C> ProgramImage<P, C> {
    /// 恢复本版本编译器输出的原生代码与执行布局。
    ///
    /// # Safety
    /// 必须来自本版本编译器的同一个P入口，携带完整机器码／重定位或同源原生代码绑定、
    /// 入口 ABI、调度及前缀关系；不得由宿主输入构造或伪造条件模板的证明。
    #[doc(hidden)]
    pub unsafe fn from_compiler_output(program: ExecutionPlan, stages: Vec<StageCode<C>>) -> Self {
        Self {
            program,
            stages,
            marker: std::marker::PhantomData,
        }
    }
    pub fn program(&self) -> &ExecutionPlan {
        &self.program
    }
    pub fn stages(&self) -> &[StageCode<C>] {
        &self.stages
    }
    pub fn into_parts(self) -> (ExecutionPlan, Vec<StageCode<C>>) {
        (self.program, self.stages)
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
    pub(crate) fn stages_mut(&mut self) -> &mut [StageCode] {
        &mut self.stages
    }
}
