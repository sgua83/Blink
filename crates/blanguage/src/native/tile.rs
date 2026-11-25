//! 每个原生范围拥有完整逻辑 tile；发布后按阶段执行所有 lane，私有值随 lane 保存。
use super::*;
use crate::native::{ParallelRangePart, TileCollectivePart};
use super::allocation::Location;
use std::ffi::c_void;
use crate::builtin_types::ScalarEncoding;

#[repr(C)]
struct TileCollectiveLayout {
    operation: CollectiveOperator,
    exclusive: u32,
    length_offset: u32,
    plane_stride: u32,
}

#[repr(C)]
struct TileCollectiveField {
    identity: u64,
    scalar: ScalarEncoding,
    input_offset: u32,
    output_offset: u32,
    input_high_delta: u32,
    word: u32,
    result: Reg,
    scalar_origin: *const Scalar,
}

#[repr(C)]
struct TileCollectiveCode {
    length: u64,
    identity_start: u64,
    loop_start: u64,
    accumulate_start: u64,
    fill_start: u64,
    fill_loop_start: u64,
}

#[repr(C)]
struct TilePublishInput {
    location: operands::MoveLocation,
    scalar: ScalarEncoding,
    low_offset: u32,
    high_offset: u32,
}

#[repr(C)]
struct TilePublishCode {
    length: u64,
    value_end: u64,
    high_start: u64,
    high_store: u64,
}

#[repr(C)]
struct TilePublishField {
    input: TilePublishInput,
    word: u32,
    value: Reg,
    scalar_origin: *const Scalar,
}

type TileSlot = *mut c_void;

#[repr(C)]
struct TilePublishHost {
    reserve: unsafe extern "C" fn(TileSlot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(TileSlot, usize),
    next: unsafe extern "C" fn(TileSlot) -> TilePublishField,
    observe: unsafe extern "C" fn(TileSlot, usize, usize, TilePublishCode, TilePublishField),
}

#[repr(C)]
struct TileCollectiveHost {
    reserve: unsafe extern "C" fn(TileSlot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(TileSlot, usize),
    next: unsafe extern "C" fn(TileSlot, CollectiveOperator) -> TileCollectiveField,
    observe: unsafe extern "C" fn(TileSlot, usize, TileCollectiveLayout, TileCollectiveCode, TileCollectiveField),
}

struct TileFields<'a, 'stage, Op> {
    emitter: &'a mut Emitter<'stage>,
    values: std::slice::Iter<'a, (u32, Reg)>,
    tile: usize,
    operation: Op,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::emit_tile_publish"]
    fn emit_tile_publish(context: TileSlot, base_offset: u32, host: TilePublishHost);
    #[link_name = "bootstrap.x64_instruction::tile_lane"]
    fn encode_tile_lane(width: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::emit_tile_collective"]
    fn emit_tile_collective(context: TileSlot, layout: TileCollectiveLayout, host: TileCollectiveHost);
}

impl Emitter<'_> {
    fn tile_length_slot(&self) -> u32 {
        self.frame.end as u32
    }
    pub(super) fn shared_stream(&self, word: SharedWord) -> u32 {
        self.frame.tile_shared[&word.aligned()]
    }
    pub(super) fn tile_length(&mut self, dst: Reg) {
        self.copy(self.location(dst), Location::Stack(self.tile_length_slot()));
    }
    pub(super) fn tile_lane(&mut self, dst: Reg) {
        let width = self.workgroup;
        self.emit_bytes(16, |output| unsafe { encode_tile_lane(width, output) });
        self.store(dst);
    }
    pub(super) fn private_stream(&self, slot: PrivateSlot) -> u32 {
        self.frame.tile_private[&slot]
    }

    pub(super) fn publish_values(&mut self, publication: &Publication) {
        let base_offset = self.tile_length_slot() + 4;
        let mut state = TileFields {
            emitter: self,
            values: publication.values.iter(),
            tile: publication.tile,
            operation: (),
        };
        unsafe {
            emit_tile_publish(std::ptr::from_mut(&mut state).cast(), base_offset, TilePublishHost {
                reserve: reserve_tile::<()>,
                commit: commit_tile::<()>,
                next: next_publish,
                observe: observe_publish,
            });
        }
    }

    fn publication_offset(&self, tile: usize, word: u32) -> u32 {
        let field = SharedWord::Published { tile, word };
        if self.used_tile_words.contains(&field) {
            self.shared_stream(field) + word % 4
        } else {
            u32::MAX
        }
    }

    pub(super) fn tile_collective(&mut self, collective: &TileCollective) {
        let layout = TileCollectiveLayout {
            operation: collective.op.operator(),
            exclusive: u32::from(collective.op.exclusive()),
            length_offset: self.tile_length_slot(),
            plane_stride: self.workgroup * 4,
        };
        let mut state = TileFields {
            emitter: self,
            values: collective.results.iter(),
            tile: collective.tile,
            operation: collective.op,
        };
        unsafe {
            emit_tile_collective(std::ptr::from_mut(&mut state).cast(), layout, TileCollectiveHost {
                reserve: reserve_tile::<TileOp>,
                commit: commit_tile::<TileOp>,
                next: next_collective,
                observe: observe_collective,
            });
        }
    }
}

unsafe extern "C" fn reserve_tile<Op>(slot: TileSlot, count: usize) -> NativeCodeBuffer {
    let state = unsafe { &mut *slot.cast::<TileFields<'_, '_, Op>>() };
    state.emitter.reserve_native_code(count)
}

unsafe extern "C" fn commit_tile<Op>(slot: TileSlot, end: usize) {
    let state = unsafe { &mut *slot.cast::<TileFields<'_, '_, Op>>() };
    unsafe { state.emitter.commit_native_code(end) };
}

unsafe extern "C" fn next_publish(slot: TileSlot) -> TilePublishField {
    let state = unsafe { &mut *slot.cast::<TileFields<'_, '_, ()>>() };
    let emitter = &*state.emitter;
    for &(word, value) in state.values.by_ref() {
        let scalar = &emitter.registers[value];
        let low_offset = emitter.publication_offset(state.tile, word);
        let high_offset = if scalar.size() == 8 {
            emitter.publication_offset(state.tile, word + 4)
        } else {
            u32::MAX
        };
        if low_offset != u32::MAX || high_offset != u32::MAX {
            return TilePublishField {
                input: TilePublishInput {
                    location: emitter.location(value).into(),
                    scalar: (*scalar).into(),
                    low_offset,
                    high_offset,
                },
                word,
                value,
                scalar_origin: scalar,
            };
        }
    }
    TilePublishField {
        input: TilePublishInput {
            location: Location::Immediate(0).into(),
            scalar: Scalar::U32.into(),
            low_offset: u32::MAX,
            high_offset: u32::MAX,
        },
        word: 0,
        value: 0,
        scalar_origin: std::ptr::null(),
    }
}

unsafe extern "C" fn observe_publish(
    slot: TileSlot, start: usize, value_start: usize, code: TilePublishCode, field: TilePublishField,
) {
    let state = unsafe { &mut *slot.cast::<TileFields<'_, '_, ()>>() };
    let emitter = &mut *state.emitter;
    let tile = state.tile;
    let TilePublishField { input, word, value, scalar_origin } = field;
    let scalar = unsafe { *scalar_origin };
    if start != value_start {
        emitter.parallel_abi_range(start..value_start, ParallelRangePart::TilePublishPointer { tile });
    }
    let value_end = value_start + code.value_end as usize;
    let high_start = value_start + code.high_start as usize;
    let high_store = value_start + code.high_store as usize;
    emitter.parallel_abi_range(value_start..value_end, ParallelRangePart::TilePublishLoad {
        tile, word, value, scalar,
    });
    let end = value_start + code.length as usize;
    if value_end != high_start {
        emitter.parallel_abi_range(value_end..high_start, ParallelRangePart::TilePublishStore {
            tile, word, value, scalar, part: 0, storage_offset: input.low_offset, unit_bytes: scalar.size().min(4),
        });
    }
    if high_store != end {
        emitter.parallel_abi_range(high_start..high_store, ParallelRangePart::TilePublishShift {
            tile, word, value, scalar, part: 1,
        });
        emitter.parallel_abi_range(high_store..end, ParallelRangePart::TilePublishStore {
            tile, word, value, scalar, part: 1, storage_offset: input.high_offset, unit_bytes: 4,
        });
    }
}

unsafe extern "C" fn next_collective(slot: TileSlot, operator: CollectiveOperator) -> TileCollectiveField {
    let state = unsafe { &mut *slot.cast::<TileFields<'_, '_, TileOp>>() };
    let Some(&(word, result)) = state.values.next() else {
        return TileCollectiveField {
            identity: 0,
            scalar: Scalar::U32.into(),
            input_offset: 0,
            output_offset: 0,
            input_high_delta: 0,
            word: 0,
            result: 0,
            scalar_origin: std::ptr::null(),
        };
    };
    let emitter = &*state.emitter;
    let source = emitter.tiles[state.tile].shared_word(state.tile, word);
    let byte = match source {
        SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4,
    };
    let input_base = emitter.shared_stream(source);
    let scalar = &emitter.registers[result];
    let input_high_delta = if scalar.size() == 8 {
        let high = emitter.tiles[state.tile].shared_word(state.tile, word + 4);
        emitter.shared_stream(high).wrapping_sub(input_base)
    } else {
        0
    };
    TileCollectiveField {
        identity: operator.identity(*scalar),
        scalar: (*scalar).into(),
        input_offset: input_base + byte,
        output_offset: emitter.private_stream(PrivateSlot::Register(result)),
        input_high_delta,
        word,
        result,
        scalar_origin: scalar,
    }
}

unsafe extern "C" fn observe_collective(
    slot: TileSlot, start: usize, layout: TileCollectiveLayout, code: TileCollectiveCode, field: TileCollectiveField,
) {
    let state = unsafe { &mut *slot.cast::<TileFields<'_, '_, TileOp>>() };
    let emitter = &mut *state.emitter;
    let tile = state.tile;
    let operation = state.operation;
    let scalar = unsafe { *field.scalar_origin };
    let range = |part| ParallelRangePart::TileCollective {
        tile, word: field.word, result: field.result, scalar, operation, part,
    };
    let identity_start = start + code.identity_start as usize;
    let loop_start = start + code.loop_start as usize;
    let accumulate_start = start + code.accumulate_start as usize;
    let fill_start = start + code.fill_start as usize;
    let end = start + code.length as usize;
    emitter.parallel_abi_range(start..identity_start, range(TileCollectivePart::Setup {
        input_offset: field.input_offset, output_offset: field.output_offset,
        length_offset: layout.length_offset, plane_stride: layout.plane_stride,
    }));
    emitter.parallel_abi_range(identity_start..loop_start, range(TileCollectivePart::Identity { bits: field.identity }));
    if loop_start != accumulate_start {
        emitter.parallel_abi_range(loop_start..accumulate_start, range(TileCollectivePart::ScanOutput));
    }
    emitter.parallel_abi_range(accumulate_start..fill_start, range(TileCollectivePart::Accumulate {
        input_high_delta: field.input_high_delta, loop_start,
    }));
    if fill_start != end {
        let loop_start = start + code.fill_loop_start as usize;
        emitter.parallel_abi_range(fill_start..end, range(TileCollectivePart::Fill {
            length_offset: layout.length_offset, loop_start,
        }));
    }
}
