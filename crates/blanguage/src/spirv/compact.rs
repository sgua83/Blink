//! 由布尔独占前缀产生唯一目标序号，保持原始记录字节与输入顺序。
use super::*;
impl Emitter<'_> {
    fn compact_when(&mut self, condition: u32, body: impl FnOnce(&mut Self)) {
        let run = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[condition, run, end]);
        self.emit(248, &[run]);
        body(self);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
    pub(super) fn compact_scatter(&mut self, c: &Compaction, count: u32, active: u32) {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let one = self.constant(Scalar::U32, 1);
        let first = self.id();
        self.emit(170, &[b, first, self.index, zero]);
        self.compact_when(first, |e| {
            let empty = e.id();
            e.emit(170, &[b, empty, count, zero]);
            e.compact_when(empty, |e| {
                let target = e.address_scalar(c.count, zero, 1, c.count_word);
                e.emit(62, &[target, zero, 2, 4]);
            });
        });
        self.compact_when(active, |e| {
            let flag = e.address_scalar(
                c.flags,
                e.index,
                e.program.buffers[c.flags].stride / 4,
                c.flag_word,
            );
            let value = e.id();
            e.emit(61, &[u, value, flag, 2, 4]);
            let keep = e.id();
            e.emit(171, &[b, keep, value, zero]);
            let at = e.address_scalar(e.program.buffers.len(), e.index, 1, 0);
            let prefix = e.id();
            e.emit(61, &[u, prefix, at, 2, 4]);
            e.compact_when(keep, |e| {
                let stride = e.program.buffers[c.source].stride / 4;
                for word in 0..stride {
                    let source = e.address_scalar(c.source, e.index, stride, word);
                    let value = e.id();
                    e.emit(61, &[u, value, source, 2, 4]);
                    let target = e.address_scalar(c.target, prefix, stride, word);
                    e.emit(62, &[target, value, 2, 4]);
                }
            });
            let next = e.id();
            e.emit(128, &[u, next, e.index, one]);
            let last = e.id();
            e.emit(170, &[b, last, next, count]);
            e.compact_when(last, |e| {
                let chosen = e.id();
                e.emit(169, &[u, chosen, keep, one, zero]);
                let total = e.id();
                e.emit(128, &[u, total, prefix, chosen]);
                let target = e.address_scalar(c.count, zero, 1, c.count_word);
                e.emit(62, &[target, total, 2, 4]);
            });
        });
    }
}
