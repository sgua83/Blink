//! 发布点切分块内阶段；私有值与共享 word 分别按实际消费者安排生命周期。
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SharedWord {
    Input { binding: usize, word: u32 },
    Published { tile: usize, word: u32 },
}
impl SharedWord {
    pub(crate) fn aligned(self) -> Self {
        match self {
            Self::Input { binding, word } => Self::Input { binding, word: word & !3 },
            Self::Published { tile, word } => Self::Published { tile, word: word & !3 },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PrivateSlot {
    Register(Reg),
    Local(usize),
}
pub(crate) struct TilePhase<'a> {
    pub code: &'a [Inst],
    pub step: Option<TileStep<'a>>,
    pub live_in: BTreeSet<PrivateSlot>,
    pub live_out: BTreeSet<PrivateSlot>,
}
#[derive(Clone, Copy)]
pub(crate) enum TileStep<'a> {
    Publish(&'a Publication),
    Collective(&'a TileCollective),
    Barrier,
}
impl TilePhase<'_> {
    pub fn preserved_out(&self) -> BTreeSet<PrivateSlot> {
        let mut slots = self.live_out.clone();
        if let Some(TileStep::Collective(c)) = self.step {
            for (_, r) in &c.results {
                slots.remove(&PrivateSlot::Register(*r));
            }
        }
        slots
    }
}
impl Stage {
    pub(crate) fn has_tile_steps(&self) -> bool {
        self.code.iter().any(|i| {
            matches!(
                i,
                Inst::TilePublish(_) | Inst::TileCollective(_) | Inst::TileBarrier
            )
        })
    }
    pub fn shared_word(&self, tile: usize, word: u32) -> SharedWord {
        match self.tiles[tile].source {
            TileSource::Input {
                binding,
                word: base,
            } => SharedWord::Input {
                binding,
                word: base + word,
            },
            TileSource::Published => SharedWord::Published { tile, word },
        }
    }
    pub(crate) fn input_words(&self) -> BTreeMap<(usize, u32), BTreeSet<(u32, Scalar)>> {
        let used = self.storage_words();
        let mut words = BTreeMap::<_, BTreeSet<_>>::new();
        for tile in &self.tiles {
            if let TileSource::Input { binding, word: base } = tile.source {
                for (scalar, byte) in &tile.ty.components {
                    for part in 0..scalar.size().div_ceil(4) {
                        let word = base + byte + part * 4;
                        if used.contains(&SharedWord::Input { binding, word }) {
                            let unit = match scalar.size() { 1 => Scalar::U8, 2 => Scalar::U16, _ => Scalar::U32 };
                            words.entry((binding, word & !3)).or_default().insert((word, unit));
                        }
                    }
                }
            }
        }
        words
    }
    pub(crate) fn tile_phases(&self) -> Vec<TilePhase<'_>> {
        let mut phases = Vec::new();
        let mut begin = 0;
        for (i, inst) in self.code.iter().enumerate() {
            let step = match inst {
                Inst::TilePublish(p) => Some(TileStep::Publish(p)),
                Inst::TileCollective(c) => Some(TileStep::Collective(c)),
                Inst::TileBarrier => Some(TileStep::Barrier),
                _ => None,
            };
            if step.is_some() {
                phases.push(TilePhase {
                    code: &self.code[begin..i],
                    step,
                    live_in: BTreeSet::new(),
                    live_out: BTreeSet::new(),
                });
                begin = i + 1;
            }
        }
        phases.push(TilePhase {
            code: &self.code[begin..],
            step: None,
            live_in: BTreeSet::new(),
            live_out: BTreeSet::new(),
        });
        let mut live = BTreeSet::new();
        for phase in phases.iter_mut().rev() {
            phase.live_out = live.clone();
            match phase.step {
                Some(TileStep::Publish(publication)) => live.extend(
                    publication
                        .values
                        .iter()
                        .map(|(_, r)| PrivateSlot::Register(*r)),
                ),
                Some(TileStep::Collective(c)) => {
                    for (_, r) in &c.results {
                        live.remove(&PrivateSlot::Register(*r));
                    }
                }
                Some(TileStep::Barrier) | None => {}
            }
            live = live_before(
                phase.code,
                live,
                &BTreeSet::new(),
                &BTreeSet::new(),
                &BTreeSet::new(),
            );
            phase.live_in = live.clone();
        }
        phases
    }
    pub(crate) fn tile_private_slots(&self) -> BTreeSet<PrivateSlot> {
        self.tile_phases()
            .iter()
            .flat_map(|p| p.live_out.iter().copied())
            .collect()
    }
    pub(crate) fn private_scalar(&self, slot: PrivateSlot) -> Scalar {
        match slot {
            PrivateSlot::Register(r) => self.registers[r],
            PrivateSlot::Local(s) => {
                let (end, ty) = self.locals.range(s + 1..).next().unwrap();
                ty.components[s + ty.components.len() - end].0
            }
        }
    }
    fn tile_layout(&self) -> BTreeMap<SharedWord, usize> {
        fn loads(stage: &Stage, code: &[Inst], phase: i64, last: &mut BTreeMap<SharedWord, i64>, groups: &mut Vec<Vec<SharedWord>>) {
            for inst in code {
                match inst {
                    Inst::Atomic(Atomic {
                        target: IndexedSource::Tile { tile, .. },
                        word,
                        offset: None,
                        ..
                    }) => {
                        last.insert(stage.shared_word(*tile, *word).aligned(), phase);
                    }
                    Inst::LoadIndexed {
                        source: IndexedSource::Tile { tile, .. },
                        ..
                    }
                    | Inst::Atomic(Atomic {
                        target: IndexedSource::Tile { tile, .. },
                        offset: Some(_),
                        ..
                    }) => {
                        for (scalar, byte) in &stage.tiles[*tile].ty.components {
                            for part in 0..scalar.size().div_ceil(4) {
                                last.insert(stage.shared_word(*tile, *byte + part * 4).aligned(), phase);
                            }
                        }
                        let bias = match stage.tiles[*tile].source { TileSource::Input { word, .. } => word % 4, TileSource::Published => 0 };
                        let mut group: Vec<_> = (0..(stage.tiles[*tile].ty.size + bias).div_ceil(4))
                            .map(|w| stage.shared_word(*tile, w * 4).aligned()).collect();
                        let mut i = 0;
                        while i < groups.len() {
                            if groups[i].iter().any(|w| group.contains(w)) { group.extend(groups.remove(i)); }
                            else { i += 1; }
                        }
                        group.sort_unstable();
                        group.dedup();
                        groups.push(group);
                    }
                    Inst::TileLoad { dst, tile, word, .. } => {
                        for part in 0..stage.registers[*dst].size().div_ceil(4) {
                            last.insert(stage.shared_word(*tile, *word + part * 4).aligned(), phase);
                        }
                    }
                    Inst::Scope(body) => loads(stage, body, phase, last, groups),
                    Inst::If { yes, no, .. } => {
                        loads(stage, yes, phase, last, groups);
                        loads(stage, no, phase, last, groups);
                    }
                    Inst::Loop {
                        condition, body, ..
                    } => {
                        loads(stage, condition, phase, last, groups);
                        loads(stage, body, phase, last, groups);
                    }
                    _ => {}
                }
            }
        }
        let mut last = BTreeMap::new();
        let mut births = BTreeMap::new();
        let mut groups = Vec::new();
        let mut phase = 0usize;
        for inst in &self.code {
            match inst {
                Inst::TilePublish(publication) => {
                    births.insert(publication.tile, phase as i64);
                    phase += 1;
                }
                Inst::TileCollective(c) => {
                    for (word, result) in &c.results {
                        for part in 0..self.registers[*result].size().div_ceil(4) {
                            last.insert(self.shared_word(c.tile, *word + part * 4).aligned(), phase as i64);
                        }
                    }
                    phase += 1;
                }
                Inst::TileBarrier => phase += 1,
                _ => loads(self, std::slice::from_ref(inst), phase as i64, &mut last, &mut groups),
            }
        }
        let mut words: Vec<_> = last
            .iter()
            .map(|(word, end)| {
                let begin = match word {
                    SharedWord::Input { .. } => -1,
                    SharedWord::Published { tile, .. } => births[tile],
                };
                (begin, *word, *end)
            })
            .collect();
        let mut units: Vec<_> = groups
            .into_iter()
            .map(|group| {
                let begin = words
                    .iter()
                    .filter(|(_, w, _)| group.contains(w))
                    .map(|(b, _, _)| *b)
                    .min()
                    .unwrap();
                let end = words
                    .iter()
                    .filter(|(_, w, _)| group.contains(w))
                    .map(|(_, _, e)| *e)
                    .max()
                    .unwrap();
                words.retain(|(_, w, _)| !group.contains(w));
                (begin, group, end)
            })
            .collect();
        units.extend(words.into_iter().map(|(b, w, e)| (b, vec![w], e)));
        units.sort_unstable();
        let mut ends = Vec::new();
        let mut layout = BTreeMap::new();
        for (begin, group, end) in units {
            // 动态下标所在流保持物理字顺序，其余流仍按真实生命周期复用。
            let width = group.len();
            let slot = (0..=ends.len())
                .find(|start| {
                    (*start..*start + width)
                        .all(|i| ends.get(i).is_none_or(|previous| *previous < begin))
                })
                .unwrap();
            ends.resize(ends.len().max(slot + width), i64::MIN);
            for (i, word) in group.into_iter().enumerate() {
                ends[slot + i] = end;
                layout.insert(word, slot + i);
            }
        }
        layout
    }
    pub fn word_offset(&self, word: SharedWord) -> u64 {
        self.tile_layout()[&word.aligned()] as u64 * self.workgroup_width() as u64 * 4
    }
    pub fn tile_data_bytes(&self) -> u64 {
        self.tile_layout().values().max().map_or(0, |slot| {
            (*slot as u64 + 1) * self.workgroup_width() as u64 * 4
        })
    }
    pub fn tile_scratch_bytes(&self) -> u64 {
        let bytes_per_lane = self
            .code
            .iter()
            .filter_map(|i| match i {
                Inst::TileCollective(c) if !c.results.is_empty() => {
                    Some(if c.op.operator() == CollectiveOperator::Sum {
                        c.results.iter().map(|(_,result)|self.registers[*result].size().div_ceil(4) * 4).max().unwrap_or(0)
                    } else {
                        c.results.iter().map(|(_,result)|self.registers[*result].size().div_ceil(4) * 4).sum()
                    })
                }
                _ => None,
            })
            .max()
            .unwrap_or(0);
        (self.workgroup_width() as u64).next_power_of_two() * u64::from(bytes_per_lane)
    }
    pub fn tile_bytes(&self) -> u64 {
        self.tile_data_bytes() + self.tile_scratch_bytes()
    }
}

pub(crate) fn loop_live(
    condition: &[Inst],
    test: Reg,
    body: &[Inst],
    after: &BTreeSet<PrivateSlot>,
    break_live: &BTreeSet<PrivateSlot>,
    continue_live: &BTreeSet<PrivateSlot>,
    exit_live: &BTreeSet<PrivateSlot>,
) -> BTreeSet<PrivateSlot> {
    // Gen/kill transfers are pointwise; the empty-header result closes the backedge.
    let mut need = live_before(body, BTreeSet::new(), after, &BTreeSet::new(), exit_live);
    need.extend(after.iter().copied());
    need.insert(PrivateSlot::Register(test));
    live_before(condition, need, break_live, continue_live, exit_live)
}

pub(crate) fn live_before(
    code: &[Inst],
    mut live: BTreeSet<PrivateSlot>,
    break_live: &BTreeSet<PrivateSlot>,
    continue_live: &BTreeSet<PrivateSlot>,
    exit_live: &BTreeSet<PrivateSlot>,
) -> BTreeSet<PrivateSlot> {
    use PrivateSlot::{Local, Register};
    for inst in code.iter().rev() {
        match inst {
            Inst::If { condition, yes, no } => {
                let a = live_before(yes, live.clone(), break_live, continue_live, exit_live);
                live = live_before(no, live, break_live, continue_live, exit_live);
                live.extend(a);
                live.insert(Register(*condition));
            }
            Inst::Loop {
                condition,
                test,
                body,
            } => {
                live = loop_live(
                    condition,
                    *test,
                    body,
                    &live,
                    break_live,
                    continue_live,
                    exit_live,
                );
            }
            Inst::Scope(body) => {
                live = live_before(body, live.clone(), break_live, continue_live, &live)
            }
            Inst::Exit => live = exit_live.clone(),
            Inst::Break => live = break_live.clone(),
            Inst::Continue => live = continue_live.clone(),
            Inst::LoadLocal(dst, slot) => {
                live.remove(&Register(*dst));
                live.insert(Local(*slot));
            }
            Inst::StoreLocal(slot, src) => {
                live.remove(&Local(*slot));
                live.insert(Register(*src));
            }
            _ => {
                for dst in inst.results() {
                    live.remove(&Register(dst));
                }
                inst.all_operands(&mut |reg| { live.insert(Register(reg)); true });
                live.extend(inst.local_reads().into_iter().map(Local));
            }
        }
    }
    live
}

impl Inst {
    pub(crate) fn results(&self) -> impl Iterator<Item = Reg> {
        let mut division = [None, None];
        let mut collective = &[][..];
        let values: &[Reg] = match self {
            Self::Call(call) => &call.values,
            Self::TextureSample { result, .. } | Self::TextureFetch { result, .. } => {
                result
            }
            Self::TextureSize { result, .. } => result,
            Self::TextureLevels { dst, .. }
            | Self::TextureAddress { dst, .. }
            | Self::TextureFormat { dst, .. }
            | Self::TextureRead { dst, .. } => std::slice::from_ref(dst),
            Self::Append(a) => std::slice::from_ref(&a.accepted),
            Self::Atomic(a) => a.result.as_slice(),
            Self::Constant(r, ..)
            | Self::Index(r)
            | Self::Uniform(r, _)
            | Self::LoadLocal(r, _)
            | Self::TileLength(r)
            | Self::TileLane(r) => std::slice::from_ref(r),
            Self::LoadIndexed { dst, .. }
            | Self::Load { dst, .. }
            | Self::LoadLocalIndexed { dst, .. }
            | Self::Subgroup { dst, .. }
            | Self::Print { dst, .. }
            | Self::TileLoad { dst, .. }
            | Self::ViewLoad { dst, .. }
            | Self::Unary { dst, .. }
            | Self::Binary { dst, .. }
            | Self::Cast { dst, .. }
            | Self::Math { dst, .. }
            | Self::Select { dst, .. } => std::slice::from_ref(dst),
            Self::RayQuery { result, .. } => result.registers(),
            Self::IntegerDivide { result, .. } => {
                division = [result.quotient(), result.remainder()];
                &[]
            },
            Self::TileCollective(c) => {
                collective = &c.results;
                &[]
            },
            _ => &[],
        };
        values.iter().copied()
            .chain(division.into_iter().flatten())
            .chain(collective.iter().map(|(_, reg)| *reg))
    }
    pub(crate) fn all_operands(&self, f: &mut impl FnMut(Reg) -> bool) -> bool {
        let values = match self {
            Self::Call(call) => call.all_operands(f),
            Self::TextureSample { coordinate, lod, .. } =>
                coordinate.iter().all(|reg| f(*reg)) && f(*lod),
            Self::TextureFetch { coordinate, level, .. }
            | Self::TextureRead { coordinate, level, .. } =>
                coordinate.iter().all(|reg| f(*reg)) && f(*level),
            Self::TextureSize { level, .. } => f(*level),
            Self::Append(value) => value.values.iter().all(|(_, reg)| f(*reg)),
            Self::Atomic(value) => {
                let operands = match &value.op {
                    AtomicOp::Load => true,
                    AtomicOp::Store(reg) | AtomicOp::Add(reg) | AtomicOp::Sub(reg)
                    | AtomicOp::Exchange(reg) | AtomicOp::Min(reg) | AtomicOp::Max(reg)
                    | AtomicOp::And(reg) | AtomicOp::Or(reg) | AtomicOp::Xor(reg) => f(*reg),
                    AtomicOp::CompareExchange { expected, desired, .. } =>
                        f(*expected) && f(*desired),
                };
                operands && value.target.all_operands(f) && value.offset.iter().all(|reg| f(*reg))
            }
            Self::TilePublish(value) => value.values.iter().all(|(_, reg)| f(*reg)),
            Self::RayQuery { origin, direction, minimum, maximum, .. } =>
                origin.iter().chain(direction).all(|reg| f(*reg)) && f(*minimum) && f(*maximum),
            Self::LoadIndexed { source, offset, .. } => source.all_operands(f) && f(*offset),
            Self::StoreIndexed { source, offset, value, .. } =>
                source.all_operands(f) && f(*offset) && f(*value),
            Self::StoreLocal(_, value) | Self::Print { value, .. } => f(*value),
            Self::LoadLocalIndexed { index, .. } | Self::Load { index, .. }
            | Self::TileLoad { index, .. } => f(*index),
            Self::StoreLocalIndexed { index, value, .. } | Self::Store { index, value, .. } =>
                f(*index) && f(*value),
            Self::Subgroup { value, argument, .. } =>
                value.iter().chain(argument).all(|reg| f(*reg)),
            Self::ViewLoad { address, index, .. } => address.iter().all(|reg| f(*reg)) && f(*index),
            Self::Unary { a, .. } | Self::Cast { src: a, .. } => f(*a),
            Self::IntegerDivide { src, divisor, .. } => f(*src) && match divisor {
                IntegerDivisor::Constant(_) => true,
                IntegerDivisor::Value(reg) => f(*reg),
            },
            Self::Binary { a, b, .. } => f(*a) && f(*b),
            Self::Math { args, .. } => args.iter().all(|reg| f(*reg)),
            Self::Select { condition, a, b, .. } => f(*condition) && f(*a) && f(*b),
            Self::If { condition, .. } => f(*condition),
            Self::Loop { test, .. } => f(*test),
            _ => true,
        };
        values && match self {
            Self::TextureSample { texture, .. } | Self::TextureFetch { texture, .. }
            | Self::TextureSize { texture, .. } | Self::TextureLevels { texture, .. }
            | Self::TextureAddress { texture, .. } | Self::TextureFormat { texture, .. }
            | Self::TextureRead { texture, .. } => texture.element.iter().all(|reg| f(*reg)),
            _ => true,
        }
    }
    pub(crate) fn local_reads(&self) -> Vec<usize> {
        match self {
            Self::Call(call) => call.locals(false),
            Self::LoadLocal(_, slot) => vec![*slot],
            Self::LoadLocalIndexed { base, len, .. } => (*base..*base + *len).collect(),
            _ => Vec::new(),
        }
    }
    pub(crate) fn local_writes(&self) -> Vec<usize> {
        match self {
            Self::Call(call) => call.locals(true),
            Self::StoreLocal(slot, _) => vec![*slot],
            Self::StoreLocalIndexed { base, len, .. } => (*base..*base + *len).collect(),
            _ => Vec::new(),
        }
    }
}
pub(crate) fn private_spans(code: &[Inst]) -> Vec<std::ops::Range<usize>> {
    fn collect(code: &[Inst], out: &mut BTreeSet<(usize, usize)>) {
        for inst in code {
            match inst {
                Inst::LoadLocalIndexed { base, len, .. }
                | Inst::StoreLocalIndexed { base, len, .. } => { out.insert((*base, *base + *len)); }
                Inst::Call(call) => {
                    for arg in &call.arguments {
                        if let CallArgument::Reference { address, .. } = arg
                            && let Some(range) = address.local_range()
                        {
                            out.insert((range.start, range.end));
                        }
                    }
                }
                Inst::Scope(body) => collect(body, out),
                Inst::If { yes, no, .. } => {
                    collect(yes, out);
                    collect(no, out);
                }
                Inst::Loop {
                    condition, body, ..
                } => {
                    collect(condition, out);
                    collect(body, out);
                }
                _ => {}
            }
        }
    }
    let mut ranges = BTreeSet::new();
    collect(code, &mut ranges);
    ranges.into_iter().map(|(begin, end)| begin..end).collect()
}
