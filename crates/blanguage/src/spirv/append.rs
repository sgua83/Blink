//! 活动 subgroup 统一预留一段容量，再按实际活动 rank 写入各自记录。
use super::*;

impl Emitter<'_> {
    pub(super) fn append(&mut self, a: &Append) {
        let relaxed = self.constant(Scalar::U32, 0);
        let counter = self.address_scalar(Scalar::U32, a.count.binding, relaxed, a.count.word);
        let input = AppendEmission {
            uint_type: self.types[&Scalar::U32],
            bool_type: self.types[&Scalar::Bool],
            ballot_type: self.ballot_type,
            device_scope: self.constant(Scalar::U32, 1),
            relaxed,
            subgroup_scope: self.constant(Scalar::U32, 3),
            active: self.constant(Scalar::Bool, 1),
            counter,
            push_pointer_type: self.ptr_push_u32,
            push: self.push,
            capacity_word: self.constant(Scalar::U32, 2 + a.target as u32),
            outer: self.current_label,
            accepted: self.value(a.accepted),
            next: self.next,
        };
        let at = self.body.len();
        self.body.reserve(173);
        let reservation = unsafe {
            let emitted = encode_append_reservation(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = reservation.next;
        self.current_label = reservation.write_label;
        for (word, value) in &a.values {
            let scalar = self.stage.registers[*value];
            let raw = self.raw_bits(*value);
            self.store_raw_buffer(scalar, a.target, reservation.ticket, *word, raw);
        }
        let finish = reservation.finish;
        self.emit_control(4, |body| unsafe { next_control_block(finish, finish, 1, body) });
    }
}
