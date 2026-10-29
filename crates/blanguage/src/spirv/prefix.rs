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
        let source = self.address_scalar(
            prefix.binding,
            zero,
            self.program.buffers[prefix.binding].stride,
            prefix.word,
        );
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
                    let target = self.address_scalar(self.program.indirect_buffer(), index, 4, 0);
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
        gid: u32,
        num_groups: u32,
        vector_type: u32,
        collective: bool,
    ) -> (u32, u32) {
        let u = self.types[&Scalar::U32];
        let wide = self.u64_type;
        let groups = self.id();
        self.emit(61, &[vector_type, groups, num_groups]);
        let mut coordinates = Vec::new();
        for (vector, component) in [(gid, 0), (gid, 1), (gid, 2), (groups, 0), (groups, 1)] {
            let v = self.id();
            self.emit(81, &[u, v, vector, component]);
            let w = self.id();
            self.emit(113, &[wide, w, v]);
            coordinates.push(w);
        }
        let zrows = self.id();
        self.emit(132, &[wide, zrows, coordinates[2], coordinates[4]]);
        let row = self.id();
        self.emit(128, &[wide, row, coordinates[1], zrows]);
        let width = self.constant(Scalar::U32, self.stage.workgroup_width());
        let width64 = self.id();
        self.emit(113, &[wide, width64, width]);
        let pitch = self.id();
        self.emit(132, &[wide, pitch, coordinates[3], width64]);
        let offset = self.id();
        self.emit(132, &[wide, offset, row, pitch]);
        let index = self.id();
        self.emit(128, &[wide, index, coordinates[0], offset]);
        let three = self.constant(Scalar::U32, 3);
        let base = self.prefix_push(1);
        let word = self.id();
        self.emit(128, &[u, word, base, three]);
        let source = self.address_scalar(self.program.indirect_buffer(), word, 4, 0);
        let count = self.id();
        self.emit(61, &[u, count, source, 2, 4]);
        let count64 = self.id();
        self.emit(113, &[wide, count64, count]);
        let active = self.id();
        self.emit(176, &[self.types[&Scalar::Bool], active, index, count64]);
        if collective {
            // 矩形网格补齐的整组先统一退出，不能写越界部分和，也不能分歧进入屏障。
            let groups = self.prefix_ceil(count, width);
            let zero = self.constant(Scalar::U32, 0);
            let one = self.constant(Scalar::U32, 1);
            let empty = self.id();
            self.emit(170, &[self.types[&Scalar::Bool], empty, groups, zero]);
            let groups_nonzero = self.id();
            self.emit(169, &[u, groups_nonzero, empty, one, groups]);
            let group = self.id();
            self.emit(134, &[wide, group, index, width64]);
            let groups64 = self.id();
            self.emit(113, &[wide, groups64, groups_nonzero]);
            let present = self.id();
            self.emit(176, &[self.types[&Scalar::Bool], present, group, groups64]);
            let proceed = self.id();
            let stop = self.id();
            self.emit(247, &[proceed, 0]);
            self.emit(250, &[present, proceed, stop]);
            self.emit(248, &[stop]);
            self.emit(253, &[]);
            self.emit(248, &[proceed]);
        }
        self.index = self.id();
        self.emit(113, &[u, self.index, index]);
        (count, active)
    }
}
