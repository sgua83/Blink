//! 输入与中间发布共用共享布局；阶段屏障覆盖所有 workgroup invocations。
use super::*;
use crate::ir::{TileKey, dataflow::InputFragments};
use std::collections::{btree_map, btree_set};
use std::ffi::c_void;

#[repr(C)]
struct TilePhaseView {
    code: Span,
    step: dataflow::Node,
    saved: Span,
}

#[repr(C)]
struct TilePhaseValue {
    value: u32,
    value_type: u32,
    module_constant: u32,
}

#[repr(C)]
struct TilePhaseHost {
    phases: Span,
    phase: unsafe extern "C" fn(*const c_void, usize) -> TilePhaseView,
    value: unsafe extern "C" fn(StructuredContext, Reg) -> TilePhaseValue,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_tile_phases"]
    fn emit_tile_phases(active: u32, control: StructuredHost, host: TilePhaseHost, emit: BlockHost);
}

unsafe extern "C" fn tile_phase(phases: *const c_void, index: usize) -> TilePhaseView {
    let phase = unsafe { &*phases.cast::<TilePhase<'_>>().add(index) };
    let step = phase.step.map_or(dataflow::Node::EMPTY, dataflow::Node::instruction);
    TilePhaseView { code: Span::slice(phase.code), step, saved: Span::slice(&phase.saved) }
}

unsafe extern "C" fn tile_phase_value(context: StructuredContext, register: Reg) -> TilePhaseValue {
    let emitter = &unsafe { &*context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    let value = emitter.value(register);
    TilePhaseValue {
        value,
        value_type: emitter.ty(register),
        module_constant: u32::from(emitter.constants.values().any(|&constant| constant == value)),
    }
}

#[repr(C)]
struct TileCell {
    unit: ScalarCode,
    slot: u32,
    byte: u32,
    present: u32,
}

#[repr(C)]
struct TilePlanes {
    narrow: u32,
    narrow_pointer: u32,
    wide: u32,
    wide_pointer: u32,
}

#[repr(C)]
pub(super) struct TileStorage {
    cell: unsafe extern "C" fn(*mut c_void, TileKey) -> TileCell,
    tiles: Span,
    components: unsafe extern "C" fn(*mut c_void, usize) -> *mut scan::TreeComponent,
    planes: TilePlanes,
    width: u32,
    lane: u32,
    length: u32,
    data_words: u32,
}

unsafe extern "C" fn tile_cell(context: *mut c_void, key: TileKey) -> TileCell {
    let emitter = unsafe { &*context.cast::<Emitter<'_>>() };
    emitter.tile_layout.get(&key.word()).map_or(TileCell::ABSENT, |&(unit, slot, byte)| {
        TileCell { unit: unit.into(), slot: slot as u32, byte, present: 1 }
    })
}

impl TileCell {
    const ABSENT: Self = Self { unit: ScalarCode::U32, slot: 0, byte: 0, present: 0 };
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
    planes: TilePlanes,
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
    row: unsafe extern "C" fn(*mut c_void) -> TileInputRow,
    fragment: unsafe extern "C" fn(*mut c_void) -> TileInputFragment,
}

unsafe extern "C" fn tile_input_row(cursor: *mut c_void) -> TileInputRow {
    let cursor = unsafe { &mut *cursor.cast::<TileInputCursor<'_>>() };
    let Some((&SharedWord::Input { binding, word }, &(unit, slot, byte))) = cursor.cells.next() else {
        return TileInputRow { binding: 0, word: 0, fragments: 0, cell: TileCell::ABSENT };
    };
    let fragments = cursor.inputs.get(&(binding, word)).map_or(0, |fragments| {
        cursor.fragments = fragments.iter();
        fragments.len()
    });
    let cell = TileCell { unit: unit.into(), slot: slot as u32, byte, present: 1 };
    TileInputRow { binding, word, fragments, cell }
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
            planes: self.tile_planes(), count, active, width: self.workgroup_width, label: self.current_label,
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

    fn tile_planes(&self) -> TilePlanes {
        TilePlanes {
            narrow: self.tile_shared, narrow_pointer: self.tile_pointer,
            wide: self.tile_shared64, wide_pointer: self.tile_pointer64,
        }
    }

    pub(super) fn tile_storage(&self) -> TileStorage {
        TileStorage {
            cell: tile_cell, tiles: Span::slice(&self.stage.tiles), components: tile_tree_components,
            planes: self.tile_planes(), width: self.workgroup_width,
            lane: self.tile_lane, length: self.tile_length, data_words: self.tile_data_words,
        }
    }

    pub(super) fn tile_phases(&mut self, active: u32, phases: Vec<TilePhase<'_>>) -> Result<(), &'static str> {
        let mut emission = StructuredEmission { emitter: self, edges: Vec::new(), error: None };
        let control = emission.host();
        let emit = block_host(emission.emitter);
        let host = TilePhaseHost {
            phases: Span::slice(&phases),
            phase: tile_phase,
            value: tile_phase_value,
        };
        unsafe { emit_tile_phases(active, control, host, emit); }
        emission.error.map_or(Ok(()), Err)
    }
}
