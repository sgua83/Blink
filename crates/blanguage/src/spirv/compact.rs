//! Stable record scatter and the common input to both GPU scatter stages.
use super::*;

#[repr(C)]
pub(super) struct ScatterInput {
    flag: PageAddress,
    count: u32,
    active: u32,
    bool_type: u32,
    prefix_descriptor: u32,
}

impl ScatterInput {
    // Fixed control and arithmetic, plus two paged u32 loads.
    pub(super) const WORD_CAPACITY: usize = 326;
    pub(super) const STORE_CAPACITY: usize = 109 + 5;
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::encode_record_scatter"]
    fn encode_record_scatter(
        input: ScatterInput, copy: *const RecordCopy, count_descriptor: u32, count_word: u32,
        body: *mut u32,
    ) -> StageWords;
}

impl Emitter<'_> {
    pub(super) fn scatter_input(
        &mut self, flag: usize, flag_word: u32, prefix: usize, count: u32, active: u32,
    ) -> ScatterInput {
        ScatterInput {
            flag: self.page_input(
                Scalar::U32, PageSource::Dataset(self.buffers[flag]), self.index, Some(flag_word), 0,
            ),
            count,
            active,
            bool_type: self.types[&Scalar::Bool],
            prefix_descriptor: self.buffers[prefix],
        }
    }

    pub(super) fn compact_scatter(&mut self, c: &Compaction, count: u32, active: u32) {
        let count_word = self.constant(Scalar::U32, c.count_word);
        let count_descriptor = self.buffers[c.count];
        let copy = self.record_copy_input(c.source, c.target);
        let input = self.scatter_input(c.flags, c.flag_word, self.program.buffers.len(), count, active);
        let capacity = ScatterInput::WORD_CAPACITY + 2 * ScatterInput::STORE_CAPACITY
            + copy.as_ref().map_or(0, RecordCopy::word_capacity);
        self.emit_stage(capacity, |body| unsafe {
            encode_record_scatter(input, copy.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
                count_descriptor, count_word, body)
        });
    }
}
