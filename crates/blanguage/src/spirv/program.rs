//! 编译完整阶段及实际消费的集合／间接启动入口；GPU 资源装载不再调用 Shader 发射器。
use super::*;
use std::ops::Deref;

pub fn compile_program<P: Deref<Target = Program>>(program: P) -> Result<ProgramImage<P>, String> {
    crate::Compiler::available().map_err(|error| error.to_string())?.spirv(program)
}
fn compile_stages(worker: &crate::compiler::Worker, program: &Program) -> Result<Vec<StageCode>, String> {
    if program.target() == crate::ast::Target::Cpu {
        return Err("CPU 专属程序不能提交给 GPU".into());
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
    pub fn spirv<P: Deref<Target = Program>>(&self, program: P) -> Result<ProgramImage<P>, String> {
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
    let selected = stage.selection.is_some();
    let tail = match stage.operation {
        Operation::Reduce(_) | Operation::CustomReduce(_) => Some(Pass::ReductionTail),
        Operation::ScanExclusive(_)
        | Operation::CustomScan(_)
        | Operation::CustomSegmentedReduce(_)
        | Operation::Compact(_)
        | Operation::SegmentedReduce(_) => Some(Pass::ScanTail),
        Operation::Map if selected => Some(Pass::ScanTail),
        Operation::Map | Operation::StableSort(_) => None,
    };
    let scan_add = if matches!(
        stage.operation,
        Operation::ScanExclusive(_)
            | Operation::CustomScan(_)
            | Operation::CustomSegmentedReduce(_)
            | Operation::Compact(_)
            | Operation::SegmentedReduce(_)
    ) || selected
    {
        Some(Pass::ScanAdd)
    } else {
        None
    };
    let finish = match &stage.operation {
        Operation::Map if selected => Some(("scatter", Pass::SelectionScatter)),
        Operation::Compact(_) => Some(("scatter", Pass::CompactScatter)),
        Operation::SegmentedReduce(_) | Operation::CustomSegmentedReduce(_) => {
            Some(("finish", Pass::ScanRootAdd))
        }
        Operation::ScanExclusive(scan) if scan.needs_root_add(program) => {
            Some(("root-add", Pass::ScanRootAdd))
        }
        Operation::CustomScan(scan) if scan.needs_root_add(program) => {
            Some(("root-add", Pass::ScanRootAdd))
        }
        _ => None,
    };
    let dispatch = if selected {
        Some(Pass::SelectionArguments)
    } else {
        stage.prefix.map(Pass::DispatchArguments)
    };
    let sort = if let Operation::StableSort(sort) = &stage.operation {
        let passes = (0..8)
            .map(|i| Pass::SortHistogram(i * 4))
            .chain((0..8).map(|i| Pass::SortScatter(i * 4)))
            .chain([
                Pass::SortScan,
                Pass::ScanTail,
                Pass::ScanAdd,
                Pass::SortGather,
            ])
            .chain((0..sort.keys.len() - 1).map(Pass::SortKey));
        Some(
            passes
                .map(|pass| {
                    Ok(ShaderCode {
                        words: emit_pass(program, i, pass)?,
                        ranks: matches!(pass, Pass::SortHistogram(_) | Pass::SortScatter(_)),
                    })
                })
                .collect::<Result<_, &'static str>>()?,
        )
    } else {
        None
    };
    Ok(StageCode {
        main: emit(program, i)?,
        tail: tail.map(|p| emit_pass(program, i, p)).transpose()?,
        scan_add: scan_add.map(|p| emit_pass(program, i, p)).transpose()?,
        finish: finish
            .map(|(name, p)| Ok((name.into(), emit_pass(program, i, p)?)))
            .transpose()?,
        dispatch: dispatch.map(|p| emit_pass(program, i, p)).transpose()?,
        selector: selected
            .then(|| emit_pass(program, i, Pass::SelectionScan))
            .transpose()?,
        sort,
    })
}
