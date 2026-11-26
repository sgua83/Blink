//! 发布点切分块内阶段；私有值与共享 word 分别按实际消费者安排生命周期。
use super::*;
#[cfg(feature = "compiler")]
use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "compiler")]
mod private_live;
#[cfg(feature = "compiler")]
pub(crate) use private_live::{HOST as PRIVATE_LIVE_HOST, Host as PrivateLiveHost};

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
    first: usize,
    count: usize,
    width: usize,
    plane: usize,
}

#[cfg(feature = "compiler")]
impl TilePlacement {
    fn members<'a>(&self, words: &'a [TileMember]) -> &'a [TileMember] {
        &words[self.first..self.first + self.count]
    }
}

#[cfg(feature = "compiler")]
#[repr(C)]
struct TileMember {
    storage: u64,
    owner: usize,
    byte: u64,
    slot: usize,
}

#[cfg(feature = "compiler")]
impl From<SharedWord> for TileMember {
    fn from(word: SharedWord) -> Self {
        let (storage, owner, byte) = match word {
            SharedWord::Input { binding, word } => (0, binding, word),
            SharedWord::Published { tile, word } => (1, tile, word),
        };
        Self { storage, owner, byte: u64::from(byte), slot: 0 }
    }
}

#[cfg(feature = "compiler")]
impl TileMember {
    fn key(&self) -> SharedWord {
        let word = self.byte as u32;
        if self.storage == 0 {
            SharedWord::Input { binding: self.owner, word }
        } else {
            SharedWord::Published { tile: self.owner, word }
        }
    }
}

#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub(crate) struct TileExtents {
    pub(crate) narrow: usize,
    pub(crate) wide: usize,
}

#[cfg(feature = "compiler")]
pub(crate) struct PlacedTiles {
    pub(crate) words: dataflow::TileLayout,
    pub(crate) extents: TileExtents,
}

#[cfg(feature = "compiler")]
#[derive(Default)]
struct TilePlan {
    rows: Vec<TilePlacement>,
    words: Vec<TileMember>,
    cells: BTreeMap<SharedWord, (Scalar, SharedWord, u32)>,
    extents: TileExtents,
}

#[cfg(feature = "compiler")]
#[repr(C)]
struct TilePlanes { narrow: *mut i64, wide: *mut i64 }

#[cfg(feature = "compiler")]
type TileReserve = unsafe extern "C" fn(*mut std::ffi::c_void, usize, usize) -> TilePlanes;

#[cfg(feature = "compiler")]
unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::place_tile"]
    fn place_tile(rows: *mut TilePlacement, count: usize, words: *mut TileMember,
        context: *mut std::ffi::c_void, reserve: TileReserve) -> TileExtents;
}

#[cfg(feature = "compiler")]
unsafe extern "C" fn reserve_tile_planes(context: *mut std::ffi::c_void, narrow: usize, wide: usize) -> TilePlanes {
    let planes = unsafe { &mut *context.cast::<[Vec<i64>; 2]>() };
    *planes = [Vec::with_capacity(narrow), Vec::with_capacity(wide)];
    TilePlanes { narrow: planes[0].as_mut_ptr(), wide: planes[1].as_mut_ptr() }
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
#[cfg(feature = "compiler")]
impl<'a> TilePhase<'a> {
    pub(crate) fn from_code(code: &'a [Inst]) -> Vec<Self> {
        private_live::tile_phases(code)
    }
}

#[cfg(feature = "compiler")]
impl PrivateSlot {
    pub(crate) fn scalar<'a>(
        self, registers: &'a [Scalar], locals: &'a BTreeMap<usize, Ty>,
    ) -> &'a Scalar {
        match self {
            Self::Register(reg) => &registers[reg],
            Self::Local(slot) => {
                let (end, ty) = locals.range(slot + 1..).next().unwrap();
                &ty.components[slot + ty.components.len() - end].0
            }
        }
    }
}

impl Tile {
    pub(crate) fn shared_word(&self, tile: usize, word: u32) -> SharedWord {
        match self.source {
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
}
impl Stage {
    pub fn shared_word(&self, tile: usize, word: u32) -> SharedWord {
        self.tiles[tile].shared_word(tile, word)
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn tile_phases(&self) -> Vec<TilePhase<'_>> {
        TilePhase::from_code(&self.code)
    }
    // (实际单元类型，所属类型平面的槽，单元内字节)；CPU继续使用原u32字流。
    #[cfg(feature = "compiler")]
    pub(crate) fn tile_layout(
        &self, wide: bool, used: Option<&mut BTreeSet<SharedWord>>,
    ) -> PlacedTiles {
        let TilePlan { rows, words, cells, extents } = self.tile_placement(wide, used);
        let mut layout = BTreeMap::new();
        for row in rows {
            let unit = [Scalar::U32, Scalar::U64][row.plane];
            for member in row.members(&words) {
                let key = member.key();
                if wide {
                    // A cell belongs to this key at its low word or the next word.
                    for (&part, &(_, canonical, byte)) in cells.range(key..).take(2) {
                        if canonical == key {
                            layout.insert(part, (unit, member.slot, byte));
                        }
                    }
                } else {
                    layout.insert(key, (unit, member.slot, 0));
                }
            }
        }
        PlacedTiles { words: layout, extents }
    }

    #[cfg(feature = "compiler")]
    fn tile_placement(&self, wide: bool, used: Option<&mut BTreeSet<SharedWord>>) -> TilePlan {
        if self.tiles.is_empty() {
            return TilePlan::default();
        }
        let dataflow::LayoutFacts { mut last, births, groups } = dataflow::tile_layout_facts(self, used);
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
        }
        let groups: Vec<Vec<SharedWord>> = if wide {
            let cells = &cells;
            groups.into_iter().flat_map(|group| {
                [Scalar::U32, Scalar::U64].into_iter().filter_map(move |unit| {
                    let words: BTreeSet<_> = group.words().filter_map(|word| cells.get(&word))
                        .filter(|(kind, _, _)| *kind == unit).map(|(_, word, _)| *word).collect();
                    (!words.is_empty()).then(|| words.into_iter().collect())
                })
            }).collect()
        } else {
            groups.into_iter().map(|group| group.words().collect()).collect()
        };
        let birth = |word: SharedWord| match word {
            SharedWord::Input { .. } => -1,
            SharedWord::Published { tile, .. } => births[&tile],
        };
        let mut rows = Vec::new();
        let mut words = Vec::new();
        let mut place = |begin, group: &[SharedWord], end| {
            let plane = usize::from(wide && cells[&group[0]].0 == Scalar::U64);
            let first = words.len();
            words.extend(group.iter().copied().map(TileMember::from));
            rows.push(TilePlacement { begin, end, first, count: group.len(), width: 0, plane });
        };
        for group in groups {
            let (begin, end) = group.iter().filter_map(|word| {
                last.remove(word).map(|end| (birth(*word), end))
            }).reduce(|(begin, end), (next_begin, next_end)| {
                (begin.min(next_begin), end.max(next_end))
            }).unwrap();
            place(begin, &group, end);
        }
        for (word, end) in last {
            place(birth(word), std::slice::from_ref(&word), end);
        }
        rows.sort_unstable_by(|left, right| {
            left.begin.cmp(&right.begin).then_with(|| {
                left.members(&words).iter().map(TileMember::key)
                    .cmp(right.members(&words).iter().map(TileMember::key))
            }).then_with(|| left.end.cmp(&right.end))
        });
        let mut planes = [Vec::<i64>::new(), Vec::new()];
        let extents = unsafe {
            place_tile(rows.as_mut_ptr(), rows.len(), words.as_mut_ptr(),
                std::ptr::from_mut(&mut planes).cast(), reserve_tile_planes)
        };
        TilePlan { rows, words, cells, extents }
    }

    #[cfg(feature = "compiler")]
    pub fn tile_data_bytes(&self) -> u64 {
        let words = self.tile_placement(false, None).extents.narrow;
        words as u64 * u64::from(self.workgroup_width()) * 4
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
