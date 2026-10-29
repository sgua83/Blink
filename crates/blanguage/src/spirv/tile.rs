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
        for (source, word) in self.stage.input_words() {
            let at = self.address_scalar(
                source,
                self.index,
                self.program.buffers[source].stride / 4,
                word,
            );
            let raw = self.id();
            self.emit(61, &[u, raw, at, 2, 4]);
            let target = self.tile_word_address(
                SharedWord::Input {
                    binding: source,
                    word,
                },
                self.tile_lane,
            );
            self.emit(62, &[target, raw]);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        self.tile_barrier();
    }
    pub(super) fn tile_word_address(&mut self, word: SharedWord, index: u32) -> u32 {
        let u = self.types[&Scalar::U32];
        let offset = self.constant(Scalar::U32, (self.stage.word_offset(word) / 4) as u32);
        let at = self.id();
        self.emit(128, &[u, at, index, offset]);
        self.shared_at(self.tile_pointer, self.tile_shared, at)
    }
    pub(super) fn tile_load(&mut self, dst: Reg, tile: usize, index: Reg, word: u32) {
        let mut words = Vec::new();
        for part in 0..self.stage.registers[dst].size()/4 {
            let at = self.tile_word_address(self.stage.shared_word(tile, word + part), self.value(index));
            let raw = self.id();
            self.emit(61, &[self.types[&Scalar::U32], raw, at]);
            words.push(raw);
        }
        self.from_words(dst, &words);
    }
    pub(super) fn tile_barrier(&mut self) {
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x148);
        self.emit(224, &[scope, scope, semantics]);
    }
    pub(super) fn publish_values(&mut self, publication: &Publication) {
        for (word, value) in &publication.values {
          for (part, raw) in self.to_words(*value).into_iter().enumerate() {
            let at = self.tile_word_address(
                SharedWord::Published {
                    tile: publication.tile,
                    word: *word + part as u32,
                },
                self.tile_lane,
            );
            self.emit(62, &[at, raw]);
          }
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
