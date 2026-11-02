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
pub enum ContinuationCode {
    ReductionTail(NativeCode),
    ScanAdd(NativeCode),
    CompactScatter(NativeCode),
    SegmentedFinish(NativeCode),
    Scan {
        prefix: NativeCode,
        apply: NativeCode,
        source: usize,
        target: usize,
    },
}
#[derive(Clone, Debug)]
pub struct SortCode {
    pub source: usize,
    pub target: usize,
    pub initialize: Vec<NativeCode>,
    pub histogram: NativeCode,
    pub scatter: NativeCode,
    pub gather: NativeCode,
}
#[derive(Clone, Debug)]
pub enum StageCode {
    Element {
        main: NativeCode,
        continuation: Option<ContinuationCode>,
        result: RangeResult,
    },
    Sort(SortCode),
}
#[derive(Clone, Debug)]
pub struct ProgramImage<P> {
    program: ExecutionPlan,
    marker: std::marker::PhantomData<fn() -> P>,
    stages: Vec<StageCode>,
}
impl<P> ProgramImage<P> {
    /// 恢复本版本编译器输出的原生代码与执行布局。
    ///
    /// # Safety
    /// 必须来自本版本编译器的同一个 P 入口，包含完整未修改的机器码、重定位、
    /// 入口 ABI、调度及前缀关系；不得由宿主输入构造或伪造条件模板的证明。
    #[doc(hidden)]
    pub unsafe fn from_compiler_output(program: ExecutionPlan, stages: Vec<StageCode>) -> Self {
        Self {
            program,
            stages,
            marker: std::marker::PhantomData,
        }
    }
    pub fn program(&self) -> &ExecutionPlan {
        &self.program
    }
    pub fn stages(&self) -> &[StageCode] {
        &self.stages
    }
    pub fn into_parts(self) -> (ExecutionPlan, Vec<StageCode>) {
        (self.program, self.stages)
    }
}
