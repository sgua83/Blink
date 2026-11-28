//! 发布点切分块内阶段；私有值与共享 word 分别按实际消费者安排生命周期。
use super::*;
use std::{ffi::c_void, mem::MaybeUninit};
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
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct TileKey { storage: u64, owner: usize, pub(super) byte: u32 }

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
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub(crate) struct TileExtents {
    pub(crate) narrow: usize,
    pub(crate) wide: usize,
}

#[cfg(feature = "compiler")]
#[derive(Default)]
pub(crate) struct PlacedTiles {
    pub(crate) words: dataflow::TileLayout,
    pub(crate) extents: TileExtents,
}

#[cfg(feature = "compiler")]
#[repr(C)]
struct TileLayoutResult { extents: TileExtents, count: usize }

#[cfg(feature = "compiler")]
#[allow(dead_code)]
#[repr(u32)]
enum TileScratch { Rows, Members, Narrow, Wide, Cells }

#[cfg(feature = "compiler")]
#[repr(C)]
struct TileLayoutInput {
    tiles: dataflow::Span,
    groups: dataflow::Span,
    publication_phases: dataflow::Span,
    uses: *mut dataflow::TileUse,
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
    words: unsafe extern "C" fn(*mut c_void, usize) -> *mut dataflow::TilePlacedWord,
}

#[cfg(feature = "compiler")]
struct TileStorage {
    scratch: [Vec<MaybeUninit<u64>>; 5],
    words: dataflow::TileLayout,
}

#[cfg(feature = "compiler")]
unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::place_tile"]
    fn place_tile(input: TileLayoutInput, host: TileLayoutHost) -> TileLayoutResult;
}

#[cfg(feature = "compiler")]
unsafe extern "C" fn reserve_tile_storage(context: *mut c_void, bank: TileScratch, count: usize) -> *mut c_void {
    let storage = unsafe { &mut *context.cast::<TileStorage>() };
    let rows = &mut storage.scratch[bank as usize];
    rows.resize_with(count, MaybeUninit::uninit);
    rows.as_mut_ptr().cast()
}

#[cfg(feature = "compiler")]
unsafe extern "C" fn reserve_placed_words(context: *mut c_void, count: usize) -> *mut dataflow::TilePlacedWord {
    let words = unsafe { &mut (*context.cast::<TileStorage>()).words };
    words.reserve_exact(count);
    words.as_mut_ptr()
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

#[repr(u32)]
enum StorageInstruction { Ordinary, Read, Components, Collective, Publish, Barrier, Preserve, Block }

#[repr(C)]
struct StorageStep {
    kind: StorageInstruction,
    byte: u32,
    tile: usize,
    bytes: u32,
    first: StageSpan,
    second: StageSpan,
}

#[repr(C)]
struct StorageTile {
    storage: u64,
    owner: usize,
    start: u32,
    bytes: u32,
    components: StageSpan,
}

#[repr(C)]
struct StoragePart { byte: u32, bytes: u32 }

#[repr(C)]
struct StorageHost {
    stage: *const c_void,
    step: unsafe extern "C" fn(*const c_void, StageSpan, usize) -> StorageStep,
    tile: unsafe extern "C" fn(*const c_void, usize) -> StorageTile,
    component: unsafe extern "C" fn(StageSpan, usize) -> StoragePart,
    result: unsafe extern "C" fn(*const c_void, StageSpan, usize) -> StoragePart,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct TileGroup {
    storage: u64,
    owner: usize,
    first: u32,
    end: u32,
}

type StorageRead = unsafe extern "C" fn(*mut c_void, usize, u32, i64);

#[repr(C)]
struct StorageOutput {
    context: *mut c_void,
    read: StorageRead,
    publication_phases: *mut i64,
    group_storage: *mut c_void,
    reserve_groups: unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void,
}

#[repr(C)]
pub(super) struct StorageFacts { pub(super) preserve: u32, pub(super) group_count: usize }

unsafe extern "C" {
    #[link_name = "codec.type_layout::storage_facts"]
    fn b_storage_facts(code: StageSpan, host: StorageHost, phase: i64, output: StorageOutput) -> StorageFacts;
}

unsafe extern "C" fn storage_step(stage: *const c_void, code: StageSpan, index: usize) -> StorageStep {
    use StorageInstruction::*;
    let inst = unsafe { &*code.data.cast::<Inst>().add(index) };
    let mut step = StorageStep {
        kind: Ordinary, byte: 0, tile: 0, bytes: 0, first: StageSpan::EMPTY, second: StageSpan::EMPTY,
    };
    match &inst.kind {
        InstKind::TileLoad { dst, tile, word, .. } => {
            step.kind = Read;
            step.tile = *tile;
            step.byte = *word;
            step.bytes = unsafe { &*stage.cast::<Stage>() }.registers[*dst].size();
        }
        InstKind::Atomic(Atomic { target: IndexedSource::Tile { tile, .. }, word, scalar, offset: None, .. }) => {
            step.kind = Read;
            step.tile = *tile;
            step.byte = *word;
            step.bytes = scalar.size();
        }
        InstKind::LoadIndexed { source: IndexedSource::Tile { tile, .. }, .. }
        | InstKind::Atomic(Atomic { target: IndexedSource::Tile { tile, .. }, .. }) => {
            step.kind = Components;
            step.tile = *tile;
        }
        InstKind::TileCollective(value) => {
            step.kind = Collective;
            step.tile = value.tile;
            step.first = StageSpan::borrow(&value.results);
        }
        InstKind::TilePublish(value) => {
            step.kind = Publish;
            step.tile = value.tile;
        }
        InstKind::TileBarrier => step.kind = Barrier,
        InstKind::Append(_) | InstKind::Subgroup { .. } => step.kind = Preserve,
        InstKind::Scope(body) => {
            step.kind = Block;
            step.first = StageSpan::borrow(body);
        }
        InstKind::If { yes, no, .. } | InstKind::Loop { condition: yes, body: no, .. } => {
            step.kind = Block;
            step.first = StageSpan::borrow(yes);
            step.second = StageSpan::borrow(no);
        }
        _ => {}
    }
    step
}

unsafe extern "C" fn storage_tile(stage: *const c_void, index: usize) -> StorageTile {
    let tile = &unsafe { &*stage.cast::<Stage>() }.tiles[index];
    let (storage, owner, start) = match tile.shared_word(index, 0) {
        SharedWord::Input { binding, word } => (0, binding, word),
        SharedWord::Published { tile, word } => (1, tile, word),
    };
    StorageTile { storage, owner, start, bytes: tile.ty.size, components: StageSpan::borrow(&tile.ty.components) }
}

unsafe extern "C" fn storage_component(components: StageSpan, index: usize) -> StoragePart {
    let (scalar, byte) = unsafe { *components.data.cast::<(Scalar, u32)>().add(index) };
    StoragePart { byte, bytes: scalar.size() }
}

unsafe extern "C" fn storage_result(stage: *const c_void, results: StageSpan, index: usize) -> StoragePart {
    let (byte, reg) = unsafe { *results.data.cast::<(u32, Reg)>().add(index) };
    StoragePart { byte, bytes: unsafe { &*stage.cast::<Stage>() }.registers[reg].size() }
}

unsafe extern "C" fn reserve_tile_groups(storage: *mut c_void, count: usize) -> *mut c_void {
    let groups = unsafe { &mut *storage.cast::<Vec<MaybeUninit<TileGroup>>>() };
    groups.resize_with(count, MaybeUninit::uninit);
    groups.as_mut_ptr().cast()
}

pub(super) fn storage_facts(
    stage: &Stage, context: *mut c_void, read: StorageRead,
    layout: Option<(&mut [i64], &mut Vec<MaybeUninit<TileGroup>>)>,
) -> StorageFacts {
    let (phase, publication_phases, group_storage) = layout.map_or(
        (-1, std::ptr::null_mut(), std::ptr::null_mut()),
        |(phases, groups)| (0, phases.as_mut_ptr(), std::ptr::from_mut(groups).cast()),
    );
    let host = StorageHost {
        stage: std::ptr::from_ref(stage).cast(), step: storage_step, tile: storage_tile,
        component: storage_component, result: storage_result,
    };
    let output = StorageOutput { context, read, publication_phases, group_storage, reserve_groups: reserve_tile_groups };
    unsafe { b_storage_facts(StageSpan::borrow(&stage.code), host, phase, output) }
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
    pub fn storage_words(&self) -> BTreeSet<SharedWord> {
        let mut words = BTreeSet::new();
        if !self.tiles.is_empty() {
            unsafe extern "C" fn read(context: *mut c_void, tile: usize, offset: u32, _phase: i64) {
                let (stage, words) = unsafe { &mut *context.cast::<(&Stage, &mut BTreeSet<SharedWord>)>() };
                words.insert(stage.shared_word(tile, offset));
            }
            let mut context = (self, &mut words);
            storage_facts(self, std::ptr::from_mut(&mut context).cast(), read, None);
        }
        words
    }

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
        &self, wide: bool,
    ) -> (PlacedTiles, dataflow::InputFragments, Vec<dataflow::TileUse>) {
        self.place_tile_layout(wide, true)
    }

    #[cfg(feature = "compiler")]
    fn place_tile_layout(
        &self, wide: bool, publish: bool,
    ) -> (PlacedTiles, dataflow::InputFragments, Vec<dataflow::TileUse>) {
        if self.tiles.is_empty() {
            return (PlacedTiles::default(), Vec::new(), Vec::new());
        }
        let dataflow::LayoutFacts { reads: mut uses, publication_phases, groups } = dataflow::tile_layout_facts(self);
        // Vulkan normalizes the layout input to cells after the exact reads are consumed.
        let inputs = if publish { dataflow::input_fragments(self, &uses) } else { Vec::new() };
        let mut storage = TileStorage {
            scratch: Default::default(), words: Vec::new(),
        };
        let input = TileLayoutInput {
            tiles: dataflow::Span::slice(&self.tiles), groups: dataflow::Span::slice(&groups),
            publication_phases: dataflow::Span::slice(&publication_phases),
            uses: uses.as_mut_ptr(), use_count: uses.len(), wide: u32::from(wide), publish: u32::from(publish),
        };
        let result = unsafe {
            place_tile(input, TileLayoutHost {
                ir: dataflow::HOST, context: std::ptr::from_mut(&mut storage).cast(),
                reserve: reserve_tile_storage, words: reserve_placed_words,
            })
        };
        // B initialized and compacted precisely this prefix of the output storage.
        unsafe { storage.words.set_len(result.count); }
        (PlacedTiles { words: storage.words, extents: result.extents }, inputs, uses)
    }


    #[cfg(feature = "compiler")]
    pub fn tile_data_bytes(&self) -> u64 {
        self.tile_data_bytes_for(self.workgroup_width())
    }
    #[cfg(feature = "compiler")]
    pub(super) fn tile_data_bytes_for(&self, width: u32) -> u64 {
        let words = self.place_tile_layout(false, false).0.extents.narrow;
        words as u64 * u64::from(width) * 4
    }
    pub fn tile_scratch_bytes(&self) -> u64 {
        // A stage with a contributing collective uses the original tile width.
        let width = self.tiles.first().map_or(Self::DEFAULT_WORKGROUP_WIDTH, |tile| tile.width);
        self.tile_scratch_bytes_for(width)
    }
    pub(super) fn tile_scratch_bytes_for(&self, width: u32) -> u64 {
        unsafe {
            tile_scratch_bytes(
                width, StageSpan::borrow(&self.code), std::ptr::from_ref(&self.registers).cast(),
                stage_instruction, scratch_field_bytes,
            )
        }
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
    unsafe { b_tile_uses(StageSpan::borrow(code), u32::from(has_tiles), stage_instruction, output, include_tile) != 0 }
}

unsafe extern "C" {
    #[link_name = "codec.type_layout::tile_uses"]
    fn b_tile_uses(code: StageSpan, has_tiles: u32,
        inspect: unsafe extern "C" fn(StageSpan, usize) -> StageInstructionFields, output: *mut c_void,
        include: unsafe extern "C" fn(*mut c_void, usize)) -> u32;
}

unsafe extern "C" fn include_tile(output: *mut c_void, tile: usize) {
    unsafe { &mut *output.cast::<BTreeSet<usize>>() }.insert(tile);
}

unsafe extern "C" {
    #[link_name = "codec.type_layout::tile_scratch_bytes"]
    fn tile_scratch_bytes(
        width: u32, code: StageSpan, registers: *const c_void,
        inspect: unsafe extern "C" fn(StageSpan, usize) -> StageInstructionFields,
        scalar_bytes: unsafe extern "C" fn(*const c_void, StageSpan, usize) -> u32,
    ) -> u64;
}

unsafe extern "C" fn scratch_field_bytes(
    registers: *const c_void, results: StageSpan, index: usize,
) -> u32 {
    let registers = unsafe { &*registers.cast::<Vec<Scalar>>() };
    let (_, reg) = unsafe { *results.data.cast::<(u32, Reg)>().add(index) };
    registers[reg].size()
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
