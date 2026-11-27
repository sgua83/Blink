//! 发布点切分块内阶段；私有值与共享 word 分别按实际消费者安排生命周期。
use super::*;
use std::ffi::c_void;
#[cfg(feature = "compiler")]
use std::{collections::{BTreeMap, BTreeSet}, mem::MaybeUninit};
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
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct TileKey { storage: u64, owner: usize, byte: u32 }

#[cfg(feature = "compiler")]
impl From<SharedWord> for TileKey {
    fn from(word: SharedWord) -> Self {
        match word {
            SharedWord::Input { binding, word } => Self { storage: 0, owner: binding, byte: word },
            SharedWord::Published { tile, word } => Self { storage: 1, owner: tile, byte: word },
        }
    }
}

#[cfg(feature = "compiler")]
impl TileKey {
    pub(crate) fn word(self) -> SharedWord {
        if self.storage == 0 {
            SharedWord::Input { binding: self.owner, word: self.byte }
        } else {
            SharedWord::Published { tile: self.owner, word: self.byte }
        }
    }
}

#[cfg(feature = "compiler")]
#[repr(C)]
struct TileUse { key: TileKey, end: i64 }

#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct TileCell { byte: u32, bytes: u32 }

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
#[allow(dead_code)]
#[repr(u32)]
enum TileScratch { Rows, Members, Narrow, Wide }

#[cfg(feature = "compiler")]
#[repr(C)]
struct TileLayoutInput {
    tiles: dataflow::Span,
    groups: dataflow::Span,
    uses: *mut TileUse,
    use_count: usize,
    wide: u32,
    publish: u32,
}

#[cfg(feature = "compiler")]
#[repr(C)]
struct TileLayoutHost {
    ir: dataflow::Host,
    context: *mut c_void,
    reserve: unsafe extern "C" fn(*mut c_void, TileScratch, usize) -> *mut c_void,
    describe: unsafe extern "C" fn(*mut c_void, TileKey, TileCell),
    cell: unsafe extern "C" fn(*const c_void, TileKey) -> TileCell,
    birth: unsafe extern "C" fn(*const c_void, usize) -> i64,
    publish: unsafe extern "C" fn(*mut c_void, TileKey, u32, usize, u32),
}

#[cfg(feature = "compiler")]
struct TileStorage<'a> {
    births: &'a BTreeMap<usize, i64>,
    cells: BTreeMap<SharedWord, TileCell>,
    scratch: [Vec<MaybeUninit<u64>>; 4],
    layout: Option<&'a mut dataflow::TileLayout>,
}

#[cfg(feature = "compiler")]
unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::place_tile"]
    fn place_tile(input: TileLayoutInput, host: TileLayoutHost) -> TileExtents;
}

#[cfg(feature = "compiler")]
unsafe extern "C" fn reserve_tile_storage(context: *mut c_void, bank: TileScratch, count: usize) -> *mut c_void {
    let storage = unsafe { &mut *context.cast::<TileStorage<'_>>() };
    let rows = &mut storage.scratch[bank as usize];
    rows.resize_with(count, MaybeUninit::uninit);
    rows.as_mut_ptr().cast()
}

#[cfg(feature = "compiler")]
unsafe extern "C" fn describe_tile_cell(context: *mut c_void, key: TileKey, cell: TileCell) {
    unsafe { &mut *context.cast::<TileStorage<'_>>() }.cells.insert(key.word(), cell);
}

#[cfg(feature = "compiler")]
unsafe extern "C" fn tile_cell(context: *const c_void, key: TileKey) -> TileCell {
    unsafe { &*context.cast::<TileStorage<'_>>() }.cells.get(&key.word()).copied().unwrap_or_default()
}

#[cfg(feature = "compiler")]
unsafe extern "C" fn tile_birth(context: *const c_void, tile: usize) -> i64 {
    unsafe { &*context.cast::<TileStorage<'_>>() }.births[&tile]
}

#[cfg(feature = "compiler")]
unsafe extern "C" fn publish_tile_word(context: *mut c_void, key: TileKey, bytes: u32, slot: usize, part: u32) {
    let storage = unsafe { &mut *context.cast::<TileStorage<'_>>() };
    let unit = if bytes == 8 { Scalar::U64 } else { Scalar::U32 };
    // The B planner publishes only when the caller supplies the final layout.
    unsafe { storage.layout.as_deref_mut().unwrap_unchecked() }.insert(key.word(), (unit, slot, part));
}


#[cfg(feature = "compiler")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub(crate) enum PrivateSlot {
    Register(Reg),
    Local(usize),
}
#[cfg(feature = "compiler")]
pub(crate) struct TilePhase<'a> {
    pub code: &'a [Inst],
    pub step: Option<&'a Inst>,
    pub live_in: Vec<PrivateSlot>,
    pub saved: Vec<PrivateSlot>,
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
        let mut words = BTreeMap::new();
        let extents = self.place_tile_layout(wide, used, Some(&mut words));
        PlacedTiles { words, extents }
    }

    #[cfg(feature = "compiler")]
    fn place_tile_layout(
        &self, wide: bool, used: Option<&mut BTreeSet<SharedWord>>,
        layout: Option<&mut dataflow::TileLayout>,
    ) -> TileExtents {
        if self.tiles.is_empty() {
            return TileExtents::default();
        }
        let dataflow::LayoutFacts { last, births, groups } = dataflow::tile_layout_facts(self, used);
        let mut uses: Vec<_> = last.into_iter().map(|(word, end)| TileUse { key: word.into(), end }).collect();
        let publish = u32::from(layout.is_some());
        let mut storage = TileStorage { births: &births, cells: BTreeMap::new(), scratch: Default::default(), layout };
        let input = TileLayoutInput {
            tiles: dataflow::Span::slice(&self.tiles), groups: dataflow::Span::slice(&groups),
            uses: uses.as_mut_ptr(), use_count: uses.len(), wide: u32::from(wide), publish,
        };
        unsafe {
            place_tile(input, TileLayoutHost {
                ir: dataflow::HOST, context: std::ptr::from_mut(&mut storage).cast(),
                reserve: reserve_tile_storage, describe: describe_tile_cell,
                cell: tile_cell, birth: tile_birth, publish: publish_tile_word,
            })
        }
    }


    #[cfg(feature = "compiler")]
    pub fn tile_data_bytes(&self) -> u64 {
        self.tile_data_bytes_for(self.workgroup_width())
    }
    #[cfg(feature = "compiler")]
    pub(super) fn tile_data_bytes_for(&self, width: u32) -> u64 {
        let words = self.place_tile_layout(false, None, None).narrow;
        words as u64 * u64::from(width) * 4
    }
    pub fn tile_scratch_bytes(&self) -> u64 {
        self.tile_scratch_bytes_for(self.workgroup_width())
    }
    pub(super) fn tile_scratch_bytes_for(&self, width: u32) -> u64 {
        let mut input = TileScratchInput { code: self.code.iter(), registers: &self.registers };
        let host = TileScratchHost { next: next_scratch_group, scalar_bytes: scratch_field_bytes };
        unsafe { tile_scratch_bytes(width, std::ptr::from_mut(&mut input).cast(), host) }
    }
    #[cfg(feature = "compiler")]
    pub fn tile_bytes(&self) -> u64 {
        let width = self.workgroup_width();
        self.tile_data_bytes_for(width) + self.tile_scratch_bytes_for(width)
    }
}

pub(super) fn inspect_tile_uses(code: &[Inst], has_tiles: bool, output: Option<&mut BTreeSet<usize>>) -> bool {
    let output = output.map_or(std::ptr::null_mut(), |set| std::ptr::from_mut(set).cast());
    // Both the original code and the optional set are lent through this synchronous query.
    unsafe { b_tile_uses(StageCode::borrow(code), u32::from(has_tiles), stage_instruction, output, include_tile) != 0 }
}

unsafe extern "C" {
    #[link_name = "codec.type_layout::tile_uses"]
    fn b_tile_uses(code: StageCode, has_tiles: u32,
        inspect: unsafe extern "C" fn(StageCode, usize) -> StageInstructionFields, output: *mut c_void,
        include: unsafe extern "C" fn(*mut c_void, usize)) -> u32;
}

unsafe extern "C" fn include_tile(output: *mut c_void, tile: usize) {
    unsafe { &mut *output.cast::<BTreeSet<usize>>() }.insert(tile);
}

#[repr(C)]
struct TileScratchGroup {
    results: *const c_void,
    count: usize,
    operation: CollectiveOperator,
}

#[repr(C)]
struct TileScratchHost {
    next: unsafe extern "C" fn(*mut c_void) -> TileScratchGroup,
    scalar_bytes: unsafe extern "C" fn(*const c_void, *const c_void, usize) -> u32,
}

struct TileScratchInput<'a> {
    code: std::slice::Iter<'a, Inst>,
    registers: &'a [Scalar],
}

unsafe extern "C" {
    #[link_name = "codec.type_layout::tile_scratch_bytes"]
    fn tile_scratch_bytes(width: u32, context: *mut c_void, host: TileScratchHost) -> u64;
}

unsafe extern "C" fn next_scratch_group(context: *mut c_void) -> TileScratchGroup {
    let input = unsafe { &mut *context.cast::<TileScratchInput<'_>>() };
    for inst in input.code.by_ref() {
        if let InstKind::TileCollective(collective) = &inst.kind && !collective.results.is_empty() {
            return TileScratchGroup {
                results: collective.results.as_ptr().cast(),
                count: collective.results.len(),
                operation: collective.op.operator(),
            };
        }
    }
    TileScratchGroup { results: std::ptr::null(), count: 0, operation: CollectiveOperator::Sum }
}

unsafe extern "C" fn scratch_field_bytes(
    context: *const c_void, results: *const c_void, index: usize,
) -> u32 {
    let input = unsafe { &*context.cast::<TileScratchInput<'_>>() };
    let (_, reg) = unsafe { *results.cast::<(u32, Reg)>().add(index) };
    input.registers[reg].size()
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
