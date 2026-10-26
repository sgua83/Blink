//! 发布点切分块内阶段；私有值与共享 word 分别按实际消费者安排生命周期。
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SharedWord {
    Input { binding: usize, word: u32 },
    Published { tile: usize, word: u32 },
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
    pub fn input_words(&self) -> BTreeSet<(usize, u32)> {
        self.storage_words()
            .into_iter()
            .filter_map(|word| match word {
                SharedWord::Input { binding, word } => Some((binding, word)),
                SharedWord::Published { .. } => None,
            })
            .collect()
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
    fn tile_layout(&self) -> BTreeMap<SharedWord, usize> {
        fn loads(stage: &Stage, code: &[Inst], phase: i64, last: &mut BTreeMap<SharedWord, i64>) {
            for inst in code {
                match inst {
                    Inst::Atomic(Atomic {
                        target: AtomicTarget::Tile(tile),
                        word,
                        offset: None,
                        ..
                    }) => {
                        last.insert(stage.shared_word(*tile, *word), phase);
                    }
                    Inst::LoadIndexed {
                        source: IndexedSource::Tile { tile, .. },
                        ..
                    }
                    | Inst::Atomic(Atomic {
                        target: AtomicTarget::Tile(tile),
                        offset: Some(_),
                        ..
                    }) => {
                        for word in 0..stage.tiles[*tile].ty.size / 4 {
                            last.insert(stage.shared_word(*tile, word), phase);
                        }
                    }
                    Inst::TileLoad { tile, word, .. } => {
                        last.insert(stage.shared_word(*tile, *word), phase);
                    }
                    Inst::Scope(body) => loads(stage, body, phase, last),
                    Inst::If { yes, no, .. } => {
                        loads(stage, yes, phase, last);
                        loads(stage, no, phase, last);
                    }
                    Inst::Loop {
                        condition, body, ..
                    } => {
                        loads(stage, condition, phase, last);
                        loads(stage, body, phase, last);
                    }
                    _ => {}
                }
            }
        }
        let phases = self.tile_phases();
        let mut last = BTreeMap::new();
        let mut births = BTreeMap::new();
        for (phase, p) in phases.iter().enumerate() {
            loads(self, p.code, phase as i64, &mut last);
            match p.step {
                Some(TileStep::Publish(publication)) => {
                    births.insert(publication.tile, phase as i64);
                }
                Some(TileStep::Collective(c)) => {
                    for (word, _) in &c.results {
                        last.insert(self.shared_word(c.tile, *word), phase as i64);
                    }
                }
                Some(TileStep::Barrier) | None => {}
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
        let mut groups: Vec<Vec<SharedWord>> = Vec::new();
        fn indexed_groups(stage: &Stage, code: &[Inst], groups: &mut Vec<Vec<SharedWord>>) {
            for inst in code {
                match inst {
                    Inst::LoadIndexed {
                        source: IndexedSource::Tile { tile, .. },
                        ..
                    }
                    | Inst::Atomic(Atomic {
                        target: AtomicTarget::Tile(tile),
                        offset: Some(_),
                        ..
                    }) => {
                        let mut group: Vec<_> = (0..stage.tiles[*tile].ty.size / 4)
                            .map(|w| stage.shared_word(*tile, w))
                            .collect();
                        let mut i = 0;
                        while i < groups.len() {
                            if groups[i].iter().any(|w| group.contains(w)) {
                                group.extend(groups.remove(i));
                            } else {
                                i += 1;
                            }
                        }
                        group.sort_unstable();
                        group.dedup();
                        groups.push(group);
                    }
                    Inst::Scope(body) => indexed_groups(stage, body, groups),
                    Inst::If { yes, no, .. } => {
                        indexed_groups(stage, yes, groups);
                        indexed_groups(stage, no, groups);
                    }
                    Inst::Loop {
                        condition, body, ..
                    } => {
                        indexed_groups(stage, condition, groups);
                        indexed_groups(stage, body, groups);
                    }
                    _ => {}
                }
            }
        }
        indexed_groups(self, &self.code, &mut groups);
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
        self.tile_layout()[&word] as u64 * self.workgroup_width() as u64 * 4
    }
    pub fn tile_data_bytes(&self) -> u64 {
        self.tile_layout().values().max().map_or(0, |slot| {
            (*slot as u64 + 1) * self.workgroup_width() as u64 * 4
        })
    }
    pub fn tile_scratch_bytes(&self) -> u64 {
        let streams = self
            .code
            .iter()
            .filter_map(|i| match i {
                Inst::TileCollective(c) if !c.results.is_empty() => {
                    Some(if c.op.operator() == CollectiveOperator::Sum {
                        1
                    } else {
                        c.results.len()
                    })
                }
                _ => None,
            })
            .max()
            .unwrap_or(0);
        (self.workgroup_width() as u64).next_power_of_two() * 4 * streams as u64
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
    let mut header = BTreeSet::new();
    loop {
        let mut need = live_before(body, header.clone(), after, &header, exit_live);
        need.extend(after.iter().copied());
        need.insert(PrivateSlot::Register(test));
        let next = live_before(condition, need, break_live, continue_live, exit_live);
        if next == header {
            return header;
        }
        header = next;
    }
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
                live.extend(inst.operands().into_iter().map(Register));
                live.extend(inst.local_reads().into_iter().map(Local));
            }
        }
    }
    live
}

impl Inst {
    pub(crate) fn results(&self) -> Vec<Reg> {
        match self {
            Self::TextureSample { result, .. } | Self::TextureFetch { result, .. } => {
                result.to_vec()
            }
            Self::TextureSize { result, .. } => result.to_vec(),
            Self::TextureLevels { dst, .. } => vec![*dst],
            Self::Append(a) => vec![a.accepted],
            Self::Atomic(a) => a.result.iter().copied().collect(),
            Self::Constant(r, ..)
            | Self::Index(r)
            | Self::Uniform(r, _)
            | Self::LoadLocal(r, _)
            | Self::TileLength(r)
            | Self::TileLane(r) => vec![*r],
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
            | Self::Select { dst, .. } => vec![*dst],
            Self::RayQuery { result, .. } => result.registers().to_vec(),
            Self::IntegerDivide { result, .. } => result.outputs().map(|(_, r)| r).collect(),
            Self::TileCollective(c) => c.results.iter().map(|(_, r)| *r).collect(),
            _ => Vec::new(),
        }
    }
    pub(crate) fn operands(&self) -> Vec<Reg> {
        let mut values = match self {
            Self::TextureSample {
                coordinate, lod, ..
            } => vec![coordinate[0], coordinate[1], *lod],
            Self::TextureFetch {
                coordinate, level, ..
            } => vec![coordinate[0], coordinate[1], *level],
            Self::TextureSize { level, .. } => vec![*level],
            Self::Append(a) => a.values.iter().map(|(_, r)| *r).collect(),
            Self::Atomic(a) => {
                let mut values = a.op.operands();
                values.push(a.index);
                values.extend(a.offset);
                values
            }
            Self::TilePublish(p) => p.values.iter().map(|(_, r)| *r).collect(),
            Self::RayQuery {
                origin,
                direction,
                minimum,
                maximum,
                ..
            } => {
                let mut r = origin.to_vec();
                r.extend(direction);
                r.extend([*minimum, *maximum]);
                r
            }
            Self::LoadIndexed { source, offset, .. } => {
                let mut r = source.operands();
                r.push(*offset);
                r
            }
            Self::StoreIndexed {
                source,
                offset,
                value,
                ..
            } => {
                let mut r = source.operands();
                r.extend([*offset, *value]);
                r
            }
            Self::StoreLocal(_, r) => vec![*r],
            Self::LoadLocalIndexed { index, .. } => vec![*index],
            Self::StoreLocalIndexed { index, value, .. } => vec![*index, *value],
            Self::Subgroup {
                value, argument, ..
            } => value.iter().chain(argument).copied().collect(),
            Self::Print { value, .. } => vec![*value],
            Self::ViewLoad { address, index, .. } => vec![address[0], address[1], *index],
            Self::Load { index, .. } | Self::TileLoad { index, .. } => vec![*index],
            Self::Store { index, value, .. } => vec![*index, *value],
            Self::Unary { a, .. } | Self::Cast { src: a, .. } => vec![*a],
            Self::IntegerDivide { src, divisor, .. } => match divisor {
                IntegerDivisor::Constant(_) => vec![*src],
                IntegerDivisor::Value(r) => vec![*src, *r],
            },
            Self::Binary { a, b, .. } => vec![*a, *b],
            Self::Math { args, .. } => args.clone(),
            Self::Select {
                condition, a, b, ..
            } => vec![*condition, *a, *b],
            Self::If { condition, .. } => vec![*condition],
            Self::Loop { test, .. } => vec![*test],
            _ => Vec::new(),
        };
        if let Self::TextureSample { texture, .. }
        | Self::TextureFetch { texture, .. }
        | Self::TextureSize { texture, .. }
        | Self::TextureLevels { texture, .. } = self
        {
            values.extend(texture.element);
        }
        values
    }
    pub(crate) fn local_reads(&self) -> Vec<usize> {
        match self {
            Self::LoadLocal(_, slot) => vec![*slot],
            Self::LoadLocalIndexed { base, len, .. } => (*base..*base + *len).collect(),
            _ => Vec::new(),
        }
    }
    pub(crate) fn local_writes(&self) -> Vec<usize> {
        match self {
            Self::StoreLocal(slot, _) => vec![*slot],
            Self::StoreLocalIndexed { base, len, .. } => (*base..*base + *len).collect(),
            _ => Vec::new(),
        }
    }
}
pub(crate) fn private_spans(code: &[Inst]) -> Vec<std::ops::Range<usize>> {
    fn collect(code: &[Inst], out: &mut Vec<std::ops::Range<usize>>) {
        for inst in code {
            match inst {
                Inst::LoadLocalIndexed { base, len, .. }
                | Inst::StoreLocalIndexed { base, len, .. } => out.push(*base..*base + *len),
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
    let mut ranges = Vec::new();
    collect(code, &mut ranges);
    ranges.sort_unstable_by_key(|r| (r.start, r.end));
    let mut result: Vec<std::ops::Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = result.last_mut()
            && range.start < last.end
        {
            last.end = last.end.max(range.end);
        } else {
            result.push(range);
        }
    }
    result
}
