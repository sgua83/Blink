//! 输入与中间发布共用共享布局；阶段屏障覆盖所有 workgroup invocations。
use super::*;
use crate::ir::dataflow::InputFragments;
use std::collections::{btree_map, btree_set};
use std::ffi::c_void;

#[repr(u32)]
enum TilePhaseKind { End, Plain, Publish, Barrier, Collective }

#[repr(C)]
struct TilePhaseView {
    code: Span,
    step: *const c_void,
    kind: TilePhaseKind,
}

#[derive(Default)]
#[repr(C)]
struct TilePhaseValue {
    register: Reg,
    value: u32,
    value_type: u32,
    module_constant: u32,
}

struct TilePhaseCursor<'a> {
    phases: std::slice::Iter<'a, TilePhase<'a>>,
    saved: btree_set::Iter<'a, PrivateSlot>,
}

#[repr(C)]
struct TilePhaseHost {
    cursor: *mut c_void,
    next_phase: unsafe extern "C" fn(*mut c_void) -> TilePhaseView,
    next_value: unsafe extern "C" fn(*mut c_void, StructuredContext) -> TilePhaseValue,
    label: unsafe extern "C" fn(StructuredContext) -> u32,
    publish: unsafe extern "C" fn(StructuredContext, *const c_void),
    barrier: unsafe extern "C" fn(StructuredContext),
    collective: unsafe extern "C" fn(StructuredContext, *const c_void),
    zero: unsafe extern "C" fn(StructuredContext, Reg) -> u32,
    bind: unsafe extern "C" fn(StructuredContext, Reg, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_tile_phases"]
    fn emit_tile_phases(active: u32, control: StructuredHost, host: TilePhaseHost);
}

unsafe extern "C" fn next_tile_phase(cursor: *mut c_void) -> TilePhaseView {
    let cursor = unsafe { &mut *cursor.cast::<TilePhaseCursor<'_>>() };
    let Some(phase) = cursor.phases.next() else {
        return TilePhaseView { code: Span::EMPTY, step: std::ptr::null(), kind: TilePhaseKind::End };
    };
    cursor.saved = phase.saved.iter();
    let (kind, step) = match phase.step.map(|inst| &inst.kind) {
        Some(InstKind::TilePublish(value)) => (TilePhaseKind::Publish, std::ptr::from_ref(value).cast()),
        Some(InstKind::TileCollective(value)) => (TilePhaseKind::Collective, std::ptr::from_ref(value).cast()),
        Some(InstKind::TileBarrier) => (TilePhaseKind::Barrier, std::ptr::null()),
        _ => (TilePhaseKind::Plain, std::ptr::null()),
    };
    TilePhaseView { code: Span::slice(phase.code), step, kind }
}

unsafe extern "C" fn next_tile_value(cursor: *mut c_void, context: StructuredContext) -> TilePhaseValue {
    let cursor = unsafe { &mut *cursor.cast::<TilePhaseCursor<'_>>() };
    for slot in &mut cursor.saved {
        if let PrivateSlot::Register(register) = *slot {
            let emitter = &unsafe { &*context.cast::<StructuredEmission<'_, '_>>() }.emitter;
            let value = emitter.value(register);
            return TilePhaseValue {
                register,
                value,
                value_type: emitter.ty(register),
                module_constant: u32::from(emitter.constants.values().any(|&constant| constant == value)),
            };
        }
    }
    TilePhaseValue::default()
}

unsafe extern "C" fn tile_phase_label(context: StructuredContext) -> u32 {
    unsafe { &*context.cast::<StructuredEmission<'_, '_>>() }.emitter.current_label
}

unsafe extern "C" fn tile_phase_publish(context: StructuredContext, step: *const c_void) {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    emitter.publish_values(unsafe { &*step.cast::<Publication>() });
}

unsafe extern "C" fn tile_phase_barrier(context: StructuredContext) {
    unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter.tile_barrier();
}

unsafe extern "C" fn tile_phase_collective(context: StructuredContext, step: *const c_void) {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    emitter.tile_collective(unsafe { &*step.cast::<crate::ir::TileCollective>() });
}

unsafe extern "C" fn tile_phase_zero(context: StructuredContext, register: Reg) -> u32 {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    emitter.constant(emitter.stage.registers[register], 0)
}

unsafe extern "C" fn tile_phase_bind(context: StructuredContext, register: Reg, value: u32) {
    unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter.regs[register] = value;
}

#[repr(u32)]
enum TileKeyKind { Input, Published }

#[repr(C)]
struct TileKey {
    owner: usize,
    word: u32,
    kind: TileKeyKind,
}

impl From<SharedWord> for TileKey {
    fn from(value: SharedWord) -> Self {
        match value {
            SharedWord::Input { binding, word } => Self { owner: binding, word, kind: TileKeyKind::Input },
            SharedWord::Published { tile, word } => Self { owner: tile, word, kind: TileKeyKind::Published },
        }
    }
}

#[repr(C)]
struct TileCell {
    unit: ScalarCode,
    slot: u32,
    byte: u32,
    variable: u32,
    pointer_type: u32,
}

#[repr(C)]
pub(super) struct TileStorage {
    cell: unsafe extern "C" fn(*mut c_void, TileKey) -> TileCell,
    word: unsafe extern "C" fn(*mut c_void, usize, u32) -> TileKey,
    width: u32,
    lane: u32,
    length: u32,
}

#[repr(C)]
struct TilePublication {
    tile: usize,
    words: usize,
    fields: Span,
    registers: Span,
    ids: *const u32,
}

unsafe extern "C" fn tile_cell(context: *mut c_void, key: TileKey) -> TileCell {
    let emitter = unsafe { &*context.cast::<Emitter<'_>>() };
    let shared = match key.kind {
        TileKeyKind::Input => SharedWord::Input { binding: key.owner, word: key.word },
        TileKeyKind::Published => SharedWord::Published { tile: key.owner, word: key.word },
    };
    emitter.tile_layout.get(&shared.aligned()).map_or(TileCell::ABSENT, |&(unit, slot, part)| {
        emitter.placed_tile_cell(unit, slot, part + key.word % 4)
    })
}

impl TileCell {
    const ABSENT: Self = Self {
        unit: ScalarCode::U32, slot: 0, byte: 0, variable: 0, pointer_type: 0,
    };
}

unsafe extern "C" fn tile_word(context: *mut c_void, tile: usize, word: u32) -> TileKey {
    unsafe { &*context.cast::<Emitter<'_>>() }.stage.shared_word(tile, word).into()
}

#[repr(C)]
struct TileInputLayout {
    page: PageInputs,
    descriptors: *const u32,
    count: u32,
    active: u32,
    width: u32,
    label: u32,
}

#[repr(C)]
struct TileInputRange {
    lane: u32,
    length: u32,
    label: u32,
}

#[repr(C)]
struct TileInputRow {
    binding: usize,
    word: u32,
    fragments: usize,
    cell: TileCell,
}

#[repr(C)]
struct TileInputFragment {
    byte: u32,
    unit: ScalarCode,
}

struct TileInputCursor<'a> {
    cells: btree_map::Range<'a, SharedWord, (Scalar, usize, u32)>,
    inputs: &'a InputFragments,
    fragments: btree_set::Iter<'a, (u32, Scalar)>,
}

#[repr(C)]
struct TileInputSource {
    cursor: *mut c_void,
    row: unsafe extern "C" fn(*mut c_void, *mut c_void) -> TileInputRow,
    fragment: unsafe extern "C" fn(*mut c_void) -> TileInputFragment,
}

unsafe extern "C" fn tile_input_row(cursor: *mut c_void, context: *mut c_void) -> TileInputRow {
    let cursor = unsafe { &mut *cursor.cast::<TileInputCursor<'_>>() };
    let Some((&SharedWord::Input { binding, word }, &(unit, slot, byte))) = cursor.cells.next() else {
        return TileInputRow { binding: 0, word: 0, fragments: 0, cell: TileCell::ABSENT };
    };
    let emitter = unsafe { &*context.cast::<Emitter<'_>>() };
    let fragments = cursor.inputs.get(&(binding, word)).map_or(0, |fragments| {
        cursor.fragments = fragments.iter();
        fragments.len()
    });
    TileInputRow { binding, word, fragments, cell: emitter.placed_tile_cell(unit, slot, byte) }
}

unsafe extern "C" fn tile_input_fragment(cursor: *mut c_void) -> TileInputFragment {
    let cursor = unsafe { &mut *cursor.cast::<TileInputCursor<'_>>() };
    let &(byte, unit) = cursor.fragments.next().unwrap();
    TileInputFragment { byte, unit: unit.into() }
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_tile_input"]
    fn emit_tile_input(input: TileInputLayout, source: TileInputSource, memory: MemoryHost) -> TileInputRange;
    #[link_name = "bootstrap.spirv_envelope::tile_storage_read"]
    fn tile_storage_read(storage: TileStorage, key: TileKey, index: u32, offset: u32,
        scalar: ScalarCode, memory: MemoryHost) -> u32;
    #[link_name = "bootstrap.spirv_envelope::emit_tile_publication"]
    fn emit_tile_publication(input: TilePublication, storage: TileStorage, workspace: *mut u32,
        scalar: unsafe extern "C" fn(*mut c_void, Reg) -> ScalarCode, memory: MemoryHost);
}

impl Emitter<'_> {
    pub(super) fn tile_input(&mut self, count: u32, active: u32, inputs: &InputFragments) {
        let end = SharedWord::Published { tile: 0, word: 0 };
        let empty = std::collections::BTreeSet::new();
        let mut cursor = TileInputCursor {
            cells: self.tile_layout.range(..end), inputs, fragments: empty.iter(),
        };
        let input = TileInputLayout {
            page: self.page_inputs(0, self.index, Some(0), 0), descriptors: self.buffers.as_ptr(),
            count, active, width: self.workgroup_width, label: self.current_label,
        };
        let source = TileInputSource {
            cursor: std::ptr::from_mut(&mut cursor).cast(),
            row: tile_input_row, fragment: tile_input_fragment,
        };
        let range = unsafe { emit_tile_input(input, source, self.memory_host()) };
        self.tile_lane = range.lane;
        self.tile_length = range.length;
        self.current_label = range.label;
    }

    fn placed_tile_cell(&self, unit: Scalar, slot: usize, byte: u32) -> TileCell {
        let (pointer_type, variable) = if unit == Scalar::U64 {
            (self.tile_pointer64, self.tile_shared64)
        } else {
            (self.tile_pointer, self.tile_shared)
        };
        TileCell { unit: unit.into(), slot: slot as u32, byte, variable, pointer_type }
    }

    pub(super) fn tile_storage(&self) -> TileStorage {
        TileStorage { cell: tile_cell, word: tile_word, width: self.workgroup_width,
            lane: self.tile_lane, length: self.tile_length }
    }

    pub(super) fn tile_read(
        &mut self, scalar: Scalar, tile: usize, index: u32, word: u32, offset: Option<u32>,
    ) -> u32 {
        let key = self.stage.shared_word(tile, word).into();
        unsafe { tile_storage_read(self.tile_storage(), key, index, offset.unwrap_or(0), scalar.into(), self.memory_host()) }
    }

    pub(super) fn tile_barrier(&mut self) {
        self.workgroup_barrier(0x148);
    }
    pub(super) fn publish_values(&mut self, publication: &Publication) {
        let words = self.stage.tiles[publication.tile].ty.size.div_ceil(4) as usize;
        let input = TilePublication {
            tile: publication.tile, words, ids: self.regs.as_ptr(),
            fields: Span::field(&publication.values, |field| &field.0),
            registers: Span::field(&publication.values, |field| &field.1),
        };
        let mut workspace = vec![0; publication.values.len() + words];
        unsafe {
            emit_tile_publication(input, self.tile_storage(), workspace.as_mut_ptr(), value_scalar, self.memory_host());
        }
    }
    pub(super) fn tile_phases(&mut self, active: u32, phases: Vec<TilePhase<'_>>) -> Result<(), &'static str> {
        let empty = std::collections::BTreeSet::new();
        let mut cursor = TilePhaseCursor { phases: phases.iter(), saved: empty.iter() };
        let mut emission = StructuredEmission { emitter: self, error: None };
        let host = TilePhaseHost {
            cursor: std::ptr::from_mut(&mut cursor).cast(),
            next_phase: next_tile_phase,
            next_value: next_tile_value,
            label: tile_phase_label,
            publish: tile_phase_publish,
            barrier: tile_phase_barrier,
            collective: tile_phase_collective,
            zero: tile_phase_zero,
            bind: tile_phase_bind,
        };
        unsafe { emit_tile_phases(active, emission.host(), host); }
        emission.error.map_or(Ok(()), Err)
    }
}
