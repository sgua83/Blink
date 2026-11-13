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
struct TileInputBoundaryCode {
    length: u64,
    skip_field: u64,
}

#[repr(C)]
struct TileInputCopy {
    data_offset: u32,
    stride_offset: u32,
    source_offset: u32,
    storage_offset: u32,
    length_offset: u32,
    unit_bytes: u32,
}

#[repr(C)]
struct TilePrivateCopy {
    location: operands::MoveLocation,
    scalar: ScalarEncoding,
    storage_offset: u32,
    plane_stride: u32,
    restore: u32,
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
    #[link_name = "bootstrap.x64_instruction::tile_index_base"]
    fn encode_tile_index_base(storage_offset: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tile_field_address"]
    fn encode_tile_field_address(width: u32, byte_offset: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tile_read_split"]
    fn encode_tile_read_split(dst: operands::MoveLocation, low: operands::MoveLocation,
        high: operands::MoveLocation, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tile_publish_value"]
    fn encode_tile_publish_value(input: TilePublishInput, output: *mut u8) -> TilePublishCode;
    #[link_name = "bootstrap.x64_instruction::tile_lane_pointer"]
    fn encode_tile_lane_pointer(base_offset: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tile_phase_advance"]
    fn encode_tile_phase_advance(base_offset: u32, length_offset: u32, bytes_back: u64, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tile_private_copy"]
    fn encode_tile_private_copy(input: TilePrivateCopy, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tile_lane"]
    fn encode_tile_lane(width: u32, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::tile_input_boundary"]
    fn encode_tile_input_boundary(width: u32, length_offset: u32, output: *mut u8) -> TileInputBoundaryCode;
    #[link_name = "bootstrap.x64_instruction::tile_input_copy"]
    fn encode_tile_input_copy(input: TileInputCopy, output: *mut u8) -> u64;
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
    pub(super) fn tile_index_base(&mut self, index: Reg, storage_offset: u32) {
        self.load(index);
        self.emit_bytes(12, |output| unsafe { encode_tile_index_base(storage_offset, output) });
    }

    pub(super) fn tile_load(&mut self, dst: Reg, tile: usize, index: Reg, word: u32, offset: Option<Reg>) {
        let scalar = self.stage.registers[dst];
        let (memory, high) = if let Some(offset) = offset {
            let source = IndexedSource::Tile { tile, index };
            if scalar.size() < 4 {
                let width = self.indexed_base(&source);
                self.load(offset);
                let bias = match self.stage.tiles[tile].source {
                    TileSource::Input { word, .. } => word % 4,
                    TileSource::Published => 0,
                };
                self.emit_bytes(40, |output| unsafe { encode_tile_field_address(width, word + bias, output) });
                (Memory::new(15, 0), Memory::new(15, 0))
            } else {
                let memory = self.indexed_memory(&source, offset, word);
                (memory, memory.offset(self.stage.workgroup_width()*4))
            }
        } else {
            self.tile_index_base(index, 0);
            let bias = match self.stage.shared_word(tile, word) { SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4 };
            let low = Memory::new(15, self.shared_stream(self.stage.shared_word(tile, word)) + bias);
            let high = if scalar.size() == 8 { Memory::new(15, self.shared_stream(self.stage.shared_word(tile, word + 4))) } else { low };
            (low, high)
        };
        if scalar.size() == 8 {
            let dst = self.location(dst).into();
            self.emit_bytes(48, |output| unsafe {
                encode_tile_read_split(dst, memory.into(), high.into(), output)
            });
        } else { self.read_scalar_address(dst, memory); }
    }
    pub(super) fn tile_input(&mut self) {
        let start = self.code.len();
        let width = self.stage.workgroup_width();
        let length_offset = self.tile_length_slot();
        self.code.reserve(48);
        let boundary = unsafe {
            let output = self.code.as_mut_ptr().add(start);
            let boundary = encode_tile_input_boundary(width, length_offset, output);
            self.code.set_len(start + boundary.length as usize);
            boundary
        };
        let skip = start + boundary.skip_field as usize;
        self.parallel_abi(start, ParallelRangePart::TileInputBoundary { width });

        for ((binding, word), fragments) in self.stage.input_words() {
            let data_offset = binding as u32 * BUFFER_BYTES;
            let stride_offset = data_offset + BUFFER_STRIDE;
            let storage = self.shared_stream(SharedWord::Input { binding, word });
            for (source_offset, unit) in fragments {
                let storage_offset = storage + source_offset % 4;
                let unit_bytes = unit.size();
                let input = TileInputCopy {
                    data_offset, stride_offset, source_offset, storage_offset, length_offset, unit_bytes,
                };
                let start = self.code.len();
                self.emit_bytes(80, |output| unsafe { encode_tile_input_copy(input, output) });
                self.parallel_abi(start, ParallelRangePart::TileInputCopy {
                    binding, source_offset, storage_offset, unit_bytes,
                });
            }
        }
        self.patch(skip, self.code.len());
    }
    fn tile_lane_pointer(&mut self) {
        let base_offset = self.tile_base_slot();
        self.emit_bytes(16, |output| unsafe { encode_tile_lane_pointer(base_offset, output) });
    }

    fn private_stream(&self, slot: PrivateSlot) -> u32 {
        self.frame.tile_private[&slot]
    }

    fn tile_private(&mut self, phase: usize, slots: &std::collections::BTreeSet<PrivateSlot>, restore: bool) {
        if slots.is_empty() {
            return;
        }
        let start = self.code.len();
        self.tile_lane_pointer();
        self.parallel_abi(start, ParallelRangePart::TilePrivatePointer { phase, restore });
        let plane_stride = self.stage.workgroup_width() * 4;
        for &slot in slots {
            let storage_offset = self.private_stream(slot);
            let scalar = self.stage.private_scalar(slot);
            let input = TilePrivateCopy {
                location: self.allocation.location(slot).into(),
                scalar: scalar.into(),
                storage_offset,
                plane_stride,
                restore: u32::from(restore),
            };
            let start = self.code.len();
            self.emit_bytes(64, |output| unsafe { encode_tile_private_copy(input, output) });
            let (local, slot) = match slot {
                PrivateSlot::Register(index) => (false, index),
                PrivateSlot::Local(index) => (true, index),
            };
            self.parallel_abi(start, ParallelRangePart::TilePrivate {
                phase, restore, local, slot, storage_offset, scalar,
            });
        }
    }
    pub(super) fn publish_values(&mut self, publication: &Publication) {
        let stage = self.stage;
        let tile = publication.tile;
        let used = stage.storage_words();
        let mut values = publication.values.iter().filter_map(|&(word, value)| {
            let scalar = stage.registers[value];
            let low = SharedWord::Published { tile, word };
            let high = SharedWord::Published { tile, word: word + 4 };
            let low = used.contains(&low).then_some(low);
            let high = (scalar.size() == 8 && used.contains(&high)).then_some(high);
            (low.is_some() || high.is_some()).then_some((word, value, scalar, low, high))
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
    pub(super) fn tile_phases(&mut self) -> Result<(), &'static str> {
        let start = self.code.len();
        let storage_offset = self.tile_base_slot();
        self.copy(Location::Stack(storage_offset), Location::Gpr(3));
        self.parallel_abi(start, ParallelRangePart::TileBase { storage_offset });
        self.tile_input();
        for (phase_index, phase) in self.stage.tile_phases().into_iter().enumerate() {
            let start = self.code.len();
            self.copy(Location::Gpr(3), Location::Stack(storage_offset));
            self.parallel_abi(start, ParallelRangePart::TilePhaseEnter { phase: phase_index });
            let begin = self.code.len();
            self.tile_private(phase_index, &phase.live_in, true);
            self.block(phase.code)?;
            if let Some(inst @ Inst { kind: InstKind::TilePublish(publication), .. }) = phase.step {
                let start = self.code.len();
                let first = self.operation_ranges.as_ref().map(Vec::len);
                self.publish_values(publication);
                self.operation_range(inst, start, first);
            }
            self.tile_private(phase_index, &phase.preserved_out(), false);
            let start = self.code.len();
            let length_offset = self.tile_length_slot();
            let bytes_back = (start - begin) as u64;
            self.emit_bytes(32, |output| unsafe {
                encode_tile_phase_advance(storage_offset, length_offset, bytes_back, output)
            });
            self.parallel_abi(start, ParallelRangePart::TilePhaseAdvance { phase: phase_index });
            if let Some(inst @ Inst { kind: InstKind::TileCollective(collective), .. }) = phase.step {
                let start = self.code.len();
                let first = self.operation_ranges.as_ref().map(Vec::len);
                self.tile_collective(collective);
                self.operation_range(inst, start, first);
            }
        }
        Ok(())
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
