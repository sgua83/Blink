//! 有效前缀的 GPU 间接参数与多维工作组索引；实际数量始终驻留设备。
use super::*;
impl Emitter<'_> {
    fn prefix_push(&mut self, word: u32) -> u32 {
        let zero = self.constant(Scalar::U32, 0);
        let word = self.constant(Scalar::U32, word);
        let at = self.id();
        self.emit(65, &[self.ptr_push_u32, at, self.push, zero, word]);
        let value = self.id();
        self.emit(61, &[self.types[&Scalar::U32], value, at]);
        value
    }
    pub(super) fn prefix_ceil(&mut self, value: u32, divisor: u32) -> u32 {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let quotient = self.id();
        let remainder = self.id();
        self.emit(134, &[u, quotient, value, divisor]);
        self.emit(137, &[u, remainder, value, divisor]);
        let zero = self.constant(Scalar::U32, 0);
        let one = self.constant(Scalar::U32, 1);
        let extra = self.id();
        self.emit(171, &[b, extra, remainder, zero]);
        let carry = self.id();
        self.emit(169, &[u, carry, extra, one, zero]);
        let result = self.id();
        self.emit(128, &[u, result, quotient, carry]);
        result
    }
    pub(super) fn prefix_min(&mut self, a: u32, b: u32) -> u32 {
        let less = self.id();
        self.emit(176, &[self.types[&Scalar::Bool], less, a, b]);
        let value = self.id();
        self.emit(169, &[self.types[&Scalar::U32], value, less, a, b]);
        value
    }
    pub(super) fn prefix_arguments(&mut self, prefix: &PrefixCount, invocation: u32) {
        let u = self.types[&Scalar::U32];
        let zero = self.constant(Scalar::U32, 0);
        let first = self.id();
        self.emit(170, &[self.types[&Scalar::Bool], first, invocation, zero]);
        let run = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[first, run, end]);
        self.emit(248, &[run]);
        let source = self.address_scalar(Scalar::U32,
            prefix.binding,
            zero,
            prefix.word);
        let count = self.id();
        self.emit(61, &[u, count, source, 2, 4]);
        self.domain_arguments(count);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
    pub(super) fn selection_source_arguments(&mut self, invocation: u32) {
        let zero = self.constant(Scalar::U32, 0);
        let condition = self.id();
        self.emit(
            170,
            &[self.types[&Scalar::Bool], condition, invocation, zero],
        );
        let run = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[condition, run, end]);
        self.emit(248, &[run]);
        let count = self.prefix_push(2 + self.stage.domain as u32);
        self.domain_arguments(count);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
    fn domain_arguments(&mut self, count: u32) {
        let u = self.types[&Scalar::U32];
        let zero = self.constant(Scalar::U32, 0);
        let width = self.constant(Scalar::U32, self.stage.workgroup_width());
        // 参数由执行框架写入目标设备的非零 X/Y 工作组上限。
        let max_x = self.prefix_push(0);
        let max_y = self.prefix_push(1);
        let collective =
            !matches!(self.stage.operation, Operation::Map) || self.stage.selection.is_some();
        let levels = if collective { PREFIX_LEVELS } else { 1 };
        let sorting = matches!(self.stage.operation, Operation::StableSort(_));
        let mut root_count = count;
        for series in 0..if sorting { 2 } else { 1 } {
            let mut count = root_count;
            for level in 0..levels {
                let mut groups = self.prefix_ceil(count, width);
                if collective {
                    // 空集合仍有一个组写出单位元／计数，下一层消费这个实际结果。
                    let empty = self.id();
                    self.emit(170, &[self.types[&Scalar::Bool], empty, groups, zero]);
                    let one = self.constant(Scalar::U32, 1);
                    let at_least_one = self.id();
                    self.emit(169, &[u, at_least_one, empty, one, groups]);
                    groups = at_least_one;
                }
                let x = self.prefix_min(groups, max_x);
                let rows = self.prefix_ceil(groups, max_x);
                let y = self.prefix_min(rows, max_y);
                let z = self.prefix_ceil(rows, max_y);
                for (word, value) in [x, y, z, count].into_iter().enumerate() {
                    let index = self.constant(
                        Scalar::U32,
                        ((series * PREFIX_LEVELS + level) * 4 + word) as u32,
                    );
                    let target = self.address_scalar(Scalar::U32, self.program.indirect_buffer(), index, 0);
                    self.emit(62, &[target, value, 2, 4]);
                }
                count = groups;
            }

            if sorting && series == 0 {
                let groups = self.prefix_ceil(root_count, width);
                let empty = self.id();
                self.emit(170, &[self.types[&Scalar::Bool], empty, groups, zero]);
                let one = self.constant(Scalar::U32, 1);
                let groups_nonzero = self.id();
                self.emit(169, &[u, groups_nonzero, empty, one, groups]);
                let bins = self.constant(Scalar::U32, 16);
                root_count = self.id();
                self.emit(132, &[u, root_count, groups_nonzero, bins]);
            }
        }
    }
    pub(super) fn prefix_index(
        &mut self,
        global: u32,
        global_x: u32,
        num_groups: u32,
        vector_type: u32,
        collective: bool,
    ) -> (u32, u32) {
        let three = self.constant(Scalar::U32, 3);
        let base = self.prefix_push(1);
        let word = self.id();
        self.emit(128, &[self.types[&Scalar::U32], word, base, three]);
        let count_pointer = self.address_scalar(Scalar::U32, self.program.indirect_buffer(), word, 0);
        let input = PrefixIndexEmission {
            uint_type: self.types[&Scalar::U32],
            wide_type: self.u64_type,
            bool_type: self.types[&Scalar::Bool],
            vector_type,
            global,
            global_x,
            num_groups,
            count_pointer,
            width: if collective { self.constant(Scalar::U32, self.workgroup_width) } else { 0 },
            wide_width: self.constant(Scalar::U64, self.workgroup_width),
            zero: self.constant(Scalar::U32, 0),
            one: self.constant(Scalar::U32, 1),
            collective: u32::from(collective),
            next: self.next,
        };
        let at = self.body.len();
        self.body.reserve(if collective { 134 } else { 88 });
        let emitted = unsafe {
            let emitted = encode_prefix_index(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            emitted
        };
        self.next = emitted.next;
        self.index = emitted.index;
        if collective {
            self.current_label = emitted.label;
        }
        (emitted.count, emitted.active)
    }
}
