//! 普通值入口直接返回 EAX；栈布局与调用活跃性复用原 x64 后端。
use super::operands::Operand;
use super::*;
pub(crate) struct ApplicationCode {
    pub code: NativeCode,
    pub function_end: u32,
    pub unwind: Vec<u8>,
}
pub(crate) fn unwind(prolog: u8, mut operations: Vec<(u8, Vec<u8>)>) -> Vec<u8> {
    operations.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    let slots: usize = operations.iter().map(|(_, bytes)| bytes.len() / 2).sum();
    let mut bytes = vec![1, prolog, slots as u8, 0];
    for (_, operation) in operations {
        bytes.extend(operation);
    }
    bytes.resize(bytes.len().div_ceil(4) * 4, 0);
    bytes
}
fn allocation_unwind(at: u8, frame: u32) -> Vec<u8> {
    if frame <= 128 {
        vec![at, ((frame / 8 - 1) as u8) << 4 | 2]
    } else if frame / 8 <= u16::MAX as u32 {
        let mut v = vec![at, 1];
        v.extend(((frame / 8) as u16).to_le_bytes());
        v
    } else {
        let mut v = vec![at, 0x11];
        v.extend(frame.to_le_bytes());
        v
    }
}
fn saved_unwind(at: u8, reg: u8, offset: u32, xmm: bool) -> Vec<u8> {
    let unit = if xmm { 16 } else { 8 };
    let op = if xmm { 8 } else { 4 };
    if offset / unit <= u16::MAX as u32 {
        let mut v = vec![at, reg << 4 | op];
        v.extend(((offset / unit) as u16).to_le_bytes());
        v
    } else {
        let mut v = vec![at, reg << 4 | (op + 1)];
        v.extend(offset.to_le_bytes());
        v
    }
}
pub(crate) fn emit(body: &ValueCode) -> Result<ApplicationCode, &'static str> {
    let stage = Stage {
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
        cross_access: Vec::new(),
    };
    let program = Program {
        name: String::new(),
        target: crate::ast::Target::Cpu,
        buffers: Vec::new(),
        accelerations: Vec::new(),
        textures: Vec::new(),
        uniforms: Vec::new(),
        uniform_bytes: 0,
        source_stages: 0,
        stages: Vec::new(),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(),
    };
    let graph = allocation::Graph::with_live(
        &stage,
        body.outputs
            .iter()
            .map(|&r| PrivateSlot::Register(r))
            .collect(),
    );
    let allocation = Allocation::new(&stage, &graph);
    let layout = Frame::new(&allocation, None);
    fn scratch(code: &[Inst]) -> bool {
        code.iter().any(|i| match i {
            Inst::LoadLocalIndexed { .. }
            | Inst::StoreLocalIndexed { .. }
            | Inst::ViewLoad { .. }
            | Inst::LoadIndexed { .. }
            | Inst::StoreIndexed { .. } => true,
            Inst::Scope(b) => scratch(b),
            Inst::If { yes, no, .. } => scratch(yes) || scratch(no),
            Inst::Loop {
                condition, body, ..
            } => scratch(condition) || scratch(body),
            Inst::ExternalCall(call) => call.arguments.iter().any(|a| {
                matches!(
                    a,
                    ExternalArgument::Reference {
                        address: ExternalReference {
                            storage: ExternalStorage::Indexed(_),
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
    let bytes = layout.end.div_ceil(16) * 16;
    let frame = i32::try_from(bytes + if !scratch && bytes != 0 { 8 } else { 0 })
        .map_err(|_| "应用栈帧超出x64地址编码")? as u32;
    let mut e = Emitter {
        code: Vec::new(),
        relocations: Vec::new(),
        stage: &stage,
        program: &program,
        breaks: Vec::new(),
        exits: Vec::new(),
        continues: Vec::new(),
        allocation,
        frame: layout,
    };
    let mut operations = Vec::new();
    if scratch {
        e.bytes(&[0x41, 0x57]);
        operations.push((e.code.len() as u8, vec![e.code.len() as u8, 0xf0]));
    }
    let probe = if frame >= 4096 {
        e.bytes(&[0xb8]);
        e.u32(frame);
        e.bytes(&[0xe8]);
        let at = e.code.len();
        e.u32(0);
        Some(at)
    } else {
        None
    };
    if frame != 0 {
        e.bytes(&[0x48, 0x81, 0xec]);
        e.u32(frame);
        let at = e.code.len() as u8;
        operations.push((at, allocation_unwind(at, frame)));
    }
    for (r, offset) in e.frame.saved_gpr.clone() {
        e.instruction(None, &[0x89], true, r, Operand::Memory { base: 4, offset });
        let at = e.code.len() as u8;
        operations.push((at, saved_unwind(at, r, offset, false)));
    }
    for (r, offset) in e.frame.saved_xmm.clone() {
        e.instruction(
            None,
            &[0x0f, 0x29],
            false,
            r,
            Operand::Memory { base: 4, offset },
        );
        let at = e.code.len() as u8;
        operations.push((at, saved_unwind(at, r, offset, true)));
    }
    let metadata = unwind(e.code.len() as u8, operations);
    e.block(&stage.code)?;
    e.load(body.outputs[0]);
    e.preserve_registers(true);
    if frame != 0 {
        e.bytes(&[0x48, 0x81, 0xc4]);
        e.u32(frame);
    }
    if scratch {
        e.bytes(&[0x41, 0x5f]);
    }
    e.bytes(&[0xc3]);
    let end = e.code.len() as u32;
    if let Some(at) = probe {
        let start = e.code.len();
        e.patch(at, start);
        e.bytes(&[0x4c, 0x8b, 0xd4, 0x49, 0x83, 0xc2, 0x08, 0x49, 0x89, 0xc3]);
        let begin = e.code.len();
        e.bytes(&[0x49, 0x81, 0xfb]);
        e.u32(4096);
        let tail = e.jump(Some(0x82));
        e.bytes(&[0x49, 0x81, 0xea]);
        e.u32(4096);
        e.bytes(&[0x41, 0x85, 0x02, 0x49, 0x81, 0xeb]);
        e.u32(4096);
        let back = e.jump(None);
        e.patch(back, begin);
        e.patch(tail, e.code.len());
        e.bytes(&[0x4d, 0x29, 0xda, 0x41, 0x85, 0x02, 0xc3]);
    }
    Ok(ApplicationCode {
        code: NativeCode {
            bytes: e.code,
            relocations: e.relocations,
            stack_bytes: frame,
        },
        function_end: end,
        unwind: metadata,
    })
}
