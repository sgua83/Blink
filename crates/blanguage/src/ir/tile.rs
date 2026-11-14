//! 发布点切分块内阶段；私有值与共享 word 分别按实际消费者安排生命周期。
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "compiler")]
mod private_live;

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
#[cfg(feature = "compiler")]
#[repr(C)]
struct TilePlacement {
    begin: i64,
    end: i64,
    width: usize,
    plane: usize,
    slot: usize,
}

#[cfg(feature = "compiler")]
unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::place_tile"]
    fn place_tile(rows: *mut TilePlacement, count: usize, narrow: *mut i64, wide: *mut i64);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PrivateSlot {
    Register(Reg),
    Local(usize),
}
pub(crate) struct TilePhase<'a> {
    pub code: &'a [Inst],
    pub step: Option<&'a Inst>,
    pub live_in: BTreeSet<PrivateSlot>,
    pub live_out: BTreeSet<PrivateSlot>,
}
impl TilePhase<'_> {
    pub fn preserved_out(&self) -> BTreeSet<PrivateSlot> {
        let mut slots = self.live_out.clone();
        if let Some(inst) = self.step {
            for result in inst.results() {
                slots.remove(&PrivateSlot::Register(result));
            }
        }
        slots
    }
}
impl Stage {
    pub(crate) fn has_tile_steps(&self) -> bool {
        self.code.iter().any(|i| {
            matches!(
                &(i).kind,
                InstKind::TilePublish(_) | InstKind::TileCollective(_) | InstKind::TileBarrier
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
            let step = matches!(&inst.kind,
                InstKind::TilePublish(_) | InstKind::TileCollective(_) | InstKind::TileBarrier).then_some(inst);
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
            if let Some(inst) = phase.step {
                live = live_before(std::slice::from_ref(inst), live,
                    &BTreeSet::new(), &BTreeSet::new(), &BTreeSet::new());
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
    pub(crate) fn private_scalar(&self, slot: PrivateSlot) -> Scalar {
        match slot {
            PrivateSlot::Register(r) => self.registers[r],
            PrivateSlot::Local(s) => {
                let (end, ty) = self.locals.range(s + 1..).next().unwrap();
                ty.components[s + ty.components.len() - end].0
            }
        }
    }
    // (实际单元类型，所属类型平面的槽，单元内字节)；CPU继续使用原u32字流。
    #[cfg(feature = "compiler")]
    pub(crate) fn tile_layout(&self, wide: bool) -> BTreeMap<SharedWord, (Scalar, usize, u32)> {
        fn loads(stage: &Stage, code: &[Inst], phase: i64, last: &mut BTreeMap<SharedWord, i64>, groups: &mut Vec<Vec<SharedWord>>) {
            for inst in code {
                match &(inst).kind {
                    InstKind::Atomic(Atomic {
                        target: IndexedSource::Tile { tile, .. },
                        word, scalar,
                        offset: None,
                        ..
                    }) => {
                        for part in 0..scalar.size().div_ceil(4) {
                            last.insert(stage.shared_word(*tile, *word + part * 4).aligned(), phase);
                        }
                    }
                    InstKind::LoadIndexed {
                        source: IndexedSource::Tile { tile, .. },
                        ..
                    }
                    | InstKind::Atomic(Atomic {
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
                    InstKind::TileLoad { dst, tile, word, .. } => {
                        for part in 0..stage.registers[*dst].size().div_ceil(4) {
                            last.insert(stage.shared_word(*tile, *word + part * 4).aligned(), phase);
                        }
                    }
                    InstKind::Scope(body) => loads(stage, body, phase, last, groups),
                    InstKind::If { yes, no, .. } => {
                        loads(stage, yes, phase, last, groups);
                        loads(stage, no, phase, last, groups);
                    }
                    InstKind::Loop {
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
            match &(inst).kind {
                InstKind::TilePublish(publication) => {
                    births.insert(publication.tile, phase as i64);
                    phase += 1;
                }
                InstKind::TileCollective(c) => {
                    for (word, result) in &c.results {
                        for part in 0..self.registers[*result].size().div_ceil(4) {
                            last.insert(self.shared_word(c.tile, *word + part * 4).aligned(), phase as i64);
                        }
                    }
                    phase += 1;
                }
                InstKind::TileBarrier => phase += 1,
                _ => loads(self, std::slice::from_ref(inst), phase as i64, &mut last, &mut groups),
            }
        }
        let mut cells = BTreeMap::new();
        if wide {
            for (tile, data) in self.tiles.iter().enumerate() {
                for (scalar, byte) in &data.ty.components {
                    let word = self.shared_word(tile, *byte).aligned();
                    let unit = if scalar.size() == 8 { Scalar::U64 } else { Scalar::U32 };
                    cells.insert(word, (unit, word, 0));
                    if unit == Scalar::U64 {
                        cells.insert(self.shared_word(tile, *byte + 4).aligned(), (unit, word, 4));
                    }
                }
            }
            let mut canonical = BTreeMap::<_, i64>::new();
            for (word, end) in last {
                canonical.entry(cells[&word].1).and_modify(|old| *old = (*old).max(end)).or_insert(end);
            }
            last = canonical;
            let cells = &cells;
            groups = groups.into_iter().flat_map(|group| {
                [Scalar::U32, Scalar::U64].into_iter().filter_map(move |unit| {
                    let words: BTreeSet<_> = group.iter().filter_map(|word| cells.get(word))
                        .filter(|(kind, _, _)| *kind == unit).map(|(_, word, _)| *word).collect();
                    (!words.is_empty()).then(|| words.into_iter().collect())
                })
            }).collect();
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
        let byte = |word: SharedWord| match word {
            SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word,
        };
        let mut capacity = [0; 2];
        let mut placements: Vec<_> = units.iter().map(|(begin, group, end)| {
            let unit = if wide { cells[&group[0]].0 } else { Scalar::U32 };
            let width = ((byte(*group.last().unwrap()) - byte(group[0])) / unit.size() + 1) as usize;
            let plane = usize::from(unit == Scalar::U64);
            capacity[plane] += width;
            TilePlacement { begin: *begin, end: *end, width, plane, slot: 0 }
        }).collect();
        let mut narrow = Vec::<i64>::with_capacity(capacity[0]);
        let mut wide_slots = Vec::<i64>::with_capacity(capacity[1]);
        // B initializes each plane as it assigns rows. A row adds at most its
        // width to the occupied extent, so the sum of widths bounds all writes.
        unsafe {
            place_tile(placements.as_mut_ptr(), placements.len(), narrow.as_mut_ptr(), wide_slots.as_mut_ptr());
        }
        let mut layout = BTreeMap::new();
        for ((_, group, _), placement) in units.into_iter().zip(placements) {
            let unit = [Scalar::U32, Scalar::U64][placement.plane];
            let first = byte(group[0]);
            for word in group {
                layout.insert(word, (unit, placement.slot + ((byte(word) - first) / unit.size()) as usize, 0));
            }
        }
        if wide {
            cells.into_iter().filter_map(|(word, (_, canonical, byte))| {
                layout.get(&canonical).map(|(unit, slot, _)| (word, (*unit, *slot, byte)))
            }).collect()
        } else {
            layout
        }
    }
    #[cfg(feature = "compiler")]
    pub fn tile_data_bytes(&self) -> u64 {
        self.tile_layout(false).values().map(|(_, slot, _)| slot).max().map_or(0, |slot| {
            (*slot as u64 + 1) * self.workgroup_width() as u64 * 4
        })
    }
    pub fn tile_scratch_bytes(&self) -> u64 {
        let bytes_per_lane = self
            .code
            .iter()
            .filter_map(|i| match &(i).kind {
                InstKind::TileCollective(c) if !c.results.is_empty() => {
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
    #[cfg(feature = "compiler")]
    pub fn tile_bytes(&self) -> u64 {
        self.tile_data_bytes() + self.tile_scratch_bytes()
    }
}

#[cfg(feature = "compiler")]
pub(crate) fn loop_live(
    condition: &[Inst], test: Reg, body: &[Inst], after: &BTreeSet<PrivateSlot>,
    break_live: &BTreeSet<PrivateSlot>, continue_live: &BTreeSet<PrivateSlot>, exit_live: &BTreeSet<PrivateSlot>,
) -> BTreeSet<PrivateSlot> {
    private_live::loop_header(condition, test, body, after, break_live, continue_live, exit_live)
}
#[cfg(feature = "compiler")]
pub(crate) fn live_before(
    code: &[Inst], live: BTreeSet<PrivateSlot>, break_live: &BTreeSet<PrivateSlot>,
    continue_live: &BTreeSet<PrivateSlot>, exit_live: &BTreeSet<PrivateSlot>,
) -> BTreeSet<PrivateSlot> {
    private_live::before(code, live, break_live, continue_live, exit_live)
}

#[cfg(not(feature = "compiler"))]
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

#[cfg(not(feature = "compiler"))]
pub(crate) fn live_before(
    code: &[Inst],
    mut live: BTreeSet<PrivateSlot>,
    break_live: &BTreeSet<PrivateSlot>,
    continue_live: &BTreeSet<PrivateSlot>,
    exit_live: &BTreeSet<PrivateSlot>,
) -> BTreeSet<PrivateSlot> {
    use PrivateSlot::{Local, Register};
    for inst in code.iter().rev() {
        match &(inst).kind {
            InstKind::If { condition, yes, no } => {
                let a = live_before(yes, live.clone(), break_live, continue_live, exit_live);
                live = live_before(no, live, break_live, continue_live, exit_live);
                live.extend(a);
                live.insert(Register(*condition));
            }
            InstKind::Loop {
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
            InstKind::Scope(body) => {
                live = live_before(body, live.clone(), break_live, continue_live, &live)
            }
            InstKind::Exit => live = exit_live.clone(),
            InstKind::Break => live = break_live.clone(),
            InstKind::Continue => live = continue_live.clone(),
            InstKind::LoadLocal(dst, slot) => {
                live.remove(&Register(*dst));
                live.insert(Local(*slot));
            }
            InstKind::StoreLocal(slot, src) => {
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
        let values: &[Reg] = match &self.kind {
            InstKind::Call(call) => &call.values,
            InstKind::TextureSample { result, .. } | InstKind::TextureFetch { result, .. } => {
                result
            }
            InstKind::TextureSize { result, .. } => result,
            InstKind::TextureBindingAddress { dst, .. }
            | InstKind::TextureLevels { dst, .. }
            | InstKind::TextureAddress { dst, .. }
            | InstKind::TextureFormat { dst, .. }
            | InstKind::TextureRead { dst, .. } => std::slice::from_ref(dst),
            InstKind::Append(a) => std::slice::from_ref(&a.accepted),
            InstKind::Atomic(a) => a.result.as_slice(),
            InstKind::Constant(r, ..)
            | InstKind::FunctionAddress(r, _)
            | InstKind::Index(r)
            | InstKind::Uniform(r, _)
            | InstKind::BufferStride(r, _)
            | InstKind::LoadLocal(r, _)
            | InstKind::TileLength(r)
            | InstKind::TileLane(r) => std::slice::from_ref(r),
            InstKind::PositionAddress { dst, .. }
            | InstKind::LoadIndexed { dst, .. }
            | InstKind::Load { dst, .. }
            | InstKind::LoadLocalIndexed { dst, .. }
            | InstKind::Subgroup { dst, .. }
            | InstKind::Print { dst, .. }
            | InstKind::TileLoad { dst, .. }
            | InstKind::ViewLoad { dst, .. }
            | InstKind::Unary { dst, .. }
            | InstKind::Binary { dst, .. }
            | InstKind::Cast { dst, .. }
            | InstKind::Math { dst, .. }
            | InstKind::Select { dst, .. } => std::slice::from_ref(dst),
            InstKind::RayQuery { result, .. } => result.registers(),
            InstKind::IntegerDivide { result, .. } => {
                division = [result.quotient(), result.remainder()];
                &[]
            },
            InstKind::TileCollective(c) => {
                collective = &c.results;
                &[]
            },
            _ => &[],
        };
        values.iter().copied()
            .chain(if let InstKind::Call(call)=&self.kind{call.error}else{None})
            .chain(division.into_iter().flatten())
            .chain(collective.iter().map(|(_, reg)| *reg))
    }
    pub(crate) fn all_operands(&self, f: &mut impl FnMut(Reg) -> bool) -> bool {
        let values = match &self.kind {
            InstKind::Call(call) => call.all_operands(f),
            InstKind::Raise(error) => f(*error),
            InstKind::FunctionAddress(_, CallTarget::Indirect(reg)) => f(*reg),
            InstKind::PositionAddress { address, .. } => address.all_operands(f),
            InstKind::TextureSample { coordinate, lod, .. } =>
                coordinate.iter().all(|reg| f(*reg)) && f(*lod),
            InstKind::TextureFetch { coordinate, level, .. }
            | InstKind::TextureRead { coordinate, level, .. } =>
                coordinate.iter().all(|reg| f(*reg)) && f(*level),
            InstKind::TextureSize { level, .. } => f(*level),
            InstKind::Append(value) => value.values.iter().all(|(_, reg)| f(*reg)),
            InstKind::Atomic(value) => {
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
            InstKind::TilePublish(value) => value.values.iter().all(|(_, reg)| f(*reg)),
            InstKind::RayQuery { origin, direction, minimum, maximum, cull_back_faces, .. } =>
                origin.iter().chain(direction).all(|reg| f(*reg))
                    && f(*minimum) && f(*maximum) && cull_back_faces.iter().all(|reg| f(*reg)),
            InstKind::LoadIndexed { source, offset, .. } => source.all_operands(f) && f(*offset),
            InstKind::StoreIndexed { source, offset, value, .. } =>
                source.all_operands(f) && f(*offset) && f(*value),
            InstKind::StoreLocal(_, value) | InstKind::Print { value, .. } => f(*value),
            InstKind::LoadLocalIndexed { index, .. } | InstKind::Load { index, .. }
            | InstKind::TileLoad { index, .. } => f(*index),
            InstKind::StoreLocalIndexed { index, value, .. } | InstKind::Store { index, value, .. } =>
                f(*index) && f(*value),
            InstKind::Subgroup { value, argument, .. } =>
                value.iter().chain(argument).all(|reg| f(*reg)),
            InstKind::ViewLoad { address, index, stride, .. } => address.iter().all(|reg| f(*reg)) && stride.all_operands(f) && f(*index),
            InstKind::Unary { a, .. } | InstKind::Cast { src: a, .. } => f(*a),
            InstKind::IntegerDivide { src, divisor, .. } => f(*src) && match divisor {
                IntegerDivisor::Constant(_) => true,
                IntegerDivisor::Value(reg) => f(*reg),
            },
            InstKind::Binary { a, b, .. } => f(*a) && f(*b),
            InstKind::Math { args, .. } => args.iter().all(|reg| f(*reg)),
            InstKind::Select { condition, a, b, .. } => f(*condition) && f(*a) && f(*b),
            InstKind::If { condition, .. } => f(*condition),
            InstKind::Loop { test, .. } => f(*test),
            _ => true,
        };
        values && match &self.kind {
            InstKind::TextureBindingAddress { texture, .. }
            | InstKind::TextureSample { texture, .. } | InstKind::TextureFetch { texture, .. }
            | InstKind::TextureSize { texture, .. } | InstKind::TextureLevels { texture, .. }
            | InstKind::TextureAddress { texture, .. } | InstKind::TextureFormat { texture, .. }
            | InstKind::TextureRead { texture, .. } => texture.parameter.iter().chain(&texture.element).all(|reg| f(*reg)),
            _ => true,
        }
    }
    pub(crate) fn local_reads(&self) -> Vec<usize> {
        match &self.kind {
            InstKind::Call(call) => call.locals(false),
            InstKind::LoadLocal(_, slot) => vec![*slot],
            InstKind::LoadLocalIndexed { base, len, .. } => (*base..*base + *len).collect(),
            InstKind::LoadIndexed { source, .. } => source.aliases().filter_map(MemoryAlias::local_range).flatten().collect(),
            InstKind::Atomic(a) if a.op.reads() => a.target.aliases().filter_map(MemoryAlias::local_range).flatten().collect(),
            _ => Vec::new(),
        }
    }
    pub(crate) fn local_writes(&self) -> Vec<usize> {
        match &self.kind {
            InstKind::Call(call) => call.locals(true),
            InstKind::StoreLocal(slot, _) => vec![*slot],
            InstKind::StoreLocalIndexed { base, len, .. } => (*base..*base + *len).collect(),
            InstKind::StoreIndexed { source, .. } => source.aliases().filter_map(MemoryAlias::local_range).flatten().collect(),
            InstKind::Atomic(a) if a.op.writes() => a.target.aliases().filter_map(MemoryAlias::local_range).flatten().collect(),
            _ => Vec::new(),
        }
    }
}
pub(crate) fn private_spans(code: &[Inst]) -> Vec<std::ops::Range<usize>> {
    fn collect(code: &[Inst], out: &mut BTreeSet<(usize, usize)>) {
        for inst in code {
            match &(inst).kind {
                InstKind::LoadLocalIndexed { base, len, .. }
                | InstKind::StoreLocalIndexed { base, len, .. } => { out.insert((*base, *base + *len)); }
                InstKind::Call(call) => {
                    for arg in &call.arguments {
                        if let CallArgument::Reference { address, .. } = arg {
                            out.extend(address.local_ranges().map(|r| (r.start, r.end)));
                        }
                    }
                    for effect in &call.memory {
                        if let Some(range) = effect.alias.local_range() { out.insert((range.start, range.end)); }
                    }
                }
                InstKind::PositionAddress { address, .. } => {
                    out.extend(address.local_ranges().map(|r| (r.start, r.end)));
                }
                InstKind::LoadIndexed { source, .. } | InstKind::StoreIndexed { source, .. } => {
                    out.extend(source.aliases().filter_map(MemoryAlias::local_range).map(|r| (r.start, r.end)));
                }
                InstKind::Atomic(a) => {
                    out.extend(a.target.aliases().filter_map(MemoryAlias::local_range).map(|r| (r.start, r.end)));
                }
                InstKind::Scope(body) => collect(body, out),
                InstKind::If { yes, no, .. } => {
                    collect(yes, out);
                    collect(no, out);
                }
                InstKind::Loop {
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
