//! Stable original indices and the indirect dispatch for the selected domain.
use super::{compact::ScatterInput, *};

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::encode_index_scatter"]
    fn encode_index_scatter(
        input: ScatterInput, target: u32, layout: DispatchLayout, max_x: u32, max_y: u32,
        first_byte: u32, scatter_words: usize, host: MemoryHost,
    ) -> u32;
}

impl Emitter<'_> {
    pub(super) fn selection_scatter(
        &mut self, selection: Selection, count: u32, active: u32, groups: [u32; 2],
    ) {
        let first = self.continuation_byte();
        let layout = self.dispatch_layout();
        let target = self.buffers[self.program.buffers.len()];
        let input = self.scatter_input(
            selection.binding, selection.word, self.execution_buffers.selection(), count, active,
        );
        let scatter_words = ScatterInput::WORD_CAPACITY + ScatterInput::STORE_CAPACITY;
        self.current_label = unsafe {
            encode_index_scatter(input, target, layout, groups[0], groups[1], first, scatter_words, self.memory_host())
        };
    }
}
