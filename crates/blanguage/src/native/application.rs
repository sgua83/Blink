//! 普通函数入口／结果使用 Windows x64 ABI；栈布局与调用活跃性复用原后端。
use super::*;
use crate::native::{AbiRangeKind, FunctionAbi};
pub(crate) fn emit(body: &ValueCode) -> Result<NativeCode, &'static str> {
    emit_value(body, &[], Some(&Ty::scalar(Scalar::U32)), false, None, None)
}
pub(crate) fn emit_function(function: &FunctionBody) -> Result<NativeCode, &'static str> {
    emit_value(
        &function.body,
        &function.parameters,
        function.result.as_ref(),
        function.fallible,
        function.source_origin.as_ref(),
        Some(&function.name),
    )
}
use super::external::{abi_component, AbiComponent, AbiType, CallValueLoan, ResultLocation};
use std::ffi::c_void;

#[derive(Clone, Copy)]
#[repr(C)]
struct FunctionFrame {
    bytes: u64,
    home: u64,
    result: u64,
    stack: u64,
    result_location: ResultLocation,
}

#[repr(C)]
struct FunctionParameter {
    abi: AbiType,
    values: usize,
}

#[repr(u32)]
#[allow(dead_code)]
enum FunctionRangeKind { HiddenHome, Home, Input, ResultAddress, ScratchSave, FrameAllocate, NonvolatileSave }

#[repr(C)]
struct FunctionRange {
    start: usize,
    end: usize,
    index: usize,
    position: usize,
    kind: FunctionRangeKind,
}

#[repr(C)]
struct FunctionHost {
    parameter: unsafe extern "C" fn(*const c_void, usize) -> FunctionParameter,
    loan: unsafe extern "C" fn(*const c_void, usize, usize, usize) -> CallValueLoan,
    component: AbiComponent,
    publish: unsafe extern "C" fn(*mut c_void, FunctionRange),
}

const FUNCTION_HOST: FunctionHost = FunctionHost {
    parameter: function_parameter,
    loan: function_loan,
    component: abi_component,
    publish: publish_function,
};

#[repr(C)]
struct FrameOutput {
    code: *mut u8,
    operations: *mut UnwindOperation,
    saved: *const allocation::SavedRegister,
    count: usize,
}

#[repr(C)]
struct FunctionPrologue {
    length: usize,
    probe_field: usize,
    operations: usize,
    prolog: u8,
}

#[repr(C)]
struct FunctionReturn {
    length: usize,
    value_length: usize,
    literal: u64,
    literal_field: usize,
}

#[repr(C)]
struct FunctionEpilogue {
    saved_end: usize,
    frame_end: usize,
    scratch_end: usize,
    end: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_layout::function_frame"]
    fn function_frame(data_end: u64, result: AbiType, flags: u32) -> FunctionFrame;
    #[link_name = "bootstrap.call_layout::function_prologue"]
    fn function_prologue(context: *mut c_void, count: usize, frame: FunctionFrame, flags: u32,
        output: FrameOutput, host: FunctionHost) -> FunctionPrologue;
    #[link_name = "bootstrap.call_layout::function_result"]
    fn function_result(frame: FunctionFrame, flags: u32, value: CallValueLoan,
        component: AbiComponent, output: *mut u8) -> FunctionReturn;
    #[link_name = "bootstrap.frame_layout::function_epilogue"]
    fn function_epilogue(frame: u32, scratch: u32, saved: *const allocation::SavedRegister, count: usize,
        image: *mut u8, end: usize, exits: *const usize, exit_count: usize) -> FunctionEpilogue;
}

struct FunctionEmission<'a, 'ir> {
    emitter: &'a mut Emitter<'ir>,
    parameters: &'a [CallParameter],
    inputs: &'a [Reg],
}

// A native reference parameter carries the two original pointer words.
const REFERENCE_FIELDS: [(Scalar, u32); 2] = [(Scalar::U32, 0), (Scalar::U32, 4)];

unsafe extern "C" fn function_parameter(context: *const c_void, index: usize) -> FunctionParameter {
    let state = unsafe { &*context.cast::<FunctionEmission<'_, '_>>() };
    match &state.parameters[index].kind {
        CallParameterKind::Value(ty) => FunctionParameter { abi: AbiType::value(ty), values: ty.components.len() },
        CallParameterKind::Reference { .. } => FunctionParameter { abi: AbiType::reference(), values: REFERENCE_FIELDS.len() },
    }
}

unsafe extern "C" fn function_loan(context: *const c_void, index: usize, first: usize, count: usize) -> CallValueLoan {
    let state = unsafe { &*context.cast::<FunctionEmission<'_, '_>>() };
    let fields = match &state.parameters[index].kind {
        CallParameterKind::Value(ty) => ty.components.as_slice(),
        CallParameterKind::Reference { .. } => &REFERENCE_FIELDS,
    };
    let e = &*state.emitter;
    CallValueLoan::new(fields, &state.inputs[first..first + count], &e.allocation, e.stage)
}

unsafe extern "C" fn publish_function(context: *mut c_void, range: FunctionRange) {
    let state = unsafe { &mut *context.cast::<FunctionEmission<'_, '_>>() };
    let e = &mut *state.emitter;
    let slot = range.position;
    let parameter_index = range.index;
    let kind = match range.kind {
        FunctionRangeKind::HiddenHome => AbiRangeKind::HiddenResultHome { slot },
        FunctionRangeKind::Home => AbiRangeKind::ParameterHome {
            slot, parameter_index, origin: state.parameters[parameter_index].origin.clone(),
        },
        FunctionRangeKind::Input => AbiRangeKind::ParameterInput {
            slot, parameter_index, origin: state.parameters[parameter_index].origin.clone(),
        },
        FunctionRangeKind::ResultAddress => AbiRangeKind::ResultAddressSave { stack_offset: slot as u32 },
        FunctionRangeKind::ScratchSave => AbiRangeKind::ScratchSave,
        FunctionRangeKind::FrameAllocate => AbiRangeKind::FrameAllocate { bytes: slot as u32 },
        FunctionRangeKind::NonvolatileSave => AbiRangeKind::NonvolatileSave,
    };
    e.record_abi_range(range.start..range.end, kind);
}

fn emit_value(
    body: &ValueCode,
    parameters: &[CallParameter],
    result: Option<&Ty>,
    fallible: bool,
    definition: Option<&SourceOrigin>,
    instance: Option<&str>,
) -> Result<NativeCode, &'static str> {
    let mut stage = Stage {
        operation_source: None,
        generated_origin: None,
        tiles: Vec::new(),
        domain: 0,
        prefix: None,
        selection: None,
        operation: Operation::Map,
        boundary_before: false,
        registers: body.registers.clone(),
        locals: body.locals.clone(),
        code: body.code.clone(),
        reads: Vec::new(),
        writes: Vec::new(),
        write_regions: Default::default(),
        prefix_guarantees: Vec::new(),
        element_guarantees: Vec::new(),
        cross_access: Vec::new(),
    };
    reads::place(&mut stage.code, &mut stage.registers, &stage.writes);
    let (mut graph, _) = allocation::Graph::with_live(
        &stage,
        body.outputs
            .iter()
            .map(|&r| PrivateSlot::Register(r))
            .collect(),
    );
    graph.function_inputs(&body.inputs);
    let allocation = Allocation::new(&stage, &mut graph);
    let layout = Frame::new(&allocation, None);
    let flags = u32::from(fallible) | (u32::from(graph.needs_r15) << 1);
    let plan = unsafe { function_frame(layout.end, AbiType::result(result), flags) };
    i32::try_from(plan.bytes).map_err(|_| "应用栈帧超出x64地址编码")?;
    let mut e = Emitter {
        used_tile_words: &super::EMPTY_TILE_WORDS,
        unwind: Vec::new(),
        prolog: 0,
        probe: None,
        code: Vec::new(),
        relocations: Vec::new(),
        calls: Vec::new(),
        operation_ranges: Vec::new(),
        function_abi: Some(FunctionAbi {
            definition: definition.cloned(), instance: instance.map(str::to_owned), ranges: Vec::new(),
        }),
        literals: std::collections::BTreeMap::new(),
        stage: &stage,
        buffers: &[],
        textures: &[],
        breaks: Vec::new(),
        exits: Vec::new(),
        continues: Vec::new(),
        allocation,
        frame: layout,
    };
    let saved = &e.frame.saved;
    // Fixed saves/allocation, four register homes, a hidden result address,
    // and the existing per-parameter and per-component encoding bounds.
    e.code.reserve(2 + 17 + 9 * saved.len() + 4 * 9 + 24
        + 16 * parameters.len() + 24 * body.inputs.len());
    e.unwind.reserve(2 + saved.len());
    let output = FrameOutput { code: e.code.as_mut_ptr(), operations: e.unwind.as_mut_ptr(),
        saved: saved.as_ptr(), count: saved.len() };
    let mut emission = FunctionEmission { emitter: &mut e, parameters, inputs: &body.inputs };
    let entry = unsafe {
        function_prologue(std::ptr::from_mut(&mut emission).cast(),
            parameters.len(), plan, flags, output, FUNCTION_HOST)
    };
    unsafe {
        e.code.set_len(entry.length);
        e.unwind.set_len(entry.operations);
    }
    e.probe = (entry.probe_field != 0).then_some(entry.probe_field);
    e.prolog = entry.prolog;
    // The function owns this exit scope. Nested scopes still own their own exits.
    e.exits.push(Vec::new());
    e.block(&stage.code)?;

    let fields = result.map_or(&[][..], |ty| ty.components.as_slice());
    let value = CallValueLoan::new(fields, &body.outputs, &e.allocation, e.stage);
    let start = e.code.len();
    e.code.reserve(26 + 48 * fields.len().min(body.outputs.len()));
    let output = unsafe {
        let output = function_result(plan, flags, value, abi_component, e.code.as_mut_ptr().add(start));
        e.code.set_len(start + output.length);
        output
    };
    let result_end = start + output.value_length;
    let result_kind = match plan.result_location {
        ResultLocation::None => None,
        ResultLocation::Integer => Some(AbiRangeKind::IntegerResult),
        ResultLocation::Float => Some(AbiRangeKind::FloatResult),
        ResultLocation::Indirect => Some(AbiRangeKind::IndirectResult { stack_offset: plan.result as u32 }),
    };
    if let Some(kind) = result_kind {
        e.record_abi_range(start..result_end, kind);
    }
    e.record_literal(start, (output.literal, false), output.literal_field);
    if fallible {
        e.abi_range(result_end, AbiRangeKind::FallibleSuccessStatus);
    }

    let tail_start = e.code.len();
    let saved = &e.frame.saved;
    let exits = e.exits.pop().unwrap();
    e.code.reserve(10 + 9 * saved.len());
    let tail = unsafe {
        let tail = function_epilogue(plan.bytes as u32, u32::from(graph.needs_r15), saved.as_ptr(),
            saved.len(), e.code.as_mut_ptr(), tail_start, exits.as_ptr(), exits.len());
        e.code.set_len(tail.end);
        tail
    };
    e.record_abi_range(tail_start..tail.saved_end, AbiRangeKind::NonvolatileRestore);
    if tail.frame_end != tail.saved_end {
        e.record_abi_range(tail.saved_end..tail.frame_end, AbiRangeKind::FrameRelease { bytes: plan.bytes as u32 });
    }
    if tail.scratch_end != tail.frame_end {
        e.record_abi_range(tail.frame_end..tail.scratch_end, AbiRangeKind::ScratchRestore);
    }
    e.record_abi_range(tail.scratch_end..tail.end, AbiRangeKind::FunctionReturn);
    Ok(e.finish(plan.stack as u32, true))
}
