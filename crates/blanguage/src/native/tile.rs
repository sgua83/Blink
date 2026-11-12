//! 每个原生范围拥有完整逻辑 tile；发布后按阶段执行所有 lane，私有值随 lane 保存。
use super::*;
use crate::native::{ParallelRangePart, TileCollectivePart};
use super::allocation::Location;
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
        self.bytes(&[0x8b, 0x84, 0x24]);
        self.u32(self.tile_length_slot());
        self.store(dst);
    }
    pub(super) fn tile_lane(&mut self, dst: Reg) {
        self.bytes(&[0x89, 0xd8, 0x31, 0xd2, 0xb9]);
        self.u32(self.stage.workgroup_width());
        self.bytes(&[0xf7, 0xf1, 0x89, 0xd0]);
        self.store(dst);
    }
    pub(super) fn tile_load(&mut self, dst: Reg, tile: usize, index: Reg, word: u32, offset: Option<Reg>) {
        let scalar = self.stage.registers[dst];
        let (memory, high) = if let Some(offset) = offset {
            let source = IndexedSource::Tile { tile, index };
            if scalar.size() < 4 {
                self.indexed_base(&source);
                self.load(offset);
                let bias = match self.stage.tiles[tile].source { TileSource::Input { word, .. } => word % 4, TileSource::Published => 0 };
                self.bytes(&[0x05]); self.u32(word + bias);
                self.bytes(&[0x89, 0xc1, 0x83, 0xe1, 3, 0x83, 0xe0, 0xfc]);
                self.scale_index(0, self.stage.workgroup_width());
                self.bytes(&[0x48, 0x01, 0xc8, 0x49, 0x01, 0xc7]);
                (Memory::new(15, 0), Memory::new(15, 0))
            } else {
                let memory = self.indexed_memory(&source, offset, word);
                (memory, memory.offset(self.stage.workgroup_width()*4))
            }
        } else {
            self.load(index);
            self.bytes(&[0x49, 0x89, 0xe7]);
            self.offset_address(4);
            let bias = match self.stage.shared_word(tile, word) { SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4 };
            let low = Memory::new(15, self.shared_stream(self.stage.shared_word(tile, word)) + bias);
            let high = if scalar.size() == 8 { Memory::new(15, self.shared_stream(self.stage.shared_word(tile, word + 4))) } else { low };
            (low, high)
        };
        if scalar.size() == 8 {
            self.instruction(None, &[0x8b], false, 1, high.into());
            self.instruction(None, &[0x8b], false, 0, memory.into());
            self.bytes(&[0x48, 0xc1, 0xe1, 32, 0x48, 0x09, 0xc8]);
            self.store(dst);
        } else { self.read_scalar_address(dst, memory); }
    }
    pub(super) fn tile_input(&mut self) {
        let start = self.code.len();
        let width = self.stage.workgroup_width();
        self.bytes(&[0x89, 0xd8, 0x31, 0xd2, 0xb9]);
        self.u32(width);
        self.bytes(&[0xf7, 0xf1, 0x85, 0xd2]);
        let skip = self.jump(Branch::Flags(0x85));
        // r14d 是对齐的范围末端；只有全局最后一块可以是不完整 tile。
        self.bytes(&[0x44, 0x89, 0xf0, 0x29, 0xd8, 0xb9]);
        self.u32(width);
        self.bytes(&[0x39, 0xc8, 0x0f, 0x47, 0xc1, 0x89, 0x84, 0x24]);
        self.u32(self.tile_length_slot());
        self.parallel_abi(start, ParallelRangePart::TileInputBoundary { width });
        for ((source, word), fragments) in self.stage.input_words() {
          for (byte, unit) in fragments {
            let start = self.code.len();
            self.buffer_load(source as u32 * BUFFER_BYTES, 10);
            self.copy(Location::Gpr(0), Location::Gpr(3));
            self.buffer_index(source, 0, Some(10));
            self.bytes(&[0x4c, 0x8d, 0x9c, 0x24]);
            let storage_offset = self.shared_stream(SharedWord::Input { binding: source, word }) + byte % 4;
            self.u32(storage_offset);
            self.bytes(&[0x8b, 0x8c, 0x24]);
            self.u32(self.tile_length_slot());
            let begin = self.code.len();
            if unit.size() < 4 {
                self.instruction(None, &[0x0f, 0xb6 + u8::from(unit.size() == 2)], false, 0, operands::Operand::memory(10, byte));
            } else {
                self.instruction(None, &[0x8b], false, 0, operands::Operand::memory(10, byte));
            }
            self.write_value_address(Location::Gpr(0), Memory::new(11, 0), unit.size());
            self.buffer_advance(source, 10);
            self.bytes(&[0x49, 0x83, 0xc3, 0x04, 0xff, 0xc9]);
            let again = self.jump(Branch::Flags(0x85));
            self.patch(again, begin);
            self.parallel_abi(start, ParallelRangePart::TileInputCopy { binding: source, source_offset: byte, storage_offset, unit_bytes: unit.size() });
          }
        }
        self.patch(skip, self.code.len());
    }
    fn tile_lane_pointer(&mut self) {
        // r10 = rsp + 4 * (全局 index - tile base)。所有共享／私有流再加固定偏移。
        self.bytes(&[0x89, 0xd8, 0x2b, 0x84, 0x24]);
        self.u32(self.tile_base_slot());
        self.bytes(&[0x4c, 0x8d, 0x14, 0x84]);
    }
    fn private_stream(&self, slot: PrivateSlot) -> u32 {
        self.frame.tile_private[&slot]
    }
    fn tile_restore(&mut self, phase: usize, slots: &std::collections::BTreeSet<PrivateSlot>) {
        let start = self.code.len();
        self.tile_lane_pointer();
        self.parallel_abi(start, ParallelRangePart::TilePrivatePointer { phase, restore: true });
        for slot in slots {
            let start = self.code.len();
            let storage_offset = self.private_stream(*slot);
            self.bytes(&[0x41, 0x8b, 0x82]);
            self.u32(storage_offset);
            let scalar = self.stage.private_scalar(*slot);
            if scalar.size() == 8 {
                self.bytes(&[0x41, 0x8b, 0x8a]);
                self.u32(storage_offset + self.stage.workgroup_width()*4);
                self.bytes(&[0x48, 0xc1, 0xe1, 32, 0x48, 0x09, 0xc8]);
            }
            self.copy_scalar(self.allocation.location(*slot), Location::Gpr(0), scalar);
            let (local, slot) = match *slot { PrivateSlot::Register(index) => (false, index), PrivateSlot::Local(index) => (true, index) };
            self.parallel_abi(start, ParallelRangePart::TilePrivate { phase, restore: true, local, slot, storage_offset, scalar });
        }
    }
    fn tile_save(&mut self, phase: usize, slots: &std::collections::BTreeSet<PrivateSlot>) {
        let start = self.code.len();
        self.tile_lane_pointer();
        self.parallel_abi(start, ParallelRangePart::TilePrivatePointer { phase, restore: false });
        for slot in slots {
            let start = self.code.len();
            let storage_offset = self.private_stream(*slot);
            let scalar = self.stage.private_scalar(*slot);
            self.copy_scalar(Location::Gpr(0), self.allocation.location(*slot), scalar);
            self.bytes(&[0x41, 0x89, 0x82]);
            self.u32(storage_offset);
            if scalar.size() == 8 {
                self.bytes(&[0x48, 0xc1, 0xe8, 32, 0x41, 0x89, 0x82]);
                self.u32(storage_offset + self.stage.workgroup_width()*4);
            }
            let (local, slot) = match *slot { PrivateSlot::Register(index) => (false, index), PrivateSlot::Local(index) => (true, index) };
            self.parallel_abi(start, ParallelRangePart::TilePrivate { phase, restore: false, local, slot, storage_offset, scalar });
        }
    }
    pub(super) fn publish_values(&mut self, publication: &Publication) {
        let start = self.code.len();
        self.tile_lane_pointer();
        self.parallel_abi(start, ParallelRangePart::TilePublishPointer { tile: publication.tile });
        let used = self.stage.storage_words();
        for (word, value) in &publication.values {
            let start = self.code.len();
            self.load(*value);
            let scalar = self.stage.registers[*value];
            self.parallel_abi(start, ParallelRangePart::TilePublishLoad { tile: publication.tile, word: *word, value: *value, scalar });
            for part in 0..scalar.size().div_ceil(4) {
                if part != 0 {
                    let start = self.code.len();
                    self.bytes(&[0x48, 0xc1, 0xe8, 32]);
                    self.parallel_abi(start, ParallelRangePart::TilePublishShift { tile: publication.tile, word: *word, value: *value, scalar, part });
                }
                let shared = SharedWord::Published { tile: publication.tile, word: *word + part * 4 };
                if used.contains(&shared) {
                    let at = self.shared_stream(shared) + word % 4;
                    let unit_bytes = scalar.size().min(4);
                    let start = self.code.len();
                    self.write_value_address(Location::Gpr(0), Memory::new(10, at), unit_bytes);
                    self.parallel_abi(start, ParallelRangePart::TilePublishStore { tile: publication.tile, word: *word, value: *value, scalar, part, storage_offset: at, unit_bytes });
                }
            }
        }
    }
    pub(super) fn tile_phases(&mut self) -> Result<(), &'static str> {
        let start = self.code.len();
        let storage_offset = self.tile_base_slot();
        self.bytes(&[0x89, 0x9c, 0x24]);
        self.u32(storage_offset);
        self.parallel_abi(start, ParallelRangePart::TileBase { storage_offset });
        self.tile_input();
        for (phase_index, phase) in self.stage.tile_phases().into_iter().enumerate() {
            let start = self.code.len();
            self.bytes(&[0x8b, 0x9c, 0x24]);
            self.u32(self.tile_base_slot());
            self.parallel_abi(start, ParallelRangePart::TilePhaseEnter { phase: phase_index });
            let begin = self.code.len();
            self.tile_restore(phase_index, &phase.live_in);
            self.block(phase.code)?;
            if let Some(inst @ Inst { kind: InstKind::TilePublish(publication), .. }) = phase.step {
                let start = self.code.len();
                let first = self.operation_ranges.as_ref().map(Vec::len);
                self.publish_values(publication);
                self.operation_range(inst, start, first);
            }
            self.tile_save(phase_index, &phase.preserved_out());
            let start = self.code.len();
            self.bytes(&[0xff, 0xc3, 0x89, 0xd8, 0x2b, 0x84, 0x24]);
            self.u32(self.tile_base_slot());
            self.bytes(&[0x3b, 0x84, 0x24]);
            self.u32(self.tile_length_slot());
            let again = self.jump(Branch::Flags(0x82));
            self.patch(again, begin);
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
        for (word, result) in &collective.results {
            let start = self.code.len();
            self.bytes(&[0x4c, 0x8d, 0x94, 0x24]);
            let input_offset = self.shared_stream(self.stage.shared_word(collective.tile, *word))
                + match self.stage.shared_word(collective.tile, *word) { SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4 };
            self.u32(input_offset);
            self.bytes(&[0x4c, 0x8d, 0x9c, 0x24]);
            let output_offset = self.private_stream(PrivateSlot::Register(*result));
            self.u32(output_offset);
            self.bytes(&[0x8b, 0x8c, 0x24]);
            let length_offset = self.tile_length_slot();
            self.u32(length_offset);
            let scalar = self.stage.registers[*result];
            let plane_stride = self.stage.workgroup_width()*4;
            self.parallel_abi(start, ParallelRangePart::TileCollective { tile: collective.tile, word: *word, result: *result, scalar, operation: collective.op,
                part: TileCollectivePart::Setup { input_offset, output_offset, length_offset, plane_stride } });
            let float = scalar.is_float();
            let wide = scalar.size() == 8;
            let operator = collective.op.operator();
            let identity = operator.identity(scalar);
            let high_delta = if wide {
                (self.shared_stream(self.stage.shared_word(collective.tile, *word + 4)) as i64
                    - self.shared_stream(self.stage.shared_word(collective.tile, *word)) as i64) as u32
            } else { 0 };
            let store = |e: &mut Self| {
                if float && !wide {
                    e.bytes(&[0xf3, 0x41, 0x0f, 0x11, 0x03]);
                } else {
                    if float { e.copy_scalar(allocation::Location::Gpr(0), allocation::Location::Xmm(0), scalar); }
                    e.bytes(&[0x41, 0x89, 0x03]);
                    if wide {
                        e.bytes(&[0x48, 0x89, 0xc2, 0x48, 0xc1, 0xea, 32, 0x41, 0x89, 0x93]);
                        e.u32(plane_stride);
                    }
                }
            };
            let start = self.code.len();
            if identity == 0 {
                self.bytes(if float {
                    &[0x0f, 0x57, 0xc0]
                } else {
                    &[0x31, 0xc0]
                });
            } else {
                if wide {self.bytes(&[0x48]);}
                self.bytes(&[0xb8]);
                if wide {self.bytes(&identity.to_le_bytes());} else {self.u32(identity as u32);}
                if float {
                    self.instruction(Some(0x66), &[0x0f, 0x6e], wide, 0, operands::Operand::Register(0));
                }
            }
            self.parallel_abi(start, ParallelRangePart::TileCollective { tile: collective.tile, word: *word, result: *result, scalar, operation: collective.op,
                part: TileCollectivePart::Identity { bits: identity } });
            let begin = self.code.len();
            if collective.op.exclusive() {
                let start = self.code.len();
                store(self);
                self.bytes(&[0x49, 0x83, 0xc3, 0x04]);
                self.parallel_abi(start, ParallelRangePart::TileCollective { tile: collective.tile, word: *word, result: *result, scalar, operation: collective.op,
                    part: TileCollectivePart::ScanOutput });
            }
            let start = self.code.len();
            if float && wide {
                self.instruction(None, &[0x8b], false, 2, operands::Operand::memory(10, 0));
                self.instruction(None, &[0x8b], false, 8, operands::Operand::memory(10, high_delta));
                self.bytes(&[0x49, 0xc1, 0xe0, 32, 0x4c, 0x09, 0xc2]);
                self.copy_scalar(allocation::Location::Xmm(1), allocation::Location::Gpr(2), scalar);
                let opcode = match operator { CollectiveOperator::Min=>0x5d, CollectiveOperator::Max=>0x5f, _=>0x58 };
                self.instruction(Some(0xf2), &[0x0f, opcode], false, 0, operands::Operand::Register(1));
            } else if float {
                self.bytes(&[
                    0xf3,
                    0x41,
                    0x0f,
                    match operator {
                        CollectiveOperator::Sum => Some(0x58),
                        CollectiveOperator::Min => Some(0x5d),
                        CollectiveOperator::Max => Some(0x5f),
                        CollectiveOperator::BitAnd
                        | CollectiveOperator::BitOr
                        | CollectiveOperator::BitXor => None,
                    }
                    .unwrap(),
                    0x02,
                ]);
            } else {
                let operand = if wide {
                    self.instruction(None, &[0x8b], false, 2, operands::Operand::memory(10, 0));
                    self.instruction(None, &[0x8b], false, 8, operands::Operand::memory(10, high_delta));
                    self.bytes(&[0x49, 0xc1, 0xe0, 32, 0x4c, 0x09, 0xc2]);
                    operands::Operand::Register(2)
                } else { operands::Operand::memory(10, 0) };
                let prefix = (scalar.size() == 2).then_some(0x66);
                let byte = u8::from(scalar.size() == 1);
                let opcode = match operator {
                    CollectiveOperator::Sum => Some(0x03),
                    CollectiveOperator::BitAnd => Some(0x23),
                    CollectiveOperator::BitOr => Some(0x0b),
                    CollectiveOperator::BitXor => Some(0x33),
                    CollectiveOperator::Min | CollectiveOperator::Max => None,
                };
                if let Some(opcode) = opcode {
                    self.instruction(prefix, &[opcode - byte], wide, 0, operand);
                } else {
                    let condition = match (scalar.is_signed(), operator == CollectiveOperator::Min) {
                        (false,true)=>0x47,(true,true)=>0x4f,(false,false)=>0x42,(true,false)=>0x4c,
                    };
                    self.instruction(prefix, &[0x3b - byte], wide, 0, operand);
                    if byte != 0 {
                        let skip = self.jump(Branch::Flags(0x80 + (condition ^ 1) - 0x40));
                        self.instruction(None, &[0x8a], false, 0, operand);
                        self.patch(skip, self.code.len());
                    } else { self.instruction(prefix, &[0x0f, condition], wide, 0, operand); }
                }
            }
            self.bytes(&[0x49, 0x83, 0xc2, 0x04, 0xff, 0xc9]);
            let again = self.jump(Branch::Flags(0x85));
            self.patch(again, begin);
            self.parallel_abi(start, ParallelRangePart::TileCollective { tile: collective.tile, word: *word, result: *result, scalar, operation: collective.op,
                part: TileCollectivePart::Accumulate { input_high_delta: high_delta, loop_start: begin } });
            if !collective.op.exclusive() {
                let start = self.code.len();
                self.bytes(&[0x8b, 0x8c, 0x24]);
                self.u32(length_offset);
                let fill = self.code.len();
                store(self);
                self.bytes(&[0x49, 0x83, 0xc3, 0x04, 0xff, 0xc9]);
                let again = self.jump(Branch::Flags(0x85));
                self.patch(again, fill);
                self.parallel_abi(start, ParallelRangePart::TileCollective { tile: collective.tile, word: *word, result: *result, scalar, operation: collective.op,
                    part: TileCollectivePart::Fill { length_offset, loop_start: fill } });
            }
        }
    }
}
