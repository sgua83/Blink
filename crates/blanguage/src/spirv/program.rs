//! 编译完整阶段及实际消费的集合／间接启动入口；GPU 资源装载不再调用 Shader 发射器。
use super::*;
use crate::spirv::{ScanShader, scan_shaders};
use std::ops::Deref;

pub fn compile_program<P: Deref<Target = Program>>(program: P) -> Result<ProgramImage<P>, String> {
    crate::Compiler::available().map_err(|error| error.to_string())?.spirv(program)
}
pub(crate) fn check_shader_pages(program: &Program) -> Result<(), &'static str> {
    if !program.stages().is_empty() && !crate::ir::dataflow::shader_pages_fit(program, PAGE_BYTES) {
        return Err("GPU 页寻址要求单条记录能容纳于一个物理页");
    }
    Ok(())
}

fn compile_stages(worker: &crate::compiler::Worker, program: &Program) -> Result<(ExecutionPlan, Vec<StageCode>), String> {
    if program.target() == crate::ast::Target::Cpu {
        return Err("CPU 专属程序不能提交给 GPU".into());
    }
    check_shader_pages(program)?;
    let requirements = program.requirements();
    let compiled = crate::parallel::try_map(worker, (0..program.stages().len()).collect(), |_| (),
        |_, index| compile_stage(program, index, requirements)).map_err(|error| error.to_string())?;
    let (layout, stages) = compiled.into_iter().unzip();
    Ok((ExecutionPlan::from_program(program, layout, requirements), stages))
}
pub(crate) fn compile_in<P: Deref<Target = Program>>(
    worker: &crate::compiler::Worker, program: &P,
) -> Result<ProgramImage<P>, String> {
    let (plan, stages) = compile_stages(worker, program)?;
    Ok(unsafe { ProgramImage::from_compiler_output(plan, stages) })
}
impl crate::Compiler {
    pub fn spirv<P: Deref<Target = Program>>(&self, program: P) -> Result<ProgramImage<P>, String> {
        let source = &*program;
        let (plan, stages) = self.execute(|worker| compile_stages(worker, source))
            .map_err(|error| error.to_string())?;
        Ok(unsafe { ProgramImage::from_compiler_output(plan, stages) })
    }
}

#[repr(u32)]
enum EntryOperation {
    Map,
    Reduce,
    Scan,
    Segmented,
    Compact,
    Sort,
    CustomScan,
    CustomSegmented,
    CustomReduce,
}

#[allow(dead_code)]
#[repr(u32)]
enum EntryTail {
    None,
    Reduce,
    Scan,
}

#[allow(dead_code)]
#[repr(u32)]
enum EntryFinish {
    None,
    Selection,
    Compact,
    Segmented,
    ScanRoot,
}

#[allow(dead_code)]
#[repr(u32)]
enum EntryDispatch {
    None,
    Prefix,
    Selection,
}

#[allow(dead_code)]
#[repr(u32)]
enum FinishKernel {
    Original,
    Main,
    Add,
}

#[repr(C)]
struct StageEntries {
    tail: EntryTail,
    finish: EntryFinish,
    dispatch: EntryDispatch,
    scan_add: u32,
    finish_kernel: FinishKernel,
    original_main: u32,
    sort_passes: usize,
}

#[allow(dead_code)]
#[repr(u32)]
enum SortShaderKind {
    Histogram,
    Scatter,
    Scan,
    ScanTail,
    ScanAdd,
    Gather,
    Key,
}

#[repr(C)]
struct SortShaderEntry {
    kind: SortShaderKind,
    parameter: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::stage_entries"]
    fn stage_entries(operation: EntryOperation, selected: u32, prefix: u32, root_add: u32, keys: usize) -> StageEntries;
    #[link_name = "bootstrap.spirv_envelope::sort_shader_entry"]
    fn sort_shader_entry(index: usize) -> SortShaderEntry;
}

pub(crate) fn compile_stage(
    program: &Program, i: usize, requirements: DeviceRequirements,
) -> Result<(ExecutionStage, StageCode), &'static str> {
    let stage = &program.stages()[i];
    let (tile_layout, mut inputs) = shader_stage_inputs(stage);
    // Shader input also includes the physical low half of a wide word.
    let mut layout = ExecutionStage::from_stage(stage, inputs.keys().copied().collect());
    let can_use_single_group = program.buffers[stage.domain].count
        .is_none_or(|count| count <= layout.workgroup_width());
    let features = ProgramShaderFeatures::from(requirements);
    let (operation, root_add, keys) = match &stage.operation {
        Operation::Map => (EntryOperation::Map, false, 0),
        Operation::Reduce(_) => (EntryOperation::Reduce, false, 0),
        Operation::CustomReduce(_) => (EntryOperation::CustomReduce, false, 0),
        Operation::ScanExclusive(scan) => (EntryOperation::Scan, scan.needs_root_add(program), 0),
        Operation::CustomScan(scan) => (EntryOperation::CustomScan, scan.needs_root_add(program), 0),
        Operation::SegmentedReduce(_) => (EntryOperation::Segmented, false, 0),
        Operation::CustomSegmentedReduce(_) => (EntryOperation::CustomSegmented, false, 0),
        Operation::Compact(_) => (EntryOperation::Compact, false, 0),
        Operation::StableSort(sort) => (EntryOperation::Sort, false, sort.keys.len()),
    };
    let entries = unsafe { stage_entries(
        operation, u32::from(stage.selection.is_some()), u32::from(stage.prefix.is_some()),
        u32::from(root_add), keys,
    ) };
    let [root_scan, tail_scan] = scan_shaders(
        &stage.operation, stage.selection.is_some(), program.buffers[stage.domain].count,
    );
    let original_facts = (entries.original_main != 0).then(|| ShaderStageFacts::new(stage, true));
    let mut emit = |pass, kernel: Option<&PreparedReduction>, publishes_summary| {
        let (words, bytes) = emit_shader(
            program, stage, pass, &mut inputs, &tile_layout, features, kernel, original_facts.as_ref(), publishes_summary,
        )?;
        layout.shared_bytes = layout.shared_bytes.max(bytes);
        Ok::<_, &'static str>(words)
    };
    let (dispatch, selected) = match entries.dispatch {
        EntryDispatch::None => (None, false),
        EntryDispatch::Prefix => (stage.prefix.map(Pass::DispatchArguments), false),
        EntryDispatch::Selection => (Some(Pass::SelectionArguments), true),
    };
    let tail = match entries.tail {
        EntryTail::None => None,
        EntryTail::Reduce => Some(Pass::ReductionTail),
        EntryTail::Scan => Some(Pass::ScanTail),
    };
    let segmented = matches!(entries.finish, EntryFinish::Segmented);
    let finish = match entries.finish {
        EntryFinish::None => None,
        EntryFinish::Selection => Some(("scatter", Pass::SelectionScatter)),
        EntryFinish::Compact => Some(("scatter", Pass::CompactScatter)),
        EntryFinish::Segmented => Some(("finish", Pass::ScanRootAdd)),
        EntryFinish::ScanRoot => Some(("root-add", Pass::ScanRootAdd)),
    };
    let sorting = entries.sort_passes != 0;
    let mut additional = sorting.then(|| {
        (0..entries.sort_passes).map(|index| {
            let entry = unsafe { sort_shader_entry(index) };
            let (pass, ranks) = match entry.kind {
                SortShaderKind::Histogram => (Pass::SortHistogram(entry.parameter as u32), true),
                SortShaderKind::Scatter => (Pass::SortScatter(entry.parameter as u32), true),
                SortShaderKind::Scan => (Pass::SortScan, false),
                SortShaderKind::ScanTail => (Pass::ScanTail, false),
                SortShaderKind::ScanAdd => (Pass::ScanAdd, false),
                SortShaderKind::Gather => (Pass::SortGather, false),
                SortShaderKind::Key => (Pass::SortKey(entry.parameter), false),
            };
            let publishes_summary = match pass {
                Pass::SortScan => root_scan != ScanShader::Terminal,
                Pass::ScanTail => tail_scan != ScanShader::Terminal,
                _ => true,
            };
            Ok(ShaderCode { words: emit(pass, None, publishes_summary)?, ranks })
        }).collect::<Result<Vec<_>, &'static str>>()
    }).transpose()?;
    let main_kernel = prepare_reduction(stage, Pass::Main);
    let main = emit(Pass::Main, main_kernel.as_ref(), selected || sorting || root_scan != ScanShader::Terminal)?;
    let finish_main = main_kernel.as_ref().filter(|_| matches!(entries.finish_kernel, FinishKernel::Main));
    let tail_kernel = tail.and_then(|pass| prepare_reduction(stage, pass));
    let tail = tail.map(|pass| emit(pass, tail_kernel.as_ref(), tail_scan != ScanShader::Terminal)).transpose()?;
    let (scan_add, finish_add) = if entries.scan_add != 0 {
        let kernel = prepare_reduction(stage, Pass::ScanAdd);
        let words = emit(Pass::ScanAdd, kernel.as_ref(), true)?;
        (Some(words), kernel.filter(|_| matches!(entries.finish_kernel, FinishKernel::Add)))
    } else {
        (None, None)
    };
    let finish = finish.map(|(name, pass)| {
        let kernel = finish_main.or(finish_add.as_ref());
        if segmented && can_use_single_group {
            additional = Some(vec![ShaderCode {
                words: emit(Pass::SegmentedSingleGroup, kernel, true)?,
                ranks: false,
            }]);
        }
        Ok((name.into(), emit(pass, kernel, true)?))
    }).transpose()?;
    let root_pass = if sorting { Pass::SortScan } else if selected { Pass::SelectionScan } else { Pass::Main };
    for (role, pass, kernel) in [
        (root_scan, root_pass, main_kernel.as_ref()),
        (tail_scan, Pass::ScanTail, tail_kernel.as_ref()),
    ] {
        if role == ScanShader::Both {
            additional.get_or_insert_with(Vec::new).push(ShaderCode {
                words: emit(pass, kernel, false)?,
                ranks: false,
            });
        }
    }
    let code = StageCode {
        main,
        tail,
        scan_add,
        finish,
        dispatch: dispatch.map(|p| emit(p, None, true)).transpose()?,
        selector: selected.then(|| emit(Pass::SelectionScan, None, root_scan != ScanShader::Terminal)).transpose()?,
        additional,
    };
    Ok((layout, code))
}
