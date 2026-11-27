//! 完整程序的机器码编译由编译器拥有；执行器只装载已生成的范围与集合入口。
use super::*;
use crate::names::ModuleName;
use std::ops::Deref;

pub fn compile_program<P: Deref<Target = Program>>(program: P) -> Result<ProgramImage<P>, String> {
    crate::Compiler::available().map_err(|error| error.to_string())?.native(program)
}
fn compile_stages(
    worker: &crate::compiler::Worker, program: &Program,
) -> Result<Vec<(ExecutionStage, StageCode)>, String> {
    if program.target() == crate::ast::Target::Gpu {
        return Err("GPU 专属程序不能提交给 CPU".into());
    }
    crate::parallel::try_map(worker, 0..program.stages().len(), |_| (),
        |_, index| compile_stage(program, index)).map_err(|error| error.to_string())
}
fn compile_functions(worker: &crate::compiler::Worker, program: &Program) -> Result<Vec<(String, NativeCode)>, String> {
    crate::parallel::try_map(worker, program.functions.iter(), |_| (), |_, function| {
        super::emit_function(function).map(|code| (function.name.clone(), code))
    }).map_err(|error| error.to_string())
}
/// The object's code entries are assigned as the root image is assembled.
pub(crate) fn compile_linked(
    worker: &crate::compiler::Worker,
    program: &crate::ExecutableProgram,
    mut entry: impl FnMut(Option<&str>, NativeCode) -> usize,
) -> Result<ProgramImage<crate::ExecutableProgram, usize>, String> {
    let (layout, stages) = compile_stages(worker, program)?.into_iter()
        .map(|(layout, stage)| (layout, stage.map_code(|code| entry(None, code))))
        .unzip();
    let functions = compile_functions(worker, program)?.into_iter().map(|(name, code)| {
        let index = entry(Some(&name), code);
        (name, index)
    }).collect();
    let plan = ExecutionPlan::from_program(program, layout, program.requirements(), program.execution_buffers());
    Ok(unsafe { ProgramImage::from_compiler_output(plan, stages, functions, None) })
}

pub(crate) fn compile_in<P: Deref<Target = Program>>(
    worker: &crate::compiler::Worker, program: P,
) -> Result<ProgramImage<P>, String> {
    let (layout, stages) = compile_stages(worker, &program)?.into_iter().unzip();
    let functions = compile_functions(worker, &program)?;
    let resources=program.resources.as_ref().map(|resources|
        crate::resource::compile_image(resources,worker).map_err(|error|error.to_string())).transpose()?;
    Ok(unsafe { ProgramImage::from_compiler_output(ExecutionPlan::from_program(&program, layout, program.requirements(), program.execution_buffers()), stages, functions,resources) })
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
            link_inputs(link, inputs, std::iter::empty(), |names| {
                crate::linker::bind_root_contracts(names, |name| {
                    source.imports.get(name).map(Vec::as_slice)
                })
            })?;
            Ok::<_, String>(image)
        }).map_err(|error| error.to_string()).map(|image| {
            // The worker compiled the original program borrowed from P.
            unsafe { image.with_entry_type() }
        })
    }
}

/// Names borrow the original root code. Only the linked output can change.
pub(crate) struct ModuleLink<'a> {
    pub(crate) local: Vec<ModuleName<'a>>,
    pub(crate) external: Vec<ModuleName<'a>>,
    pub(crate) types: usize,
    pub(crate) programs: usize,
    pub(crate) output: &'a mut Option<crate::native::ModuleCode>,
}

pub(crate) fn link_inputs<'a, 'object>(
    image: ModuleLink<'a>, inputs: Vec<crate::object::Input>,
    borrowed: impl IntoIterator<Item = &'object crate::object::Object>,
    bind_roots: impl FnOnce(Vec<ModuleName<'a>>) -> Result<Vec<crate::linker::RootView<'a>>, String>,
) -> Result<bool, String> {
    let linked = if image.external.is_empty() {
        None
    } else {
        let roots = bind_roots(image.external)?;
        let types = u32::try_from(image.types).map_err(|error| error.to_string())?;
        let programs = u32::try_from(image.programs).map_err(|error| error.to_string())?;
        crate::linker::native::modules(inputs, borrowed, &roots, &image.local, types, programs)
            .map_err(|error| error.to_string())?
    };
    if *image.output == linked {
        return Ok(false);
    }
    *image.output = linked;
    Ok(true)
}

#[allow(dead_code)] // Additional entries are sent by the B range emitter.
#[derive(Clone, Copy)]
#[repr(u32)]
enum RangeEntry {
    Map,
    Reduce,
    CustomReduce,
    CustomScan,
    CustomSegmentedReduce,
    SegmentedReduce,
    Scan,
    Compact,
    Sort,
    ReduceTail,
    ScanPrefix,
    ScanApply,
    SegmentPrefix,
    SegmentApply,
    SegmentedFinish,
    ScanAdd,
    CompactScatter,
    SortKey,
    SortHistogram,
    SortScatter,
    SortGather,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum RangeDestination { Main, Tail, Apply, Histogram, Scatter, Gather, Initializers }

#[allow(dead_code)]
#[repr(u32)]
enum RangeShape { Element, ReduceTail, CustomScan, SegmentedFinish, ScanAdd, CompactScatter, Sort }

#[repr(C)]
struct RangeProduct {
    shape: RangeShape,
    has_values: u32,
    words: usize,
}

#[repr(C)]
struct RangeSink {
    context: *mut std::ffi::c_void,
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, RangeEntry, RangeDestination, usize) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::emit_ranges"]
    fn emit_ranges(entry: RangeEntry, words_or_keys: usize, complete: u32, sink: RangeSink) -> RangeProduct;
}

// The discriminant and payload come from the same original operation.
struct RangeInput<'a> {
    program: &'a Program,
    index: usize,
    entry: RangeEntry,
    payload: *const std::ffi::c_void,
    words_or_keys: usize,
    head: Option<u32>,
    count: Option<PrefixCount>,
}

impl<'a> RangeInput<'a> {
    fn new(program: &'a Program, index: usize) -> Self {
        use RangeEntry as Entry;
        fn borrow<T>(value: &T) -> *const std::ffi::c_void {
            std::ptr::from_ref(value).cast()
        }
        let (entry, payload, words_or_keys, head, count) = match &program.stages()[index].operation {
            Operation::Map => (Entry::Map, std::ptr::null(), 0, None, None),
            Operation::Reduce(value) =>
                (Entry::Reduce, borrow(value), value.ty.size.div_ceil(4) as usize, None, None),
            Operation::CustomReduce(value) =>
                (Entry::CustomReduce, borrow(value), value.ty.size.div_ceil(4) as usize, None, None),
            Operation::CustomScan(value) =>
                (Entry::CustomScan, borrow(&value.values), value.summary_words(), value.head_word, None),
            Operation::CustomSegmentedReduce(value) =>
                (Entry::CustomSegmentedReduce, borrow(&value.values), value.summary_words(),
                 Some(value.head_word), Some(value.count)),
            Operation::SegmentedReduce(value) =>
                (Entry::SegmentedReduce, borrow(value), value.summary_words(), None, None),
            Operation::ScanExclusive(value) =>
                (Entry::Scan, borrow(value), value.summary_words(), None, None),
            Operation::Compact(value) => (Entry::Compact, borrow(value), 1, None, None),
            Operation::StableSort(value) => (Entry::Sort, borrow(value), value.keys.len(), None, None),
        };
        Self { program, index, entry, payload, words_or_keys, head, count }
    }

    unsafe fn value<T>(&self) -> &T {
        unsafe { &*self.payload.cast() }
    }

    fn custom(&self, pass: custom_reduce::Pass) -> Result<NativeCode, &'static str> {
        let reduction: &CustomReduction = unsafe { self.value() };
        custom_reduce::emit(self.program, self.index, reduction, pass, self.head, self.count)
    }
}

struct RangeEmission<'a> {
    input: RangeInput<'a>,
    inputs: crate::ir::dataflow::InputFragments,
    used: std::collections::BTreeSet<SharedWord>,
    tile_layout: crate::ir::dataflow::TileLayout,
    code: [Option<NativeCode>; 6],
    initializers: Vec<NativeCode>,
    error: Option<&'static str>,
}

impl<'a> RangeEmission<'a> {
    fn new(program: &'a Program, index: usize) -> Self {
        let stage = &program.stages()[index];
        let mut used = std::collections::BTreeSet::new();
        let layout = stage.tile_layout(false, Some(&mut used));
        let inputs = crate::ir::dataflow::input_fragments(stage, &used);
        Self {
            input: RangeInput::new(program, index), inputs, used, tile_layout: layout.words,
            code: Default::default(), initializers: Vec::new(), error: None,
        }
    }

    fn emit(&mut self, entry: RangeEntry, key: usize) -> Result<NativeCode, &'static str> {
        use custom_reduce::Pass;
        use RangeEntry as Entry;
        let input = &self.input;
        let program = input.program;
        let code = match entry {
            Entry::Map => return emit_map(program, input.index, &self.inputs, &self.used, std::mem::take(&mut self.tile_layout)),
            Entry::Reduce => reduce::emit(program, unsafe { input.value() }),
            Entry::CustomReduce => return input.custom(Pass::Reduce),
            Entry::CustomScan => return input.custom(Pass::Scan),
            Entry::CustomSegmentedReduce => return input.custom(Pass::SegmentReduce),
            Entry::SegmentedReduce => segmented_reduce::emit(program, unsafe { input.value() }, false),
            Entry::Scan => scan::emit_scan(program, unsafe { input.value() }, false),
            Entry::Compact => compact::emit_count(program, unsafe { input.value() }),
            Entry::Sort => sort::initialize(program, unsafe { input.value() }, key, true),
            Entry::ReduceTail => return input.custom(Pass::ReduceTail),
            Entry::ScanPrefix => return input.custom(Pass::ScanPrefix),
            Entry::ScanApply => return input.custom(Pass::ScanAdd),
            Entry::SegmentPrefix => return input.custom(Pass::SegmentPrefix),
            Entry::SegmentApply => return input.custom(Pass::SegmentFinish),
            Entry::SegmentedFinish => emit_segmented_reduce_finish(program, unsafe { input.value() }),
            Entry::ScanAdd => emit_scan_add(program, unsafe { input.value() }),
            Entry::CompactScatter => emit_compact_scatter(program, unsafe { input.value() }),
            Entry::SortKey => emit_sort_key(program, unsafe { input.value() }, key),
            Entry::SortHistogram => emit_sort_histogram(program),
            Entry::SortScatter => emit_sort_scatter(program),
            Entry::SortGather => emit_sort_gather(program, unsafe { input.value() }),
        };
        Ok(code)
    }

    fn generate(&mut self, complete: u32) -> Result<RangeProduct, &'static str> {
        let (entry, words) = (self.input.entry, self.input.words_or_keys);
        let sink = RangeSink { context: std::ptr::from_mut(self).cast(), emit: emit_range_entry };
        let product = unsafe { emit_ranges(entry, words, complete, sink) };
        self.error.map_or(Ok(product), Err)
    }

    fn finish(self, product: RangeProduct) -> (ExecutionStage, StageCode) {
        let [main, tail, apply, histogram, scatter, gather] = self.code;
        let stage = &self.input.program.stages()[self.input.index];
        let layout = ExecutionStage::from_stage(stage, crate::ir::dataflow::input_snapshot_words(&self.inputs));
        let continuation = match product.shape {
            RangeShape::Element => None,
            RangeShape::ReduceTail => Some(ContinuationCode::ReductionTail(tail.unwrap())),
            RangeShape::CustomScan => {
                let reduction: &CustomReduction = unsafe { self.input.value() };
                Some(ContinuationCode::Scan {
                    prefix: tail.unwrap(), apply: apply.unwrap(),
                    source: reduction.fields.source, target: reduction.fields.target,
                })
            }
            RangeShape::SegmentedFinish => Some(ContinuationCode::SegmentedFinish(tail.unwrap())),
            RangeShape::ScanAdd => Some(ContinuationCode::ScanAdd(tail.unwrap())),
            RangeShape::CompactScatter => Some(ContinuationCode::CompactScatter(tail.unwrap())),
            RangeShape::Sort => {
                let sort: &Sorting = unsafe { self.input.value() };
                return (layout, StageCode::Sort(SortCode {
                    source: sort.source, target: sort.target, initialize: self.initializers,
                    histogram: histogram.unwrap(), scatter: scatter.unwrap(), gather: gather.unwrap(),
                }));
            }
        };
        let result = if product.has_values == 0 { RangeResult::None } else { RangeResult::Values(product.words) };
        (layout, StageCode::Element { main: main.unwrap(), continuation, result })
    }
}

unsafe extern "C" fn emit_range_entry(
    context: *mut std::ffi::c_void, entry: RangeEntry, destination: RangeDestination, key: usize,
) -> u32 {
    let emission = unsafe { &mut *context.cast::<RangeEmission<'_>>() };
    match emission.emit(entry, key) {
        Ok(code) => {
            if matches!(destination, RangeDestination::Initializers) {
                emission.initializers.push(code);
            } else {
                emission.code[destination as usize] = Some(code);
            }
            1
        }
        Err(error) => {
            emission.error = Some(error);
            0
        }
    }
}

pub fn emit(program: &Program, index: usize) -> Result<NativeCode, &'static str> {
    let mut emission = RangeEmission::new(program, index);
    emission.generate(0)?;
    Ok(emission.code[RangeDestination::Main as usize].take().unwrap())
}

pub(crate) fn compile_stage(program: &Program, index: usize) -> Result<(ExecutionStage, StageCode), &'static str> {
    let mut emission = RangeEmission::new(program, index);
    let product = emission.generate(1)?;
    Ok(emission.finish(product))
}
