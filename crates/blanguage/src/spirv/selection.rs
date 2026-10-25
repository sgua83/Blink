//! 原始索引的稳定筛选；计数与间接启动记录留在提交私有存储。
use super::*;
impl Emitter<'_> {
    fn selection_when(&mut self, condition: u32, body: impl FnOnce(&mut Self)) {
        let run = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[condition, run, end]);
        self.emit(248, &[run]);
        body(self);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
    fn selection_arguments(&mut self, count: u32) {
        let u = self.types[&Scalar::U32];
        let zero = self.constant(Scalar::U32, 0);
        let one = self.constant(Scalar::U32, 1);
        let at_x = self.address_scalar(self.program.indirect_buffer(), zero, 1, 0);
        let at_y = self.address_scalar(self.program.indirect_buffer(), one, 1, 0);
        let max_x = self.id();
        let max_y = self.id();
        self.emit(61, &[u, max_x, at_x, 2, 4]);
        self.emit(61, &[u, max_y, at_y, 2, 4]);
        let width = self.constant(Scalar::U32, self.stage.workgroup_width());
        let groups = self.prefix_ceil(count, width);
        let x = self.prefix_min(groups, max_x);
        let rows = self.prefix_ceil(groups, max_x);
        let y = self.prefix_min(rows, max_y);
        let z = self.prefix_ceil(rows, max_y);
        for (word, value) in [x, y, z, count].into_iter().enumerate() {
            let index = self.constant(Scalar::U32, (PREFIX_LEVELS * 4 + word) as u32);
            let target = self.address_scalar(self.program.indirect_buffer(), index, 1, 0);
            self.emit(62, &[target, value, 2, 4]);
        }
    }
    pub(super) fn selection_scatter(&mut self, selection: Selection, count: u32, active: u32) {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let one = self.constant(Scalar::U32, 1);
        let first = self.id();
        self.emit(170, &[b, first, self.index, zero]);
        self.selection_when(first, |e| {
            let empty = e.id();
            e.emit(170, &[b, empty, count, zero]);
            e.selection_when(empty, |e| e.selection_arguments(zero));
        });
        self.selection_when(active, |e| {
            let flag = e.address_scalar(
                selection.binding,
                e.index,
                e.program.buffers[selection.binding].stride / 4,
                selection.word,
            );
            let value = e.id();
            e.emit(61, &[u, value, flag, 2, 4]);
            let keep = e.id();
            e.emit(171, &[b, keep, value, zero]);
            let at = e.address_scalar(e.program.selection_buffer(), e.index, 1, 0);
            let prefix = e.id();
            e.emit(61, &[u, prefix, at, 2, 4]);
            e.selection_when(keep, |e| {
                let target = e.address_scalar(e.program.buffers.len(), prefix, 1, 0);
                e.emit(62, &[target, e.index, 2, 4]);
            });
            let next = e.id();
            e.emit(128, &[u, next, e.index, one]);
            let last = e.id();
            e.emit(170, &[b, last, next, count]);
            e.selection_when(last, |e| {
                let chosen = e.id();
                e.emit(169, &[u, chosen, keep, one, zero]);
                let total = e.id();
                e.emit(128, &[u, total, prefix, chosen]);
                e.selection_arguments(total);
            });
        });
    }
}
