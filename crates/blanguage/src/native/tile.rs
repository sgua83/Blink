//! 每个原生范围拥有完整逻辑 tile；发布后按阶段执行所有 lane，私有值随 lane 保存。
use super::*;
use crate::native::{ParallelRangePart, TileCollectivePart};
use super::allocation::Location;
use crate::builtin_types::ScalarEncoding;

#[repr(C)]
struct TileCollectiveInput {
    identity: u64,
    scalar: ScalarEncoding,
    operation: u32,
    exclusive: u32,
    input_offset: u32,
    output_offset: u32,
    length_offset: u32,
    plane_stride: u32,
    input_high_delta: u32,
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

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::tile_publish_value"]
    fn encode_tile_publish_value(input: TilePublishInput, output: *mut u8) -> TilePublishCode;
    #[link_name = "bootstrap.x64_instruction::tile_lane_pointer"]
    fn encode_tile_lane_pointer(base_offset: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tile_lane"]
    fn encode_tile_lane(width: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tile_collective"]
    fn encode_tile_collective(input: TileCollectiveInput, output: *mut u8) -> TileCollectiveCode;
}

impl Emitter<'_> {
    fn tile_length_slot(&self) -> u32 {
        self.frame.end as u32
    }
    fn tile_base_slot(&self) -> u32 {
        self.tile_length_slot() + 4
    }
    pub(super) fn shared_stream(&self, word: SharedWord) -> u32 {
        self.frame.tile_shared[&word.aligned()]
    }
    pub(super) fn tile_length(&mut self, dst: Reg) {
        self.copy(self.location(dst), Location::Stack(self.tile_length_slot()));
    }
    pub(super) fn tile_lane(&mut self, dst: Reg) {
        let width = self.stage.workgroup_width();
        self.emit_bytes(16, |output| unsafe { encode_tile_lane(width, output) });
        self.store(dst);
    }
    fn tile_lane_pointer(&mut self) {
        let base_offset = self.tile_base_slot();
        self.emit_bytes(16, |output| unsafe { encode_tile_lane_pointer(base_offset, output) });
    }

    pub(super) fn private_stream(&self, slot: PrivateSlot) -> u32 {
        self.frame.tile_private[&slot]
    }

    pub(super) fn publish_values(&mut self, publication: &Publication) {
        let stage = self.stage;
        let used = self.used_tile_words;
        let tile = publication.tile;
        let mut values = publication.values.iter().filter_map(|&(word, value)| {
            let scalar = stage.registers[value];
            let parts = crate::ir::dataflow::publish_parts(used, tile, word, scalar.size());
            (parts != 0).then(|| {
                let low = (parts & 1 != 0).then_some(SharedWord::Published { tile, word });
                let high = (parts & 2 != 0).then(|| SharedWord::Published { tile, word: word + 4 });
                (word, value, scalar, low, high)
            })
        }).peekable();
        if values.peek().is_none() {
            return;
        }
        let start = self.code.len();
        self.tile_lane_pointer();
        self.parallel_abi(start, ParallelRangePart::TilePublishPointer { tile });
        for (word, value, scalar, low, high) in values {
            let low = low.map(|field| self.shared_stream(field) + word % 4);
            let high = high.map(|field| self.shared_stream(field) + word % 4);
            let input = TilePublishInput {
                location: self.location(value).into(),
                scalar: scalar.into(),
                low_offset: low.unwrap_or(u32::MAX),
                high_offset: high.unwrap_or(u32::MAX),
            };
            let start = self.code.len();
            self.code.reserve(48);
            let encoding = unsafe {
                let output = self.code.as_mut_ptr().add(start);
                let encoding = encode_tile_publish_value(input, output);
                self.code.set_len(start + encoding.length as usize);
                encoding
            };
            let value_end = start + encoding.value_end as usize;
            let high_start = start + encoding.high_start as usize;
            let high_store = start + encoding.high_store as usize;
            self.parallel_abi_range(start..value_end, ParallelRangePart::TilePublishLoad {
                tile, word, value, scalar,
            });
            if let Some(storage_offset) = low {
                self.parallel_abi_range(value_end..high_start, ParallelRangePart::TilePublishStore {
                    tile, word, value, scalar, part: 0, storage_offset, unit_bytes: scalar.size().min(4),
                });
            }
            if let Some(storage_offset) = high {
                self.parallel_abi_range(high_start..high_store, ParallelRangePart::TilePublishShift {
                    tile, word, value, scalar, part: 1,
                });
                self.parallel_abi(high_store, ParallelRangePart::TilePublishStore {
                    tile, word, value, scalar, part: 1, storage_offset, unit_bytes: 4,
                });
            }
        }
    }
    pub(super) fn tile_collective(&mut self, collective: &TileCollective) {
        for &(word, result) in &collective.results {
            let source = self.stage.shared_word(collective.tile, word);
            let byte = match source {
                SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4,
            };
            let input_offset = self.shared_stream(source) + byte;
            let output_offset = self.private_stream(PrivateSlot::Register(result));
            let length_offset = self.tile_length_slot();
            let scalar = self.stage.registers[result];
            let plane_stride = self.stage.workgroup_width() * 4;
            let operator = collective.op.operator();
            let identity = operator.identity(scalar);
            let exclusive = collective.op.exclusive();
            let input_high_delta = if scalar.size() == 8 {
                let high = self.stage.shared_word(collective.tile, word + 4);
                self.shared_stream(high).wrapping_sub(self.shared_stream(source))
            } else {
                0
            };
            let input = TileCollectiveInput {
                identity,
                scalar: scalar.into(),
                operation: reduce::reduction_operator(operator),
                exclusive: u32::from(exclusive),
                input_offset,
                output_offset,
                length_offset,
                plane_stride,
                input_high_delta,
            };
            let start = self.code.len();
            self.code.reserve(128);
            let encoding = unsafe {
                let output = self.code.as_mut_ptr().add(start);
                let encoding = encode_tile_collective(input, output);
                self.code.set_len(start + encoding.length as usize);
                encoding
            };
            let identity_start = start + encoding.identity_start as usize;
            let loop_start = start + encoding.loop_start as usize;
            let accumulate_start = start + encoding.accumulate_start as usize;
            let fill_start = start + encoding.fill_start as usize;
            let range_part = |part| ParallelRangePart::TileCollective {
                tile: collective.tile, word, result, scalar, operation: collective.op, part,
            };
            self.parallel_abi_range(start..identity_start, range_part(TileCollectivePart::Setup {
                input_offset, output_offset, length_offset, plane_stride,
            }));
            self.parallel_abi_range(identity_start..loop_start,
                range_part(TileCollectivePart::Identity { bits: identity }));
            if exclusive {
                self.parallel_abi_range(loop_start..accumulate_start, range_part(TileCollectivePart::ScanOutput));
            }
            self.parallel_abi_range(accumulate_start..fill_start,
                range_part(TileCollectivePart::Accumulate { input_high_delta, loop_start }));
            if !exclusive {
                let loop_start = start + encoding.fill_loop_start as usize;
                self.parallel_abi(fill_start, range_part(TileCollectivePart::Fill { length_offset, loop_start }));
            }
        }
    }
}
