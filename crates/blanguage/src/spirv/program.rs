//! 编译完整阶段及实际消费的集合／间接启动入口；GPU 资源装载不再调用 Shader 发射器。
use super::*;
use std::ops::Deref;

#[derive(Clone, Debug)]
pub struct ShaderCode {
    pub words: Vec<u32>,
    pub ranks: bool,
}
#[derive(Clone, Debug)]
pub struct StageCode {
    pub main: Vec<u32>,
    pub tail: Option<Vec<u32>>,
    pub scan_add: Option<Vec<u32>>,
    pub finish: Option<(String, Vec<u32>)>,
    pub dispatch: Option<Vec<u32>>,
    pub selector: Option<Vec<u32>>,
    pub sort: Option<Vec<ShaderCode>>,
}
#[derive(Clone, Debug)]
pub struct ProgramImage<P> {
    program: ExecutionPlan,
    marker: std::marker::PhantomData<fn() -> P>,
    stages: Vec<StageCode>,
}
impl<P> ProgramImage<P> {
    /// 恢复本版本编译器输出的 SPIR-V 与执行布局。
    ///
    /// # Safety
    /// 必须来自本版本编译器的同一个 P 入口，包含完整未修改的 Shader、
    /// 调度、设备需求及前缀关系；不得由宿主输入构造或伪造条件模板的证明。
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
    if program.target() == crate::ast::Target::Cpu {
        return Err("CPU 专属程序不能提交给 GPU".into());
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
