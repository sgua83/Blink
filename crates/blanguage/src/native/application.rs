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
use super::external::{abi_component, AbiComponent, AbiType, CallValueLoan};
use std::ffi::c_void;

#[repr(C)]
struct FunctionParameter {
    abi: AbiType,
    values: usize,
}

#[repr(u32)]
#[allow(dead_code)]
enum FunctionRangeKind {
    HiddenHome, Home, Input, ResultAddress, ScratchSave, FrameAllocate, NonvolatileSave,
    IntegerResult, FloatResult, IndirectResult, SuccessStatus, NonvolatileRestore,
    FrameRelease, ScratchRestore, FunctionReturn,
}

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
struct FunctionCodeInput {
    data_end: u64,
    result: AbiType,
    flags: u32,
    parameters: usize,
    inputs: usize,
    result_fields: usize,
    outputs: usize,
    saved: usize,
}

#[repr(C)]
struct FunctionExitBuffer {
    code: NativeCodeBuffer,
    saved: allocation::SavedLayout,
}

#[repr(C)]
struct FunctionCodeHost {
    abi: FunctionHost,
    entry: unsafe extern "C" fn(*mut c_void, usize, usize) -> FrameOutput,
    body: unsafe extern "C" fn(*mut c_void, FrameEntry, structure::ControlTargets) -> u32,
    reserve: unsafe extern "C" fn(*mut c_void, usize) -> NativeCodeBuffer,
    result: unsafe extern "C" fn(*const c_void) -> CallValueLoan,
    commit: unsafe extern "C" fn(*mut c_void, usize),
    literal: unsafe extern "C" fn(*mut c_void, usize, Bits, usize),
    exit_buffer: unsafe extern "C" fn(*mut c_void, usize) -> FunctionExitBuffer,
}

#[repr(C)]
struct FunctionCode { stack: u64, oversized: u32 }

unsafe extern "C" {
    #[link_name = "bootstrap.call_layout::emit_function_code"]
    fn emit_function_code(context: *mut c_void, input: FunctionCodeInput, host: FunctionCodeHost) -> FunctionCode;
}

struct FunctionEmission<'a, 'ir> {
    emitter: &'a mut Emitter<'ir>,
    parameters: &'a [CallParameter],
    inputs: &'a [Reg],
    code: &'a [Inst],
    outputs: &'a [Reg],
    fields: &'a [(Scalar, u32)],
    body_result: Result<(), &'static str>,
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
    CallValueLoan::new(fields, &state.inputs[first..first + count], &e.allocation, &e.registers)
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
        FunctionRangeKind::IntegerResult => AbiRangeKind::IntegerResult,
        FunctionRangeKind::FloatResult => AbiRangeKind::FloatResult,
        FunctionRangeKind::IndirectResult => AbiRangeKind::IndirectResult { stack_offset: slot as u32 },
        FunctionRangeKind::SuccessStatus => AbiRangeKind::FallibleSuccessStatus,
        FunctionRangeKind::NonvolatileRestore => AbiRangeKind::NonvolatileRestore,
        FunctionRangeKind::FrameRelease => AbiRangeKind::FrameRelease { bytes: slot as u32 },
        FunctionRangeKind::ScratchRestore => AbiRangeKind::ScratchRestore,
        FunctionRangeKind::FunctionReturn => AbiRangeKind::FunctionReturn,
    };
    e.record_abi_range(range.start..range.end, kind);
}

unsafe extern "C" fn entry_buffer(context: *mut c_void, code_bytes: usize, operations: usize) -> FrameOutput {
    let state = unsafe { &mut *context.cast::<FunctionEmission<'_, '_>>() };
    state.emitter.reserve_frame(code_bytes, operations)
}

unsafe extern "C" fn emit_body(context: *mut c_void, entry: FrameEntry, targets: structure::ControlTargets) -> u32 {
    let state = unsafe { &mut *context.cast::<FunctionEmission<'_, '_>>() };
    let e = &mut *state.emitter;
    unsafe { e.publish_frame(entry) };
    state.body_result = e.block(state.code, targets);
    u32::from(state.body_result.is_ok())
}

unsafe extern "C" fn reserve_code(context: *mut c_void, count: usize) -> NativeCodeBuffer {
    let state = unsafe { &mut *context.cast::<FunctionEmission<'_, '_>>() };
    NativeCodeBuffer::reserve(&mut state.emitter.code, count)
}

unsafe extern "C" fn result_value(context: *const c_void) -> CallValueLoan {
    let state = unsafe { &*context.cast::<FunctionEmission<'_, '_>>() };
    let e = &*state.emitter;
    CallValueLoan::new(state.fields, state.outputs, &e.allocation, &e.registers)
}

unsafe extern "C" fn commit_code(context: *mut c_void, end: usize) {
    let state = unsafe { &mut *context.cast::<FunctionEmission<'_, '_>>() };
    unsafe { state.emitter.code.set_len(end) };
}

unsafe extern "C" fn record_literal(context: *mut c_void, start: usize, bits: Bits, field: usize) {
    let state = unsafe { &mut *context.cast::<FunctionEmission<'_, '_>>() };
    state.emitter.record_literal(start, (bits, false), field);
}

unsafe extern "C" fn exit_buffer(context: *mut c_void, count: usize) -> FunctionExitBuffer {
    let state = unsafe { &mut *context.cast::<FunctionEmission<'_, '_>>() };
    let e = &mut *state.emitter;
    let code = NativeCodeBuffer::reserve(&mut e.code, count);
    FunctionExitBuffer { code, saved: e.frame.saved }
}

const FUNCTION_CODE_HOST: FunctionCodeHost = FunctionCodeHost {
    abi: FUNCTION_HOST,
    entry: entry_buffer,
    body: emit_body,
    reserve: reserve_code,
    result: result_value,
    commit: commit_code,
    literal: record_literal,
    exit_buffer,
};

fn emit_value(
    body: &ValueCode,
    parameters: &[CallParameter],
    result: Option<&Ty>,
    fallible: bool,
    definition: Option<&SourceOrigin>,
    instance: Option<&str>,
) -> Result<NativeCode, &'static str> {
    let (code, registers) = reads::place(&body.code, &body.registers, &[]);
    let mut graph = allocation::Graph::with_live(
        &code,
        &registers,
        &body.locals,
        body.outputs
            .iter()
            .map(|&r| PrivateSlot::Register(r))
            .collect(),
    );
    graph.function_inputs(&body.inputs);
    let allocation = Allocation::new(&body.locals, &mut graph, false);
    let needs_r15 = u32::from(graph.needs_r15);
    drop(graph);
    let layout = Frame::new(&allocation, None);
    let flags = u32::from(fallible) | (needs_r15 << 1);
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
        registers: &registers,
        tiles: &[],
        range_origin: OperationOrigin::default(),
        workgroup: Stage::DEFAULT_WORKGROUP_WIDTH,
        buffer_count: 0,
        textures: &[],
        allocation,
        frame: layout,
    };

    let fields = result.map_or(&[][..], |ty| ty.components.as_slice());
    let input = FunctionCodeInput {
        data_end: e.frame.end,
        result: AbiType::result(result),
        flags,
        parameters: parameters.len(),
        inputs: body.inputs.len(),
        result_fields: fields.len(),
        outputs: body.outputs.len(),
        saved: e.frame.saved_count,
    };
    let mut emission = FunctionEmission {
        emitter: &mut e, parameters, inputs: &body.inputs,
        code: &code, outputs: &body.outputs, fields, body_result: Ok(()),
    };
    let compiled = unsafe {
        emit_function_code(std::ptr::from_mut(&mut emission).cast(), input, FUNCTION_CODE_HOST)
    };
    if compiled.oversized != 0 {
        return Err("应用栈帧超出x64地址编码");
    }
    emission.body_result?;
    Ok(e.finish(compiled.stack as u32, true))
}
