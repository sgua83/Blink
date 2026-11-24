//! 输入与中间发布共用共享布局；阶段屏障覆盖所有 workgroup invocations。
use super::*;
use std::collections::btree_set;
use std::ffi::c_void;

#[repr(C)]
struct TileRangeWords {
    words: u64,
    next: u32,
    lane: u32,
    length: u32,
}

#[repr(u32)]
enum TilePhaseKind { End, Plain, Publish, Barrier, Collective }

#[repr(C)]
struct TilePhaseView {
    code: InstructionBlock,
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
    #[link_name = "bootstrap.spirv_envelope::tile_range"]
    fn emit_tile_range(
        body: *mut u32, uint_type: u32, bool_type: u32, index: u32, count: u32, width: u32, first: u32,
    ) -> TileRangeWords;
    #[link_name = "bootstrap.spirv_envelope::emit_tile_phases"]
    fn emit_tile_phases(active: u32, control: StructuredHost, host: TilePhaseHost);
}

unsafe extern "C" fn next_tile_phase(cursor: *mut c_void) -> TilePhaseView {
    let cursor = unsafe { &mut *cursor.cast::<TilePhaseCursor<'_>>() };
    let Some(phase) = cursor.phases.next() else {
        return TilePhaseView { code: (&[] as &[Inst]).into(), step: std::ptr::null(), kind: TilePhaseKind::End };
    };
    cursor.saved = phase.saved.iter();
    let (kind, step) = match phase.step.map(|inst| &inst.kind) {
        Some(InstKind::TilePublish(value)) => (TilePhaseKind::Publish, std::ptr::from_ref(value).cast()),
        Some(InstKind::TileCollective(value)) => (TilePhaseKind::Collective, std::ptr::from_ref(value).cast()),
        Some(InstKind::TileBarrier) => (TilePhaseKind::Barrier, std::ptr::null()),
        _ => (TilePhaseKind::Plain, std::ptr::null()),
    };
    TilePhaseView { code: phase.code.into(), step, kind }
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

pub(super) enum TileByte {
    Fixed(u32),
    Dynamic(u32),
}

impl Emitter<'_> {
    pub(super) fn tile_input(
        &mut self, count: u32, active: u32, inputs: &mut crate::ir::dataflow::InputFragments,
    ) {
        let width = self.constant(Scalar::U32, self.workgroup_width);
        let at = self.body.len();
        self.body.reserve(26);
        let range = unsafe {
            let range = emit_tile_range(
                self.body.as_mut_ptr().add(at), self.types[&Scalar::U32], self.types[&Scalar::Bool],
                self.index, count, width, self.next,
            );
            self.body.set_len(at + range.words as usize);
            range
        };
        self.next = range.next;
        self.tile_lane = range.lane;
        self.tile_length = range.length;
        for (key, (unit, _, part)) in self.tile_layout {
            if *unit == Scalar::U64 && *part == 0 {
                if let SharedWord::Input { binding, word } = key {
                    inputs.entry((*binding, *word)).or_default();
                }
            }
        }
        if inputs.is_empty() {
            return;
        }
        let load = self.id();
        let end = self.id();
        self.open_active(active, load, end);
        for (&(source, word), fragments) in inputs.iter() {
            let key = SharedWord::Input { binding: source, word };
            let (unit, _, part) = self.tile_layout[&key];
            if unit == Scalar::U64 {
                if part == 0 {
                    let raw = self.load_raw_buffer(unit, source, self.index, word);
                    let at = self.tile_word_address(key, self.tile_lane, None).0;
                    self.store_pointer(at, raw);
                }
                continue;
            }
            let components: Vec<_> = fragments.iter().map(|&(byte, unit)| {
                let raw = self.load_raw_buffer(unit, source, self.index, byte);
                (unit, byte % 4, raw)
            }).collect();
            let raw = self.pack_words(4, components)[0];
            let target = self.tile_word_address(
                SharedWord::Input {
                    binding: source,
                    word,
                },
                self.tile_lane,
                None,
            );
            self.store_pointer(target.0, raw);
        }
        self.merge_active(end, true);
        self.tile_barrier();
    }
    pub(super) fn tile_word_address(
        &mut self, word: SharedWord, index: u32, offset: Option<u32>,
    ) -> (u32, TileByte) {
        let (unit, slot, part) = self.tile_layout[&word.aligned()];
        let byte = part + match word {
            SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4,
        };
        let (pointer_type, shared) = if unit == Scalar::U64 {
            (self.tile_pointer64, self.tile_shared64)
        } else {
            (self.tile_pointer, self.tile_shared)
        };
        let (offset_id, bias, cell_bytes, width) = if let Some(offset) = offset {
            let bias = if byte == 0 { 0 } else { self.constant(Scalar::U32, byte) };
            (offset, bias, self.constant(Scalar::U32, unit.size()),
             self.constant(Scalar::U32, self.workgroup_width))
        } else {
            (0, 0, 0, 0)
        };
        let base = if slot == 0 { 0 } else {
            self.constant(Scalar::U32, slot as u32 * self.workgroup_width)
        };
        let input = TileAddress {
            index_type: self.types[&Scalar::U32], pointer_type, shared, index,
            offset: offset_id, bias, cell_bytes, width, base, next: self.next,
        };
        let at = self.body.len();
        self.body.reserve(30);
        let emitted = unsafe {
            let emitted = emit_tile_address(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
        let byte = if offset.is_some() { TileByte::Dynamic(emitted.byte) } else { TileByte::Fixed(byte) };
        (emitted.pointer, byte)
    }

    fn read_tile_part(&mut self, scalar: Scalar, unit: Scalar, pointer: u32, byte: TileByte) -> u32 {
        let (byte, cell_bytes, bits_per_byte, shift) = match byte {
            TileByte::Fixed(byte) => {
                let bits = (byte % unit.size()) * 8;
                let shift = if bits == 0 { 0 } else { self.constant(Scalar::U32, bits) };
                (0, 0, 0, shift)
            }
            TileByte::Dynamic(byte) => (
                byte, self.constant(Scalar::U32, unit.size()), self.constant(Scalar::U32, 8), 0,
            ),
        };
        let input = TileExtract {
            index_type: self.types[&Scalar::U32], cell_type: self.types[&unit], pointer,
            byte, cell_bytes, bits_per_byte, shift, next: self.next,
        };
        let at = self.body.len();
        self.body.reserve(19);
        let emitted = unsafe {
            let emitted = emit_tile_extract(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
        self.join_words(scalar, &[emitted.value], unit)
    }

    pub(super) fn tile_read(
        &mut self, scalar: Scalar, tile: usize, index: u32, word: u32, offset: Option<u32>,
    ) -> u32 {
        let key = self.stage.shared_word(tile, word);
        let unit = self.tile_layout[&key.aligned()].0;
        let (pointer, byte) = self.tile_word_address(key, index, offset);
        if scalar.size() < unit.size() {
            return self.read_tile_part(scalar, unit, pointer, byte);
        }
        if scalar.size() > unit.size() {
            let high = self.tile_word_address(self.stage.shared_word(tile, word + 4), index, offset).0;
            return self.load_raw(scalar, unit, &[pointer, high], false);
        }
        self.load_raw(scalar, unit, &[pointer], false)
    }
    pub(super) fn tile_barrier(&mut self) {
        self.workgroup_barrier(0x148);
    }
    pub(super) fn publish_values(&mut self, publication: &Publication) {
        let mut components = Vec::new();
        for (word, value) in &publication.values {
            let scalar = self.stage.registers[*value];
            let raw = self.raw_bits(*value);
            if scalar.size() == 8 {
                let key = self.stage.shared_word(publication.tile, *word);
                if self.tile_layout.contains_key(&key.aligned()) {
                    let at = self.tile_word_address(key, self.tile_lane, None).0;
                    self.store_pointer(at, raw);
                }
            } else {
                components.push((scalar, *word, raw));
            }
        }
        let cells: std::collections::BTreeSet<_> = components.iter().map(|(_, word, _)| word / 4).collect();
        let packed = self.pack_words(self.stage.tiles[publication.tile].ty.size, components);
        for word in cells {
            let at = self.tile_word_address(
                SharedWord::Published {
                    tile: publication.tile,
                    word: word * 4,
                },
                self.tile_lane,
                None,
            );
            self.store_pointer(at.0, packed[word as usize]);
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
