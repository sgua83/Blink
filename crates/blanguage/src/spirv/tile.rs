//! 输入与中间发布共用共享布局；阶段屏障覆盖所有 workgroup invocations。
use super::*;
use crate::ir::dataflow::InputFragments;
use std::collections::{btree_map, btree_set};
use std::ffi::c_void;

#[repr(C)]
struct TilePhaseView {
    code: Span,
    step: dataflow::Node,
    present: u32,
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
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_tile_phases"]
    fn emit_tile_phases(active: u32, control: StructuredHost, host: TilePhaseHost, emit: BlockHost);
}

unsafe extern "C" fn next_tile_phase(cursor: *mut c_void) -> TilePhaseView {
    let cursor = unsafe { &mut *cursor.cast::<TilePhaseCursor<'_>>() };
    let Some(phase) = cursor.phases.next() else {
        return TilePhaseView { code: Span::EMPTY, step: dataflow::Node::EMPTY, present: 0 };
    };
    cursor.saved = phase.saved.iter();
    let step = phase.step.map_or(dataflow::Node::EMPTY, dataflow::Node::instruction);
    TilePhaseView { code: Span::slice(phase.code), step, present: 1 }
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
    components: unsafe extern "C" fn(*mut c_void, usize) -> *mut scan::TreeComponent,
    width: u32,
    lane: u32,
    length: u32,
    shared: u32,
    pointer: u32,
    data_words: u32,
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

unsafe extern "C" fn tile_tree_components(context: *mut c_void, count: usize) -> *mut scan::TreeComponent {
    let emitter = unsafe { &mut *context.cast::<Emitter<'_>>() };
    // The B emitter writes and consumes the active prefix during one collection.
    emitter.tile_tree_components.reserve(count);
    emitter.tile_tree_components.as_mut_ptr()
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
        TileStorage {
            cell: tile_cell, word: tile_word, components: tile_tree_components,
            width: self.workgroup_width, lane: self.tile_lane, length: self.tile_length,
            shared: self.tile_shared, pointer: self.tile_pointer, data_words: self.tile_data_words,
        }
    }

    pub(super) fn tile_phases(&mut self, active: u32, phases: Vec<TilePhase<'_>>) -> Result<(), &'static str> {
        let empty = std::collections::BTreeSet::new();
        let mut cursor = TilePhaseCursor { phases: phases.iter(), saved: empty.iter() };
        let mut emission = StructuredEmission { emitter: self, edges: Vec::new(), error: None };
        let control = emission.host();
        let emit = block_host(emission.emitter);
        let host = TilePhaseHost {
            cursor: std::ptr::from_mut(&mut cursor).cast(),
            next_phase: next_tile_phase,
            next_value: next_tile_value,
        };
        unsafe { emit_tile_phases(active, control, host, emit); }
        emission.error.map_or(Ok(()), Err)
    }
}
