//! 已知局部值直接进入 SSA 使用点；分支合流与循环携带的实际状态保留在局部槽。
use super::*;

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
    fn block(&mut self, code: &mut Vec<Inst>, registers: &[Scalar]) {
        let mut out = Vec::new();
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
                    scoped.block(body, registers);
                    self.locals.clear();
                    self.memory.clear();
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
                    y.block(yes, registers);
                    n.block(no, registers);
                    // 只在两个路径交付同一个 SSA 值时转发；分支内的新值不会泄露到另一条路径。
                    self.locals = y
                        .locals
                        .iter()
                        .filter_map(|(slot, value)| {
                            let a = y.root(*value);
                            n.locals
                                .get(slot)
                                .filter(|b| n.root(**b) == a)
                                .map(|_| (*slot, a))
                        })
                        .collect();
                    self.memory.clear();
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
                    let mut cycle = self.clone();
                    cycle.memory.clear();
                    cycle.block(condition, registers);
                    *test = cycle.root(*test);
                    cycle.block(body, registers);
                    self.memory.clear();
                }
                Inst::ExternalCall(call) => {
                    for slot in call.locals(true) {
                        self.locals.remove(&slot);
                    }
                    if call.arguments.iter().any(|arg|matches!(arg,ExternalArgument::Reference{access,address:ExternalReference{storage:ExternalStorage::Indexed(_),..},..} if access.writes())) {self.memory.clear();}
                }
                _ => {}
            }
            out.push(inst);
        }
        *code = out;
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
