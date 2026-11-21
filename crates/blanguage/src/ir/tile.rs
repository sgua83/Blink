//! 发布点切分块内阶段；私有值与共享 word 分别按实际消费者安排生命周期。
use super::*;
#[cfg(feature = "compiler")]
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

#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PrivateSlot {
    Register(Reg),
    Local(usize),
}
#[cfg(feature = "compiler")]
pub(crate) struct TilePhase<'a> {
    pub code: &'a [Inst],
    pub step: Option<&'a Inst>,
    pub live_in: BTreeSet<PrivateSlot>,
    pub saved: BTreeSet<PrivateSlot>,
}
impl Stage {
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
    #[cfg(feature = "compiler")]
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
                    saved: BTreeSet::new(),
                });
                begin = i + 1;
            }
        }
        if phases.is_empty() {
            return phases;
        }
        phases.push(TilePhase {
            code: &self.code[begin..],
            step: None,
            live_in: BTreeSet::new(),
            saved: BTreeSet::new(),
        });
        let empty = BTreeSet::new();
        let mut live = BTreeSet::new();
        for phase in phases.iter_mut().rev() {
            if let Some(inst) = phase.step {
                private_live::phase_step(inst, &mut live, &mut phase.saved);
            }
            live = live_before(phase.code, live, &empty, &empty, &empty);
            phase.live_in = live.clone();
        }
        phases
    }
    #[cfg(feature = "compiler")]
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
    pub(crate) fn tile_layout(
        &self, wide: bool, used: Option<&mut BTreeSet<SharedWord>>,
    ) -> dataflow::TileLayout {
        if self.tiles.is_empty() {
            return BTreeMap::new();
        }
        let dataflow::LayoutFacts { mut last, births, mut groups } = dataflow::tile_layout_facts(self, used);
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
        self.tile_layout(false, None).values().map(|(_, slot, _)| slot).max().map_or(0, |slot| {
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

impl Inst {
    #[cfg(feature = "compiler")]
    pub(crate) fn results(&self) -> impl Iterator<Item = Reg> {
        super::dataflow::results(self)
    }
    #[cfg(not(feature = "compiler"))]
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
    #[cfg(feature = "compiler")]
    pub(crate) fn all_operands(&self, f: &mut impl FnMut(Reg) -> bool) -> bool {
        super::dataflow::instruction_operands(self, f)
    }
    #[cfg(not(feature = "compiler"))]
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
    pub(crate) fn local_reads(&self) -> impl Iterator<Item = usize> + '_ {
        self.local_slots(false)
    }
    pub(crate) fn local_writes(&self) -> impl Iterator<Item = usize> + '_ {
        self.local_slots(true)
    }
    #[cfg(feature = "compiler")]
    fn local_slots(&self, writing: bool) -> impl Iterator<Item = usize> + '_ {
        super::dataflow::instruction_locals(self, writing)
    }
    #[cfg(not(feature = "compiler"))]
    fn local_slots(&self, writing: bool) -> impl Iterator<Item = usize> + '_ {
        let (slot, range) = match (&self.kind, writing) {
            (InstKind::LoadLocal(_, slot), false) | (InstKind::StoreLocal(slot, _), true) =>
                (Some(*slot), None),
            (InstKind::LoadLocalIndexed { base, len, .. }, false)
            | (InstKind::StoreLocalIndexed { base, len, .. }, true) =>
                (None, Some(*base..*base + *len)),
            _ => (None, None),
        };
        let call = match &self.kind { InstKind::Call(call) => Some(call), _ => None };
        let indexed = match (&self.kind, writing) {
            (InstKind::LoadIndexed { source, .. }, false)
            | (InstKind::StoreIndexed { source, .. }, true) => Some(source),
            _ => None,
        };
        let atomic = match &self.kind {
            InstKind::Atomic(a) if if writing { a.op.writes() } else { a.op.reads() } => Some(&a.target),
            _ => None,
        };
        slot.into_iter().chain(range.into_iter().flatten())
            .chain(call.into_iter().flat_map(move |call| call.locals(writing)))
            .chain(indexed.into_iter().flat_map(|source| source.aliases())
                .filter_map(MemoryAlias::local_range).flatten())
            .chain(atomic.into_iter().flat_map(|target| target.aliases())
                .filter_map(MemoryAlias::local_range).flatten())
    }
}
