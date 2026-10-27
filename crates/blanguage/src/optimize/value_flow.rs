//! 已知局部值直接进入 SSA 使用点；分支合流与循环携带的实际状态保留在局部槽。
use super::*;

type LocalValues = BTreeMap<usize, Reg>;

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

#[derive(Clone, Default)]
struct Values {
    aliases: BTreeMap<Reg, Reg>,
    expressions: BTreeMap<String, Reg>,
    memory: BTreeMap<(usize, Reg, u32), Reg>,
    locals: BTreeMap<usize, Reg>,
}
impl Values {
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
    fn block(&mut self, code: &mut Vec<Inst>, registers: &[Scalar]) -> Flow {
        let mut out = Vec::new();
        let mut flow = Flow::default();
        let mut continuing = true;
        // 只指向本直线块已生成的 IR；不会跟随 Values 克隆跨控制／协作边界。
        let mut divisions: BTreeMap<(Reg, IntegerDivisor), (usize, IntegerDivisionResult)> =
            BTreeMap::new();
        for mut inst in std::mem::take(code) {
            map_operands(&mut inst, &|r| self.root(r));
            if matches!(
                inst,
                Inst::If { .. }
                    | Inst::Loop { .. }
                    | Inst::Scope(_)
                    | Inst::Exit
                    | Inst::TilePublish(_)
                    | Inst::TileCollective(_)
                    | Inst::TileBarrier
                    | Inst::Break
                    | Inst::Continue
            ) {
                divisions.clear();
            }
            if let Inst::IntegerDivide {
                result,
                src,
                divisor,
            } = &mut inst
            {
                let mut quotient = None;
                let mut remainder = None;
                for (op, dst) in result.outputs() {
                    let key = format!("divide{op:?}:{divisor:?}:{src}");
                    if let Some(old) = self.expressions.get(&key) {
                        self.aliases.insert(dst, *old);
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
                        out[position] = Inst::IntegerDivide {
                            result: IntegerDivisionResult::Both {
                                quotient: q,
                                remainder: r,
                            },
                            src: *src,
                            divisor: *divisor,
                        };
                        continue;
                    }
                }
                if !matches!(retained, IntegerDivisionResult::Both { .. }) {
                    divisions.insert(key, (out.len(), retained));
                }
            }
            let key = match &inst {
                Inst::Constant(_, scalar, bits) => Some(format!("C{scalar:?}:{bits}")),
                Inst::Index(_) => Some("index".into()),
                Inst::Uniform(_, offset) => Some(format!("U{offset}")),
                Inst::Unary { op, a, .. } => Some(format!("unary{op:?}:{a}")),
                Inst::Binary { op, a, b, .. } => Some(format!("binary{op:?}:{a}:{b}")),
                Inst::Cast { dst, src } => Some(format!("cast{:?}:{src}", registers[*dst])),
                Inst::Math { op, args, .. } => Some(format!("math{op:?}:{args:?}")),
                Inst::Select {
                    condition, a, b, ..
                } => Some(format!("select{condition}:{a}:{b}")),
                _ => None,
            };
            if let (Some(key), Some(dst)) = (key, destination(&inst)) {
                if let Some(old) = self.expressions.get(&key) {
                    self.aliases.insert(dst, *old);
                    continue;
                }
                self.expressions.insert(key, dst);
            }
            match &mut inst {
                Inst::Scope(body) => {
                    let mut scoped = self.clone();
                    let mut scope = scoped.block(body, registers);
                    merge(&mut scope.next, scope.exits.take());
                    continuing = self.resume(&mut flow, scope);
                }
                Inst::Exit => {
                    merge(&mut flow.exits, Some(self.local_values()));
                    continuing = false;
                }
                Inst::Break => {
                    merge(&mut flow.breaks, Some(self.local_values()));
                    continuing = false;
                }
                Inst::Continue => {
                    merge(&mut flow.continues, Some(self.local_values()));
                    continuing = false;
                }
                Inst::Atomic(_) | Inst::Append(_) | Inst::TileBarrier => self.memory.clear(),
                Inst::LoadLocal(dst, slot) => {
                    if let Some(value) = self.locals.get(slot).copied() {
                        self.aliases.insert(*dst, self.root(value));
                        continue;
                    }
                    // 后续读取同一版本可直接复用；真实写入会更新其值。
                    self.locals.insert(*slot, *dst);
                }
                Inst::StoreLocal(slot, value) => {
                    self.locals.insert(*slot, *value);
                }
                Inst::StoreLocalIndexed { base, len, .. } => {
                    self.locals
                        .retain(|slot, _| !(*base..*base + *len).contains(slot));
                }
                Inst::StoreIndexed {
                    source: IndexedSource::Data { binding, .. },
                    ..
                } => self.memory.retain(|(b, _, _), _| b != binding),
                Inst::Load {
                    dst,
                    binding,
                    index,
                    word,
                } => {
                    let key = (*binding, *index, *word);
                    if let Some(old) = self.memory.get(&key) {
                        self.aliases.insert(*dst, *old);
                        continue;
                    }
                    self.memory.insert(key, *dst);
                }
                Inst::Store {
                    binding,
                    index,
                    word,
                    value,
                } => {
                    self.memory
                        .retain(|(b, i, w), _| b != binding || (*i == *index && *w != *word));
                    self.memory.insert((*binding, *index, *word), *value);
                }
                Inst::If { yes, no, .. } => {
                    let mut y = self.clone();
                    let mut n = self.clone();
                    let mut branches = y.block(yes, registers);
                    branches.join(n.block(no, registers));
                    // 已退出的分支不流入普通继续点；各出口只交付共同的同一 SSA 值。
                    continuing = self.resume(&mut flow, branches);
                }
                Inst::Loop {
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
                    let mut cycle = self.clone();
                    cycle.memory.clear();
                    let condition_flow = cycle.block(condition, registers);
                    *test = cycle.root(*test);
                    let mut loop_flow = Flow {
                        next: condition_flow.next.as_ref().map(|_| after),
                        breaks: condition_flow.breaks,
                        continues: condition_flow.continues,
                        exits: condition_flow.exits,
                    };
                    if condition_flow.next.is_some() {
                        let body_flow = cycle.block(body, registers);
                        // 普通／continue 回边和 break 由本循环接收；Scope 退出仍向外传递。
                        // 已写槽仍保留在原循环携带状态，不把某次迭代的定义交给循环后。
                        merge(&mut loop_flow.exits, body_flow.exits);
                    }
                    continuing = self.resume(&mut flow, loop_flow);
                }
                Inst::Call(call) => {
                    for slot in call.locals(true) {
                        self.locals.remove(&slot);
                    }
                    if call.arguments.iter().any(|arg|matches!(arg,CallArgument::Reference{access,address:CallReference{storage:CallStorage::Indexed(_),..},..} if access.writes())) {self.memory.clear();}
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
pub(super) fn run(code: &mut Vec<Inst>, registers: &[Scalar]) {
    run_value(code, registers, &mut []);
}
pub(super) fn with_prelude(
    prelude: &mut Vec<Inst>,
    bodies: &mut [&mut Vec<Inst>],
    registers: &[Scalar],
) -> impl Fn(Reg) -> Reg + use<> {
    let mut entry = Values::default();
    entry.block(prelude, registers);
    let mut aliases = entry.clone();
    for body in bodies {
        let mut values = entry.clone();
        values.block(body, registers);
        aliases.aliases.extend(values.aliases);
    }
    move |r| aliases.root(r)
}
pub(super) fn run_value(code: &mut Vec<Inst>, registers: &[Scalar], outputs: &mut [Reg]) {
    let mut values = Values::default();
    values.block(code, registers);
    for r in outputs {
        *r = values.root(*r);
    }
}
