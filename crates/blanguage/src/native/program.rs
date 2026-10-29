//! 完整程序的机器码编译由编译器拥有；执行器只装载已生成的范围与集合入口。
use super::*;
use std::ops::Deref;

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
pub fn compile_program<P: Deref<Target = Program>>(program: P) -> Result<ProgramImage<P>, String> {
    if program.target() == crate::ast::Target::Gpu {
        return Err("GPU 专属程序不能提交给 CPU".into());
    }
    let source = &*program;
    let stages = crate::parallel::try_map(
        (0..source.stages().len()).collect(),
        || (),
        |_, index| compile_stage(source, index),
    )
    .map_err(|error| error.to_string())?;
    Ok(ProgramImage {
        program: ExecutionPlan::from_program(&program),
        stages,
        marker: std::marker::PhantomData,
    })
}

pub(crate) fn compile_stage(program: &Program, i: usize) -> Result<StageCode, &'static str> {
    let stage = &program.stages()[i];
    let main = emit(program, i)?;
    let (continuation, result) = match &stage.operation {
        Operation::StableSort(sort) => {
            let mut initialize = vec![main];
            for key in (0..sort.keys.len() - 1).rev() {
                initialize.push(emit_sort_key(program, sort, key));
            }
            return Ok(StageCode::Sort(SortCode {
                source: sort.source,
                target: sort.target,
                initialize,
                histogram: emit_sort_histogram(program),
                scatter: emit_sort_scatter(program),
                gather: emit_sort_gather(program, sort),
            }));
        }
        Operation::CustomScan(scan) => {
            return Ok(StageCode::Element {
                main,
                continuation: Some(ContinuationCode::Scan {
                    prefix: emit_custom_scan_prefix(program, i)?,
                    apply: emit_custom_scan_add(program, i)?,
                    source: scan.values.fields.source,
                    target: scan.values.fields.target,
                }),
                result: RangeResult::Values(scan.summary_words()),
            });
        }
        Operation::CustomSegmentedReduce(reduction) => {
            return Ok(StageCode::Element {
                main,
                continuation: Some(ContinuationCode::Scan {
                    prefix: emit_custom_segmented_prefix(program, i)?,
                    apply: emit_custom_segmented_finish(program, i)?,
                    source: reduction.values.fields.source,
                    target: reduction.values.fields.target,
                }),
                result: RangeResult::Values(reduction.summary_words()),
            });
        }
        Operation::ScanExclusive(scan) => (
            Some(ContinuationCode::ScanAdd(emit_scan_add(program, scan))),
            RangeResult::Values(scan.summary_words()),
        ),
        Operation::SegmentedReduce(reduction) => (
            Some(ContinuationCode::SegmentedFinish(
                emit_segmented_reduce_finish(program, reduction),
            )),
            RangeResult::Values(reduction.summary_words()),
        ),
        Operation::CustomReduce(reduction) => (
            Some(ContinuationCode::ReductionTail(emit_custom_reduce_tail(
                program, i,
            )?)),
            RangeResult::Values(reduction.ty.size.div_ceil(4) as usize),
        ),
        Operation::Compact(compaction) => (
            Some(ContinuationCode::CompactScatter(emit_compact_scatter(
                program, compaction,
            ))),
            RangeResult::UnsignedSum,
        ),
        Operation::Reduce(reduction) => (
            None,
            RangeResult::Values(reduction.ty.size.div_ceil(4) as usize),
        ),
        Operation::Map => (None, RangeResult::None),
    };
    Ok(StageCode::Element {
        main,
        continuation,
        result,
    })
}
