//! 普通函数入口／结果使用 Windows x64 ABI；栈布局与调用活跃性复用原后端。
use super::operands::Operand;
use super::*;
pub(crate) fn emit(body: &ValueCode) -> Result<NativeCode, &'static str> {
    emit_value(body, &[], Some(&Ty::scalar(Scalar::U32)))
}
pub(crate) fn emit_function(function: &FunctionBody) -> Result<NativeCode, &'static str> {
    emit_value(
        &function.body,
        &function.parameters,
        function.result.as_ref(),
    )
}
use super::external::{floating, indirect};
fn emit_value(
    body: &ValueCode,
    parameters: &[CallParameter],
    result: Option<&Ty>,
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
        cross_access: Vec::new(),
    };
    let program = Program {
        name: String::new(),
        #[cfg(feature = "compiler")]
        source_origin: None,
        #[cfg(feature = "compiler")]
        generated_origin: None,
        target: crate::ast::Target::Cpu,
        buffers: Vec::new(),
        accelerations: Vec::new(),
        textures: Vec::new(),
        uniforms: Vec::new(),
        uniform_bytes: 0,
        source_stages: 0,
        stages: Vec::new(),
        functions: std::sync::Arc::from([]),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(), length_requirements: Vec::new(), imports: Default::default(),
    };
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
            InstKind::LoadLocalIndexed { .. }
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
    let scratch = scratch(&stage.code);
    let hidden = result.is_some_and(indirect);
    let result_pointer = hidden.then_some(layout.end.div_ceil(8) as u32 * 8);
    let bytes = (layout.end + if hidden { 8 } else { 0 }).div_ceil(16) * 16;
    let frame = i32::try_from(bytes + if !scratch && bytes != 0 { 8 } else { 0 })
        .map_err(|_| "应用栈帧超出x64地址编码")? as u32;
    let mut e = Emitter {
        unwind: Vec::new(),
        prolog: 0,
        probe: None,
        code: Vec::new(),
        relocations: Vec::new(),
        calls: Vec::new(),
        literals: std::collections::BTreeMap::new(),
        stage: &stage,
        program: &program,
        breaks: Vec::new(),
        exits: Vec::new(),
        continues: Vec::new(),
        allocation,
        frame: layout,
    };
    // Home every register argument before allocation, probing or input moves may overwrite it.
    // These are the Windows x64 caller-provided home slots, not another parameter copy owner.
    for slot in 0..(parameters.len() + usize::from(hidden)).min(4) {
        let value = if hidden && slot == 0 {
            None
        } else {
            parameters.get(slot - usize::from(hidden))
        };
        let memory = Operand::memory(4, 8 + slot as u32 * 8);
        if let Some(CallParameterKind::Value(ty)) = value.map(|parameter| &parameter.kind) && floating(ty) {
            e.instruction(Some(if ty == &Ty::scalar(Scalar::F64) { 0xf2 } else { 0xf3 }), &[0x0f, 0x11], false, slot as u8, memory);
        } else {
            e.instruction(None, &[0x89], true, [1, 2, 8, 9][slot], memory);
        }
    }
    if scratch { e.push_registers(&[15]); }
    e.allocate_frame(frame);
    e.preserve_registers(false);
    let home = frame + if scratch { 8 } else { 0 } + 8;
    if let Some(at) = result_pointer {
        e.instruction(None, &[0x8b], true, 0, Operand::memory(4, home));
        e.instruction(None, &[0x89], true, 0, Operand::memory(4, at));
    }
    let mut inputs = body.inputs.iter();
    for (i, parameter) in parameters.iter().enumerate() {
        let slot = home + (i + usize::from(hidden)) as u32 * 8;
        match &parameter.kind {
            CallParameterKind::Value(ty) => {
                let base = if indirect(ty) {
                    e.instruction(None, &[0x8b], true, 1, Operand::memory(4, slot));
                    1
                } else {
                    4
                };
                for (_, offset) in &ty.components {
                    let dst = *inputs.next().unwrap();
                    e.read_scalar(dst, base, if base == 4 { slot + offset } else { *offset });
                }
            }
            CallParameterKind::Reference { .. } => {
                for offset in [0, 4] {
                    let dst = *inputs.next().unwrap();
                    e.read_scalar(dst, 4, slot + offset);
                }
            }
        }
    }
    e.block(&stage.code)?;
    if let Some(ty) = result {
        if let Some(at) = result_pointer {
            e.instruction(None, &[0x8b], true, 1, Operand::memory(4, at));
            for ((_, offset), reg) in ty.components.iter().zip(&body.outputs) {
                e.write_scalar(*reg, 1, *offset);
            }
            e.bytes(&[0x48, 0x89, 0xc8]);
        } else if floating(ty) {
            e.float(body.outputs[0], 0);
        } else {
            e.bytes(&[0x31, 0xc9]);
            for ((scalar, offset), reg) in ty.components.iter().zip(&body.outputs) {
                e.load(*reg);
                if scalar.is_signed() && scalar.size() < 4 {
                    e.bytes(&[0x25]);
                    e.u32(scalar.bit_mask() as u32);
                }
                if *offset != 0 {
                    e.bytes(&[0x48, 0xc1, 0xe0, (*offset * 8) as u8]);
                }
                e.bytes(&[0x48, 0x09, 0xc1]);
            }
            e.bytes(&[0x48, 0x89, 0xc8]);
        }
    }
    e.preserve_registers(true);
    if frame != 0 {
        e.bytes(&[0x48, 0x81, 0xc4]);
        e.u32(frame);
    }
    if scratch {
        e.bytes(&[0x41, 0x5f]);
    }
    e.bytes(&[0xc3]);
    Ok(e.finish(frame + u32::from(scratch) * 8))
}
