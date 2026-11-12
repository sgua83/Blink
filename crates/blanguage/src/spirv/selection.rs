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
    pub(super) fn selection_scatter(
        &mut self, selection: Selection, count: u32, active: u32, groups: [u32; 2],
    ) {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let one = self.constant(Scalar::U32, 1);
        let first = self.id();
        self.emit(170, &[b, first, self.index, zero]);
        self.selection_when(first, |e| {
            let empty = e.id();
            e.emit(170, &[b, empty, count, zero]);
            e.selection_when(empty, |e| e.dispatch_grid(zero, groups, PREFIX_LEVELS, 1, false, false));
        });
        self.selection_when(active, |e| {
            let flag = e.address_scalar(Scalar::U32,
                selection.binding,
                e.index,
                selection.word);
            let value = e.id();
            e.emit(61, &[u, value, flag, 2, 4]);
            let keep = e.id();
            e.emit(171, &[b, keep, value, zero]);
            let at = e.address_scalar(Scalar::U32, e.program.selection_buffer(), e.index, 0);
            let prefix = e.id();
            e.emit(61, &[u, prefix, at, 2, 4]);
            e.selection_when(keep, |e| {
                let target = e.address_scalar(Scalar::U32, e.program.buffers.len(), prefix, 0);
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
                e.dispatch_grid(total, groups, PREFIX_LEVELS, 1, false, false);
            });
        });
    }
}
