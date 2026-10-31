//! 每个原生范围拥有完整逻辑 tile；发布后按阶段执行所有 lane，私有值随 lane 保存。
use super::*;
use super::allocation::Location;
impl Emitter<'_> {
    fn tile_length_slot(&self) -> u32 {
        self.frame.end as u32
    }
    fn tile_base_slot(&self) -> u32 {
        self.tile_length_slot() + 4
    }
    pub(super) fn tile_storage_base(&self) -> u32 {
        self.tile_length_slot() + 4 + if self.stage.has_tile_steps() { 4 } else { 0 }
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
            let base = self.tile_storage_base();
            let bias = match self.stage.shared_word(tile, word) { SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4 };
            let low = Memory::new(15, base + self.stage.word_offset(self.stage.shared_word(tile, word)) as u32 + bias);
            let high = if scalar.size() == 8 { Memory::new(15, base + self.stage.word_offset(self.stage.shared_word(tile, word + 4)) as u32) } else { low };
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
        let width = self.stage.workgroup_width();
        self.bytes(&[0x89, 0xd8, 0x31, 0xd2, 0xb9]);
        self.u32(width);
        self.bytes(&[0xf7, 0xf1, 0x85, 0xd2]);
        let skip = self.jump(Some(0x85));
        // r14d 是对齐的范围末端；只有全局最后一块可以是不完整 tile。
        self.bytes(&[0x44, 0x89, 0xf0, 0x29, 0xd8, 0xb9]);
        self.u32(width);
        self.bytes(&[0x39, 0xc8, 0x0f, 0x47, 0xc1, 0x89, 0x84, 0x24]);
        self.u32(self.tile_length_slot());
        for ((source, word), fragments) in self.stage.input_words() {
          for (byte, unit) in fragments {
            self.bytes(&[0x4d, 0x8b, 0x94, 0x24]);
            self.u32(source as u32 * 8);
            self.bytes(&[0x89, 0xd8]);
            self.scale_index(0, self.program.buffers[source].stride);
            self.bytes(&[0x49, 0x01, 0xc2, 0x4c, 0x8d, 0x9c, 0x24]);
            self.u32(
                self.tile_storage_base()
                    + self.stage.word_offset(SharedWord::Input {
                        binding: source,
                        word,
                    }) as u32 + byte % 4,
            );
            self.bytes(&[0x8b, 0x8c, 0x24]);
            self.u32(self.tile_length_slot());
            let begin = self.code.len();
            if unit.size() < 4 {
                self.instruction(None, &[0x0f, 0xb6 + u8::from(unit.size() == 2)], false, 0, operands::Operand::memory(10, byte));
            } else {
                self.instruction(None, &[0x8b], false, 0, operands::Operand::memory(10, byte));
            }
            self.write_value_address(Location::Gpr(0), Memory::new(11, 0), unit.size());
            self.bytes(&[0x49, 0x81, 0xc2]);
            self.u32(self.program.buffers[source].stride);
            self.bytes(&[0x49, 0x83, 0xc3, 0x04, 0xff, 0xc9]);
            let again = self.jump(Some(0x85));
            self.patch(again, begin);
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
    fn tile_restore(&mut self, slots: &std::collections::BTreeSet<PrivateSlot>) {
        self.tile_lane_pointer();
        for slot in slots {
            self.bytes(&[0x41, 0x8b, 0x82]);
            self.u32(self.private_stream(*slot));
            let scalar = self.stage.private_scalar(*slot);
            if scalar.size() == 8 {
                self.bytes(&[0x41, 0x8b, 0x8a]);
                self.u32(self.private_stream(*slot) + self.stage.workgroup_width()*4);
                self.bytes(&[0x48, 0xc1, 0xe1, 32, 0x48, 0x09, 0xc8]);
            }
            self.copy_scalar(self.allocation.location(*slot), Location::Gpr(0), scalar);
        }
    }
    fn tile_save(&mut self, slots: &std::collections::BTreeSet<PrivateSlot>) {
        self.tile_lane_pointer();
        for slot in slots {
            let scalar = self.stage.private_scalar(*slot);
            self.copy_scalar(Location::Gpr(0), self.allocation.location(*slot), scalar);
            self.bytes(&[0x41, 0x89, 0x82]);
            self.u32(self.private_stream(*slot));
            if scalar.size() == 8 {
                self.bytes(&[0x48, 0xc1, 0xe8, 32, 0x41, 0x89, 0x82]);
                self.u32(self.private_stream(*slot) + self.stage.workgroup_width()*4);
            }
        }
    }
    pub(super) fn publish_values(&mut self, publication: &Publication) {
        self.tile_lane_pointer();
        let used = self.stage.storage_words();
        for (word, value) in &publication.values {
            self.load(*value);
            let scalar = self.stage.registers[*value];
            for part in 0..scalar.size().div_ceil(4) {
                if part != 0 { self.bytes(&[0x48, 0xc1, 0xe8, 32]); }
                let shared = SharedWord::Published { tile: publication.tile, word: *word + part * 4 };
                if used.contains(&shared) {
                    let at = self.tile_storage_base() + self.stage.word_offset(shared) as u32 + word % 4;
                    self.write_value_address(Location::Gpr(0), Memory::new(10, at), scalar.size().min(4));
                }
            }
        }
    }
    pub(super) fn tile_phases(&mut self) -> Result<(), &'static str> {
        self.bytes(&[0x89, 0x9c, 0x24]);
        self.u32(self.tile_base_slot());
        self.tile_input();
        for phase in self.stage.tile_phases() {
            self.bytes(&[0x8b, 0x9c, 0x24]);
            self.u32(self.tile_base_slot());
            let begin = self.code.len();
            self.tile_restore(&phase.live_in);
            self.block(phase.code)?;
            if let Some(TileStep::Publish(publication)) = phase.step {
                self.publish_values(publication);
            }
            self.tile_save(&phase.preserved_out());
            self.bytes(&[0xff, 0xc3, 0x89, 0xd8, 0x2b, 0x84, 0x24]);
            self.u32(self.tile_base_slot());
            self.bytes(&[0x3b, 0x84, 0x24]);
            self.u32(self.tile_length_slot());
            let again = self.jump(Some(0x82));
            self.patch(again, begin);
            if let Some(TileStep::Collective(collective)) = phase.step {
                self.tile_collective(collective);
            }
        }
        Ok(())
    }
    pub(super) fn tile_collective(&mut self, collective: &TileCollective) {
        for (word, result) in &collective.results {
            self.bytes(&[0x4c, 0x8d, 0x94, 0x24]);
            self.u32(
                self.tile_storage_base()
                    + self
                        .stage
                        .word_offset(self.stage.shared_word(collective.tile, *word))
                        as u32
                    + match self.stage.shared_word(collective.tile, *word) { SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4 },
            );
            self.bytes(&[0x4c, 0x8d, 0x9c, 0x24]);
            self.u32(self.private_stream(PrivateSlot::Register(*result)));
            self.bytes(&[0x8b, 0x8c, 0x24]);
            self.u32(self.tile_length_slot());
            let scalar = self.stage.registers[*result];
            let float = scalar.is_float();
            let wide = scalar.size() == 8;
            let operator = collective.op.operator();
            let identity = operator.identity(scalar);
            let high_delta = if wide {
                (self.stage.word_offset(self.stage.shared_word(collective.tile, *word + 4)) as i64
                    - self.stage.word_offset(self.stage.shared_word(collective.tile, *word)) as i64) as u32
            } else { 0 };
            let store = |e: &mut Self| {
                if float && !wide {
                    e.bytes(&[0xf3, 0x41, 0x0f, 0x11, 0x03]);
                } else {
                    if float { e.copy_scalar(allocation::Location::Gpr(0), allocation::Location::Xmm(0), scalar); }
                    e.bytes(&[0x41, 0x89, 0x03]);
                    if wide {
                        e.bytes(&[0x48, 0x89, 0xc2, 0x48, 0xc1, 0xea, 32, 0x41, 0x89, 0x93]);
                        e.u32(e.stage.workgroup_width()*4);
                    }
                }
            };
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
            let begin = self.code.len();
            if collective.op.exclusive() {
                store(self);
                self.bytes(&[0x49, 0x83, 0xc3, 0x04]);
            }
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
                        let skip = self.jump(Some(0x80 + (condition ^ 1) - 0x40));
                        self.instruction(None, &[0x8a], false, 0, operand);
                        self.patch(skip, self.code.len());
                    } else { self.instruction(prefix, &[0x0f, condition], wide, 0, operand); }
                }
            }
            self.bytes(&[0x49, 0x83, 0xc2, 0x04, 0xff, 0xc9]);
            let again = self.jump(Some(0x85));
            self.patch(again, begin);
            if !collective.op.exclusive() {
                self.bytes(&[0x8b, 0x8c, 0x24]);
                self.u32(self.tile_length_slot());
                let fill = self.code.len();
                store(self);
                self.bytes(&[0x49, 0x83, 0xc3, 0x04, 0xff, 0xc9]);
                let again = self.jump(Some(0x85));
                self.patch(again, fill);
            }
        }
    }
}
