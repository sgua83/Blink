//! 普通值 IR 直接成为范围累积与摘要合并机器码，不经宿主解释合并器。
use super::*;
use crate::ast::Access;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pass {
    Reduce,
    ReduceTail,
    Scan,
    ScanPrefix,
    ScanAdd,
    SegmentReduce,
    SegmentPrefix,
    SegmentFinish,
}
pub fn emit_custom_reduce_tail(
    program: &Program,
    index: usize,
) -> Result<NativeCode, &'static str> {
    let Operation::CustomReduce(r) = &program.stages[index].operation else {
        return Err("摘要合并需要自定义归约阶段");
    };
    emit(program, index, r, Pass::ReduceTail, None)
}
pub fn emit_custom_scan_prefix(
    program: &Program,
    index: usize,
) -> Result<NativeCode, &'static str> {
    scan(program, index, Pass::ScanPrefix)
}
pub fn emit_custom_scan_add(program: &Program, index: usize) -> Result<NativeCode, &'static str> {
    scan(program, index, Pass::ScanAdd)
}
fn scan(program: &Program, index: usize, pass: Pass) -> Result<NativeCode, &'static str> {
    let Operation::CustomScan(r) = &program.stages[index].operation else {
        return Err("自定义扫描代码生成需要扫描阶段");
    };
    emit(program, index, &r.values, pass, r.head_word)
}
pub fn emit_custom_segmented_prefix(
    program: &Program,
    index: usize,
) -> Result<NativeCode, &'static str> {
    segmented(program, index, Pass::SegmentPrefix)
}
pub fn emit_custom_segmented_finish(
    program: &Program,
    index: usize,
) -> Result<NativeCode, &'static str> {
    segmented(program, index, Pass::SegmentFinish)
}
fn segmented(program: &Program, index: usize, pass: Pass) -> Result<NativeCode, &'static str> {
    let Operation::CustomSegmentedReduce(r) = &program.stages[index].operation else {
        return Err("分段摘要与段尾代码需要自定义分段归约阶段");
    };
    emit(program, index, &r.values, pass, Some(r.head_word))
}
fn reg(stage: &mut Stage, scalar: Scalar) -> Reg {
    let r = stage.registers.len();
    stage.registers.push(scalar);
    r
}
fn segment_tail(
    stage: &mut Stage,
    r: &CustomReduction,
    head: u32,
    carry: usize,
    index: Reg,
    length_slot: usize,
) {
    let k = r.ty.components.len();
    let one = reg(stage, Scalar::U32);
    let next = reg(stage, Scalar::U32);
    let length = reg(stage, Scalar::U32);
    let last = reg(stage, Scalar::Bool);
    stage.code.push(Inst::Constant(one, Scalar::U32, 1));
    stage.code.push(Inst::Binary {
        dst: next,
        op: BinaryOp::IAdd,
        a: index,
        b: one,
    });
    stage.code.push(Inst::LoadLocal(length, length_slot));
    stage.code.push(Inst::Binary {
        dst: last,
        op: BinaryOp::Compare(Comparison::Equal),
        a: next,
        b: length,
    });
    let tail_slot = stage.locals.len();
    stage.locals.push(Scalar::Bool);
    let is_last = reg(stage, Scalar::Bool);
    let next_head = reg(stage, Scalar::Bool);
    stage.code.push(Inst::If {
        condition: last,
        yes: vec![
            Inst::Constant(is_last, Scalar::Bool, 1),
            Inst::StoreLocal(tail_slot, is_last),
        ],
        no: vec![
            Inst::Load {
                dst: next_head,
                binding: r.fields.source,
                index: next,
                word: head,
            },
            Inst::StoreLocal(tail_slot, next_head),
        ],
    });
    let tail = reg(stage, Scalar::Bool);
    stage.code.push(Inst::LoadLocal(tail, tail_slot));
    let segments = reg(stage, Scalar::U32);
    let target = reg(stage, Scalar::U32);
    let mut write = vec![
        Inst::LoadLocal(segments, carry + k + 1),
        Inst::Binary {
            dst: target,
            op: BinaryOp::ISub,
            a: segments,
            b: one,
        },
    ];
    for (i, (scalar, offset)) in r.ty.components.iter().enumerate() {
        let value = reg(stage, *scalar);
        write.push(Inst::LoadLocal(value, carry + i));
        write.push(Inst::Store {
            binding: r.fields.target,
            index: target,
            word: r.fields.target_word + offset / 4,
            value,
        });
    }
    stage.code.push(Inst::If {
        condition: tail,
        yes: write,
        no: Vec::new(),
    });
}
pub(super) fn emit(
    program: &Program,
    index: usize,
    r: &CustomReduction,
    pass: Pass,
    head_word: Option<u32>,
) -> Result<NativeCode, &'static str> {
    let packed = matches!(
        pass,
        Pass::ReduceTail | Pass::ScanPrefix | Pass::SegmentPrefix
    );
    let apply = pass == Pass::ScanAdd;
    let finish = pass == Pass::SegmentFinish;
    let prefix = matches!(pass, Pass::ScanPrefix | Pass::SegmentPrefix);
    let count = if let Operation::CustomSegmentedReduce(s) = &program.stages[index].operation {
        Some(s.count)
    } else {
        None
    };
    let counted = count.is_some();
    let history = head_word.is_some() && !apply && !finish && pass != Pass::SegmentPrefix;
    let kernel = r.kernel(
        &program.stages[index],
        matches!(
            pass,
            Pass::Reduce | Pass::Scan | Pass::SegmentReduce | Pass::SegmentFinish
        ),
        !apply,
    );
    let k = r.ty.components.len();
    let summary = k + usize::from(head_word.is_some()) + usize::from(counted);
    let mut stage = kernel.stage;
    let mut program = program.clone();
    let private = program.buffers.len();
    if apply || finish {
        program.buffers.push(Buffer {
            name: String::from("<range>"),
            access: Access::Read,
            ty: Ty::vector(Scalar::U32, summary),
            stride: summary as u32 * 4,
            count: Some(1),
        });
    }
    if packed {
        program.buffers[r.fields.source].stride = summary as u32 * 4;
    }
    if prefix {
        program.buffers[r.fields.target].stride = summary as u32 * 4;
    }
    let mut combine = stage.code.split_off(kernel.project_end);
    let project = stage.code.split_off(kernel.initialize_end);
    let carry = stage.locals.len();
    stage.locals.extend(r.ty.components.iter().map(|(s, _)| *s));
    if head_word.is_some() && !apply {
        stage.locals.push(Scalar::Bool);
    }
    if counted {
        stage.locals.push(Scalar::U32);
    }
    if apply {
        stage.code.truncate(kernel.invariant_end);
    }
    let private_index = reg(&mut stage, Scalar::U32);
    if apply || finish {
        stage
            .code
            .push(Inst::Constant(private_index, Scalar::U32, 0));
    }
    let length_slot = stage.locals.len();
    if finish {
        stage.locals.push(Scalar::U32);
        let length = reg(&mut stage, Scalar::U32);
        stage.code.push(Inst::Load {
            dst: length,
            binding: private,
            index: private_index,
            word: k as u32,
        });
        stage.code.push(Inst::StoreLocal(length_slot, length));
    }
    if apply || finish {
        for (i, (scalar, _)) in r.ty.components.iter().enumerate() {
            let reg = stage.registers.len();
            stage.registers.push(*scalar);
            stage.code.push(Inst::Load {
                dst: reg,
                binding: private,
                index: private_index,
                word: i as u32,
            });
            stage.code.push(Inst::StoreLocal(carry + i, reg));
        }
    } else {
        for (i, reg) in kernel.initial.iter().enumerate() {
            stage.code.push(Inst::StoreLocal(carry + i, *reg));
        }
        if history {
            let zero = stage.registers.len();
            stage.registers.push(Scalar::Bool);
            stage.code.push(Inst::Constant(zero, Scalar::Bool, 0));
            stage.code.push(Inst::StoreLocal(carry + k, zero));
        }
    }
    if counted {
        let value = reg(&mut stage, Scalar::U32);
        stage.code.push(if finish {
            Inst::Load {
                dst: value,
                binding: private,
                index: private_index,
                word: k as u32 + 1,
            }
        } else {
            Inst::Constant(value, Scalar::U32, 0)
        });
        stage.code.push(Inst::StoreLocal(carry + k + 1, value));
    }
    let initialize_end = stage.code.len();
    let index = stage.registers.len();
    stage.registers.push(Scalar::U32);
    stage.code.push(Inst::Index(index));
    let head = head_word.map(|word| {
        let raw = reg(&mut stage, Scalar::Bool);
        stage.code.push(Inst::Load {
            dst: raw,
            binding: r.fields.source,
            index,
            word: if packed { k as u32 } else { word },
        });
        let head = if counted && !packed {
            let zero = reg(&mut stage, Scalar::U32);
            let first = reg(&mut stage, Scalar::Bool);
            let head = reg(&mut stage, Scalar::Bool);
            stage.code.push(Inst::Constant(zero, Scalar::U32, 0));
            stage.code.push(Inst::Binary {
                dst: first,
                op: BinaryOp::Compare(Comparison::Equal),
                a: index,
                b: zero,
            });
            stage.code.push(Inst::Binary {
                dst: head,
                op: BinaryOp::Or,
                a: raw,
                b: first,
            });
            head
        } else {
            raw
        };
        if apply {
            stage.code.push(Inst::If {
                condition: head,
                yes: vec![Inst::Break],
                no: Vec::new(),
            });
        } else if matches!(pass, Pass::Scan | Pass::SegmentReduce | Pass::SegmentFinish) {
            let mut reset: Vec<_> = kernel
                .initial
                .iter()
                .enumerate()
                .map(|(i, reg)| Inst::StoreLocal(carry + i, *reg))
                .collect();
            if history {
                let flag = reg(&mut stage, Scalar::Bool);
                reset.push(Inst::Constant(flag, Scalar::Bool, 1));
                reset.push(Inst::StoreLocal(carry + k, flag));
            }
            if counted {
                let before = reg(&mut stage, Scalar::U32);
                let one = reg(&mut stage, Scalar::U32);
                let after = reg(&mut stage, Scalar::U32);
                reset.push(Inst::LoadLocal(before, carry + k + 1));
                reset.push(Inst::Constant(one, Scalar::U32, 1));
                reset.push(Inst::Binary {
                    dst: after,
                    op: BinaryOp::IAdd,
                    a: before,
                    b: one,
                });
                reset.push(Inst::StoreLocal(carry + k + 1, after));
            }
            stage.code.push(Inst::If {
                condition: head,
                yes: reset,
                no: Vec::new(),
            });
        }
        head
    });
    for (i, reg) in kernel.left.iter().enumerate() {
        stage.code.push(Inst::LoadLocal(*reg, carry + i));
    }
    for (i, reg) in kernel.source.iter().enumerate() {
        stage.code.push(Inst::Load {
            dst: *reg,
            binding: if apply {
                r.fields.target
            } else {
                r.fields.source
            },
            index,
            word: if packed {
                i as u32
            } else if apply {
                r.fields.target_word + r.ty.components[i].1 / 4
            } else {
                r.fields.source_word + r.source_ty.components[i].1 / 4
            },
        });
    }
    let right_count = if counted && packed {
        let right = reg(&mut stage, Scalar::U32);
        stage.code.push(Inst::Load {
            dst: right,
            binding: r.fields.source,
            index,
            word: k as u32 + 1,
        });
        Some(right)
    } else {
        None
    };
    stage.code.extend(project);
    if pass == Pass::Scan || prefix {
        for (i, reg) in kernel.left.iter().enumerate() {
            stage.code.push(Inst::Store {
                binding: r.fields.target,
                index,
                word: if packed {
                    i as u32
                } else {
                    r.fields.target_word + r.ty.components[i].1 / 4
                },
                value: *reg,
            });
        }
        if pass == Pass::ScanPrefix && head.is_some() {
            let flag = stage.registers.len();
            stage.registers.push(Scalar::Bool);
            stage.code.push(Inst::LoadLocal(flag, carry + k));
            stage.code.push(Inst::Store {
                binding: r.fields.target,
                index,
                word: k as u32,
                value: flag,
            });
        }
        if pass == Pass::SegmentPrefix {
            let length = reg(&mut stage, Scalar::U32);
            let segments = reg(&mut stage, Scalar::U32);
            stage.code.push(Inst::Uniform(length, 0));
            stage.code.push(Inst::LoadLocal(segments, carry + k + 1));
            stage.code.push(Inst::Store {
                binding: r.fields.target,
                index,
                word: k as u32,
                value: length,
            });
            stage.code.push(Inst::Store {
                binding: r.fields.target,
                index,
                word: k as u32 + 1,
                value: segments,
            });
        }
    }
    let arguments: std::collections::BTreeMap<_, _> = kernel
        .right
        .iter()
        .zip(&kernel.projected)
        .map(|(a, b)| (*a, *b))
        .collect();
    let bind = |r| arguments.get(&r).copied().unwrap_or(r);
    crate::optimize::map(&mut combine, &bind, &|s| s);
    for (i, reg) in kernel.result.iter().enumerate() {
        if apply {
            combine.push(Inst::Store {
                binding: r.fields.target,
                index,
                word: r.fields.target_word + r.ty.components[i].1 / 4,
                value: bind(*reg),
            });
        } else {
            combine.push(Inst::StoreLocal(carry + i, bind(*reg)));
        }
    }
    if let Some(head) = head.filter(|_| prefix) {
        let mut reset: Vec<_> = kernel
            .projected
            .iter()
            .enumerate()
            .map(|(i, reg)| Inst::StoreLocal(carry + i, *reg))
            .collect();
        if history {
            let flag = reg(&mut stage, Scalar::Bool);
            reset.push(Inst::Constant(flag, Scalar::Bool, 1));
            reset.push(Inst::StoreLocal(carry + k, flag));
        }
        stage.code.push(Inst::If {
            condition: head,
            yes: reset,
            no: combine,
        });
    } else {
        stage.code.extend(combine);
    }
    if let Some(right) = right_count {
        let before = reg(&mut stage, Scalar::U32);
        let after = reg(&mut stage, Scalar::U32);
        stage.code.push(Inst::LoadLocal(before, carry + k + 1));
        stage.code.push(Inst::Binary {
            dst: after,
            op: BinaryOp::IAdd,
            a: before,
            b: right,
        });
        stage.code.push(Inst::StoreLocal(carry + k + 1, after));
    }
    if finish {
        segment_tail(&mut stage, r, head_word.unwrap(), carry, index, length_slot);
    }
    let loop_end = stage.code.len();
    if pass == Pass::SegmentPrefix {
        let count = count.unwrap();
        let zero = reg(&mut stage, Scalar::U32);
        let total = reg(&mut stage, Scalar::U32);
        stage.code.push(Inst::Constant(zero, Scalar::U32, 0));
        stage.code.push(Inst::LoadLocal(total, carry + k + 1));
        stage.code.push(Inst::Store {
            binding: count.binding,
            index: zero,
            word: count.word,
            value: total,
        });
    }
    let mut live: std::collections::BTreeSet<_> =
        (0..k).map(|i| PrivateSlot::Local(carry + i)).collect();
    if history {
        live.insert(PrivateSlot::Local(carry + k));
    }
    if counted {
        live.insert(PrivateSlot::Local(carry + k + 1));
    }
    if finish {
        live.insert(PrivateSlot::Local(length_slot));
    }
    if head.is_some() && matches!(pass, Pass::Scan | Pass::SegmentReduce | Pass::SegmentFinish) {
        live.extend(kernel.initial.iter().map(|r| PrivateSlot::Register(*r)));
    }
    live.extend(live_before(
        &stage.code[initialize_end..loop_end],
        live.clone(),
        &live,
        &live,
        &std::collections::BTreeSet::new(),
    ));
    let graph = allocation::Graph::with_live(&stage, live);
    let allocation = Allocation::new(&stage, &graph);
    let layout = Frame::new(&allocation, None);
    let frame = i32::try_from(layout.end.div_ceil(16) * 16)
        .map_err(|_| "原生集合栈帧超过 x64 范围函数 ABI")? as u32;
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
    e.enter(frame);
    e.block(&stage.code[..initialize_end])?;
    let begin = e.code.len();
    e.bytes(&[0x44, 0x39, 0xf3]);
    let end = e.jump(Some(0x83));
    if apply && head.is_some() {
        e.breaks.push(Vec::new());
    }
    e.block(&stage.code[initialize_end..loop_end])?;
    e.bytes(&[0xff, 0xc3]);
    let back = e.jump(None);
    e.patch(back, begin);
    e.patch(end, e.code.len());
    if apply && head.is_some() {
        for jump in e.breaks.pop().unwrap() {
            e.patch(jump, e.code.len());
        }
    }
    e.block(&stage.code[loop_end..])?;
    // 五个固定保存寄存器之后，从原 ABI 的第五参数取得摘要目标。
    if matches!(
        pass,
        Pass::Reduce | Pass::ReduceTail | Pass::Scan | Pass::SegmentReduce
    ) {
        e.bytes(&[0x4c, 0x8b, 0xbc, 0x24]);
        e.u32(frame + 80);
        for i in 0..summary {
            let value = e.allocation.location(PrivateSlot::Local(carry + i));
            e.write_memory(value, 15, i as u32 * 4);
        }
    }
    e.leave(frame);
    Ok(NativeCode {
        bytes: e.code,
        relocations: e.relocations,
        stack_bytes: frame,
    })
}
