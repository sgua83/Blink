//! 编译完整阶段及实际消费的集合／间接启动入口；GPU 资源装载不再调用 Shader 发射器。
use super::*;
use crate::spirv::{ScanShader, scan_shaders};
use std::{ffi::c_void, ops::Deref};

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
    let buffers = program.execution_buffers();
    let compiled = crate::parallel::try_map(worker, 0..program.stages().len(), |_| (),
        |_, index| compile_stage(program, index, requirements, buffers)).map_err(|error| error.to_string())?;
    let (layout, stages) = compiled.into_iter().unzip();
    Ok((ExecutionPlan::from_program(program, layout, requirements, buffers), stages))
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

#[allow(dead_code)] // Passed by bootstrap.spirv_envelope.
#[repr(u32)]
enum EntryOutput {
    Main, Tail, ScanAdd, Scatter, SegmentedFinish, RootAdd, Dispatch, Selector, Additional,
}

#[allow(dead_code)] // Passed by bootstrap.spirv_envelope.
#[repr(u32)]
enum EntryKernel { None, PrepareMain, PrepareTail, PrepareAdd, Temporary, Main, Tail, Add }

#[repr(C)]
struct StageEntryInput {
    operation: EntryOperation,
    selected: u32,
    prefix: u32,
    root_add: u32,
    keys: usize,
    prefix_binding: usize,
    prefix_word: u32,
    root_scan: ScanShader,
    tail_scan: ScanShader,
    single_group: u32,
}

#[repr(C)]
struct StageEntryRequest {
    pass: ShaderPass,
    output: EntryOutput,
    kernel: EntryKernel,
    flags: u32,
}

#[repr(C)]
struct StageEntrySink {
    context: *mut c_void,
    emit: unsafe extern "C" fn(*mut c_void, StageEntryRequest) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_stage_entries"]
    fn emit_stage_entries(input: StageEntryInput, sink: StageEntrySink);
}

fn compile_entries<F: FnMut(StageEntryRequest) -> bool>(input: StageEntryInput, mut emit: F) {
    unsafe extern "C" fn forward<F: FnMut(StageEntryRequest) -> bool>(
        context: *mut c_void, request: StageEntryRequest,
    ) -> u32 {
        u32::from(unsafe { (&mut *context.cast::<F>())(request) })
    }
    let context = std::ptr::from_mut(&mut emit).cast();
    unsafe { emit_stage_entries(input, StageEntrySink { context, emit: forward::<F> }); }
}

pub(crate) fn compile_stage(
    program: &Program, i: usize, requirements: DeviceRequirements, buffers: ExecutionBuffers,
) -> Result<(ExecutionStage, StageCode), &'static str> {
    let stage = &program.stages()[i];
    let (tile_layout, mut inputs) = shader_stage_inputs(stage);
    // Shader input also includes the physical low half of a wide word.
    let mut layout = ExecutionStage::from_stage(stage, inputs.keys().copied().collect());
    let can_use_single_group = program.buffers[stage.domain].count
        .is_none_or(|count| count <= layout.workgroup_width());
    let features = ProgramShaderFeatures::new(requirements, buffers);
    let operation = EntryOperation::from(&stage.operation);
    let (root_add, keys) = match &stage.operation {
        Operation::ScanExclusive(scan) => (scan.needs_root_add(program), 0),
        Operation::CustomScan(scan) => (scan.needs_root_add(program), 0),
        Operation::StableSort(sort) => (false, sort.keys.len()),
        _ => (false, 0),
    };
    let [root_scan, tail_scan] = scan_shaders(
        &stage.operation, stage.selection.is_some(), program.buffers[stage.domain].count,
    );
    let (prefix_binding, prefix_word) = stage.prefix.map_or((0, 0), |prefix| (prefix.binding, prefix.word));
    let input = StageEntryInput {
        operation, selected: u32::from(stage.selection.is_some()), prefix: u32::from(stage.prefix.is_some()),
        root_add: u32::from(root_add), keys, prefix_binding, prefix_word, root_scan, tail_scan,
        single_group: u32::from(can_use_single_group),
    };
    let mut code = StageCode {
        main: Vec::new(), tail: None, scan_add: None, finish: None, dispatch: None, selector: None, additional: None,
    };
    let mut original_facts = None;
    let mut main_kernel = None;
    let mut tail_kernel = None;
    let mut add_kernel = None;
    let mut error = None;
    compile_entries(input, |request| {
        const SUMMARY: u32 = 1;
        const RANKS: u32 = 2;
        const SHARED_FACTS: u32 = 4;
        let pass = request.pass;
        let temporary;
        let kernel = match request.kernel {
            EntryKernel::None => None,
            EntryKernel::PrepareMain => {
                main_kernel = prepare_reduction(stage, pass);
                main_kernel.as_ref()
            }
            EntryKernel::PrepareTail => {
                tail_kernel = prepare_reduction(stage, pass);
                tail_kernel.as_ref()
            }
            EntryKernel::PrepareAdd => {
                add_kernel = prepare_reduction(stage, pass);
                add_kernel.as_ref()
            }
            EntryKernel::Temporary => {
                temporary = prepare_reduction(stage, pass);
                temporary.as_ref()
            }
            EntryKernel::Main => main_kernel.as_ref(),
            EntryKernel::Tail => tail_kernel.as_ref(),
            EntryKernel::Add => add_kernel.as_ref(),
        };
        let facts = (request.flags & SHARED_FACTS != 0)
            .then(|| &*original_facts.get_or_insert_with(|| ShaderStageFacts::new(stage, true, true)));
        let result = emit_shader(
            program, stage, pass, &mut inputs, &tile_layout, features, kernel, facts,
            request.flags & SUMMARY != 0,
        );
        let (words, bytes) = match result {
            Ok(result) => result,
            Err(value) => { error = Some(value); return false; }
        };
        layout.shared_bytes = layout.shared_bytes.max(bytes);
        match request.output {
            EntryOutput::Main => code.main = words,
            EntryOutput::Tail => code.tail = Some(words),
            EntryOutput::ScanAdd => code.scan_add = Some(words),
            EntryOutput::Scatter => code.finish = Some(("scatter".into(), words)),
            EntryOutput::SegmentedFinish => code.finish = Some(("finish".into(), words)),
            EntryOutput::RootAdd => code.finish = Some(("root-add".into(), words)),
            EntryOutput::Dispatch => code.dispatch = Some(words),
            EntryOutput::Selector => code.selector = Some(words),
            EntryOutput::Additional => code.additional.get_or_insert_with(Vec::new)
                .push(ShaderCode { words, ranks: request.flags & RANKS != 0 }),
        }
        true
    });
    error.map_or(Ok((layout, code)), Err)
}
