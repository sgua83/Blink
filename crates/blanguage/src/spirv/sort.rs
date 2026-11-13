//! 4-bit LSD 基数排序。subgroup 合成逻辑序号，稳定性不依赖 LocalInvocationIndex 的硬件映射。
use super::*;
unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::encode_sort_gather"]
    fn encode_sort_gather(
        indices: PageAddress, layout: *const RecordCopy, active: u32, body: *mut u32,
    ) -> StageWords;
}

#[repr(C)]
struct SortKeyWrite {
    type_id: u32,
    key: u32,
    sign: u32,
    zero: u32,
    shift: u32,
    bias: u32,
    inverted: u32,
    key_pointer: u32,
    index_pointer: u32,
    original: u32,
    next: u32,
    label: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::write_sort_key"]
    fn write_sort_key(input: SortKeyWrite, body: *mut u32) -> StageWords;
}

impl Emitter<'_> {
    pub(super) fn sort_initialize(
        &mut self,
        s: &Sorting,
        key_index: usize,
        first: bool,
        active: u32,
    ) {
        let run = self.id();
        let end = self.id();
        self.emit_control(9, |body| unsafe { open_selection(end, active, run, end, body) });

        // Both work fields belong to the same eight-byte record.
        let [key_pointer, index_pointer] = self.paged_pointers(
            Scalar::U32, PageSource::Dataset(self.buffers[s.target]), self.index, 0, 0, true,
        );
        let original = if first {
            self.index
        } else {
            self.load_raw(Scalar::U32, Scalar::U32, &[index_pointer], true)
        };
        let (word, scalar, order) = s.keys[key_index];
        let unit = Self::physical_unit(scalar);
        let [low, high] = self.paged_pointers(
            unit, PageSource::Dataset(self.buffers[s.source]), original, word, 0, scalar == Scalar::F64,
        );
        let key = self.load_raw(Scalar::U32, unit, &[low], true);
        let sign = if scalar == Scalar::F64 {
            self.load_raw(Scalar::U32, Scalar::U32, &[high], true)
        } else {
            key
        };
        let (zero, shift) = if scalar.is_float() {
            (self.constant(Scalar::U32, 0), self.constant(Scalar::U32, 31))
        } else {
            (0, 0)
        };
        let bias_bits = match scalar {
            Scalar::F32 => 0x8000_0000,
            Scalar::F64 => 0,
            _ => scalar.order_bias() as u32,
        };
        let bias = if bias_bits != 0 { self.constant(Scalar::U32, bias_bits) } else { 0 };
        let inverted = if order == SortOrder::Descending {
            self.constant(Scalar::U32, u32::MAX)
        } else {
            0
        };
        let input = SortKeyWrite {
            type_id: self.types[&Scalar::U32], key, sign, zero, shift, bias, inverted,
            key_pointer, index_pointer, original, next: self.next, label: run,
        };
        // At most five binary instructions followed by two aligned stores.
        self.emit_stage(35, |body| unsafe { write_sort_key(input, body) });
        self.emit_control(4, |body| unsafe { next_control_block(end, end, 1, body) });
    }

    pub(super) fn sort_gather(&mut self, s: &Sorting, active: u32) {
        let layout = self.record_copy_input(s.source, s.target);
        let indices = self.page_input(
            Scalar::U32, PageSource::Dataset(self.buffers[self.program.buffers.len()]),
            self.index, Some(4), 0,
        );
        // Branch, one paged u32 index load and the original record copy.
        let capacity = 13 + 109 + 6 + layout.as_ref().map_or(0, RecordCopy::word_capacity);
        self.emit_stage(capacity, |body| unsafe {
            encode_sort_gather(indices, layout.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
                active, body)
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
