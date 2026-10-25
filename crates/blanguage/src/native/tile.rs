//! 每个原生范围拥有完整逻辑 tile；发布后按阶段执行所有 lane，私有值随 lane 保存。
use super::*;
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
    pub(super) fn tile_load(&mut self, dst: Reg, tile: usize, index: Reg, word: u32) {
        self.load(index);
        self.bytes(&[0x48, 0x69, 0xc0]);
        self.u32(4);
        self.bytes(&[0x49, 0x89, 0xe7, 0x49, 0x01, 0xc7]);
        self.read_scalar(
            dst,
            15,
            self.tile_storage_base()
                + self.stage.word_offset(self.stage.shared_word(tile, word)) as u32,
        );
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
        for (source, word) in self.stage.input_words() {
            self.bytes(&[0x4d, 0x8b, 0x94, 0x24]);
            self.u32(source as u32 * 8);
            self.bytes(&[0x89, 0xd8, 0x48, 0x69, 0xc0]);
            self.u32(self.program.buffers[source].stride);
            self.bytes(&[0x49, 0x01, 0xc2, 0x4c, 0x8d, 0x9c, 0x24]);
            self.u32(
                self.tile_storage_base()
                    + self.stage.word_offset(SharedWord::Input {
                        binding: source,
                        word,
                    }) as u32,
            );
            self.bytes(&[0x8b, 0x8c, 0x24]);
            self.u32(self.tile_length_slot());
            let begin = self.code.len();
            self.bytes(&[0x41, 0x8b, 0x82]);
            self.u32(word * 4);
            self.bytes(&[0x41, 0x89, 0x03]);
            self.bytes(&[0x49, 0x81, 0xc2]);
            self.u32(self.program.buffers[source].stride);
            self.bytes(&[0x49, 0x83, 0xc3, 0x04, 0xff, 0xc9]);
            let again = self.jump(Some(0x85));
            self.patch(again, begin);
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
        self.tile_storage_base()
            + self.stage.tile_data_bytes() as u32
            + self.stage.tile_private_slots().range(..slot).count() as u32
                * self.stage.workgroup_width()
                * 4
    }
    fn tile_restore(&mut self, slots: &std::collections::BTreeSet<PrivateSlot>) {
        self.tile_lane_pointer();
        for slot in slots {
            self.bytes(&[0x41, 0x8b, 0x82]);
            self.u32(self.private_stream(*slot));
            self.store_location(self.allocation.location(*slot));
        }
    }
    fn tile_save(&mut self, slots: &std::collections::BTreeSet<PrivateSlot>) {
        self.tile_lane_pointer();
        for slot in slots {
            self.load_location(self.allocation.location(*slot));
            self.bytes(&[0x41, 0x89, 0x82]);
            self.u32(self.private_stream(*slot));
        }
    }
    pub(super) fn publish_values(&mut self, publication: &Publication) {
        self.tile_lane_pointer();
        for (word, value) in &publication.values {
            self.load(*value);
            self.bytes(&[0x41, 0x89, 0x82]);
            self.u32(
                self.tile_storage_base()
                    + self.stage.word_offset(SharedWord::Published {
                        tile: publication.tile,
                        word: *word,
                    }) as u32,
            );
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
                        as u32,
            );
            self.bytes(&[0x4c, 0x8d, 0x9c, 0x24]);
            self.u32(self.private_stream(PrivateSlot::Register(*result)));
            self.bytes(&[0x8b, 0x8c, 0x24]);
            self.u32(self.tile_length_slot());
            let scalar = self.stage.registers[*result];
            let float = scalar == Scalar::F32;
            let operator = collective.op.operator();
            let identity = operator.identity(scalar);
            if identity == 0 {
                self.bytes(if float {
                    &[0x0f, 0x57, 0xc0]
                } else {
                    &[0x31, 0xc0]
                });
            } else {
                self.bytes(&[0xb8]);
                self.u32(identity);
                if float {
                    self.bytes(&[0x66, 0x0f, 0x6e, 0xc0]);
                }
            }
            let begin = self.code.len();
            if collective.op.exclusive() {
                self.bytes(if float {
                    &[0xf3, 0x41, 0x0f, 0x11, 0x03]
                } else {
                    &[0x41, 0x89, 0x03]
                });
                self.bytes(&[0x49, 0x83, 0xc3, 0x04]);
            }
            if float {
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
                let opcode = match operator {
                    CollectiveOperator::Sum => Some(0x03),
                    CollectiveOperator::BitAnd => Some(0x23),
                    CollectiveOperator::BitOr => Some(0x0b),
                    CollectiveOperator::BitXor => Some(0x33),
                    CollectiveOperator::Min | CollectiveOperator::Max => None,
                };
                if let Some(opcode) = opcode {
                    self.bytes(&[0x41, opcode, 0x02]);
                } else {
                    self.bytes(&[0x41, 0x8b, 0x12, 0x39, 0xd0, 0x0f]);
                    let min = operator == CollectiveOperator::Min;
                    self.bytes(&[
                        match (scalar == Scalar::I32, min) {
                            (false, true) => 0x47,
                            (true, true) => 0x4f,
                            (false, false) => 0x42,
                            (true, false) => 0x4c,
                        },
                        0xc2,
                    ]);
                }
            }
            self.bytes(&[0x49, 0x83, 0xc2, 0x04, 0xff, 0xc9]);
            let again = self.jump(Some(0x85));
            self.patch(again, begin);
            if !collective.op.exclusive() {
                self.bytes(&[0x8b, 0x8c, 0x24]);
                self.u32(self.tile_length_slot());
                let fill = self.code.len();
                self.bytes(if float {
                    &[0xf3, 0x41, 0x0f, 0x11, 0x03]
                } else {
                    &[0x41, 0x89, 0x03]
                });
                self.bytes(&[0x49, 0x83, 0xc3, 0x04, 0xff, 0xc9]);
                let again = self.jump(Some(0x85));
                self.patch(again, fill);
            }
        }
    }
}
