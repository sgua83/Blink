//! 输入与中间发布共用共享布局；阶段屏障覆盖所有 workgroup invocations。
use super::*;
impl Emitter<'_> {
    pub(super) fn tile_input(&mut self, count: u32, active: u32) {
        let u = self.types[&Scalar::U32];
        let width = self.constant(Scalar::U32, self.stage.workgroup_width());
        self.tile_lane = self.id();
        self.emit(137, &[u, self.tile_lane, self.index, width]);
        let base = self.id();
        self.emit(130, &[u, base, self.index, self.tile_lane]);
        let remaining = self.id();
        self.emit(130, &[u, remaining, count, base]);
        let tail = self.id();
        self.emit(176, &[self.types[&Scalar::Bool], tail, remaining, width]);
        self.tile_length = self.id();
        self.emit(169, &[u, self.tile_length, tail, remaining, width]);
        if self.stage.input_words().is_empty() {
            return;
        }
        let load = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, load, end]);
        self.emit(248, &[load]);
        for ((source, word), fragments) in self.stage.input_words() {
            let components: Vec<_> = fragments.into_iter().map(|(byte, unit)| {
                let raw = self.load_raw_buffer(unit, source, self.index, self.program.buffers[source].stride, byte);
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
            self.emit(62, &[target.0, raw]);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        self.tile_barrier();
    }
    pub(super) fn tile_word_address(&mut self, word: SharedWord, index: u32, offset: Option<u32>) -> (u32, u32) {
        let u = self.types[&Scalar::U32];
        let bias = match word { SharedWord::Input { word, .. } | SharedWord::Published { word, .. } => word % 4 };
        let mut byte = self.constant(Scalar::U32, bias);
        let mut index = index;
        if let Some(offset) = offset {
            let added = self.id();
            self.emit(128, &[u, added, offset, byte]);
            byte = added;
            let four = self.constant(Scalar::U32, 4);
            let component = self.id();
            self.emit(134, &[u, component, byte, four]);
            let width = self.constant(Scalar::U32, self.stage.workgroup_width());
            let scaled = self.id();
            self.emit(132, &[u, scaled, component, width]);
            let lane = self.id();
            self.emit(128, &[u, lane, index, scaled]);
            index = lane;
        }
        let base = self.constant(Scalar::U32, (self.stage.word_offset(word) / 4) as u32);
        let at = self.id();
        self.emit(128, &[u, at, index, base]);
        (self.shared_at(self.tile_pointer, self.tile_shared, at), byte)
    }
    pub(super) fn tile_read(&mut self, scalar: Scalar, tile: usize, index: u32, word: u32, offset: Option<u32>) -> u32 {
        let mut pointers = Vec::new();
        let mut shift = 0;
        for part in 0..scalar.size().div_ceil(4) {
            let (at, byte) = self.tile_word_address(self.stage.shared_word(tile, word + part * 4), index, offset);
            pointers.push(at);
            shift = byte;
        }
        if scalar.size() < 4 {
            let four = self.constant(Scalar::U32, 4);
            let byte = self.id();
            self.emit(137, &[self.types[&Scalar::U32], byte, shift, four]);
            let eight = self.constant(Scalar::U32, 8);
            shift = self.id();
            self.emit(132, &[self.types[&Scalar::U32], shift, byte, eight]);
            let raw = self.id();
            let shifted = self.id();
            self.emit(61, &[self.types[&Scalar::U32], raw, pointers[0]]);
            self.emit(194, &[self.types[&Scalar::U32], shifted, raw, shift]);
            self.join_words(scalar, &[shifted], Scalar::U32)
        } else {
            self.load_raw(scalar, Scalar::U32, &pointers, false)
        }
    }
    pub(super) fn tile_barrier(&mut self) {
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x148);
        self.emit(224, &[scope, scope, semantics]);
    }
    pub(super) fn publish_values(&mut self, publication: &Publication) {
        let mut components = Vec::new();
        for (word, value) in &publication.values {
            let scalar = self.stage.registers[*value];
            components.push((scalar, *word, self.to_words(*value, Self::raw_scalar(scalar))[0]));
        }
        let packed = self.pack_words(self.stage.tiles[publication.tile].ty.size, components);
        let cells: std::collections::BTreeSet<_> = self.stage.storage_words().into_iter().filter_map(|word| match word {
            SharedWord::Published { tile, word } if tile == publication.tile => Some(word / 4),
            _ => None,
        }).collect();
        for word in cells {
            let at = self.tile_word_address(
                SharedWord::Published {
                    tile: publication.tile,
                    word: word * 4,
                },
                self.tile_lane,
                None,
            );
            self.emit(62, &[at.0, packed[word as usize]]);
        }
    }
    pub(super) fn tile_phases(&mut self, active: u32) -> Result<(), &'static str> {
        for phase in self.stage.tile_phases() {
            let before = self.current_label;
            let execute = self.id();
            let end = self.id();
            self.emit(247, &[end, 0]);
            self.emit(250, &[active, execute, end]);
            self.emit(248, &[execute]);
            self.block(phase.code, None)?;
            if let Some(TileStep::Publish(publication)) = phase.step {
                self.publish_values(publication);
            }
            let producer = self.current_label;
            self.emit(249, &[end]);
            self.emit(248, &[end]);
            // 有效 lane 的私有 SSA 值跨发布点延续；无效尾 lane 只参加统一屏障。
            for slot in &phase.preserved_out() {
                if let PrivateSlot::Register(r) = slot {
                    let value = self.id();
                    let zero = self.constant(self.stage.registers[*r], 0);
                    self.emit(
                        245,
                        &[self.ty(*r), value, self.value(*r), producer, zero, before],
                    );
                    self.regs[*r] = value;
                }
            }
            match phase.step {
                Some(TileStep::Publish(_) | TileStep::Barrier) => self.tile_barrier(),
                Some(TileStep::Collective(c)) => self.tile_collective(c),
                None => {}
            }
        }
        Ok(())
    }
}
