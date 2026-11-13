//! 仅在读写与跨元素依赖允许时融合同域阶段；phase 保留全域阶段边界。
use crate::ir::*;
use std::collections::{BTreeMap, BTreeSet};
mod invariant;
mod selection;
mod value_flow;
pub(crate) use invariant::prepare_reduction;

#[repr(C)]
struct BindingSpan {
    data: *const usize,
    count: usize,
}

impl From<&[usize]> for BindingSpan {
    fn from(bindings: &[usize]) -> Self {
        Self { data: bindings.as_ptr(), count: bindings.len() }
    }
}

#[repr(C)]
struct StageBindings {
    reads: BindingSpan,
    writes: BindingSpan,
    cross: BindingSpan,
}

impl From<&Stage> for StageBindings {
    fn from(stage: &Stage) -> Self {
        Self {
            reads: stage.reads.as_slice().into(),
            writes: stage.writes.as_slice().into(),
            cross: stage.cross_access.as_slice().into(),
        }
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct FusionBlock {
    instructions: *const std::ffi::c_void,
    count: usize,
}

impl From<&[Inst]> for FusionBlock {
    fn from(code: &[Inst]) -> Self {
        Self { instructions: code.as_ptr().cast(), count: code.len() }
    }
}

#[repr(C)]
struct FusionNode {
    first: FusionBlock,
    second: FusionBlock,
    next: usize,
    ordered: u32,
}

#[derive(Default)]
#[repr(C)]
struct DomainField {
    binding: usize,
    word: u32,
    present: u32,
}

impl From<Option<(usize, u32)>> for DomainField {
    fn from(field: Option<(usize, u32)>) -> Self {
        field.map_or_else(Self::default, |(binding, word)| Self { binding, word, present: 1 })
    }
}

#[repr(C)]
struct FusionStage {
    bindings: StageBindings,
    body: FusionBlock,
    domain: usize,
    prefix: DomainField,
    selection: DomainField,
    map: u32,
    boundary_before: u32,
}

impl From<&Stage> for FusionStage {
    fn from(stage: &Stage) -> Self {
        Self {
            bindings: stage.into(),
            body: stage.code.as_slice().into(),
            domain: stage.domain,
            prefix: stage.prefix.map(|field| (field.binding, field.word)).into(),
            selection: stage.selection.map(|field| (field.binding, field.word)).into(),
            map: u32::from(matches!(stage.operation, Operation::Map)),
            boundary_before: u32::from(stage.boundary_before),
        }
    }
}

unsafe extern "C" fn read_fusion_node(block: FusionBlock, next: usize) -> FusionNode {
    let code = unsafe { std::slice::from_raw_parts(block.instructions.cast::<Inst>(), block.count) };
    for (offset, inst) in code[next..].iter().enumerate() {
        let (first, second, ordered): (&[Inst], &[Inst], u32) = match &inst.kind {
            InstKind::Scope(body) => (body, &[], 0),
            InstKind::If { yes, no, .. } => (yes, no, 0),
            InstKind::Loop { condition, body, .. } => (condition, body, 0),
            InstKind::Append(_)
            | InstKind::Subgroup { .. }
            | InstKind::Print { .. }
            | InstKind::Call(_)
            | InstKind::TileLength(_)
            | InstKind::TileLane(_)
            | InstKind::TilePublish(_)
            | InstKind::TileBarrier
            | InstKind::TileCollective(_)
            | InstKind::TileLoad { .. }
            | InstKind::LoadIndexed { source: IndexedSource::Tile { .. }, .. }
            | InstKind::Atomic(Atomic { target: IndexedSource::Tile { .. }, .. }) => (&[], &[], 1),
            _ => continue,
        };
        return FusionNode { first: first.into(), second: second.into(), next: next + offset + 1, ordered };
    }
    FusionNode { first: (&[] as &[Inst]).into(), second: (&[] as &[Inst]).into(), next: block.count, ordered: 0 }
}

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::can_fuse"]
    fn can_fuse(left: FusionStage, right: FusionStage,
        read: unsafe extern "C" fn(FusionBlock, usize) -> FusionNode) -> u32;
    #[link_name = "bootstrap.stage_fusion::merge_bindings"]
    fn merge_bindings(output: *mut usize, left_count: usize, right: BindingSpan) -> usize;
}

fn union_bindings(left: &mut Vec<usize>, right: Vec<usize>) {
    if right.is_empty() {
        return;
    }
    if left.is_empty() {
        *left = right;
        return;
    }
    let left_count = left.len();
    let total = left_count + right.len();
    left.reserve(right.len());
    let output = left.as_mut_ptr();
    // B initializes the suffix of this allocation; move only the produced union.
    unsafe {
        let start = merge_bindings(output, left_count, right.as_slice().into());
        let count = total - start;
        output.add(start).copy_to(output, count);
        left.set_len(count);
    }
}

fn fuse(input: impl IntoIterator<Item = (Stage, bool)>) -> Vec<(Stage, bool)> {
    let mut stages: Vec<(Stage, bool)> = Vec::new();
    for (mut stage, needs_optimization) in input {
        if let Some((previous, changed)) = stages.last_mut() {
            if unsafe { can_fuse((&*previous).into(), (&stage).into(), read_fusion_node) } != 0 {
                let following = OperationOrigin {
                    source_site: stage.operation_source.clone(),
                    generated_origin: stage.generated_origin.take(),
                };
                if let Some(GenerationAnchor::StageFusion { stages }) = previous.generated_origin.as_mut()
                    .and_then(std::sync::Arc::get_mut).and_then(|origin| origin.trigger.as_mut())
                {
                    stages.push(following);
                } else {
                    let retained = OperationOrigin {
                        source_site: previous.operation_source.clone(),
                        generated_origin: previous.generated_origin.take(),
                    };
                    previous.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                        reason: GenerationReason::StageFusion,
                        trigger: Some(GenerationAnchor::StageFusion { stages: vec![retained, following] }),
                    }));
                }
                let base = previous.registers.len();
                let local = previous.locals.last_key_value().map_or(0, |(end, _)| *end);
                map(&mut stage.code, &|r| r + base, &|s| s + local);
                previous.registers.extend(stage.registers);
                previous.locals.extend(stage.locals.into_iter().map(|(end, ty)| (end + local, ty)));
                previous.code.extend(stage.code);
                union_bindings(&mut previous.reads, stage.reads);
                union_bindings(&mut previous.writes, stage.writes);
                previous.prefix_guarantees.retain(|relation| !stage.write_regions.get(&relation.count.binding).is_some_and(|written|
                    written.iter().any(|&(start,end)| start < relation.count.word + 4 && relation.count.word < end)));
                previous.prefix_guarantees.extend(stage.prefix_guarantees);
                previous.element_guarantees.retain(|fact| {
                    let (binding, offset, bytes) = match fact {
                        ElementFact::Range(range) => (range.binding, range.offset, range.scalar.size()),
                        ElementFact::Index { binding, offset, .. } => (*binding, *offset, 4),
                    };
                    !stage.write_regions.get(&binding).is_some_and(|written| written.iter().any(|&(start,end)|
                        u64::from(offset) < u64::from(end) && u64::from(start) < u64::from(offset) + u64::from(bytes)))
                });
                previous.element_guarantees.extend(stage.element_guarantees);
                for (binding, written) in stage.write_regions {
                    previous.write_regions.entry(binding).or_default().extend(written);
                }
                union_bindings(&mut previous.cross_access, stage.cross_access);
                *changed = true;
                continue;
            }
        }
        stages.push((stage, needs_optimization));
    }
    stages
}

fn optimize_stages(worker: &crate::compiler::Worker, stages: Vec<(Stage, bool)>) -> std::io::Result<Vec<(Stage, bool)>> {
    crate::parallel::try_map(worker, stages, |_| (), |_, (mut stage, changed)| {
        if changed { optimize_stage(&mut stage); }
        Ok::<_, std::convert::Infallible>((stage, false))
    }).map_err(|error| match error {
        crate::parallel::Error::Resource(error) => error,
        crate::parallel::Error::Work(error) => match error {},
    })
}

pub fn optimize(program: &mut Program, worker: &crate::compiler::Worker) -> std::io::Result<()> {
    let input = std::mem::take(&mut program.stages).into_iter().map(|stage| (stage, true));
    let mut stages = optimize_stages(worker, fuse(input))?;
    let mut removed = Vec::new();
    let mut boundary = false;
    stages.retain_mut(|(stage, _)| {
        let keep = !matches!(stage.operation, Operation::Map) || !stage.code.is_empty();
        if !keep {
            boundary |= stage.boundary_before;
            removed.push(OperationOrigin {
                source_site: stage.operation_source.clone(),
                generated_origin: stage.generated_origin.clone(),
            });
        } else {
            stage.boundary_before |= std::mem::take(&mut boundary);
        }
        keep
    });
    if !removed.is_empty() {
        let retained = program.generated_origin.take();
        program.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
            reason: GenerationReason::EmptyMapRemoval,
            trigger: Some(GenerationAnchor::EmptyMapRemoval { retained, stages: removed }),
        }));
        stages = optimize_stages(worker, fuse(stages))?;
    }
    program.stages = stages.into_iter().map(|(stage, _)| stage).collect();
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
        let removed = dce(
            &mut stage.code,
            &mut BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            &stage.registers,
        );
        GeneratedOrigin::eliminated_operations(&mut stage.generated_origin, removed);
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
    let removed = dce(
        &mut value.code,
        &mut needed,
        &BTreeSet::new(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &value.registers,
    );
    GeneratedOrigin::eliminated_operations(&mut value.generated_origin, removed);
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
pub(crate) fn visit(code: &[Inst], f: &mut impl FnMut(&Inst)) {
    for i in code {
        f(i);
        match &(i).kind {
            InstKind::Scope(body) | InstKind::Cleanup{body,..} => visit(body, f),
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
            InstKind::TextureBindingAddress { dst, .. }
            | InstKind::TextureLevels { dst, .. }
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
                call.error = call.error.map(r);
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
    if let InstKind::TextureBindingAddress { texture, .. }
    | InstKind::TextureSample { texture, .. }
    | InstKind::TextureFetch { texture, .. }
    | InstKind::TextureSize { texture, .. }
    | InstKind::TextureLevels { texture, .. }
    | InstKind::TextureAddress { texture, .. }
    | InstKind::TextureFormat { texture, .. }
    | InstKind::TextureRead { texture, .. } = &mut (i).kind
    {
        texture.element = texture.element.map(r);
        texture.parameter = texture.parameter.map(r);
    }
    match &mut (i).kind {
        InstKind::PositionAddress { address, .. } => address.map_operands(r),
        InstKind::Raise(error) => *error = r(*error),
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
            cull_back_faces,
            ..
        } => {
            origin
                .iter_mut()
                .chain(direction.iter_mut())
                .for_each(|v| *v = r(*v));
            *minimum = r(*minimum);
            *maximum = r(*maximum);
            *cull_back_faces = cull_back_faces.map(r);
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
        InstKind::ViewLoad { address, index, stride, .. } => {
            address.iter_mut().for_each(|v| *v = r(*v));
            stride.map(r);
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
    registers: &[Scalar],
) -> Vec<EliminatedOperation> {
    use PrivateSlot::{Local, Register};
    let mut out = Vec::new();
    let mut removed = Vec::new();
    for mut i in std::mem::take(code).into_iter().rev() {
        if let InstKind::TileCollective(c) = &mut (i).kind {
            c.results.retain(|(_, r)| needed.contains(&Register(*r)));
        }
        if let InstKind::IntegerDivide { result, .. } = &mut (i).kind {
            let original = *result;
            let q = result.quotient().filter(|r| needed.contains(&Register(*r)));
            let r = result
                .remainder()
                .filter(|r| needed.contains(&Register(*r)));
            let Some(retained) = IntegerDivisionResult::from_parts(q, r) else {
                removed.push(EliminatedOperation::ValueResults {
                    result_types: original.outputs().map(|(_, reg)| registers[reg]).collect(),
                    integer_division_results: Some(original.outputs().map(|(result, _)| result).collect()),
                    origin: OperationOrigin::from_instruction(&i),
                });
                continue;
            };
            *result = retained;
            if original != retained {
                let origin = OperationOrigin::from_instruction(&i);
                i.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                    reason: GenerationReason::IntegerDivisionResultPrune,
                    trigger: Some(GenerationAnchor::IntegerDivisionResultPrune {
                        retained: origin,
                        input_results: original.outputs().map(|(result, _)| result).collect(),
                        output_results: retained.outputs().map(|(result, _)| result).collect(),
                    }),
                }));
            }
        }
        if !matches!(
            &(i).kind,
            InstKind::Atomic(_) | InstKind::Append(_) | InstKind::Print { .. } | InstKind::Call(_)
        ) {
            let mut results = i.results().peekable();
            if results.peek().is_some()
                && !results.any(|r| needed.contains(&Register(r)))
            {
                removed.push(EliminatedOperation::ValueResults {
                    result_types: i.results().map(|reg| registers[reg]).collect(),
                    integer_division_results: None,
                    origin: OperationOrigin::from_instruction(&i),
                });
                continue;
            }
        }
        match &i.kind {
            InstKind::StoreLocal(slot, value) if !needed.contains(&Local(*slot)) => {
                removed.push(EliminatedOperation::DirectPrivateWrite {
                    scalar: registers[*value],
                    origin: OperationOrigin::from_instruction(&i),
                });
                continue;
            }
            InstKind::StoreLocalIndexed { base, len, index, word, value }
                if !(*base..*base + *len).any(|s| needed.contains(&Local(s))) => {
                removed.push(EliminatedOperation::IndexedPrivateWrite {
                    component_count: *len,
                    index_type: registers[*index],
                    byte_offset: *word,
                    scalar: registers[*value],
                    origin: OperationOrigin::from_instruction(&i),
                });
                continue;
            }
            _ => {}
        }
        match &mut (i).kind {
            InstKind::Scope(body) => {
                let after = needed.clone();
                removed.extend(dce(body, needed, break_live, continue_live, &after, registers));
                if !scope_observable(body, &after) {
                    *needed = after;
                    continue;
                }
            }
            InstKind::If { condition, yes, no } => {
                let mut a = needed.clone();
                removed.extend(dce(yes, &mut a, break_live, continue_live, exit_live, registers));
                removed.extend(dce(no, needed, break_live, continue_live, exit_live, registers));
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
                removed.extend(dce(body, &mut before_body, needed, &header, exit_live, registers));
                before_body.extend(needed.iter().copied());
                before_body.insert(Register(*test));
                removed.extend(dce(
                    condition,
                    &mut before_body,
                    break_live,
                    continue_live,
                    exit_live,
                    registers,
                ));
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
    removed
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
