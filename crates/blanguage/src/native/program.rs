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
fn compile_functions(worker: &crate::compiler::Worker, program: &Program) -> Result<Vec<(String, NativeCode)>, String> {
    crate::parallel::try_map(worker, program.functions.iter().collect(), |_| (), |_, function| {
        super::emit_function(function).map(|code| (function.name.clone(), code))
    }).map_err(|error| error.to_string())
}
pub(crate) fn compile_in<P: Deref<Target = Program>>(
    worker: &crate::compiler::Worker, program: P,
) -> Result<ProgramImage<P>, String> {
    let stages = compile_stages(worker, &program)?;
    let functions = compile_functions(worker, &program)?;
    let resources=program.resources.as_ref().map(|resources|
        crate::resource::compile_image(resources,worker).map_err(|error|error.to_string())).transpose()?;
    Ok(unsafe { ProgramImage::from_compiler_output(ExecutionPlan::from_program(&program), stages, functions,resources) })
}
impl crate::Compiler {
    pub fn native<P: Deref<Target = Program>>(&self, program: P) -> Result<ProgramImage<P>, String> {
        self.native_with_inputs(program, Vec::new())
    }
    /// 根代码生成后，由原链接器消费实际对象／静态库；提供者源实现不参与本阶段。
    pub fn native_with_inputs<P: Deref<Target = Program>>(
        &self, program: P, inputs: Vec<crate::object::Input>,
    ) -> Result<ProgramImage<P>, String> {
        let source = &*program;
        self.execute(|worker| {
            let mut image = compile_in(worker, source)?;
            let imports = source.imports.iter().chain(source.resources.iter()
                .flat_map(|resources| &resources.programs).flat_map(|(_, program)| &program.imports))
                .map(|(name, contract)| (name.as_str(), contract)).collect();
            link_inputs(image.module_link(), inputs, &[], &imports)?;
            Ok::<_, String>(image)
        }).map_err(|error| error.to_string()).map(|mut image| {
            let modules = image.module_code.take();
            let (plan, stages, functions, resources) = image.into_parts();
            let mut image = unsafe { ProgramImage::from_compiler_output(plan, stages, functions, resources) };
            image.module_code = modules;
            image
        })
    }
}

/// Names borrow the original root code. Only the linked output can change.
pub(crate) struct ModuleLink<'a> {
    pub(crate) local: Vec<&'a str>,
    pub(crate) external: Vec<&'a str>,
    pub(crate) types: usize,
    pub(crate) programs: usize,
    pub(crate) output: &'a mut Option<crate::native::ModuleCode>,
}

pub(crate) fn link_inputs(
    image: ModuleLink<'_>, inputs: Vec<crate::object::Input>, borrowed: &[&crate::object::Object],
    imports: &std::collections::BTreeMap<&str, &Vec<u8>>,
) -> Result<(), String> {
    let roots = image.external.into_iter().map(|name| {
        let contract = imports.get(name).ok_or_else(|| format!("缺少模块函数接口 {name}"))?;
        Ok(crate::linker::RootView::new(name, contract))
    }).collect::<Result<Vec<_>, String>>()?;
    let types = u32::try_from(image.types).map_err(|error| error.to_string())?;
    let programs = u32::try_from(image.programs).map_err(|error| error.to_string())?;
    *image.output = crate::linker::native::modules(inputs, borrowed, &roots, &image.local, types, programs)
        .map_err(|error| error.to_string())?;
    Ok(())
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
            RangeResult::Values(1),
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
