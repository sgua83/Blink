//! 分段摘要携带尾段值与段头标记；归约额外携带段数，摘要始终留在设备。
use super::*;
struct Segment<'a> {
    ty: &'a Ty,
    operator: CollectiveOperator,
    counted: bool,
}
impl Segment<'_> {
    fn summary_words(&self) -> usize {
        self.ty.components.len() + 1 + usize::from(self.counted)
    }
}
impl Emitter<'_> {
    fn segment_read(&mut self, pointers: &[u32]) -> Vec<u32> {
        pointers
            .iter()
            .map(|p| {
                let value = self.id();
                self.emit(61, &[self.types[&Scalar::U32], value, *p, 2, 4]);
                value
            })
            .collect()
    }
    fn segment_write(&mut self, pointers: &[u32], values: &[u32]) {
        for (p, v) in pointers.iter().zip(values) {
            self.emit(62, &[*p, *v, 2, 4]);
        }
    }
    fn segment_address(&mut self, scan: &Segment<'_>, binding: usize, index: u32) -> Vec<u32> {
        (0..scan.summary_words() as u32)
            .map(|word| self.address_scalar(binding, index, scan.summary_words() as u32, word))
            .collect()
    }
    fn segment_node(
        &mut self,
        scan: &Segment<'_>,
        shared: u32,
        pointer: u32,
        node: u32,
    ) -> Vec<u32> {
        (0..scan.summary_words() as u32)
            .map(|word| {
                let base = self.constant(Scalar::U32, word * 64);
                let index = self.id();
                self.emit(128, &[self.types[&Scalar::U32], index, node, base]);
                self.shared_at(pointer, shared, index)
            })
            .collect()
    }
    fn segment_identity(&mut self, scan: &Segment<'_>) -> Vec<u32> {
        let zero = self.constant(Scalar::U32, 0);
        scan.ty
            .components
            .iter()
            .map(|(scalar, _)| self.constant(Scalar::U32, scan.operator.identity(*scalar)))
            .chain(std::iter::repeat_n(zero, 1 + usize::from(scan.counted)))
            .collect()
    }
    fn segment_combine(&mut self, scan: &Segment<'_>, left: &[u32], right: &[u32]) -> Vec<u32> {
        let u = self.types[&Scalar::U32];
        let k = scan.ty.components.len();
        let zero = self.constant(Scalar::U32, 0);
        let head = self.id();
        self.emit(171, &[self.types[&Scalar::Bool], head, right[k], zero]);
        let mut values = Vec::new();
        for (component, (scalar, _)) in scan.ty.components.iter().enumerate() {
            let merged =
                self.collective_pair(*scalar, scan.operator, left[component], right[component]);
            let tail = self.id();
            self.emit(169, &[u, tail, head, right[component], merged]);
            values.push(tail);
        }
        let heads = self.id();
        self.emit(197, &[u, heads, left[k], right[k]]);
        values.push(heads);
        if scan.counted {
            let count = self.id();
            self.emit(128, &[u, count, left[k + 1], right[k + 1]]);
            values.push(count);
        }
        values
    }
    pub(super) fn segmented_scan(
        &mut self,
        scan: &Scan,
        head_word: u32,
        tail: bool,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) {
        let r = &scan.values;
        let scan = &Segment {
            ty: &scan.ty,
            operator: scan.operator,
            counted: false,
        };
        let k = scan.ty.components.len();
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let identity = self.segment_identity(scan);
        let remaining =
            if scan.ty.components[0].0 == Scalar::F32 && scan.operator != CollectiveOperator::Sum {
                let group_begin = self.id();
                self.emit(130, &[u, group_begin, self.index, local]);
                let available = self.id();
                self.emit(130, &[u, available, count, group_begin]);
                Some(available)
            } else {
                None
            };
        let before = self.current_label;
        let read = self.id();
        let read_end = self.id();
        self.emit(247, &[read_end, 0]);
        self.emit(250, &[active, read, read_end]);
        self.emit(248, &[read]);
        let source = if tail {
            self.segment_address(scan, self.scan_input_binding(r), self.index)
        } else {
            scan.ty
                .components
                .iter()
                .map(|(_, offset)| r.source_word + offset / 4)
                .chain(std::iter::once(head_word))
                .map(|word| {
                    self.address_scalar(
                        r.source,
                        self.index,
                        self.program.buffers[r.source].stride / 4,
                        word,
                    )
                })
                .collect()
        };
        let mut input = self.segment_read(&source);
        if !tail {
            let is_head = self.id();
            self.emit(171, &[b, is_head, input[k], zero]);
            let one = self.constant(Scalar::U32, 1);
            input[k] = self.id();
            self.emit(169, &[u, input[k], is_head, one, zero]);
        }
        self.emit(249, &[read_end]);
        self.emit(248, &[read_end]);
        let input: Vec<_> = input
            .iter()
            .zip(&identity)
            .map(|(v, identity)| {
                let result = self.id();
                self.emit(245, &[u, result, *v, read, *identity, before]);
                result
            })
            .collect();
        let own = self.segment_node(scan, shared, pointer, local);
        self.segment_write(&own, &input);
        self.scan_barrier();
        for width in [1, 2, 4, 8, 16, 32] {
            self.segment_tree(scan, local, shared, pointer, width, false, remaining);
        }
        let last = self.constant(Scalar::U32, 63);
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[u, group, self.index, width]);
        let is_last = self.id();
        self.emit(170, &[b, is_last, local, last]);
        let save = self.id();
        let saved = self.id();
        self.emit(247, &[saved, 0]);
        self.emit(250, &[is_last, save, saved]);
        self.emit(248, &[save]);
        let total = self.segment_read(&own);
        let target = self.segment_address(scan, self.program.buffers.len(), group);
        if tail {
            self.segment_write(&target, &total);
        } else {
            self.segment_write(&target[..k], &total[..k]);
        }
        self.segment_write(&own, &identity);
        self.emit(249, &[saved]);
        self.emit(248, &[saved]);
        self.scan_barrier();
        for width in [32, 16, 8, 4, 2, 1] {
            self.segment_tree(scan, local, shared, pointer, width, true, None);
        }
        let result = self.segment_read(&own);
        if !tail {
            // 首段头写 lane+1；没有段头时只有最后一个物理 lane 写 0。
            let heads = self.id();
            self.emit(197, &[u, heads, result[k], input[k]]);
            let no_heads = self.id();
            self.emit(170, &[b, no_heads, heads, zero]);
            let empty_last = self.id();
            self.emit(167, &[b, empty_last, no_heads, is_last]);
            let save = self.id();
            let saved = self.id();
            self.emit(247, &[saved, 0]);
            self.emit(250, &[empty_last, save, saved]);
            self.emit(248, &[save]);
            let first = self.address_scalar(
                self.program.buffers.len(),
                group,
                scan.summary_words() as u32,
                k as u32,
            );
            self.emit(62, &[first, zero, 2, 4]);
            self.emit(249, &[saved]);
            self.emit(248, &[saved]);
        }
        let write = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, write, end]);
        self.emit(248, &[write]);
        if !tail {
            let is_head = self.id();
            self.emit(171, &[b, is_head, input[k], zero]);
            for (component, (_, offset)) in scan.ty.components.iter().enumerate() {
                let value = self.id();
                self.emit(
                    169,
                    &[u, value, is_head, identity[component], result[component]],
                );
                let target = self.address_scalar(
                    r.target,
                    self.index,
                    self.program.buffers[r.target].stride / 4,
                    r.target_word + offset / 4,
                );
                self.emit(62, &[target, value, 2, 4]);
            }
            let no_previous = self.id();
            self.emit(170, &[b, no_previous, result[k], zero]);
            let is_first = self.id();
            self.emit(167, &[b, is_first, is_head, no_previous]);
            let first = self.id();
            let finish = self.id();
            self.emit(247, &[finish, 0]);
            self.emit(250, &[is_first, first, finish]);
            self.emit(248, &[first]);
            let target = self.address_scalar(
                self.program.buffers.len(),
                group,
                scan.summary_words() as u32,
                k as u32,
            );
            let one = self.constant(Scalar::U32, 1);
            let encoded = self.id();
            self.emit(128, &[u, encoded, local, one]);
            self.emit(62, &[target, encoded, 2, 4]);
            self.emit(249, &[finish]);
            self.emit(248, &[finish]);
        } else {
            let target = self.segment_address(scan, r.target, self.index);
            self.segment_write(&target, &result);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
    pub(super) fn segmented_reduce(
        &mut self,
        reduction: &SegmentedReduction,
        pass: Pass,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) {
        let r = &reduction.values.fields;
        let scan = &Segment {
            ty: &reduction.values.ty,
            operator: reduction.values.operator,
            counted: true,
        };
        let k = scan.ty.components.len();
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let one = self.constant(Scalar::U32, 1);
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[u, group, self.index, width]);
        if pass == Pass::ScanAdd {
            let run = self.id();
            let end = self.id();
            self.emit(247, &[end, 0]);
            self.emit(250, &[active, run, end]);
            self.emit(248, &[run]);
            let prefix = self.segment_address(scan, self.scan_input_binding(r), group);
            let prefix = self.segment_read(&prefix);
            let target = self.segment_address(scan, r.target, self.index);
            let value = self.segment_read(&target);
            let merged = self.segment_combine(scan, &prefix, &value);
            self.segment_write(&target, &merged);
            self.emit(249, &[end]);
            self.emit(248, &[end]);
            return;
        }
        let identity = self.segment_identity(scan);
        let before = self.current_label;
        let read = self.id();
        let read_end = self.id();
        self.emit(247, &[read_end, 0]);
        self.emit(250, &[active, read, read_end]);
        self.emit(248, &[read]);
        let mut input = if pass == Pass::ScanTail {
            let source = self.segment_address(scan, self.scan_input_binding(r), self.index);
            self.segment_read(&source)
        } else {
            let source: Vec<_> = scan
                .ty
                .components
                .iter()
                .map(|(_, offset)| r.source_word + offset / 4)
                .chain(std::iter::once(reduction.head_word))
                .map(|word| {
                    self.address_scalar(
                        r.source,
                        self.index,
                        self.program.buffers[r.source].stride / 4,
                        word,
                    )
                })
                .collect();
            let mut input = self.segment_read(&source);
            let head = self.id();
            self.emit(171, &[b, head, input[k], zero]);
            let first = self.id();
            self.emit(170, &[b, first, self.index, zero]);
            let is_head = self.id();
            self.emit(166, &[b, is_head, head, first]);
            let flag = self.id();
            self.emit(169, &[u, flag, is_head, one, zero]);
            input[k] = flag;
            input.push(flag);
            input
        };
        self.emit(249, &[read_end]);
        self.emit(248, &[read_end]);
        for (value, identity) in input.iter_mut().zip(&identity) {
            let result = self.id();
            self.emit(245, &[u, result, *value, read, *identity, before]);
            *value = result;
        }
        let own = self.segment_node(scan, shared, pointer, local);
        self.segment_write(&own, &input);
        self.scan_barrier();
        let base = self.id();
        self.emit(130, &[u, base, self.index, local]);
        let remaining = self.id();
        self.emit(130, &[u, remaining, count, base]);
        for width in [1, 2, 4, 8, 16, 32] {
            self.segment_tree(scan, local, shared, pointer, width, false, Some(remaining));
        }
        let local_first = self.id();
        self.emit(170, &[b, local_first, local, zero]);
        let empty = self.id();
        self.emit(170, &[b, empty, count, zero]);
        let needed = self.id();
        self.emit(166, &[b, needed, active, empty]);
        let owner = self.id();
        self.emit(167, &[b, owner, needed, local_first]);
        let save = self.id();
        let saved = self.id();
        self.emit(247, &[saved, 0]);
        self.emit(250, &[owner, save, saved]);
        self.emit(248, &[save]);
        let last = self.constant(Scalar::U32, 63);
        let root = self.segment_node(scan, shared, pointer, last);
        if pass != Pass::ScanRootAdd {
            let total = self.segment_read(&root);
            let target = self.segment_address(scan, self.program.buffers.len(), group);
            self.segment_write(&target, &total);
        } else {
            let write_empty = self.id();
            let end_empty = self.id();
            self.emit(247, &[end_empty, 0]);
            self.emit(250, &[empty, write_empty, end_empty]);
            self.emit(248, &[write_empty]);
            let target = self.address_scalar(
                reduction.count.binding,
                zero,
                self.program.buffers[reduction.count.binding].stride / 4,
                reduction.count.word,
            );
            self.emit(62, &[target, zero, 2, 4]);
            self.emit(249, &[end_empty]);
            self.emit(248, &[end_empty]);
        }
        if pass != Pass::Main {
            self.segment_write(&root, &identity);
        }
        self.emit(249, &[saved]);
        self.emit(248, &[saved]);
        if pass == Pass::Main {
            return;
        }
        self.scan_barrier();
        for width in [32, 16, 8, 4, 2, 1] {
            self.segment_tree(scan, local, shared, pointer, width, true, None);
        }
        let result = self.segment_read(&own);
        let write = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, write, end]);
        self.emit(248, &[write]);
        if pass == Pass::ScanTail {
            let target = self.segment_address(scan, r.target, self.index);
            self.segment_write(&target, &result);
        } else {
            let inclusive = self.segment_combine(scan, &result, &input);
            // 首组不需要设备前缀；其余组只读取自己的摘要前缀。
            let first_group = self.id();
            self.emit(170, &[b, first_group, group, zero]);
            let load_prefix = self.id();
            let prefix_end = self.id();
            let before = self.current_label;
            self.emit(247, &[prefix_end, 0]);
            self.emit(250, &[first_group, prefix_end, load_prefix]);
            self.emit(248, &[load_prefix]);
            let prefix = self.segment_address(scan, self.program.buffers.len(), group);
            let prefix = self.segment_read(&prefix);
            self.emit(249, &[prefix_end]);
            self.emit(248, &[prefix_end]);
            let prefix: Vec<_> = prefix
                .iter()
                .zip(&identity)
                .map(|(v, identity)| {
                    let result = self.id();
                    self.emit(245, &[u, result, *v, load_prefix, *identity, before]);
                    result
                })
                .collect();
            let total = self.segment_combine(scan, &prefix, &inclusive);
            let next = self.id();
            self.emit(128, &[u, next, self.index, one]);
            let last = self.id();
            self.emit(170, &[b, last, next, count]);
            let look = self.id();
            let looked = self.id();
            let before = self.current_label;
            self.emit(247, &[looked, 0]);
            self.emit(250, &[last, looked, look]);
            self.emit(248, &[look]);
            let next_head = self.address_scalar(
                r.source,
                next,
                self.program.buffers[r.source].stride / 4,
                reduction.head_word,
            );
            let next_head = self.segment_read(&[next_head])[0];
            let head = self.id();
            self.emit(171, &[b, head, next_head, zero]);
            self.emit(249, &[looked]);
            self.emit(248, &[looked]);
            let tail = self.id();
            self.emit(245, &[b, tail, last, before, head, look]);
            let emit_tail = self.id();
            let emitted = self.id();
            self.emit(247, &[emitted, 0]);
            self.emit(250, &[tail, emit_tail, emitted]);
            self.emit(248, &[emit_tail]);
            let output = self.id();
            self.emit(130, &[u, output, total[k + 1], one]);
            for (component, (_, offset)) in scan.ty.components.iter().enumerate() {
                let target = self.address_scalar(
                    r.target,
                    output,
                    self.program.buffers[r.target].stride / 4,
                    r.target_word + offset / 4,
                );
                self.emit(62, &[target, total[component], 2, 4]);
            }
            self.emit(249, &[emitted]);
            self.emit(248, &[emitted]);
            let emit_count = self.id();
            let counted = self.id();
            self.emit(247, &[counted, 0]);
            self.emit(250, &[last, emit_count, counted]);
            self.emit(248, &[emit_count]);
            let target = self.address_scalar(
                reduction.count.binding,
                zero,
                self.program.buffers[reduction.count.binding].stride / 4,
                reduction.count.word,
            );
            self.emit(62, &[target, total[k + 1], 2, 4]);
            self.emit(249, &[counted]);
            self.emit(248, &[counted]);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
    fn segment_tree(
        &mut self,
        scan: &Segment<'_>,
        local: u32,
        shared: u32,
        pointer: u32,
        width: u32,
        down: bool,
        remaining: Option<u32>,
    ) {
        let u = self.types[&Scalar::U32];
        let mask = self.constant(Scalar::U32, width * 2 - 1);
        let bits = self.id();
        self.emit(199, &[u, bits, local, mask]);
        let selected = self.id();
        self.emit(170, &[self.types[&Scalar::Bool], selected, bits, mask]);
        let run = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[selected, run, end]);
        self.emit(248, &[run]);
        let width = self.constant(Scalar::U32, width);
        let other = self.id();
        self.emit(130, &[u, other, local, width]);
        let a_ptr = self.segment_node(scan, shared, pointer, other);
        let b_ptr = self.segment_node(scan, shared, pointer, local);
        let a = self.segment_read(&a_ptr);
        let b = self.segment_read(&b_ptr);
        let mut merged = if down {
            self.segment_combine(scan, &b, &a)
        } else {
            self.segment_combine(scan, &a, &b)
        };
        if let Some(remaining) = remaining.filter(|_| !down) {
            // 空右子树不参与归约，尤其不能让填充 identity 改写 NaN／有符号零。
            let one = self.constant(Scalar::U32, 1);
            let right_start = self.id();
            self.emit(128, &[u, right_start, other, one]);
            let has_right = self.id();
            self.emit(
                176,
                &[self.types[&Scalar::Bool], has_right, right_start, remaining],
            );
            for (value, left) in merged.iter_mut().zip(&a) {
                let result = self.id();
                self.emit(169, &[u, result, has_right, *value, *left]);
                *value = result;
            }
        }
        if down {
            self.segment_write(&a_ptr, &b);
        }
        self.segment_write(&b_ptr, &merged);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        self.scan_barrier();
    }
    pub(super) fn segmented_add(&mut self, scan: &Scan) {
        let r = &scan.values;
        let scan = &Segment {
            ty: &scan.ty,
            operator: scan.operator,
            counted: false,
        };
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[self.types[&Scalar::U32], group, self.index, width]);
        let prefix = self.segment_address(scan, self.scan_input_binding(r), group);
        let prefix = self.segment_read(&prefix);
        let target = self.segment_address(scan, r.target, self.index);
        let value = self.segment_read(&target);
        let merged = self.segment_combine(scan, &prefix, &value);
        let merged =
            if scan.ty.components[0].0 == Scalar::F32 && scan.operator != CollectiveOperator::Sum {
                let u = self.types[&Scalar::U32];
                let local = self.id();
                self.emit(137, &[u, local, self.index, width]);
                let zero = self.constant(Scalar::U32, 0);
                let first = self.id();
                self.emit(170, &[self.types[&Scalar::Bool], first, local, zero]);
                merged
                    .into_iter()
                    .zip(prefix)
                    .map(|(value, prefix)| {
                        let result = self.id();
                        self.emit(169, &[u, result, first, prefix, value]);
                        result
                    })
                    .collect::<Vec<_>>()
            } else {
                merged
            };
        self.segment_write(&target, &merged);
    }
    pub(super) fn segmented_root_add(&mut self, scan: &Scan) {
        let r = &scan.values;
        let u = self.types[&Scalar::U32];
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        let local = self.id();
        self.emit(134, &[u, group, self.index, width]);
        self.emit(137, &[u, local, self.index, width]);
        let first = self.address_scalar(
            self.program.buffers.len(),
            group,
            scan.summary_words() as u32,
            scan.ty.components.len() as u32,
        );
        let head = self.id();
        self.emit(61, &[u, head, first, 2, 4]);
        let one = self.constant(Scalar::U32, 1);
        let zero = self.constant(Scalar::U32, 0);
        let lane = self.id();
        self.emit(128, &[u, lane, local, one]);
        let before_head = self.id();
        let no_head = self.id();
        let before = self.id();
        let b = self.types[&Scalar::Bool];
        self.emit(176, &[b, before_head, lane, head]);
        self.emit(170, &[b, no_head, head, zero]);
        self.emit(166, &[b, before, before_head, no_head]);
        let add = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[before, add, end]);
        self.emit(248, &[add]);
        for (component, (scalar, offset)) in scan.ty.components.iter().enumerate() {
            let source = self.address_scalar(
                self.scan_input_binding(r),
                group,
                scan.summary_words() as u32,
                component as u32,
            );
            let prefix = self.id();
            self.emit(61, &[u, prefix, source, 2, 4]);
            let target = self.address_scalar(
                r.target,
                self.index,
                self.program.buffers[r.target].stride / 4,
                r.target_word + offset / 4,
            );
            let value = self.id();
            self.emit(61, &[u, value, target, 2, 4]);
            let merged = self.collective_pair(*scalar, scan.operator, prefix, value);
            let result = if *scalar == Scalar::F32 && scan.operator != CollectiveOperator::Sum {
                let first = self.id();
                self.emit(170, &[b, first, local, zero]);
                let result = self.id();
                self.emit(169, &[u, result, first, prefix, merged]);
                result
            } else {
                merged
            };
            self.emit(62, &[target, result, 2, 4]);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
}
