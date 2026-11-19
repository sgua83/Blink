//! 完整程序的机器码编译由编译器拥有；执行器只装载已生成的范围与集合入口。
use super::*;
use std::{collections::BTreeMap, ops::Deref};

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
            let link = image.module_link();
            let imports = source.imports.iter().chain(source.resources.iter()
                .flat_map(|resources| &resources.programs).flat_map(|(_, program)| &program.imports));
            let contracts = link_contracts(link.external.iter().copied(), imports);
            link_inputs(link, inputs, &[], &contracts)?;
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

pub(crate) fn link_contracts<'a>(
    names: impl Iterator<Item = &'a str>,
    definitions: impl Iterator<Item = (&'a String, &'a Vec<u8>)>,
) -> BTreeMap<&'a str, Option<&'a [u8]>> {
    let mut contracts: BTreeMap<_, _> = names.map(|name| (name, None)).collect();
    if !contracts.is_empty() {
        for (name, bytes) in definitions {
            if let Some(contract) = contracts.get_mut(name.as_str()) {
                *contract = Some(bytes.as_slice());
            }
        }
    }
    contracts
}

pub(crate) fn link_inputs(
    image: ModuleLink<'_>, inputs: Vec<crate::object::Input>, borrowed: &[&crate::object::Object],
    contracts: &BTreeMap<&str, Option<&[u8]>>,
) -> Result<bool, String> {
    let roots = image.external.into_iter().map(|name| {
        let contract = contracts.get(name).copied().flatten()
            .ok_or_else(|| format!("缺少模块函数接口 {name}"))?;
        Ok(crate::linker::RootView::new(name, contract))
    }).collect::<Result<Vec<_>, String>>()?;
    let types = u32::try_from(image.types).map_err(|error| error.to_string())?;
    let programs = u32::try_from(image.programs).map_err(|error| error.to_string())?;
    let linked = crate::linker::native::modules(inputs, borrowed, &roots, &image.local, types, programs)
        .map_err(|error| error.to_string())?;
    if *image.output == linked {
        return Ok(false);
    }
    *image.output = linked;
    Ok(true)
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum RangeOperation {
    Map,
    Reduce,
    CustomReduce,
    CustomScan,
    CustomSegmentedReduce,
    SegmentedReduce,
    Scan,
    Compact,
    Sort,
}

#[repr(u32)]
enum RangeFollow {
    None,
    ReduceTail,
    CustomScan,
    CustomSegmentedReduce,
    SegmentedFinish,
    ScanAdd,
    CompactScatter,
    Sort,
}

#[repr(C)]
struct RangeEntries {
    follow: RangeFollow,
    values: u32,
    words: usize,
    first_key: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::range_entries"]
    fn range_entries(operation: RangeOperation, words_or_keys: usize, complete: u32) -> RangeEntries;
}

// The payload borrows the exact operation kept in the original stage.
struct RangeStage<'a> {
    program: &'a Program,
    index: usize,
    operation: RangeOperation,
    payload: *const std::ffi::c_void,
    words_or_keys: usize,
}

impl<'a> RangeStage<'a> {
    fn new(program: &'a Program, index: usize) -> Self {
        use RangeOperation as Kind;
        fn payload<T>(value: &T) -> *const std::ffi::c_void {
            std::ptr::from_ref(value).cast()
        }
        let (operation, payload, words_or_keys) = match &program.stages()[index].operation {
            Operation::Map => (Kind::Map, std::ptr::null(), 0),
            Operation::Reduce(value) =>
                (Kind::Reduce, payload(value), value.ty.size.div_ceil(4) as usize),
            Operation::CustomReduce(value) =>
                (Kind::CustomReduce, payload(value), value.ty.size.div_ceil(4) as usize),
            Operation::CustomScan(value) =>
                (Kind::CustomScan, payload(value), value.summary_words()),
            Operation::CustomSegmentedReduce(value) =>
                (Kind::CustomSegmentedReduce, payload(value), value.summary_words()),
            Operation::SegmentedReduce(value) =>
                (Kind::SegmentedReduce, payload(value), value.summary_words()),
            Operation::ScanExclusive(value) => (Kind::Scan, payload(value), value.summary_words()),
            Operation::Compact(value) => (Kind::Compact, payload(value), 1),
            Operation::StableSort(value) => (Kind::Sort, payload(value), value.keys.len()),
        };
        Self { program, index, operation, payload, words_or_keys }
    }

    fn entries(&self, complete: bool) -> RangeEntries {
        unsafe { range_entries(self.operation, self.words_or_keys, u32::from(complete)) }
    }

    // The original operation discriminant fixes the borrowed payload type.
    unsafe fn value<T>(&self) -> &T {
        unsafe { &*self.payload.cast() }
    }

    fn custom(&self, value: &CustomReduction, pass: custom_reduce::Pass,
              head: Option<u32>, count: Option<PrefixCount>) -> Result<NativeCode, &'static str> {
        custom_reduce::emit(self.program, self.index, value, pass, head, count)
    }

    fn first(&self, entries: &RangeEntries) -> Result<NativeCode, &'static str> {
        use custom_reduce::Pass;
        match self.operation {
            RangeOperation::Map => emit_map(self.program, self.index),
            RangeOperation::Reduce => Ok(reduce::emit(self.program, unsafe { self.value() })),
            RangeOperation::CustomReduce => self.custom(unsafe { self.value() }, Pass::Reduce, None, None),
            RangeOperation::CustomScan => {
                let scan: &CustomScan = unsafe { self.value() };
                self.custom(&scan.values, Pass::Scan, scan.head_word, None)
            }
            RangeOperation::CustomSegmentedReduce => {
                let reduction: &CustomSegmentedReduction = unsafe { self.value() };
                self.custom(&reduction.values, Pass::SegmentReduce, Some(reduction.head_word), Some(reduction.count))
            }
            RangeOperation::SegmentedReduce =>
                Ok(segmented_reduce::emit(self.program, unsafe { self.value() }, false)),
            RangeOperation::Scan => Ok(scan::emit_scan(self.program, unsafe { self.value() }, false)),
            RangeOperation::Compact => Ok(compact::emit_count(self.program, unsafe { self.value() })),
            RangeOperation::Sort =>
                Ok(sort::initialize(self.program, unsafe { self.value() }, entries.first_key, true)),
        }
    }
}

pub fn emit(program: &Program, index: usize) -> Result<NativeCode, &'static str> {
    let stage = RangeStage::new(program, index);
    stage.first(&stage.entries(false))
}

pub(crate) fn compile_stage(program: &Program, index: usize) -> Result<StageCode, &'static str> {
    use custom_reduce::Pass;
    let stage = RangeStage::new(program, index);
    let entries = stage.entries(true);
    let main = stage.first(&entries)?;
    let continuation = match entries.follow {
        RangeFollow::None => None,
        RangeFollow::ReduceTail => Some(ContinuationCode::ReductionTail(
            stage.custom(unsafe { stage.value() }, Pass::ReduceTail, None, None)?,
        )),
        RangeFollow::CustomScan => {
            let scan: &CustomScan = unsafe { stage.value() };
            Some(ContinuationCode::Scan {
                prefix: stage.custom(&scan.values, Pass::ScanPrefix, scan.head_word, None)?,
                apply: stage.custom(&scan.values, Pass::ScanAdd, scan.head_word, None)?,
                source: scan.values.fields.source,
                target: scan.values.fields.target,
            })
        }
        RangeFollow::CustomSegmentedReduce => {
            let reduction: &CustomSegmentedReduction = unsafe { stage.value() };
            let head = Some(reduction.head_word);
            let count = Some(reduction.count);
            Some(ContinuationCode::Scan {
                prefix: stage.custom(&reduction.values, Pass::SegmentPrefix, head, count)?,
                apply: stage.custom(&reduction.values, Pass::SegmentFinish, head, count)?,
                source: reduction.values.fields.source,
                target: reduction.values.fields.target,
            })
        }
        RangeFollow::SegmentedFinish => Some(ContinuationCode::SegmentedFinish(
            emit_segmented_reduce_finish(program, unsafe { stage.value() }),
        )),
        RangeFollow::ScanAdd =>
            Some(ContinuationCode::ScanAdd(emit_scan_add(program, unsafe { stage.value() }))),
        RangeFollow::CompactScatter => Some(ContinuationCode::CompactScatter(
            emit_compact_scatter(program, unsafe { stage.value() }),
        )),
        RangeFollow::Sort => {
            let sort: &Sorting = unsafe { stage.value() };
            let initialize = std::iter::once(main)
                .chain((0..entries.first_key).rev().map(|key| emit_sort_key(program, sort, key)))
                .collect();
            return Ok(StageCode::Sort(SortCode {
                source: sort.source,
                target: sort.target,
                initialize,
                histogram: emit_sort_histogram(program),
                scatter: emit_sort_scatter(program),
                gather: emit_sort_gather(program, sort),
            }));
        }
    };
    let result = if entries.values == 0 { RangeResult::None } else { RangeResult::Values(entries.words) };
    Ok(StageCode::Element { main, continuation, result })
}
