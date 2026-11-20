//! Stable original indices and the indirect dispatch for the selected domain.
use super::{compact::ScatterInput, *};

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::encode_index_scatter"]
    fn encode_index_scatter(
        input: ScatterInput, target: u32, grid: DispatchGrid, body: *mut u32,
    ) -> StageWords;
}

impl Emitter<'_> {
    pub(super) fn selection_scatter(
        &mut self, selection: Selection, count: u32, active: u32, groups: [u32; 2],
    ) {
        let zero = self.constant(Scalar::U32, 0);
        let first = self.continuation_byte();
        let grid = self.dispatch_grid_input(zero, groups, 0, false, false, first);
        let target = self.buffers[self.program.buffers.len()];
        let input = self.scatter_input(
            selection.binding, selection.word, self.program.selection_buffer(), count, active,
        );
        let capacity = ScatterInput::WORD_CAPACITY + ScatterInput::STORE_CAPACITY
            + 2 * grid.word_capacity();
        self.emit_stage(capacity, |body| unsafe {
            encode_index_scatter(input, target, grid, body)
        });
    }
}
