//! 仅在读写与跨元素依赖允许时融合同域阶段；phase 保留全域阶段边界。
use crate::ir::*;
use std::collections::{BTreeMap, BTreeSet};
mod invariant;
mod selection;
mod value_flow;
pub(crate) use invariant::prepare_reduction;

pub fn optimize(program: &mut Program) {
    let mut stages: Vec<Stage> = Vec::new();
    for mut stage in std::mem::take(&mut program.stages) {
        if let Some(previous) = stages.last_mut() {
            if matches!(previous.operation, Operation::Map)
                && matches!(stage.operation, Operation::Map)
                && previous.subgroup_operations().is_empty()
                && stage.subgroup_operations().is_empty()
                && !previous.has_external_effects()
                && !stage.has_external_effects()
                && !previous.has_tile_scope()
                && !stage.has_tile_scope()
                && previous.domain == stage.domain
                && previous.prefix == stage.prefix
                && previous.selection == stage.selection
                && !stage.boundary_before
                && !cross_dependency(previous, &stage)
                && !cross_dependency(&stage, previous)
            {
                let base = previous.registers.len();
                let local = previous.locals.len();
                map(&mut stage.code, &|r| r + base, &|s| s + local);
                previous.registers.extend(stage.registers);
                previous.locals.extend(stage.locals);
                previous.code.extend(stage.code);
                previous.reads.extend(stage.reads);
                previous.writes.extend(stage.writes);
                previous.cross_access.extend(stage.cross_access);
                previous.reads.sort_unstable();
                previous.reads.dedup();
                previous.writes.sort_unstable();
                previous.writes.dedup();
                previous.cross_access.sort_unstable();
                previous.cross_access.dedup();
                continue;
            }
        }
        stages.push(stage);
    }
    for stage in &mut stages {
        if let Operation::CustomReduce(r)
        | Operation::CustomScan(CustomScan { values: r, .. })
        | Operation::CustomSegmentedReduce(CustomSegmentedReduction { values: r, .. }) =
            &mut stage.operation
        {
            if let Some(project) = &mut r.project {
                value_code(project);
            }
            value_code(&mut r.identity);
            value_code(&mut r.combine);
            continue;
        }
        value_flow::run(&mut stage.code, &stage.registers);
        loop {
            dce(
                &mut stage.code,
                &mut BTreeSet::new(),
                &BTreeSet::new(),
                &BTreeSet::new(),
                &BTreeSet::new(),
            );
            let preserve_scope = !stage.subgroup_operations().is_empty();
            let words = stage.storage_words();
            let mut changed = false;
            stage.code.retain_mut(|inst| {
                if let Inst::TilePublish(publication) = inst {
                    let before = publication.values.len();
                    publication.values.retain(|(word, _)| {
                        words.contains(&SharedWord::Published {
                            tile: publication.tile,
                            word: *word,
                        })
                    });
                    changed |= before != publication.values.len();
                    if publication.values.is_empty() && !preserve_scope {
                        changed = true;
                        return false;
                    }
                }
                if matches!(inst, Inst::TileCollective(c) if c.results.is_empty())
                    && !preserve_scope
                {
                    changed = true;
                    return false;
                }
                true
            });
            if selection::run(&mut stage.code, &stage.registers, stage.domain) {
                changed = true;
                value_flow::run(&mut stage.code, &stage.registers);
            }
            if !changed {
                break;
            }
        }
        if matches!(stage.operation, Operation::Map) {
            let mut reads = BTreeSet::new();
            let mut writes = BTreeSet::new();
            visit(&stage.code, &mut |i| match i {
                Inst::ExternalCall(call) => {
                    for arg in &call.arguments {
                        if let ExternalArgument::Reference {
                            access,
                            address:
                                ExternalReference {
                                    storage:
                                        ExternalStorage::Indexed(IndexedSource::Data {
                                            binding, ..
                                        }),
                                    ..
                                },
                            ..
                        } = arg
                        {
                            if access.reads() {
                                reads.insert(*binding);
                            }
                            if access.writes() {
                                writes.insert(*binding);
                            }
                        }
                    }
                }
                Inst::Append(a) => {
                    reads.insert(a.count.binding);
                    writes.extend([a.target, a.count.binding]);
                }
                Inst::Atomic(a) => {
                    if let AtomicTarget::Dataset(binding) = a.target {
                        if a.op.reads() {
                            reads.insert(binding);
                        }
                        if a.op.writes() {
                            writes.insert(binding);
                        }
                    }
                }
                Inst::LoadIndexed {
                    source: IndexedSource::Data { binding, .. },
                    ..
                }
                | Inst::Load { binding, .. } => {
                    reads.insert(*binding);
                }
                Inst::StoreIndexed {
                    source: IndexedSource::Data { binding, .. },
                    ..
                }
                | Inst::Store { binding, .. } => {
                    writes.insert(*binding);
                }
                _ => {}
            });
            reads.extend(stage.input_words().iter().map(|(source, _)| *source));
            reads.extend(stage.prefix.map(|p| p.binding));
            reads.extend(stage.selection.map(|s| s.binding));
            stage.reads = reads.into_iter().collect();
            stage.writes = writes.into_iter().collect();
            stage
                .cross_access
                .retain(|r| stage.reads.contains(r) || stage.writes.contains(r));
        }
        let mut used = BTreeSet::new();
        let mut slots = BTreeSet::new();
        visit(&stage.code, &mut |i| {
            used.extend(i.results());
            used.extend(i.operands());
            slots.extend(i.local_reads());
            slots.extend(i.local_writes());
        });
        let registers: Vec<_> = used.iter().map(|r| stage.registers[*r]).collect();
        let locals: Vec<_> = slots.iter().map(|s| stage.locals[*s]).collect();
        let regs: BTreeMap<_, _> = used.into_iter().enumerate().map(|(i, r)| (r, i)).collect();
        let local: BTreeMap<_, _> = slots.into_iter().enumerate().map(|(i, s)| (s, i)).collect();
        map(&mut stage.code, &|r| regs[&r], &|s| local[&s]);
        stage.registers = registers;
        stage.locals = locals;
    }
    stages.retain(|stage| !matches!(stage.operation, Operation::Map) || !stage.code.is_empty());
    program.stages = stages;
}
pub(crate) fn value_code(value: &mut ValueCode) {
    value_flow::run_value(&mut value.code, &value.registers, &mut value.outputs);
    let mut needed = value
        .outputs
        .iter()
        .copied()
        .map(PrivateSlot::Register)
        .collect();
    dce(
        &mut value.code,
        &mut needed,
        &BTreeSet::new(),
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    let mut used: BTreeSet<_> = value.inputs.iter().chain(&value.outputs).copied().collect();
    let mut slots = BTreeSet::new();
    visit(&value.code, &mut |i| {
        used.extend(i.results());
        used.extend(i.operands());
        slots.extend(i.local_reads());
        slots.extend(i.local_writes());
    });
    let registers = used.iter().map(|r| value.registers[*r]).collect();
    let locals = slots.iter().map(|s| value.locals[*s]).collect();
    let regs: BTreeMap<_, _> = used.into_iter().enumerate().map(|(i, r)| (r, i)).collect();
    let local: BTreeMap<_, _> = slots.into_iter().enumerate().map(|(i, s)| (s, i)).collect();
    map(&mut value.code, &|r| regs[&r], &|s| local[&s]);
    for r in value.inputs.iter_mut().chain(&mut value.outputs) {
        *r = regs[r];
    }
    value.registers = registers;
    value.locals = locals;
}
fn cross_dependency(a: &Stage, b: &Stage) -> bool {
    a.cross_access.iter().any(|r| {
        (a.writes.contains(r) && (b.reads.contains(r) || b.writes.contains(r)))
            || (a.reads.contains(r) && b.writes.contains(r))
    })
}
fn destination(i: &Inst) -> Option<Reg> {
    if matches!(
        i,
        Inst::RayQuery { .. }
            | Inst::TextureSample { .. }
            | Inst::TextureSize { .. }
            | Inst::TextureFetch { .. }
            | Inst::TileCollective(_)
            | Inst::ExternalCall(_)
            | Inst::IntegerDivide {
                result: IntegerDivisionResult::Both { .. },
                ..
            }
    ) {
        None
    } else {
        i.results().first().copied()
    }
}
fn visit(code: &[Inst], f: &mut impl FnMut(&Inst)) {
    for i in code {
        f(i);
        match i {
            Inst::Scope(body) => visit(body, f),
            Inst::If { yes, no, .. } => {
                visit(yes, f);
                visit(no, f);
            }
            Inst::Loop {
                condition, body, ..
            } => {
                visit(condition, f);
                visit(body, f);
            }
            _ => {}
        }
    }
}
pub(crate) fn map(code: &mut [Inst], r: &impl Fn(Reg) -> Reg, s: &impl Fn(usize) -> usize) {
    for i in code {
        map_operands(i, r);
        match i {
            Inst::TextureSample { result, .. } | Inst::TextureFetch { result, .. } => {
                result.iter_mut().for_each(|v| *v = r(*v))
            }
            Inst::TextureSize { result, .. } => result.iter_mut().for_each(|v| *v = r(*v)),
            Inst::TextureLevels { dst, .. } => *dst = r(*dst),
            Inst::Append(a) => a.accepted = r(a.accepted),
            Inst::Atomic(a) => a.result = a.result.map(r),
            Inst::RayQuery { result, .. } => result.map(r),
            Inst::IntegerDivide { result, .. } => result.map(r),
            Inst::TileCollective(c) => c.results.iter_mut().for_each(|(_, dst)| *dst = r(*dst)),
            Inst::Constant(dst, ..)
            | Inst::Index(dst)
            | Inst::Uniform(dst, _)
            | Inst::TileLength(dst)
            | Inst::TileLane(dst) => *dst = r(*dst),
            Inst::LoadLocal(dst, slot) => {
                *dst = r(*dst);
                *slot = s(*slot);
            }
            Inst::StoreLocal(slot, _) => *slot = s(*slot),
            Inst::LoadLocalIndexed { dst, base, .. } => {
                *dst = r(*dst);
                *base = s(*base);
            }
            Inst::StoreLocalIndexed { base, .. } => *base = s(*base),
            Inst::ExternalCall(call) => {
                for value in &mut call.values {
                    *value = r(*value);
                }
                for arg in &mut call.arguments {
                    if let ExternalArgument::Reference { address, .. } = arg
                        && let ExternalStorage::Local { base, .. } = &mut address.storage
                    {
                        *base = s(*base);
                    }
                }
            }
            Inst::LoadIndexed { dst, .. }
            | Inst::Load { dst, .. }
            | Inst::Subgroup { dst, .. }
            | Inst::Print { dst, .. }
            | Inst::TileLoad { dst, .. }
            | Inst::ViewLoad { dst, .. }
            | Inst::Unary { dst, .. }
            | Inst::Binary { dst, .. }
            | Inst::Cast { dst, .. }
            | Inst::Math { dst, .. }
            | Inst::Select { dst, .. } => *dst = r(*dst),
            Inst::Scope(body) => map(body, r, s),
            Inst::If { yes, no, .. } => {
                map(yes, r, s);
                map(no, r, s);
            }
            Inst::Loop {
                condition, body, ..
            } => {
                map(condition, r, s);
                map(body, r, s);
            }
            _ => {}
        }
    }
}
fn map_operands(i: &mut Inst, r: &impl Fn(Reg) -> Reg) {
    if let Inst::TextureSample { texture, .. }
    | Inst::TextureFetch { texture, .. }
    | Inst::TextureSize { texture, .. }
    | Inst::TextureLevels { texture, .. } = i
    {
        texture.element = texture.element.map(r);
    }
    match i {
        Inst::TextureSample {
            coordinate, lod, ..
        } => {
            coordinate.iter_mut().for_each(|v| *v = r(*v));
            *lod = r(*lod);
        }
        Inst::TextureFetch {
            coordinate, level, ..
        } => {
            coordinate.iter_mut().for_each(|v| *v = r(*v));
            *level = r(*level);
        }
        Inst::TextureSize { level, .. } => *level = r(*level),
        Inst::Append(a) => a
            .values
            .iter_mut()
            .for_each(|(_, value)| *value = r(*value)),
        Inst::Atomic(a) => {
            a.index = r(a.index);
            a.offset = a.offset.map(r);
            a.op.map(r);
        }
        Inst::TilePublish(p) => p
            .values
            .iter_mut()
            .for_each(|(_, value)| *value = r(*value)),
        Inst::RayQuery {
            origin,
            direction,
            minimum,
            maximum,
            ..
        } => {
            origin
                .iter_mut()
                .chain(direction.iter_mut())
                .for_each(|v| *v = r(*v));
            *minimum = r(*minimum);
            *maximum = r(*maximum);
        }
        Inst::LoadIndexed { source, offset, .. } => {
            source.map(r);
            *offset = r(*offset);
        }
        Inst::StoreIndexed {
            source,
            offset,
            value,
            ..
        } => {
            source.map(r);
            *offset = r(*offset);
            *value = r(*value);
        }
        Inst::StoreLocal(_, src) => *src = r(*src),
        Inst::LoadLocalIndexed { index, .. } => *index = r(*index),
        Inst::StoreLocalIndexed { index, value, .. } => {
            *index = r(*index);
            *value = r(*value);
        }
        Inst::Print { value, .. } => *value = r(*value),
        Inst::ExternalCall(call) => call.map_operands(r),
        Inst::Subgroup {
            value, argument, ..
        } => {
            for v in value.iter_mut().chain(argument) {
                *v = r(*v);
            }
        }
        Inst::ViewLoad { address, index, .. } => {
            address.iter_mut().for_each(|v| *v = r(*v));
            *index = r(*index);
        }
        Inst::Load { index, .. } | Inst::TileLoad { index, .. } => *index = r(*index),
        Inst::Store { index, value, .. } => {
            *index = r(*index);
            *value = r(*value);
        }
        Inst::Unary { a, .. } => *a = r(*a),
        Inst::Binary { a, b, .. } => {
            *a = r(*a);
            *b = r(*b);
        }
        Inst::Cast { src, .. } => *src = r(*src),
        Inst::IntegerDivide { src, divisor, .. } => {
            *src = r(*src);
            if let IntegerDivisor::Value(value) = divisor {
                *value = r(*value);
            }
        }
        Inst::Math { args, .. } => {
            for a in args {
                *a = r(*a);
            }
        }
        Inst::Select {
            condition, a, b, ..
        } => {
            *condition = r(*condition);
            *a = r(*a);
            *b = r(*b);
        }
        Inst::If { condition, .. } => *condition = r(*condition),
        Inst::Loop { test, .. } => *test = r(*test),
        _ => {}
    }
}
fn dce(
    code: &mut Vec<Inst>,
    needed: &mut BTreeSet<PrivateSlot>,
    break_live: &BTreeSet<PrivateSlot>,
    continue_live: &BTreeSet<PrivateSlot>,
    exit_live: &BTreeSet<PrivateSlot>,
) {
    use PrivateSlot::{Local, Register};
    let mut out = Vec::new();
    for mut i in std::mem::take(code).into_iter().rev() {
        if let Inst::TileCollective(c) = &mut i {
            c.results.retain(|(_, r)| needed.contains(&Register(*r)));
        }
        if let Inst::IntegerDivide { result, .. } = &mut i {
            let q = result.quotient().filter(|r| needed.contains(&Register(*r)));
            let r = result
                .remainder()
                .filter(|r| needed.contains(&Register(*r)));
            let Some(retained) = IntegerDivisionResult::from_parts(q, r) else {
                continue;
            };
            *result = retained;
        }
        let results = i.results();
        if !matches!(
            i,
            Inst::Atomic(_) | Inst::Append(_) | Inst::Print { .. } | Inst::ExternalCall(_)
        ) && !results.is_empty()
            && !results.iter().any(|r| needed.contains(&Register(*r)))
        {
            continue;
        }
        if matches!(&i, Inst::StoreLocal(slot, _) if !needed.contains(&Local(*slot))) {
            continue;
        }
        if matches!(&i, Inst::StoreLocalIndexed{base,len,..} if !(*base..*base+*len).any(|s|needed.contains(&Local(s))))
        {
            continue;
        }
        match &mut i {
            Inst::Scope(body) => {
                let after = needed.clone();
                dce(body, needed, break_live, continue_live, &after);
                if !scope_observable(body, &after) {
                    *needed = after;
                    continue;
                }
            }
            Inst::If { condition, yes, no } => {
                let mut a = needed.clone();
                let mut b = needed.clone();
                dce(yes, &mut a, break_live, continue_live, exit_live);
                dce(no, &mut b, break_live, continue_live, exit_live);
                if yes.is_empty() && no.is_empty() {
                    continue;
                }
                a.extend(b);
                a.insert(Register(*condition));
                *needed = a;
            }
            Inst::Loop {
                condition,
                test,
                body,
            } => {
                let header = loop_live(
                    condition,
                    *test,
                    body,
                    needed,
                    break_live,
                    continue_live,
                    exit_live,
                );
                let mut before_body = header.clone();
                dce(body, &mut before_body, needed, &header, exit_live);
                before_body.extend(needed.iter().copied());
                before_body.insert(Register(*test));
                dce(
                    condition,
                    &mut before_body,
                    break_live,
                    continue_live,
                    exit_live,
                );
                *needed = before_body;
            }
            _ => {
                *needed = live_before(
                    std::slice::from_ref(&i),
                    needed.clone(),
                    break_live,
                    continue_live,
                    exit_live,
                )
            }
        }
        out.push(i);
    }
    out.reverse();
    *code = out;
}
fn scope_observable(code: &[Inst], after: &BTreeSet<PrivateSlot>) -> bool {
    code.iter().any(|inst| match inst {
        Inst::StoreLocal(slot, _) => after.contains(&PrivateSlot::Local(*slot)),
        Inst::StoreLocalIndexed { base, len, .. } => {
            (*base..*base + *len).any(|s| after.contains(&PrivateSlot::Local(s)))
        }
        Inst::Scope(body) => scope_observable(body, after),
        Inst::If { yes, no, .. } => scope_observable(yes, after) || scope_observable(no, after),
        Inst::Loop { .. }
        | Inst::StoreIndexed { .. }
        | Inst::Store { .. }
        | Inst::Atomic(_)
        | Inst::Append(_)
        | Inst::Print { .. }
        | Inst::ExternalCall(_)
        | Inst::TilePublish(_)
        | Inst::TileCollective(_)
        | Inst::TileBarrier => true,
        _ => inst
            .results()
            .iter()
            .any(|r| after.contains(&PrivateSlot::Register(*r))),
    })
}
