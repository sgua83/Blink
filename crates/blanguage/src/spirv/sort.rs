//! 4-bit LSD 基数排序。subgroup 合成逻辑序号，稳定性不依赖 LocalInvocationIndex 的硬件映射。
use super::*;
unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::encode_sort_gather"]
    fn encode_sort_gather(
        indices: PageAddress, layout: *const RecordCopy, active: u32, body: *mut u32,
    ) -> StageWords;
}

#[repr(C)]
struct SortKeyInput {
    target: PageInputs,
    source: PageInputs,
    scalar: ScalarCode,
    descending: u32,
    first: u32,
    active: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::sort_initialize"]
    fn initialize_sort_key(input: SortKeyInput, host: MemoryHost) -> u32;
}

impl Emitter<'_> {
    pub(super) fn sort_initialize(
        &mut self,
        s: &Sorting,
        key_index: usize,
        first: bool,
        active: u32,
    ) {
        let (word, scalar, order) = s.keys[key_index];
        let target = self.page_inputs(
            self.buffers[s.target], self.index, Some(0), 0,
        );
        let source = self.page_inputs(
            self.buffers[s.source], self.index, Some(word), 0,
        );
        let input = SortKeyInput {
            target, source, scalar: scalar.into(),
            descending: u32::from(order == SortOrder::Descending),
            first: u32::from(first), active,
        };
        self.current_label = unsafe { initialize_sort_key(input, self.memory_host()) };
    }

    pub(super) fn sort_gather(&mut self, s: &Sorting, active: u32) {
        let layout = self.record_copy_input(s.source, s.target);
        let indices = self.page_input(
            Scalar::U32, self.buffers[self.program.buffers.len()],
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
            Scalar::U32, self.buffers[s.source], self.index, None, 0,
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
