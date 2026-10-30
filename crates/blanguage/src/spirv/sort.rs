//! 4-bit LSD 基数排序。subgroup 合成逻辑序号，稳定性不依赖 LocalInvocationIndex 的硬件映射。
use super::*;
impl Emitter<'_> {
    fn sort_when(&mut self, condition: u32, body: impl FnOnce(&mut Self)) {
        let run = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[condition, run, end]);
        self.emit(248, &[run]);
        body(self);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
    fn sort_binary(&mut self, op: u16, a: u32, b: u32) -> u32 {
        let v = self.id();
        self.emit(op, &[self.types[&Scalar::U32], v, a, b]);
        v
    }
    fn sort_store(&mut self, binding: usize, index: u32, stride: u32, word: u32, value: u32) {
        let at = self.address_scalar(Scalar::U32, binding, index, stride, word);
        self.emit(62, &[at, value, 2, 4]);
    }
    pub(super) fn sort_initialize(
        &mut self,
        s: &Sorting,
        key_index: usize,
        first: bool,
        active: u32,
    ) {
        self.sort_when(active, |e| {
            let original = if first {
                e.index
            } else {
                e.load_raw_buffer(Scalar::U32, s.target, e.index, 8, 4)
            };
            let scalar = s.keys[key_index].1;
            let unit = match scalar.size() { 1 => Scalar::U8, 2 => Scalar::U16, _ => Scalar::U32 };
            let at = e.address_scalar(unit, s.source, original, e.program.buffers[s.source].stride, s.keys[key_index].0);
            let key = e.load_raw(Scalar::U32, unit, &[at], true);
            let bias = scalar.order_bias() as u32;
            let key = if scalar.is_float() {
                let high = if scalar == Scalar::F64 {
                    e.load_raw_buffer(Scalar::U32, s.source, original, e.program.buffers[s.source].stride, s.keys[key_index].0 + 4)
                } else { key };
                let shift = e.constant(Scalar::U32, 31);
                let negative = e.sort_binary(194, high, shift);
                let zero = e.constant(Scalar::U32, 0);
                let inverse = e.sort_binary(130, zero, negative);
                let mask = if scalar == Scalar::F32 {
                    let sign = e.constant(Scalar::U32, 0x8000_0000);
                    e.sort_binary(197, inverse, sign)
                } else { inverse };
                e.sort_binary(198, key, mask)
            } else if bias != 0 {
                let sign = e.constant(Scalar::U32, bias);
                e.sort_binary(198, key, sign)
            } else {
                key
            };
            let key = if s.keys[key_index].2 == SortOrder::Descending {
                let mask = e.constant(Scalar::U32, u32::MAX);
                e.sort_binary(198, key, mask)
            } else {
                key
            };
            e.sort_store(s.target, e.index, 8, 0, key);
            e.sort_store(s.target, e.index, 8, 4, original);
        });
    }
    pub(super) fn sort_gather(&mut self, s: &Sorting, active: u32) {
        self.sort_when(active, |e| {
            let original = e.load_raw_buffer(Scalar::U32, e.program.buffers.len(), e.index, 8, 4);
            e.copy_record(s.source, original, s.target, e.index);
        });
    }
    fn sort_ballot(&mut self, predicate: u32, vector: u32) -> u32 {
        let subgroup = self.constant(Scalar::U32, 3);
        let ballot = self.id();
        self.emit(339, &[vector, ballot, subgroup, predicate]);
        let bits = self.id();
        self.emit(81, &[self.types[&Scalar::U32], bits, ballot, 0]);
        bits
    }
    pub(super) fn sort_rank(
        &mut self,
        s: &Sorting,
        shift: u32,
        histogram: bool,
        count: u32,
        inputs: &[u32],
        vector: u32,
        shared: u32,
        pointer: u32,
        temporary: u32,
    ) {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let one = self.constant(Scalar::U32, 1);
        let width = self.constant(Scalar::U32, 64);
        let half = self.constant(Scalar::U32, 32);
        let bins = self.constant(Scalar::U32, 16);
        let mask = self.constant(Scalar::U32, 15);
        let wave = self.id();
        let lane = self.id();
        self.emit(61, &[u, wave, inputs[0]]);
        self.emit(61, &[u, lane, inputs[1]]);
        let group = self.sort_binary(134, self.index, width);
        let local_base = self.sort_binary(132, wave, half);
        let local = self.sort_binary(128, local_base, lane);
        let base = self.sort_binary(132, group, width);
        self.index = self.sort_binary(128, base, local);
        let active = self.id();
        self.emit(176, &[b, active, self.index, count]);
        self.emit(62, &[temporary, zero]);
        self.sort_when(active, |e| {
            let key = e.load_raw_buffer(Scalar::U32, s.source, e.index, 8, 0);
            e.emit(62, &[temporary, key]);
        });
        let key = self.id();
        self.emit(61, &[u, key, temporary]);
        let shift = self.constant(Scalar::U32, shift);
        let shifted = self.sort_binary(194, key, shift);
        let digit = self.sort_binary(199, shifted, mask);
        let mut equal = self.sort_ballot(active, vector);
        for bit in 0..4 {
            let bit = self.constant(Scalar::U32, 1 << bit);
            let bits = self.sort_binary(199, digit, bit);
            let set = self.id();
            self.emit(171, &[b, set, bits, zero]);
            let selected = self.sort_ballot(set, vector);
            let inverted = self.id();
            self.emit(200, &[u, inverted, selected]);
            let matching = self.id();
            self.emit(169, &[u, matching, set, selected, inverted]);
            equal = self.sort_binary(199, equal, matching);
        }
        let bit = self.sort_binary(196, one, lane);
        let lower = self.sort_binary(130, bit, one);
        let prior = self.sort_binary(199, equal, lower);
        let rank = self.id();
        let total = self.id();
        self.emit(205, &[u, rank, prior]);
        self.emit(205, &[u, total, equal]);
        let init = self.id();
        self.emit(176, &[b, init, local, half]);
        self.sort_when(init, |e| {
            let at = e.shared_at(pointer, shared, local);
            e.emit(62, &[at, zero]);
        });
        self.scan_barrier();
        let first = self.id();
        self.emit(170, &[b, first, rank, zero]);
        let leader = self.id();
        self.emit(167, &[b, leader, first, active]);
        let wave_bin = self.sort_binary(132, wave, bins);
        let own_bin = self.sort_binary(128, wave_bin, digit);
        self.sort_when(leader, |e| {
            let at = e.shared_at(pointer, shared, own_bin);
            e.emit(62, &[at, total]);
        });
        self.scan_barrier();
        let groups = self.prefix_ceil(count, width);
        let empty = self.id();
        self.emit(170, &[b, empty, groups, zero]);
        let nonzero = self.id();
        self.emit(169, &[u, nonzero, empty, one, groups]);
        if histogram {
            let write = self.id();
            self.emit(176, &[b, write, local, bins]);
            self.sort_when(write, |e| {
                let a = e.shared_at(pointer, shared, local);
                let next = e.sort_binary(128, local, bins);
                let c = e.shared_at(pointer, shared, next);
                let x = e.id();
                let y = e.id();
                e.emit(61, &[u, x, a]);
                e.emit(61, &[u, y, c]);
                let total = e.sort_binary(128, x, y);
                let row = e.sort_binary(132, local, nonzero);
                let at = e.sort_binary(128, row, group);
                e.sort_store(e.program.buffers.len(), at, 4, 0, total);
            });
        } else {
            self.sort_when(active, |e| {
                let a = e.shared_at(pointer, shared, digit);
                let first = e.id();
                e.emit(61, &[u, first, a]);
                let other = e.id();
                e.emit(170, &[b, other, wave, zero]);
                let previous = e.id();
                e.emit(169, &[u, previous, other, zero, first]);
                let own_rank = e.sort_binary(128, rank, previous);
                let row = e.sort_binary(132, digit, nonzero);
                let at = e.sort_binary(128, row, group);
                let offset = e.load_raw_buffer(Scalar::U32, e.program.buffers.len(), at, 4, 0);
                let target = e.sort_binary(128, offset, own_rank);
                let original = e.load_raw_buffer(Scalar::U32, s.source, e.index, 8, 4);
                e.sort_store(s.target, target, 8, 0, key);
                e.sort_store(s.target, target, 8, 4, original);
            });
        }
    }
}
