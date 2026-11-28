//! 每个原生范围拥有完整逻辑 tile；发布后按阶段执行所有 lane，私有值随 lane 保存。
use super::*;
use crate::native::{ParallelRangePart, TileCollectivePart};
use super::allocation::Location;
use std::ffi::c_void;
use crate::builtin_types::ScalarEncoding;
use crate::ir::{TileKey, dataflow::{HOST, Host, View}};

#[repr(C)]
struct TileCollectiveLayout {
    length_offset: u32,
    plane_stride: u32,
}

#[repr(C)]
struct TileCollectiveField {
    identity: u64,
    scalar: ScalarEncoding,
    output_offset: u32,
    word: u32,
    result: Reg,
}

#[repr(C)]
struct TileCollectiveInput { offset: u32, high_delta: u32 }

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
struct TilePublishCode {
    length: u64,
    value_end: u64,
    high_start: u64,
    high_store: u64,
}

#[repr(C)]
struct TilePublishField {
    word: u32,
    value: Reg,
    low_offset: u32,
    high_offset: u32,
}

type TileSlot = *mut c_void;

#[repr(C)]
struct TilePublishHost {
    reserve: unsafe extern "C" fn(TileSlot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(TileSlot, usize),
    scalar: unsafe extern "C" fn(*const c_void, Reg) -> ScalarEncoding,
    location: unsafe extern "C" fn(*const c_void, Reg) -> operands::MoveLocation,
    placed: Span,
    used: unsafe extern "C" fn(*const c_void, TileKey) -> u32,
    observe: unsafe extern "C" fn(TileSlot, usize, usize, usize, TilePublishCode, TilePublishField),
}

#[repr(C)]
struct TileCollectiveHost {
    ir: Host,
    slots: operands::TileSlots,
    private_slots: Span,
    reserve: unsafe extern "C" fn(TileSlot, usize) -> NativeCodeBuffer,
    commit: unsafe extern "C" fn(TileSlot, usize),
    scalar: unsafe extern "C" fn(*const c_void, Reg) -> ScalarEncoding,
    observe: unsafe extern "C" fn(TileSlot, *const TileCollective, usize, TileCollectiveLayout, TileCollectiveCode,
        TileCollectiveField, TileCollectiveInput),
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::emit_tile_publish"]
    fn emit_tile_publish(context: TileSlot, publication: View, base_offset: u32, host: TilePublishHost);
    #[link_name = "bootstrap.x64_instruction::tile_lane"]
    fn encode_tile_lane(width: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::emit_tile_collective"]
    fn emit_tile_collective(context: TileSlot, collective: View, layout: TileCollectiveLayout, host: TileCollectiveHost);
}

impl Emitter<'_> {
    fn tile_length_slot(&self) -> u32 {
        self.frame.end as u32
    }
    pub(super) fn tile_length(&mut self, dst: Reg) {
        self.copy(self.location(dst), Location::Stack(self.tile_length_slot()));
    }
    pub(super) fn tile_lane(&mut self, dst: Reg) {
        let width = self.workgroup;
        self.emit_bytes(16, |output| unsafe { encode_tile_lane(width, output) });
        self.store(dst);
    }
    pub(super) fn publish_values(&mut self, publication: View) {
        let base_offset = self.tile_length_slot() + 4;
        let placed = self.frame.shared_words();
        unsafe {
            emit_tile_publish(std::ptr::from_mut(self).cast(), publication, base_offset, TilePublishHost {
                reserve: structure::reserve_scalar,
                commit: structure::commit_scalar,
                scalar: operands::body_value_scalar,
                location: operands::body_value_location,
                placed,
                used: publication_word_used,
                observe: observe_publish,
            });
        }
    }

    pub(super) fn tile_collective(&mut self, collective: View) {
        let layout = TileCollectiveLayout {
            length_offset: self.tile_length_slot(),
            plane_stride: self.workgroup * 4,
        };
        let slots = operands::TileSlots::new(self.tiles, &self.frame);
        let private_slots = self.frame.private_slots();
        unsafe {
            emit_tile_collective(std::ptr::from_mut(self).cast(), collective, layout, TileCollectiveHost {
                ir: HOST,
                slots,
                private_slots,
                reserve: structure::reserve_scalar,
                commit: structure::commit_scalar,
                scalar: operands::body_value_scalar,
                observe: observe_collective,
            });
        }
    }
}

unsafe extern "C" fn publication_word_used(slot: *const c_void, key: TileKey) -> u32 {
    let emitter = unsafe { &*slot.cast::<Emitter<'_>>() };
    u32::from(emitter.used_tile_words.contains(&key.word()))
}

unsafe extern "C" fn observe_publish(
    slot: TileSlot, tile: usize, start: usize, value_start: usize, code: TilePublishCode, field: TilePublishField,
) {
    let emitter = unsafe { &mut *slot.cast::<Emitter<'_>>() };
    let TilePublishField { word, value, low_offset, high_offset } = field;
    let scalar = emitter.registers[value];
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
            tile, word, value, scalar, part: 0, storage_offset: low_offset, unit_bytes: scalar.size().min(4),
        });
    }
    if high_store != end {
        emitter.parallel_abi_range(high_start..high_store, ParallelRangePart::TilePublishShift {
            tile, word, value, scalar, part: 1,
        });
        emitter.parallel_abi_range(high_store..end, ParallelRangePart::TilePublishStore {
            tile, word, value, scalar, part: 1, storage_offset: high_offset, unit_bytes: 4,
        });
    }
}

unsafe extern "C" fn observe_collective(
    slot: TileSlot, collective: *const TileCollective, start: usize, layout: TileCollectiveLayout,
    code: TileCollectiveCode, field: TileCollectiveField, input: TileCollectiveInput,
) {
    let emitter = unsafe { &mut *slot.cast::<Emitter<'_>>() };
    let collective = unsafe { &*collective };
    let scalar = emitter.registers[field.result];
    let range = |part| ParallelRangePart::TileCollective {
        tile: collective.tile, word: field.word, result: field.result, scalar, operation: collective.op, part,
    };
    let identity_start = start + code.identity_start as usize;
    let loop_start = start + code.loop_start as usize;
    let accumulate_start = start + code.accumulate_start as usize;
    let fill_start = start + code.fill_start as usize;
    let end = start + code.length as usize;
    emitter.parallel_abi_range(start..identity_start, range(TileCollectivePart::Setup {
        input_offset: input.offset, output_offset: field.output_offset,
        length_offset: layout.length_offset, plane_stride: layout.plane_stride,
    }));
    emitter.parallel_abi_range(identity_start..loop_start, range(TileCollectivePart::Identity { bits: field.identity }));
    if loop_start != accumulate_start {
        emitter.parallel_abi_range(loop_start..accumulate_start, range(TileCollectivePart::ScanOutput));
    }
    emitter.parallel_abi_range(accumulate_start..fill_start, range(TileCollectivePart::Accumulate {
        input_high_delta: input.high_delta, loop_start,
    }));
    if fill_start != end {
        let loop_start = start + code.fill_loop_start as usize;
        emitter.parallel_abi_range(fill_start..end, range(TileCollectivePart::Fill {
            length_offset: layout.length_offset, loop_start,
        }));
    }
}
