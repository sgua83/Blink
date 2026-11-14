//! 输入与中间发布共用共享布局；阶段屏障覆盖所有 workgroup invocations。
use super::*;

pub(super) enum TileByte {
    Fixed(u32),
    Dynamic(u32),
}

impl Emitter<'_> {
    pub(super) fn tile_input(&mut self, count: u32, active: u32) {
        let u = self.types[&Scalar::U32];
        let width = self.constant(Scalar::U32, self.workgroup_width);
        self.tile_lane = self.id();
        self.emit(137, &[u, self.tile_lane, self.index, width]);
        let remaining = self.group_remaining(self.tile_lane, count);
        let tail = self.id();
        self.emit(176, &[self.types[&Scalar::Bool], tail, remaining, width]);
        self.tile_length = self.id();
        self.select(u, self.tile_length, tail, remaining, width);
        let mut inputs = self.stage.input_words();
        for (key, (unit, _, part)) in &self.tile_layout {
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
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, load, end]);
        self.emit(248, &[load]);
        for ((source, word), fragments) in inputs {
            let key = SharedWord::Input { binding: source, word };
            let (unit, _, part) = self.tile_layout[&key];
            if unit == Scalar::U64 {
                if part == 0 {
                    let raw = self.load_raw_buffer(unit, source, self.index, word);
                    let at = self.tile_word_address(key, self.tile_lane, None).0;
                    self.emit(62, &[at, raw]);
                }
                continue;
            }
            let components: Vec<_> = fragments.into_iter().map(|(byte, unit)| {
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
            self.emit(62, &[target.0, raw]);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        self.tile_barrier();
    }
    pub(super) fn tile_data_words(&self, unit: Scalar) -> u32 {
        self.tile_layout.values().filter(|(kind, _, _)| *kind == unit)
            .map(|(_, slot, _)| (*slot as u32 + 1) * self.workgroup_width).max().unwrap_or(0)
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
                    self.emit(62, &[at, raw]);
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
            if let Some(InstKind::TilePublish(publication)) = phase.step.map(|inst| &inst.kind) {
                self.publish_values(publication);
            }
            let producer = self.current_label;
            self.emit(249, &[end]);
            self.emit(248, &[end]);
            // 有效 lane 的私有 SSA 值跨发布点延续；无效尾 lane 只参加统一屏障。
            for slot in &phase.preserved_out() {
                if let PrivateSlot::Register(r) = slot {
                    // Module constants already dominate every lane and phase.
                    if self.constants.values().any(|&id| id == self.value(*r)) { continue; }
                    let value = self.id();
                    let zero = self.constant(self.stage.registers[*r], 0);
                    self.emit(
                        245,
                        &[self.ty(*r), value, self.value(*r), producer, zero, before],
                    );
                    self.regs[*r] = value;
                }
            }
            if matches!(phase.step.map(|inst| &inst.kind), Some(InstKind::TilePublish(_) | InstKind::TileBarrier)) {
                self.tile_barrier();
            }
            if let Some(InstKind::TileCollective(c)) = phase.step.map(|inst| &inst.kind) {
                self.tile_collective(c);
            }
        }
        Ok(())
    }
}
