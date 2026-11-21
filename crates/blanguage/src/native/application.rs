//! 普通函数入口／结果使用 Windows x64 ABI；栈布局与调用活跃性复用原后端。
use super::allocation::Location;
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
use super::external::{AbiMemoryMode, AbiType, ResultLocation};
use std::ffi::c_void;

#[derive(Clone, Copy)]
#[repr(C)]
struct FunctionFrame {
    bytes: u64,
    home: u64,
    result: u64,
    result_location: ResultLocation,
}

#[repr(C)]
struct FunctionParameter {
    abi: AbiType,
    values: usize,
}

#[allow(dead_code)]
#[repr(u32)]
enum ParameterMoveKind { Home, Direct, Indirect }

#[repr(C)]
struct ParameterMove {
    index: usize,
    slot: usize,
    offset: u64,
    first: usize,
    count: usize,
    float_size: u32,
    kind: ParameterMoveKind,
}

#[allow(dead_code)]
#[repr(u32)]
enum FunctionAction {
    HiddenResultHome, ScratchSave, FrameAllocate, NonvolatileSave, ResultAddressSave,
    IndirectResult, FallibleResult, FloatResult, IntegerResult, FallibleSuccess,
    CloseExits, NonvolatileRestore, FrameRelease, ScratchRestore, Return,
}

type EmitAction = unsafe extern "C" fn(*mut c_void, FunctionAction, u64, u64);

#[repr(C)]
struct FunctionHost {
    parameter: unsafe extern "C" fn(*const c_void, usize) -> FunctionParameter,
    move_parameter: unsafe extern "C" fn(*mut c_void, ParameterMove),
    action: EmitAction,
}

const FUNCTION_HOST: FunctionHost = FunctionHost {
    parameter: function_parameter, move_parameter: emit_parameter, action: emit_action,
};

unsafe extern "C" {
    #[link_name = "bootstrap.call_layout::function_frame"]
    fn function_frame(data_end: u64, result: AbiType, flags: u32) -> FunctionFrame;
    #[link_name = "bootstrap.call_layout::function_prologue"]
    fn function_prologue(context: *mut c_void, count: usize, frame: FunctionFrame, flags: u32, host: FunctionHost);
    #[link_name = "bootstrap.call_layout::function_epilogue"]
    fn function_epilogue(context: *mut c_void, frame: FunctionFrame, flags: u32, action: EmitAction) -> u64;
}

struct FunctionEmission<'a, 'ir> {
    emitter: &'a mut Emitter<'ir>,
    parameters: &'a [CallParameter],
    body: &'a ValueCode,
    result: *const Ty,
}

unsafe extern "C" fn function_parameter(context: *const c_void, index: usize) -> FunctionParameter {
    let state = unsafe { &*context.cast::<FunctionEmission<'_, '_>>() };
    match &state.parameters[index].kind {
        CallParameterKind::Value(ty) => FunctionParameter { abi: AbiType::value(ty), values: ty.components.len() },
        CallParameterKind::Reference { .. } => FunctionParameter { abi: AbiType::reference(), values: 2 },
    }
}

unsafe extern "C" fn emit_parameter(context: *mut c_void, row: ParameterMove) {
    let state = unsafe { &mut *context.cast::<FunctionEmission<'_, '_>>() };
    let parameter = &state.parameters[row.index];
    let e = &mut *state.emitter;
    let start = e.code.len();
    let kind = match row.kind {
        ParameterMoveKind::Home => {
            e.argument_register(row.slot, row.float_size, true);
            AbiRangeKind::ParameterHome { slot: row.slot, parameter_index: row.index, origin: parameter.origin.clone() }
        }
        ParameterMoveKind::Direct | ParameterMoveKind::Indirect => {
            let values = &state.body.inputs[row.first..row.first + row.count];
            let address = row.offset as u32;
            match &parameter.kind {
                CallParameterKind::Value(ty) => {
                    let mode = if matches!(row.kind, ParameterMoveKind::Indirect) {
                        AbiMemoryMode::IndirectParameter
                    } else {
                        AbiMemoryMode::StackLoad
                    };
                    e.abi_value(address, ty, values, mode);
                }
                CallParameterKind::Reference { .. } => {
                    e.read_scalar(values[0], 4, address);
                    e.read_scalar(values[1], 4, address + 4);
                }
            }
            AbiRangeKind::ParameterInput { slot: row.slot, parameter_index: row.index, origin: parameter.origin.clone() }
        }
    };
    e.abi_range(start, kind);
}

unsafe extern "C" fn emit_action(context: *mut c_void, action: FunctionAction, first: u64, second: u64) {
    let state = unsafe { &mut *context.cast::<FunctionEmission<'_, '_>>() };
    let e = &mut *state.emitter;
    let start = e.code.len();
    let at = first as u32;
    let range = match action {
        FunctionAction::HiddenResultHome => {
            e.argument_register(0, 0, true);
            AbiRangeKind::HiddenResultHome { slot: 0 }
        }
        FunctionAction::ScratchSave => {
            e.push_registers(&[first as u8]);
            AbiRangeKind::ScratchSave
        }
        FunctionAction::FrameAllocate => {
            e.allocate_frame(at);
            AbiRangeKind::FrameAllocate { bytes: at }
        }
        FunctionAction::NonvolatileSave => {
            e.preserve_registers(false);
            AbiRangeKind::NonvolatileSave
        }
        FunctionAction::ResultAddressSave => {
            e.copy_scalar(Location::Stack(at), Location::Stack(second as u32), Scalar::U64);
            AbiRangeKind::ResultAddressSave { stack_offset: at }
        }
        FunctionAction::IndirectResult | FunctionAction::FallibleResult => {
            // The same B frame selects a result action only for the original nonempty type.
            let ty = unsafe { &*state.result };
            let mode = if matches!(action, FunctionAction::FallibleResult) {
                AbiMemoryMode::FallibleResult
            } else {
                AbiMemoryMode::IndirectResult
            };
            e.abi_value(at, ty, &state.body.outputs, mode);
            AbiRangeKind::IndirectResult { stack_offset: at }
        }
        FunctionAction::FloatResult => {
            e.float(state.body.outputs[0], 0);
            AbiRangeKind::FloatResult
        }
        FunctionAction::IntegerResult => {
            let ty = unsafe { &*state.result };
            e.abi_value(0, ty, &state.body.outputs, AbiMemoryMode::PackIntegerResult);
            AbiRangeKind::IntegerResult
        }
        FunctionAction::FallibleSuccess => {
            e.bytes(&first.to_le_bytes()[..second as usize]);
            AbiRangeKind::FallibleSuccessStatus
        }
        FunctionAction::CloseExits => {
            e.close_exits();
            return;
        }
        FunctionAction::NonvolatileRestore => {
            e.preserve_registers(true);
            AbiRangeKind::NonvolatileRestore
        }
        FunctionAction::FrameRelease => {
            e.release_frame(at);
            AbiRangeKind::FrameRelease { bytes: at }
        }
        FunctionAction::ScratchRestore => {
            e.pop_register(first as u8);
            AbiRangeKind::ScratchRestore
        }
        FunctionAction::Return => {
            e.bytes(&first.to_le_bytes()[..second as usize]);
            AbiRangeKind::FunctionReturn
        }
    };
    e.abi_range(start, range);
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
        operation_ranges: Some(Vec::new()),
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
    let mut emission = FunctionEmission {
        emitter: &mut e, parameters, body, result: result.map_or(std::ptr::null(), std::ptr::from_ref),
    };
    unsafe {
        function_prologue(std::ptr::from_mut(&mut emission).cast(), parameters.len(), plan, flags, FUNCTION_HOST);
    }
    // The function owns this exit scope. Nested scopes still own their own exits.
    emission.emitter.exits.push(Vec::new());
    emission.emitter.block(&stage.code)?;
    let stack = unsafe {
        function_epilogue(std::ptr::from_mut(&mut emission).cast(), plan, flags, emit_action)
    };
    Ok(e.finish(stack as u32))
}
