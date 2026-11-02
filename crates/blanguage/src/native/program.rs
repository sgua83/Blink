//! 完整程序的机器码编译由编译器拥有；执行器只装载已生成的范围与集合入口。
use super::*;
use std::ops::Deref;

pub fn compile_program<P: Deref<Target = Program>>(program: P) -> Result<ProgramImage<P>, String> {
    crate::Compiler::available().map_err(|error| error.to_string())?.native(program)
}
fn compile_stages(worker: &crate::compiler::Worker, program: &Program) -> Result<Vec<StageCode>, String> {
    if program.target() == crate::ast::Target::Gpu {
        return Err("GPU 专属程序不能提交给 CPU".into());
    }
    crate::parallel::try_map(worker, (0..program.stages().len()).collect(), |_| (),
        |_, index| compile_stage(program, index)).map_err(|error| error.to_string())
}
pub(crate) fn compile_in<P: Deref<Target = Program>>(
    worker: &crate::compiler::Worker, program: P,
) -> Result<ProgramImage<P>, String> {
    let stages = compile_stages(worker, &program)?;
    Ok(unsafe { ProgramImage::from_compiler_output(ExecutionPlan::from_program(&program), stages) })
}
impl crate::Compiler {
    pub fn native<P: Deref<Target = Program>>(&self, program: P) -> Result<ProgramImage<P>, String> {
        let source = &*program;
        let (plan, stages) = self.execute(|worker| {
            let stages = compile_stages(worker, source)?;
            Ok::<_, String>((ExecutionPlan::from_program(source), stages))
        }).map_err(|error| error.to_string())?;
        Ok(unsafe { ProgramImage::from_compiler_output(plan, stages) })
    }
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
