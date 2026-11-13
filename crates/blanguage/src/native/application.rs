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
use super::external::{AbiMemoryMode, AbiType, float_size};
use std::ffi::c_void;

#[repr(C)]
struct FunctionFrame {
    bytes: u64,
    home: u64,
    result: u64,
    hidden: u64,
}

#[repr(C)]
struct ParameterSlot {
    index: usize,
    home: u64,
    indirect: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.call_layout::parameter_slots"]
    fn parameter_slots(context: *const c_void, count: usize, first_slot: u64,
        parameter: unsafe extern "C" fn(*const c_void, usize) -> AbiType,
        positions: *mut ParameterSlot) -> usize;
    #[link_name = "bootstrap.call_layout::function_frame"]
    fn function_frame(data_end: u64, result: AbiType, flags: u32) -> FunctionFrame;
}
unsafe extern "C" fn parameter_type(context: *const c_void, index: usize) -> AbiType {
    let parameters = unsafe { &*context.cast::<&[CallParameter]>() };
    match &parameters[index].kind {
        CallParameterKind::Value(ty) => AbiType::value(ty),
        CallParameterKind::Reference { .. } => AbiType::reference(),
    }
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
    texture::lower(&mut stage);
    reads::place(&mut stage.code, &stage.writes);
    let mut graph = allocation::Graph::with_live(
        &stage,
        body.outputs
            .iter()
            .map(|&r| PrivateSlot::Register(r))
            .collect(),
    );
    graph.function_inputs(&body.inputs);
    let allocation = Allocation::new(&stage, &graph);
    let layout = Frame::new(&allocation, None);
    fn scratch(code: &[Inst]) -> bool {
        code.iter().any(|i| match &(i).kind {
            InstKind::TextureBindingAddress { .. }
            | InstKind::TextureSize { .. } | InstKind::TextureLevels { .. }
            | InstKind::TextureAddress { .. } | InstKind::TextureFormat { .. }
            | InstKind::TextureRead { .. } | InstKind::TextureFetch { .. }
            | InstKind::LoadLocalIndexed { .. }
            | InstKind::StoreLocalIndexed { .. }
            | InstKind::ViewLoad { .. }
            | InstKind::LoadIndexed { .. }
            | InstKind::StoreIndexed { .. }
            | InstKind::Atomic(_) => true,
            InstKind::Scope(b) => scratch(b),
            InstKind::If { yes, no, .. } => scratch(yes) || scratch(no),
            InstKind::Loop {
                condition, body, ..
            } => scratch(condition) || scratch(body),
            InstKind::PositionAddress { address, .. } => matches!(address.storage, CallStorage::Indexed(_)),
            InstKind::Call(call) => call.arguments.iter().any(|a| {
                matches!(
                    a,
                    CallArgument::Reference {
                        address: CallReference {
                            storage: CallStorage::Indexed(_),
                            ..
                        },
                        ..
                    }
                )
            }),
            _ => false,
        })
    }
    const SCRATCH_REGISTER: u8 = 15;
    let scratch = scratch(&stage.code);
    let result = result.filter(|ty| ty.size != 0);
    let flags = u32::from(fallible) | (u32::from(scratch) << 1);
    let plan = unsafe { function_frame(layout.end, AbiType::result(result), flags) };
    let frame = i32::try_from(plan.bytes).map_err(|_| "应用栈帧超出x64地址编码")? as u32;
    let hidden = plan.hidden != 0;
    let result_pointer = hidden.then_some(plan.result as u32);
    let mut e = Emitter {
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
    // Home every register argument before allocation, probing or input moves may overwrite it.
    // These are the Windows x64 caller-provided home slots, not another parameter copy owner.
    let mut slots = Vec::with_capacity(parameters.len());
    let context = (&parameters as *const &[CallParameter]).cast();
    let count = unsafe {
        parameter_slots(context, parameters.len(), plan.hidden, parameter_type, slots.as_mut_ptr())
    };
    // The B planner initialized one row per parameter with a physical slot.
    unsafe { slots.set_len(count); }
    if hidden {
        let start = e.code.len();
        e.argument_register(0, 0, true);
        e.abi_range(start, AbiRangeKind::HiddenResultHome { slot: 0 });
    }
    for row in slots.iter().take_while(|row| row.home < 32) {
        let start = e.code.len();
        let parameter = &parameters[row.index];
        let size = match &parameter.kind {
            CallParameterKind::Value(ty) => float_size(ty),
            CallParameterKind::Reference { .. } => 0,
        };
        let slot = (row.home / 8) as usize;
        e.argument_register(slot, size, true);
        e.abi_range(start, AbiRangeKind::ParameterHome {
            slot, parameter_index: row.index, origin: parameter.origin.clone(),
        });
    }
    if scratch {
        let start = e.code.len();
        e.push_registers(&[SCRATCH_REGISTER]);
        e.abi_range(start, AbiRangeKind::ScratchSave);
    }
    let start = e.code.len();
    e.allocate_frame(frame);
    e.abi_range(start, AbiRangeKind::FrameAllocate { bytes: frame });
    let start = e.code.len();
    e.preserve_registers(false);
    e.abi_range(start, AbiRangeKind::NonvolatileSave);
    let home = plan.home as u32;
    if let Some(at) = result_pointer {
        let start = e.code.len();
        e.copy_scalar(Location::Stack(at), Location::Stack(home), Scalar::U64);
        e.abi_range(start, AbiRangeKind::ResultAddressSave { stack_offset: at });
    }
    let mut inputs = body.inputs.as_slice();
    for row in slots {
        let start = e.code.len();
        let parameter = &parameters[row.index];
        let slot = (row.home / 8) as usize;
        let address = home + row.home as u32;
        match &parameter.kind {
            CallParameterKind::Value(ty) => {
                let (values, rest) = inputs.split_at(ty.components.len());
                inputs = rest;
                let mode = if row.indirect != 0 { AbiMemoryMode::IndirectParameter } else { AbiMemoryMode::StackLoad };
                e.abi_memory_value(address, ty, values, mode);
            }
            CallParameterKind::Reference { .. } => {
                let (values, rest) = inputs.split_at(2);
                inputs = rest;
                e.read_scalar(values[0], 4, address);
                e.read_scalar(values[1], 4, address + 4);
            }
        }
        e.abi_range(start, AbiRangeKind::ParameterInput { slot,
                parameter_index: row.index, origin: parameter.origin.clone() });
    }
    // 最外退出属于本函数，嵌套Scope继续拥有各自的Exit。
    e.exits.push(Vec::new());
    e.block(&stage.code)?;
    if let Some(ty) = result {
        let start = e.code.len();
        let kind = if let Some(at) = result_pointer {
            let mode = if fallible { AbiMemoryMode::FallibleResult } else { AbiMemoryMode::IndirectResult };
            e.abi_memory_value(at, ty, &body.outputs, mode);
            AbiRangeKind::IndirectResult { stack_offset: at }
        } else if float_size(ty) != 0 {
            e.float(body.outputs[0], 0);
            AbiRangeKind::FloatResult
        } else {
            e.pack_integer_result(ty, &body.outputs);
            AbiRangeKind::IntegerResult
        };
        e.abi_range(start, kind);
    }
    if fallible {
        let start = e.code.len();
        e.bytes(&[0x31, 0xc0]);
        e.abi_range(start, AbiRangeKind::FallibleSuccessStatus);
    }
    e.close_exits();
    let start = e.code.len();
    e.preserve_registers(true);
    e.abi_range(start, AbiRangeKind::NonvolatileRestore);
    if frame != 0 {
        let start = e.code.len();
        e.release_frame(frame);
        e.abi_range(start, AbiRangeKind::FrameRelease { bytes: frame });
    }
    if scratch {
        let start = e.code.len();
        e.pop_register(SCRATCH_REGISTER);
        e.abi_range(start, AbiRangeKind::ScratchRestore);
    }
    let start = e.code.len();
    e.bytes(&[0xc3]);
    e.abi_range(start, AbiRangeKind::FunctionReturn);
    Ok(e.finish(frame + u32::from(scratch) * 8))
}
