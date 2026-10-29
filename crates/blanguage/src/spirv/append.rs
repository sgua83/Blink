//! 活动 subgroup 统一预留一段容量，再按实际活动 rank 写入各自记录。
use super::*;
impl Emitter<'_> {
    pub(super) fn append(&mut self, a: &Append) {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let one = self.constant(Scalar::U32, 1);
        let scope = one; // Device
        let relaxed = zero;
        let subgroup = self.constant(Scalar::U32, 3);
        let yes = self.constant(Scalar::Bool, 1);
        let ballot = self.id();
        self.emit(339, &[self.ballot_type, ballot, subgroup, yes]);
        let size = self.id();
        self.emit(342, &[u, size, subgroup, 0, ballot]);
        let rank = self.id();
        self.emit(342, &[u, rank, subgroup, 2, ballot]);
        let leader = self.id();
        self.emit(333, &[b, leader, subgroup]);
        let counter = self.address_scalar(
            a.count.binding,
            zero,
            self.program.buffers[a.count.binding].stride,
            a.count.word,
        );
        let word = self.constant(Scalar::U32, 2 + a.target as u32);
        let ptr = self.id();
        self.emit(65, &[self.ptr_push_u32, ptr, self.push, zero, word]);
        let capacity = self.id();
        self.emit(61, &[u, capacity, ptr, 2, 4]);
        let outer = self.current_label;
        let reserve = self.id();
        let gather = self.id();
        self.emit(247, &[gather, 0]);
        self.emit(250, &[leader, reserve, gather]);
        self.emit(248, &[reserve]);
        let initial = self.id();
        self.emit(227, &[u, initial, counter, scope, relaxed]);
        let entry = self.current_label;
        let header = self.id();
        let check = self.id();
        let attempt = self.id();
        let next = self.id();
        let end = self.id();
        let old = self.id();
        let observed = self.id();
        self.emit(249, &[header]);
        self.emit(248, &[header]);
        self.emit(245, &[u, old, initial, entry, observed, next]);
        self.emit(246, &[end, next, 0]);
        self.emit(249, &[check]);
        self.emit(248, &[check]);
        let room = self.id();
        self.emit(176, &[b, room, old, capacity]);
        self.emit(250, &[room, attempt, end]);
        self.emit(248, &[attempt]);
        let available = self.id();
        self.emit(130, &[u, available, capacity, old]);
        let partial = self.id();
        self.emit(176, &[b, partial, available, size]);
        let amount = self.id();
        self.emit(169, &[u, amount, partial, available, size]);
        let desired = self.id();
        self.emit(128, &[u, desired, old, amount]);
        self.emit(
            230,
            &[u, observed, counter, scope, relaxed, relaxed, desired, old],
        );
        let accepted = self.id();
        self.emit(170, &[b, accepted, observed, old]);
        self.emit(250, &[accepted, end, next]);
        self.emit(248, &[next]);
        self.emit(249, &[header]);
        self.emit(248, &[end]);
        let leased = self.id();
        self.emit(245, &[u, leased, zero, check, amount, attempt]);
        self.emit(249, &[gather]);
        self.emit(248, &[gather]);
        let base = self.id();
        self.emit(245, &[u, base, zero, outer, old, end]);
        let length = self.id();
        self.emit(245, &[u, length, zero, outer, leased, end]);
        let shared_base = self.id();
        self.emit(338, &[u, shared_base, subgroup, base]);
        let shared_length = self.id();
        self.emit(338, &[u, shared_length, subgroup, length]);
        self.emit(176, &[b, self.value(a.accepted), rank, shared_length]);
        let write = self.id();
        let finish = self.id();
        self.emit(247, &[finish, 0]);
        self.emit(250, &[self.value(a.accepted), write, finish]);
        self.emit(248, &[write]);
        let ticket = self.id();
        self.emit(128, &[u, ticket, shared_base, rank]);
        for (word, value) in &a.values {
            let scalar = self.stage.registers[*value];
            let raw = self.to_words(*value, Self::raw_scalar(scalar))[0];
            self.store_raw_buffer(scalar, a.target, ticket, self.program.buffers[a.target].stride, *word, raw);
        }
        self.emit(249, &[finish]);
        self.emit(248, &[finish]);
    }
}
