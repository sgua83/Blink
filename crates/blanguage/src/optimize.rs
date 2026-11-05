//! 仅在读写与跨元素依赖允许时融合同域阶段；phase 保留全域阶段边界。
use crate::ir::*;
use std::collections::{BTreeMap, BTreeSet};
mod invariant;
mod selection;
mod value_flow;
pub(crate) use invariant::prepare_reduction;

pub fn optimize(program: &mut Program, worker: &crate::compiler::Worker) -> std::io::Result<()> {
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
                let local = previous.locals.last_key_value().map_or(0, |(end, _)| *end);
                map(&mut stage.code, &|r| r + base, &|s| s + local);
                previous.registers.extend(stage.registers);
                previous.locals.extend(stage.locals.into_iter().map(|(end, ty)| (end + local, ty)));
                previous.code.extend(stage.code);
                previous.reads.extend(stage.reads);
                previous.writes.extend(stage.writes);
                for (binding, written) in stage.write_regions {
                    previous.write_regions.entry(binding).or_default().extend(written);
                }
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
    stages = crate::parallel::try_map(worker, stages, |_| (), |_, mut stage| {
        optimize_stage(&mut stage);
        Ok::<_, std::convert::Infallible>(stage)
    }).map_err(|error| match error {
        crate::parallel::Error::Resource(error) => error,
        crate::parallel::Error::Work(error) => match error {},
    })?;
    stages.retain(|stage| !matches!(stage.operation, Operation::Map) || !stage.code.is_empty());
    program.stages = stages;
    Ok(())
}

fn optimize_stage(stage: &mut Stage) {
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
        return;
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
            if let InstKind::TilePublish(publication) = &mut (inst).kind {
                let before = publication.values.len();
                publication.values.retain(|(word, value)| {
                    (0..stage.registers[*value].size().div_ceil(4)).any(|part| words.contains(&SharedWord::Published {
                        tile: publication.tile,
                        word: *word + part * 4,
                    }))
                });
                changed |= before != publication.values.len();
                if publication.values.is_empty() && !preserve_scope {
                    changed = true;
                    return false;
                }
            }
            if matches!(&(inst).kind, InstKind::TileCollective(c) if c.results.is_empty())
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
        visit(&stage.code, &mut |i| match &(i).kind {
            InstKind::Call(call) => {
                for arg in &call.arguments {
                    if let CallArgument::Reference { access, address, .. } = arg {
                        for alias in address.aliases() {
                            if let MemoryAlias::Data(binding) = alias {
                                if access.reads() { reads.insert(binding); }
                                if access.writes() { writes.insert(binding); }
                            }
                        }
                    }
                }
                for effect in &call.memory {
                    if let MemoryAlias::Data(binding) = effect.alias {
                        if effect.access.reads() { reads.insert(binding); }
                        if effect.access.writes() { writes.insert(binding); }
                    }
                }
            }
            InstKind::Append(a) => {
                reads.insert(a.count.binding);
                writes.extend([a.target, a.count.binding]);
            }
            InstKind::Atomic(a) => {
                for alias in a.target.aliases() {
                    if let MemoryAlias::Data(binding) = alias {
                        if a.op.reads() { reads.insert(binding); }
                        if a.op.writes() { writes.insert(binding); }
                    }
                }
            }
            InstKind::LoadIndexed { source, .. } => {
                reads.extend(source.aliases().filter_map(|a| match a { MemoryAlias::Data(b) => Some(b), _ => None }));
            }
            InstKind::Load { binding, .. } => { reads.insert(*binding); }
            InstKind::StoreIndexed { source, .. } => {
                writes.extend(source.aliases().filter_map(|a| match a { MemoryAlias::Data(b) => Some(b), _ => None }));
            }
            InstKind::Store { binding, .. } => { writes.insert(*binding); }
            _ => {}
        });
        reads.extend(stage.input_words().keys().map(|(source, _)| *source));
        reads.extend(stage.prefix.map(|p| p.binding));
        reads.extend(stage.selection.map(|s| s.binding));
        stage.reads = reads.into_iter().collect();
        stage.writes = writes.into_iter().collect();
        stage.write_regions.retain(|binding, _| stage.writes.contains(binding));
        stage
            .cross_access
            .retain(|r| stage.reads.contains(r) || stage.writes.contains(r));
    }
    let mut used = BTreeSet::new();
    let mut slots = BTreeSet::new();
    visit(&stage.code, &mut |i| {
        used.extend(i.results());
        i.all_operands(&mut |reg| { used.insert(reg); true });
        slots.extend(i.local_reads());
        slots.extend(i.local_writes());
    });
    slots.extend(private_spans(&stage.code).into_iter().flatten());
    let registers: Vec<_> = used.iter().map(|r| stage.registers[*r]).collect();
    let mut locals = BTreeMap::new();
    let mut local = BTreeMap::new();
    for (end, ty) in &stage.locals {
        let begin = end - ty.components.len();
        if slots.range(begin..*end).count() == ty.components.len() {
            for slot in begin..*end { local.insert(slot, local.len()); }
            locals.insert(local.len(), ty.clone());
        } else {
            for slot in slots.range(begin..*end) {
                local.insert(*slot, local.len());
                locals.insert(local.len(), Ty::scalar(ty.components[slot - begin].0));
            }
        }
    }
    let regs: BTreeMap<_, _> = used.into_iter().enumerate().map(|(i, r)| (r, i)).collect();
    map(&mut stage.code, &|r| regs[&r], &|s| local[&s]);
    stage.registers = registers;
    stage.locals = locals;
}
pub(crate) fn value_code(value: &mut ValueCode) {
    value_flow::run_value(value);
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
        i.all_operands(&mut |reg| { used.insert(reg); true });
        slots.extend(i.local_reads());
        slots.extend(i.local_writes());
    });
    slots.extend(private_spans(&value.code).into_iter().flatten());
    let registers = used.iter().map(|r| value.registers[*r]).collect();
    let mut locals = BTreeMap::new();
    let mut local = BTreeMap::new();
    for (end, ty) in &value.locals {
        let begin = end - ty.components.len();
        if slots.range(begin..*end).count() == ty.components.len() {
            for slot in begin..*end { local.insert(slot, local.len()); }
            locals.insert(local.len(), ty.clone());
        } else {
            for slot in slots.range(begin..*end) {
                local.insert(*slot, local.len());
                locals.insert(local.len(), Ty::scalar(ty.components[slot - begin].0));
            }
        }
    }
    let regs: BTreeMap<_, _> = used.into_iter().enumerate().map(|(i, r)| (r, i)).collect();
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
pub(crate) fn visit(code: &[Inst], f: &mut impl FnMut(&Inst)) {
    for i in code {
        f(i);
        match &(i).kind {
            InstKind::Scope(body) => visit(body, f),
            InstKind::If { yes, no, .. } => {
                visit(yes, f);
                visit(no, f);
            }
            InstKind::Loop {
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
        match &mut (i).kind {
            InstKind::TextureSample { result, .. } | InstKind::TextureFetch { result, .. } => {
                result.iter_mut().for_each(|v| *v = r(*v))
            }
            InstKind::TextureSize { result, .. } => result.iter_mut().for_each(|v| *v = r(*v)),
            InstKind::TextureLevels { dst, .. }
            | InstKind::TextureAddress { dst, .. }
            | InstKind::TextureFormat { dst, .. }
            | InstKind::TextureRead { dst, .. } => *dst = r(*dst),
            InstKind::Append(a) => a.accepted = r(a.accepted),
            InstKind::Atomic(a) => { a.result = a.result.map(r); a.target.map_locals(s); }
            InstKind::RayQuery { result, .. } => result.map(r),
            InstKind::IntegerDivide { result, .. } => result.map(r),
            InstKind::TileCollective(c) => c.results.iter_mut().for_each(|(_, dst)| *dst = r(*dst)),
            InstKind::Constant(dst, ..)
            | InstKind::FunctionAddress(dst, _)
            | InstKind::Index(dst)
            | InstKind::Uniform(dst, _)
            | InstKind::BufferStride(dst, _)
            | InstKind::TileLength(dst)
            | InstKind::TileLane(dst) => *dst = r(*dst),
            InstKind::LoadLocal(dst, slot) => {
                *dst = r(*dst);
                *slot = s(*slot);
            }
            InstKind::StoreLocal(slot, _) => *slot = s(*slot),
            InstKind::LoadLocalIndexed { dst, base, .. } => {
                *dst = r(*dst);
                *base = s(*base);
            }
            InstKind::StoreLocalIndexed { base, .. } => *base = s(*base),
            InstKind::Call(call) => {
                for value in &mut call.values { *value = r(*value); }
                call.map_locals(s);
            }
            InstKind::PositionAddress { dst, address } => { *dst = r(*dst); address.map_locals(s); }
            InstKind::LoadIndexed { dst, source, .. } => { *dst = r(*dst); source.map_locals(s); }
            InstKind::StoreIndexed { source, .. } => source.map_locals(s),
            InstKind::Load { dst, .. }
            | InstKind::Subgroup { dst, .. }
            | InstKind::Print { dst, .. }
            | InstKind::TileLoad { dst, .. }
            | InstKind::ViewLoad { dst, .. }
            | InstKind::Unary { dst, .. }
            | InstKind::Binary { dst, .. }
            | InstKind::Cast { dst, .. }
            | InstKind::Math { dst, .. }
            | InstKind::Select { dst, .. } => *dst = r(*dst),
            InstKind::Scope(body) => map(body, r, s),
            InstKind::If { yes, no, .. } => {
                map(yes, r, s);
                map(no, r, s);
            }
            InstKind::Loop {
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
    if let InstKind::TextureSample { texture, .. }
    | InstKind::TextureFetch { texture, .. }
    | InstKind::TextureSize { texture, .. }
    | InstKind::TextureLevels { texture, .. }
    | InstKind::TextureAddress { texture, .. }
    | InstKind::TextureFormat { texture, .. }
    | InstKind::TextureRead { texture, .. } = &mut (i).kind
    {
        texture.element = texture.element.map(r);
    }
    match &mut (i).kind {
        InstKind::PositionAddress { address, .. } => address.map_operands(r),
        InstKind::FunctionAddress(_, CallTarget::Indirect(reg)) => *reg = r(*reg),
        InstKind::TextureSample {
            coordinate, lod, ..
        } => {
            coordinate.iter_mut().for_each(|v| *v = r(*v));
            *lod = r(*lod);
        }
        InstKind::TextureFetch {
            coordinate, level, ..
        }
        | InstKind::TextureRead {
            coordinate, level, ..
        } => {
            coordinate.iter_mut().for_each(|v| *v = r(*v));
            *level = r(*level);
        }
        InstKind::TextureSize { level, .. } => *level = r(*level),
        InstKind::Append(a) => a
            .values
            .iter_mut()
            .for_each(|(_, value)| *value = r(*value)),
        InstKind::Atomic(a) => {
            a.target.map(r);
            a.offset = a.offset.map(r);
            a.op.map(r);
        }
        InstKind::TilePublish(p) => p
            .values
            .iter_mut()
            .for_each(|(_, value)| *value = r(*value)),
        InstKind::RayQuery {
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
        InstKind::LoadIndexed { source, offset, .. } => {
            source.map(r);
            *offset = r(*offset);
        }
        InstKind::StoreIndexed {
            source,
            offset,
            value,
            ..
        } => {
            source.map(r);
            *offset = r(*offset);
            *value = r(*value);
        }
        InstKind::StoreLocal(_, src) => *src = r(*src),
        InstKind::LoadLocalIndexed { index, .. } => *index = r(*index),
        InstKind::StoreLocalIndexed { index, value, .. } => {
            *index = r(*index);
            *value = r(*value);
        }
        InstKind::Print { value, .. } => *value = r(*value),
        InstKind::Call(call) => call.map_operands(r),
        InstKind::Subgroup {
            value, argument, ..
        } => {
            for v in value.iter_mut().chain(argument) {
                *v = r(*v);
            }
        }
        InstKind::ViewLoad { address, index, .. } => {
            address.iter_mut().for_each(|v| *v = r(*v));
            *index = r(*index);
        }
        InstKind::Load { index, .. } | InstKind::TileLoad { index, .. } => *index = r(*index),
        InstKind::Store { index, value, .. } => {
            *index = r(*index);
            *value = r(*value);
        }
        InstKind::Unary { a, .. } => *a = r(*a),
        InstKind::Binary { a, b, .. } => {
            *a = r(*a);
            *b = r(*b);
        }
        InstKind::Cast { src, .. } => *src = r(*src),
        InstKind::IntegerDivide { src, divisor, .. } => {
            *src = r(*src);
            if let IntegerDivisor::Value(value) = divisor {
                *value = r(*value);
            }
        }
        InstKind::Math { args, .. } => {
            for a in args {
                *a = r(*a);
            }
        }
        InstKind::Select {
            condition, a, b, ..
        } => {
            *condition = r(*condition);
            *a = r(*a);
            *b = r(*b);
        }
        InstKind::If { condition, .. } => *condition = r(*condition),
        InstKind::Loop { test, .. } => *test = r(*test),
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
        if let InstKind::TileCollective(c) = &mut (i).kind {
            c.results.retain(|(_, r)| needed.contains(&Register(*r)));
        }
        if let InstKind::IntegerDivide { result, .. } = &mut (i).kind {
            let q = result.quotient().filter(|r| needed.contains(&Register(*r)));
            let r = result
                .remainder()
                .filter(|r| needed.contains(&Register(*r)));
            let Some(retained) = IntegerDivisionResult::from_parts(q, r) else {
                continue;
            };
            *result = retained;
        }
        if !matches!(
            &(i).kind,
            InstKind::Atomic(_) | InstKind::Append(_) | InstKind::Print { .. } | InstKind::Call(_)
        ) {
            let mut results = i.results().peekable();
            if results.peek().is_some()
                && !results.any(|r| needed.contains(&Register(r)))
            {
                continue;
            }
        }
        if matches!(&(i).kind, InstKind::StoreLocal(slot, _) if !needed.contains(&Local(*slot))) {
            continue;
        }
        if matches!(&(i).kind, InstKind::StoreLocalIndexed{base,len,..} if !(*base..*base+*len).any(|s|needed.contains(&Local(s))))
        {
            continue;
        }
        match &mut (i).kind {
            InstKind::Scope(body) => {
                let after = needed.clone();
                dce(body, needed, break_live, continue_live, &after);
                if !scope_observable(body, &after) {
                    *needed = after;
                    continue;
                }
            }
            InstKind::If { condition, yes, no } => {
                let mut a = needed.clone();
                dce(yes, &mut a, break_live, continue_live, exit_live);
                dce(no, needed, break_live, continue_live, exit_live);
                if yes.is_empty() && no.is_empty() {
                    continue;
                }
                needed.extend(a);
                needed.insert(Register(*condition));
            }
            InstKind::Loop {
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
                    std::mem::take(needed),
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
    code.iter().any(|inst| match &(inst).kind {
        InstKind::StoreLocal(slot, _) => after.contains(&PrivateSlot::Local(*slot)),
        InstKind::StoreLocalIndexed { base, len, .. } => {
            (*base..*base + *len).any(|s| after.contains(&PrivateSlot::Local(s)))
        }
        InstKind::Scope(body) => scope_observable(body, after),
        InstKind::If { yes, no, .. } => scope_observable(yes, after) || scope_observable(no, after),
        InstKind::Loop { .. }
        | InstKind::StoreIndexed { .. }
        | InstKind::Store { .. }
        | InstKind::Atomic(_)
        | InstKind::Append(_)
        | InstKind::Print { .. }
        | InstKind::Call(_)
        | InstKind::TilePublish(_)
        | InstKind::TileCollective(_)
        | InstKind::TileBarrier => true,
        _ => inst
            .results()
            .any(|r| after.contains(&PrivateSlot::Register(r))),
    })
}
