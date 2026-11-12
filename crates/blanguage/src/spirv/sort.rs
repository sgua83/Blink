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
    fn sort_store(&mut self, binding: usize, index: u32, word: u32, value: u32) {
        let at = self.address_scalar(Scalar::U32, binding, index, word);
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
                e.load_raw_buffer(Scalar::U32, s.target, e.index, 4)
            };
            let scalar = s.keys[key_index].1;
            let unit = Self::physical_unit(scalar);
            let [at, high] = e.paged_pointers(
                unit, PageSource::Dataset(e.buffers[s.source]), original,
                s.keys[key_index].0, 0, scalar == Scalar::F64,
            );
            let key = e.load_raw(Scalar::U32, unit, &[at], true);
            let bias = scalar.order_bias() as u32;
            let key = if scalar.is_float() {
                let high = if scalar == Scalar::F64 {
                    e.load_raw(Scalar::U32, Scalar::U32, &[high], true)
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
            e.sort_store(s.target, e.index, 0, key);
            e.sort_store(s.target, e.index, 4, original);
        });
    }
    pub(super) fn sort_gather(&mut self, s: &Sorting, active: u32) {
        self.sort_when(active, |e| {
            let original = e.load_raw_buffer(Scalar::U32, e.program.buffers.len(), e.index, 4);
            e.copy_record(s.source, original, s.target, e.index);
        });
    }
    pub(super) fn sort_rank(
        &mut self,
        s: &Sorting,
        shift: u32,
        histogram: bool,
        count: u32,
        inputs: &[u32],
        ballot_type: u32,
        shared: u32,
        shared_pointer: u32,
    ) {
        let source = self.page_input(
            Scalar::U32, PageSource::Dataset(self.buffers[s.source]), self.index, None, 0,
        );
        let totals = PageAddress { descriptor: self.buffers[self.program.buffers.len()], ..source };
        let target = PageAddress { descriptor: self.buffers[s.target], ..source };
        let input = SortRankEmission {
            source, totals, target,
            scalars: self.dispatch_scalars(),
            histogram: u32::from(histogram),
            count,
            index: self.index,
            label: self.current_label,
            wave_input: inputs[0],
            lane_input: inputs[1],
            ballot_type,
            shared,
            shared_pointer,
            shift: self.constant(Scalar::U32, shift),
            width: self.constant(Scalar::U32, 64),
            wave_width: self.constant(Scalar::U32, 32),
            bins: self.constant(Scalar::U32, 16),
            mask: self.constant(Scalar::U32, 15),
            bit_masks: [1, 2, 4, 8].map(|bit| self.constant(Scalar::U32, bit)),
            subgroup_scope: self.constant(Scalar::U32, 3),
            workgroup_scope: self.constant(Scalar::U32, 2),
            shared_semantics: self.constant(Scalar::U32, 0x108),
            pair_step: self.constant(Scalar::U64, 4),
            next: self.next,
        };
        // At most two paired addresses, one single address, and the fixed rank instructions.
        self.emit_stage(1024, |body| unsafe { encode_sort_rank(input, body) });
    }

}
