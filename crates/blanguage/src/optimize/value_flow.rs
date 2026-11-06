//! 已知局部值直接进入 SSA 使用点；分支合流与循环携带的实际状态保留在局部槽。
use super::*;

type LocalValues = BTreeMap<usize, Reg>;
// Only pending origin facts for this invocation; SSA keys never enter retained metadata.
type Reuses = BTreeMap<Reg, Vec<RetainedUse>>;
enum RetainedUse {
    Expression(ExpressionReuse),
    LocalRead(OperationOrigin),
    IndexedRead(OperationOrigin),
    IndexedStoreRead { store: OperationOrigin, read: OperationOrigin },
}

fn merge(target: &mut Option<LocalValues>, other: Option<LocalValues>) {
    if let Some(other) = other {
        if let Some(target) = target {
            target.retain(|slot, value| other.get(slot) == Some(value));
        } else {
            *target = Some(other);
        }
    }
}

#[derive(Default)]
struct Flow {
    next: Option<LocalValues>,
    breaks: Option<LocalValues>,
    continues: Option<LocalValues>,
    exits: Option<LocalValues>,
}
impl Flow {
    fn join(&mut self, other: Self) {
        merge(&mut self.next, other.next);
        merge(&mut self.breaks, other.breaks);
        merge(&mut self.continues, other.continues);
        merge(&mut self.exits, other.exits);
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ExpressionKey {
    Constant(Scalar, Bits),
    Index,
    Uniform(u32),
    BufferStride(usize),
    Unary(UnaryOp, Scalar, Reg),
    Binary(BinaryOp, Reg, Reg),
    Cast(Scalar, Reg),
    Math(MathOp, Vec<Reg>),
    Select(Reg, Reg, Reg),
    Divide(IntegerDivisionOp, IntegerDivisor, Reg),
}

#[derive(Clone, Default)]
struct Values {
    aliases: BTreeMap<Reg, Reg>,
    expressions: BTreeMap<ExpressionKey, Reg>,
    memory: BTreeMap<(usize, Reg, u32), (Reg, Option<OperationOrigin>)>,
    locals: BTreeMap<usize, Reg>,
}
impl Values {
    fn invalidate(&mut self, aliases: impl Iterator<Item = MemoryAlias>) {
        for alias in aliases {
            match alias {
                MemoryAlias::Local { base, len } =>
                    self.locals.retain(|slot, _| !(base..base + len).contains(slot)),
                MemoryAlias::Data(binding) =>
                    self.memory.retain(|(b, _, _), _| *b != binding),
            }
        }
    }
    fn root(&self, mut r: Reg) -> Reg {
        while let Some(next) = self.aliases.get(&r) {
            r = *next;
        }
        r
    }
    fn local_values(&self) -> LocalValues {
        self.locals
            .iter()
            .map(|(slot, value)| (*slot, self.root(*value)))
            .collect()
    }
    fn resume(&mut self, flow: &mut Flow, step: Flow) -> bool {
        flow.join(step);
        let next = flow.next.take();
        let continuing = next.is_some();
        self.locals = next.unwrap_or_default();
        self.memory.clear();
        continuing
    }
    fn block(&mut self, code: &mut Vec<Inst>, registers: &[Scalar], reuses: &mut Reuses) -> Flow {
        let mut out = Vec::new();
        let mut flow = Flow::default();
        let mut continuing = true;
        // 只指向本直线块已生成的 IR；不会跟随 Values 克隆跨控制／协作边界。
        let mut divisions: BTreeMap<(Reg, IntegerDivisor), (usize, IntegerDivisionResult)> =
            BTreeMap::new();
        for mut inst in std::mem::take(code) {
            map_operands(&mut inst, &|r| self.root(r));
            if matches!(
                &(inst).kind,
                InstKind::If { .. }
                    | InstKind::Loop { .. }
                    | InstKind::Scope(_)
                    | InstKind::Exit
                    | InstKind::Raise(_)
                    | InstKind::TilePublish(_)
                    | InstKind::TileCollective(_)
                    | InstKind::TileBarrier
                    | InstKind::Break
                    | InstKind::Continue
            ) {
                divisions.clear();
            }
            if let InstKind::IntegerDivide {
                result,
                src,
                divisor,
            } = &mut (inst).kind
            {
                let mut quotient = None;
                let mut remainder = None;
                let paired_input = matches!(*result, IntegerDivisionResult::Both { .. });
                for (op, dst) in result.outputs() {
                    let key = ExpressionKey::Divide(op, *divisor, *src);
                    if let Some(old) = self.expressions.get(&key) {
                        self.aliases.insert(dst, *old);
                        reuses.entry(*old).or_default().push(RetainedUse::Expression(ExpressionReuse {
                            result: match op {
                                IntegerDivisionOp::Quotient => ReuseResult::Quotient,
                                IntegerDivisionOp::Remainder => ReuseResult::Remainder,
                            },
                            paired_input,
                            origin: OperationOrigin { source_site: inst.source_site.clone(), generated_origin: inst.generated_origin.clone() },
                        }));
                    } else {
                        self.expressions.insert(key, dst);
                        match op {
                            IntegerDivisionOp::Quotient => quotient = Some(dst),
                            IntegerDivisionOp::Remainder => remainder = Some(dst),
                        }
                    }
                }
                let Some(retained) = IntegerDivisionResult::from_parts(quotient, remainder) else {
                    continue;
                };
                *result = retained;
                let key = (*src, *divisor);
                if let Some((position, previous)) = divisions.remove(&key) {
                    if let (
                        IntegerDivisionResult::Quotient(q),
                        IntegerDivisionResult::Remainder(r),
                    )
                    | (
                        IntegerDivisionResult::Remainder(r),
                        IntegerDivisionResult::Quotient(q),
                    ) = (previous, retained)
                    {
                        let kind = InstKind::IntegerDivide {
                            result: IntegerDivisionResult::Both {
                                quotient: q,
                                remainder: r,
                            },
                            src: *src,
                            divisor: *divisor,
                        };
                        let old = OperationOrigin::from_instruction(&out[position]);
                        let current = OperationOrigin::from_instruction(&inst);
                        let (quotient, remainder) = if matches!(previous, IntegerDivisionResult::Quotient(_)) {
                            (old, current)
                        } else {
                            (current, old)
                        };
                        out[position] = Inst::generated(kind, std::sync::Arc::new(GeneratedOrigin {
                            reason: GenerationReason::IntegerDivisionPair,
                            trigger: Some(GenerationAnchor::IntegerDivisionPair { quotient, remainder }),
                        }));
                        continue;
                    }
                }
                if !matches!(retained, IntegerDivisionResult::Both { .. }) {
                    divisions.insert(key, (out.len(), retained));
                }
            }
            let key = match &(inst).kind {
                InstKind::Constant(dst, scalar, bits) => Some((ExpressionKey::Constant(*scalar, *bits), *dst)),
                InstKind::Index(dst) => Some((ExpressionKey::Index, *dst)),
                InstKind::Uniform(dst, offset) => Some((ExpressionKey::Uniform(*offset), *dst)),
                InstKind::BufferStride(dst,binding) => Some((ExpressionKey::BufferStride(*binding),*dst)),
                InstKind::Unary { dst, op, a } => Some((ExpressionKey::Unary(*op, registers[*dst], *a), *dst)),
                InstKind::Binary { dst, op, a, b } => Some((ExpressionKey::Binary(*op, *a, *b), *dst)),
                InstKind::Cast { dst, src } => Some((ExpressionKey::Cast(registers[*dst], *src), *dst)),
                InstKind::Math { dst, op, args } => Some((ExpressionKey::Math(*op, args.clone()), *dst)),
                InstKind::Select {
                    dst, condition, a, b,
                } => Some((ExpressionKey::Select(*condition, *a, *b), *dst)),
                _ => None,
            };
            if let Some((key, dst)) = key {
                if let Some(old) = self.expressions.get(&key) {
                    self.aliases.insert(dst, *old);
                    reuses.entry(*old).or_default().push(RetainedUse::Expression(ExpressionReuse {
                        result: ReuseResult::Value,
                        paired_input: false,
                        origin: OperationOrigin::from_instruction(&inst),
                    }));
                    continue;
                }
                self.expressions.insert(key, dst);
            }
            match &mut (inst).kind {
                InstKind::Scope(body) => {
                    let mut scoped = Self { locals: std::mem::take(&mut self.locals), memory: std::mem::take(&mut self.memory), ..self.clone() };
                    let mut scope = scoped.block(body, registers, reuses);
                    merge(&mut scope.next, scope.exits.take());
                    continuing = self.resume(&mut flow, scope);
                }
                InstKind::Raise(_) => continuing = false,
                InstKind::Exit => {
                    merge(&mut flow.exits, Some(self.local_values()));
                    continuing = false;
                }
                InstKind::Break => {
                    merge(&mut flow.breaks, Some(self.local_values()));
                    continuing = false;
                }
                InstKind::Continue => {
                    merge(&mut flow.continues, Some(self.local_values()));
                    continuing = false;
                }
                InstKind::Atomic(a) => {
                    if a.op.writes() { self.invalidate(a.target.aliases()); }
                    self.memory.clear();
                }
                InstKind::Append(_) | InstKind::TileBarrier => self.memory.clear(),
                InstKind::LoadLocal(dst, slot) => {
                    if let Some(value) = self.locals.get(slot).copied() {
                        let value = self.root(value);
                        self.aliases.insert(*dst, value);
                        reuses.entry(value).or_default().push(RetainedUse::LocalRead(OperationOrigin {
                            source_site: inst.source_site.clone(),
                            generated_origin: inst.generated_origin.clone(),
                        }));
                        continue;
                    }
                    // 后续读取同一版本可直接复用；真实写入会更新其值。
                    self.locals.insert(*slot, *dst);
                }
                InstKind::StoreLocal(slot, value) => {
                    self.locals.insert(*slot, *value);
                }
                InstKind::StoreLocalIndexed { base, len, .. } => {
                    self.locals
                        .retain(|slot, _| !(*base..*base + *len).contains(slot));
                }
                InstKind::StoreIndexed { source, .. } => self.invalidate(source.aliases()),
                InstKind::Load {
                    dst,
                    binding,
                    index,
                    word,
                } => {
                    let key = (*binding, *index, *word);
                    if let Some((old, store)) = self.memory.get(&key) {
                        self.aliases.insert(*dst, *old);
                        if let Some(store) = store {
                            reuses.entry(*old).or_default().push(RetainedUse::IndexedStoreRead {
                                store: store.clone(),
                                read: OperationOrigin { source_site: inst.source_site.clone(), generated_origin: inst.generated_origin.clone() },
                            });
                        } else {
                            reuses.entry(*old).or_default().push(RetainedUse::IndexedRead(OperationOrigin {
                                source_site: inst.source_site.clone(), generated_origin: inst.generated_origin.clone(),
                            }));
                        }
                        continue;
                    }
                    self.memory.insert(key, (*dst, None));
                }
                InstKind::Store {
                    binding,
                    index,
                    word,
                    value,
                } => {
                    self.memory
                        .retain(|(b, i, w), _| b != binding || (*i == *index && *w != *word));
                    self.memory.insert((*binding, *index, *word), (*value, Some(OperationOrigin {
                        source_site: inst.source_site.clone(), generated_origin: inst.generated_origin.clone(),
                    })));
                }
                InstKind::If { yes, no, .. } => {
                    let mut y = self.clone();
                    let mut n = Self { locals: std::mem::take(&mut self.locals), memory: std::mem::take(&mut self.memory), ..self.clone() };
                    let mut branches = y.block(yes, registers, reuses);
                    branches.join(n.block(no, registers, reuses));
                    // 已退出的分支不流入普通继续点；各出口只交付共同的同一 SSA 值。
                    continuing = self.resume(&mut flow, branches);
                }
                InstKind::Loop {
                    condition,
                    test,
                    body,
                } => {
                    let mut written = BTreeSet::new();
                    for block in [&*condition, &*body] {
                        visit(block, &mut |i| {
                            written.extend(i.local_writes());
                        });
                    }
                    // 循环下一次进入时可能已改变；不把进入前的局部值当作循环不变量。
                    self.locals.retain(|slot, _| !written.contains(slot));
                    let after = self.local_values();
                    self.memory.clear();
                    let mut cycle = Self { locals: std::mem::take(&mut self.locals), ..self.clone() };
                    let condition_flow = cycle.block(condition, registers, reuses);
                    *test = cycle.root(*test);
                    let mut loop_flow = Flow {
                        next: condition_flow.next.as_ref().map(|_| after),
                        breaks: condition_flow.breaks,
                        continues: condition_flow.continues,
                        exits: condition_flow.exits,
                    };
                    if condition_flow.next.is_some() {
                        let body_flow = cycle.block(body, registers, reuses);
                        // 普通／continue 回边和 break 由本循环接收；Scope 退出仍向外传递。
                        // 已写槽仍保留在原循环携带状态，不把某次迭代的定义交给循环后。
                        merge(&mut loop_flow.exits, body_flow.exits);
                    }
                    continuing = self.resume(&mut flow, loop_flow);
                }
                InstKind::Call(call) => {
                    for slot in call.locals(true) {
                        self.locals.remove(&slot);
                    }
                    for effect in &call.memory {
                        if effect.access.writes() && let MemoryAlias::Data(binding) = effect.alias {
                            self.memory.retain(|(b, _, _), _| *b != binding);
                        }
                    }
                    for arg in &call.arguments {
                        if let CallArgument::Reference { access, address: CallReference { storage: CallStorage::Indexed(source), .. }, .. } = arg
                            && access.writes()
                        {
                            if matches!(source, IndexedSource::Pointer { .. }) { self.invalidate(source.aliases()); }
                            else { self.memory.clear(); }
                        }
                    }
                }
                _ => {}
            }
            out.push(inst);
            if !continuing {
                break;
            }
        }
        *code = out;
        if continuing {
            flow.next = Some(self.local_values());
        }
        flow
    }
}
fn retain_reuses(code: &mut [Inst], reuses: &mut Reuses) {
    for inst in code {
        let mut reused = Vec::new();
        let mut reads = Vec::new();
        let mut indexed = Vec::new();
        let mut indexed_reads = Vec::new();
        let result_count = inst.results().count();
        for (result_index, reg) in inst.results().enumerate() {
            let result = match &inst.kind {
                InstKind::IntegerDivide { result, .. } => match result {
                    IntegerDivisionResult::Quotient(_) => ReuseResult::Quotient,
                    IntegerDivisionResult::Remainder(_) => ReuseResult::Remainder,
                    IntegerDivisionResult::Both { quotient, .. } if *quotient == reg => ReuseResult::Quotient,
                    IntegerDivisionResult::Both { .. } => ReuseResult::Remainder,
                },
                _ => ReuseResult::Value,
            };
            for source in reuses.remove(&reg).into_iter().flatten() {
                match source {
                    RetainedUse::Expression(source) => reused.push(source),
                    RetainedUse::LocalRead(origin) => reads.push(LocalReadForward { result_index, result_count, result, origin }),
                    RetainedUse::IndexedRead(origin) => indexed_reads.push(origin),
                    RetainedUse::IndexedStoreRead { store, read } => indexed.push(IndexedStoreReadForward { result_index, result_count, result, store, read }),
                }
            }
        }
        if !reused.is_empty() {
            let retained = OperationOrigin::from_instruction(inst);
            inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::ExpressionReuse,
                trigger: Some(GenerationAnchor::ExpressionReuse { retained, reused }),
            }));
        }
        if !reads.is_empty() {
            let retained = OperationOrigin::from_instruction(inst);
            inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::LocalReadForward,
                trigger: Some(GenerationAnchor::LocalReadForward { retained, reads }),
            }));
        }
        if !indexed.is_empty() {
            let retained = OperationOrigin::from_instruction(inst);
            inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::IndexedStoreReadForward,
                trigger: Some(GenerationAnchor::IndexedStoreReadForward { retained, reads: indexed }),
            }));
        }
        if !indexed_reads.is_empty() {
            let retained = OperationOrigin::from_instruction(inst);
            inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::IndexedReadReuse,
                trigger: Some(GenerationAnchor::IndexedReadReuse { retained, reused: indexed_reads }),
            }));
        }
        match &mut inst.kind {
            InstKind::Scope(body) => retain_reuses(body, reuses),
            InstKind::If { yes, no, .. } => {
                retain_reuses(yes, reuses);
                retain_reuses(no, reuses);
            }
            InstKind::Loop { condition, body, .. } => {
                retain_reuses(condition, reuses);
                retain_reuses(body, reuses);
            }
            _ => {}
        }
    }
}
pub(super) fn run(code: &mut Vec<Inst>, registers: &[Scalar]) {
    run_code(code, registers, &mut []);
}
pub(super) fn with_prelude(
    prelude: &mut Vec<Inst>,
    bodies: &mut [&mut Vec<Inst>],
    registers: &[Scalar],
    inputs: &[(ReductionInputRole, &[Reg])],
) -> (impl Fn(Reg) -> Reg + use<>, Vec<ReductionInputReadForward>) {
    let mut entry = Values::default();
    let mut reuses = Reuses::new();
    entry.block(prelude, registers, &mut reuses);
    let mut aliases = entry.clone();
    for body in bodies.iter_mut() {
        let mut values = entry.clone();
        values.block(body, registers, &mut reuses);
        aliases.aliases.extend(values.aliases);
    }
    retain_reuses(prelude, &mut reuses);
    for body in bodies {
        retain_reuses(body, &mut reuses);
    }
    let mut forwarded = Vec::new();
    for (role, registers) in inputs {
        for (component_index, reg) in registers.iter().enumerate() {
            let reads = input_reads(*reg, &mut reuses);
            if !reads.is_empty() {
                forwarded.push(ReductionInputReadForward { role: *role, component_index, reads });
            }
        }
    }
    (move |r| aliases.root(r), forwarded)
}
fn input_reads(reg: Reg, reuses: &mut Reuses) -> Vec<OperationOrigin> {
    reuses.remove(&reg).into_iter().flatten().filter_map(|source| match source {
        RetainedUse::LocalRead(origin) => Some(origin),
        RetainedUse::Expression(_) | RetainedUse::IndexedRead(_) | RetainedUse::IndexedStoreRead { .. } => None,
    }).collect()
}
fn run_code(code: &mut Vec<Inst>, registers: &[Scalar], outputs: &mut [Reg]) -> Reuses {
    let mut values = Values::default();
    let mut reuses = Reuses::new();
    values.block(code, registers, &mut reuses);
    retain_reuses(code, &mut reuses);
    for r in outputs {
        *r = values.root(*r);
    }
    reuses
}
pub(super) fn run_value(value: &mut ValueCode) {
    let mut reuses = run_code(&mut value.code, &value.registers, &mut value.outputs);
    let mut inputs = Vec::new();
    for (input_index, reg) in value.inputs.iter().enumerate() {
        let reads = input_reads(*reg, &mut reuses);
        if !reads.is_empty() { inputs.push(InputReadForward { input_index, reads }); }
    }
    if !inputs.is_empty() {
        let retained = value.generated_origin.take();
        value.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
            reason: GenerationReason::LocalReadForward,
            trigger: Some(GenerationAnchor::LocalInputForward { retained, inputs }),
        }));
    }
}
