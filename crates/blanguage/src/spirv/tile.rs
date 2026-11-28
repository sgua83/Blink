//! 输入与中间发布共用共享布局；阶段屏障覆盖所有 workgroup invocations。
use super::*;
use crate::ir::dataflow::InputFragments;
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
struct TilePlanes {
    narrow: u32,
    narrow_pointer: u32,
    wide: u32,
    wide_pointer: u32,
}

#[repr(C)]
pub(super) struct TileStorage {
    placed: Span,
    tiles: Span,
    components: unsafe extern "C" fn(*mut c_void, usize) -> *mut scan::TreeComponent,
    planes: TilePlanes,
    width: u32,
    lane: u32,
    length: u32,
    data_words: u32,
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
struct TileInputSource {
    cells: Span,
    fragments: Span,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_tile_input"]
    fn emit_tile_input(input: TileInputLayout, source: TileInputSource, memory: MemoryHost) -> TileInputRange;
}

impl Emitter<'_> {
    pub(super) fn tile_input(&mut self, count: u32, active: u32, inputs: &InputFragments) {
        let input = TileInputLayout {
            page: self.page_inputs(0, self.index, Some(0), 0), descriptors: self.buffers.as_ptr(),
            planes: self.tile_planes(), count, active, width: self.workgroup_width, label: self.current_label,
        };
        let source = TileInputSource {
            cells: Span::slice(self.tile_layout),
            fragments: Span::slice(inputs),
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
            placed: Span::slice(self.tile_layout),
            tiles: Span::slice(&self.stage.tiles), components: tile_tree_components,
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
