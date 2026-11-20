//! 分段摘要携带尾段值与段头标记；归约额外携带段数，摘要始终留在设备。
use super::*;

#[repr(C)]
struct SegmentField {
    scalar: ScalarCode,
    word: u32,
    raw_type: u32,
    value_type: u32,
    shift: u32,
}

#[repr(C)]
struct SegmentMerge {
    operator: u32,
    uint_type: u32,
    bool_type: u32,
    zero: u32,
    value_words: u32,
    counted: u32,
    next: u32,
}

#[repr(C)]
struct SegmentWords {
    body: u64,
    decorations: u64,
    next: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::merge_segment"]
    fn merge_segment(input: SegmentMerge, fields: *const SegmentField, count: u32,
        left: *const u32, right: *const u32, values: *mut u32, body: *mut u32,
        decorations: *mut u32) -> SegmentWords;
}

struct Segment<'a> {
    ty: &'a Ty,
    operator: CollectiveOperator,
    counted: bool,
    fields: Vec<SegmentField>,
}
impl Segment<'_> {
    fn summary_words(&self) -> usize {
        self.ty.size.div_ceil(4) as usize + 1 + usize::from(self.counted)
    }
}
impl Emitter<'_> {
    fn segment<'a>(&mut self, ty: &'a Ty, operator: CollectiveOperator, counted: bool) -> Segment<'a> {
        let fields = ty.components.iter().map(|&(scalar, offset)| SegmentField {
            scalar: scalar.into(),
            word: offset / 4,
            raw_type: self.raw_type(scalar),
            value_type: self.types[&scalar],
            shift: if scalar.size() == 8 { self.constant(Scalar::U32, 32) } else { 0 },
        }).collect();
        Segment { ty, operator, counted, fields }
    }

    fn segment_identity(&mut self, scan: &Segment<'_>) -> Vec<u32> {
        let zero = self.constant(Scalar::U32, 0);
        let mut words=vec![zero;scan.summary_words()];
        for (scalar,offset) in &scan.ty.components {
            let value=self.raw_constant(*scalar,scan.operator.identity(*scalar));
            for (part,word) in self.split_words(*scalar,value, Scalar::U32).into_iter().enumerate() {
                words[(*offset/4) as usize+part]=word;
            }
        }
        words
    }
    fn segment_source(&mut self, scan: &Segment<'_>, fields: &Reduction, head_word: u32) -> Vec<u32> {
        let k=scan.ty.size.div_ceil(4) as usize;
        let zero=self.constant(Scalar::U32,0);
        let mut words=vec![zero;k+1];
        for (scalar,offset) in &scan.ty.components {
            let raw = self.load_raw_buffer(*scalar, fields.source, self.index, fields.source_word + offset);
            for (part,word) in self.split_words(*scalar, raw, Scalar::U32).into_iter().enumerate() {
                words[(offset / 4) as usize + part] = word;
            }
        }
        let at=self.address_scalar(Scalar::U32, fields.source,self.index,head_word);
        words[k]=self.read_summary_words(&[at])[0];
        words
    }
    fn segment_combine(&mut self, scan: &Segment<'_>, left: &[u32], right: &[u32]) -> Vec<u32> {
        let zero = self.constant(Scalar::U32, 0);
        let input = SegmentMerge {
            operator: scan.operator as u32,
            uint_type: self.types[&Scalar::U32],
            bool_type: self.types[&Scalar::Bool],
            zero,
            value_words: scan.ty.size.div_ceil(4),
            counted: u32::from(scan.counted),
            next: self.next,
        };
        let mut values = vec![zero; scan.summary_words()];
        let body = self.body.len();
        let decorations = self.decorations.len();
        self.body.reserve(15 + 74 * scan.fields.len());
        self.decorations.reserve(3 * scan.fields.len());
        unsafe {
            let result = merge_segment(input, scan.fields.as_ptr(), scan.fields.len() as u32,
                left.as_ptr(), right.as_ptr(), values.as_mut_ptr(), self.body.as_mut_ptr().add(body),
                self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + result.body as usize);
            self.decorations.set_len(decorations + result.decorations as usize);
            self.next = result.next;
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
        let scan = &self.segment(&scan.ty, scan.operator, false);
        let k = scan.ty.size.div_ceil(4) as usize;
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let identity = self.segment_identity(scan);
        let remaining =
            if scan.ty.components[0].0.is_float() && scan.operator != CollectiveOperator::Sum {
                Some(self.group_remaining(local, count))
            } else {
                None
            };
        let before = self.current_label;
        let read = self.id();
        let read_end = self.id();
        self.emit(247, &[read_end, 0]);
        self.emit(250, &[active, read, read_end]);
        self.emit(248, &[read]);
        let mut input = if tail {
            let source=self.summary_addresses(scan.summary_words(),self.scan_input_binding(r),self.index);
            self.read_summary_words(&source)
        } else {
            self.segment_source(scan,r,head_word)
        };
        if !tail {
            let is_head = self.id();
            self.emit(171, &[b, is_head, input[k], zero]);
            let one = self.constant(Scalar::U32, 1);
            input[k] = self.id();
            self.select(u, input[k], is_head, one, zero);
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
        let own = self.shared_planes(shared, pointer, local, scan.summary_words());
        self.write_summary_words(&own, &input);
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
        let total = self.read_summary_words(&own);
        let target = self.summary_addresses(scan.summary_words(), self.program.buffers.len(), group);
        if tail {
            self.write_summary_words(&target, &total);
        } else {
            self.write_summary_words(&target[..k], &total[..k]);
        }
        self.write_summary_words(&own, &identity);
        self.emit(249, &[saved]);
        self.emit(248, &[saved]);
        self.scan_barrier();
        for width in [32, 16, 8, 4, 2, 1] {
            self.segment_tree(scan, local, shared, pointer, width, true, None);
        }
        let result = self.read_summary_words(&own);
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
            let first = self.address_scalar(Scalar::U32,
                self.program.buffers.len(),
                group,
                k as u32 * 4);
            self.write_summary_words(&[first], &[zero]);
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
            for (scalar,offset) in &scan.ty.components {
                let word = (offset / 4) as usize;
                let n = scalar.size().div_ceil(4) as usize;
                let current = self.join_words(*scalar, &result[word..word+n], Scalar::U32);
                let empty = self.raw_constant(*scalar, scan.operator.identity(*scalar));
                let value = self.id();
                self.select(self.raw_type(*scalar), value, is_head, empty, current);
                self.store_raw_buffer(*scalar, r.target, self.index, r.target_word + offset, value);
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
            let target = self.address_scalar(Scalar::U32,
                self.program.buffers.len(),
                group,
                k as u32 * 4);
            let one = self.constant(Scalar::U32, 1);
            let encoded = self.id();
            self.emit(128, &[u, encoded, local, one]);
            self.write_summary_words(&[target], &[encoded]);
            self.emit(249, &[finish]);
            self.emit(248, &[finish]);
        } else {
            let target = self.summary_addresses(scan.summary_words(), r.target, self.index);
            self.write_summary_words(&target, &result);
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
        let scan = &self.segment(&reduction.values.ty, reduction.values.operator, true);
        let k = scan.ty.size.div_ceil(4) as usize;
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
            let prefix = self.summary_addresses(scan.summary_words(), self.scan_input_binding(r), group);
            let prefix = self.read_summary_words(&prefix);
            let target = self.summary_addresses(scan.summary_words(), r.target, self.index);
            let value = self.read_summary_words(&target);
            let merged = self.segment_combine(scan, &prefix, &value);
            self.write_summary_words(&target, &merged);
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
            let source = self.summary_addresses(scan.summary_words(), self.scan_input_binding(r), self.index);
            self.read_summary_words(&source)
        } else {
            let mut input = self.segment_source(scan,r,reduction.head_word);
            let head = self.id();
            self.emit(171, &[b, head, input[k], zero]);
            let first = self.id();
            self.emit(170, &[b, first, self.index, zero]);
            let is_head = self.id();
            self.emit(166, &[b, is_head, head, first]);
            let flag = self.id();
            self.select(u, flag, is_head, one, zero);
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
        let own = self.shared_planes(shared, pointer, local, scan.summary_words());
        self.write_summary_words(&own, &input);
        self.scan_barrier();
        let remaining = self.group_remaining(local, count);
        for width in [1, 2, 4, 8, 16, 32] {
            self.segment_tree(scan, local, shared, pointer, width, false, Some(remaining));
        }
        let (owner, empty) = self.collective_writer(local, active, count);
        let save = self.id();
        let saved = self.id();
        self.emit(247, &[saved, 0]);
        self.emit(250, &[owner, save, saved]);
        self.emit(248, &[save]);
        let last = self.constant(Scalar::U32, 63);
        let root = self.shared_planes(shared, pointer, last, scan.summary_words());
        if pass != Pass::ScanRootAdd {
            let total = self.read_summary_words(&root);
            let target = self.summary_addresses(scan.summary_words(), self.program.buffers.len(), group);
            self.write_summary_words(&target, &total);
        } else {
            let write_empty = self.id();
            let end_empty = self.id();
            self.emit(247, &[end_empty, 0]);
            self.emit(250, &[empty, write_empty, end_empty]);
            self.emit(248, &[write_empty]);
            let target = self.address_scalar(Scalar::U32,
                reduction.count.binding,
                zero,
                reduction.count.word);
            self.write_summary_words(&[target], &[zero]);
            self.emit(249, &[end_empty]);
            self.emit(248, &[end_empty]);
        }
        if pass != Pass::Main {
            self.write_summary_words(&root, &identity);
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
        let result = self.read_summary_words(&own);
        let write = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, write, end]);
        self.emit(248, &[write]);
        if pass == Pass::ScanTail {
            let target = self.summary_addresses(scan.summary_words(), r.target, self.index);
            self.write_summary_words(&target, &result);
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
            let prefix = self.summary_addresses(scan.summary_words(), self.program.buffers.len(), group);
            let prefix = self.read_summary_words(&prefix);
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
            let next_head = self.address_scalar(Scalar::U32,
                r.source,
                next,
                reduction.head_word);
            let next_head = self.read_summary_words(&[next_head])[0];
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
            for (scalar,offset) in &scan.ty.components {
                let word = (offset / 4) as usize;
                let n = scalar.size().div_ceil(4) as usize;
                let value = self.join_words(*scalar, &total[word..word+n], Scalar::U32);
                self.store_raw_buffer(*scalar, r.target, output, r.target_word + offset, value);
            }
            self.emit(249, &[emitted]);
            self.emit(248, &[emitted]);
            let emit_count = self.id();
            let counted = self.id();
            self.emit(247, &[counted, 0]);
            self.emit(250, &[last, emit_count, counted]);
            self.emit(248, &[emit_count]);
            let target = self.address_scalar(Scalar::U32,
                reduction.count.binding,
                zero,
                reduction.count.word);
            self.write_summary_words(&[target], &[total[k + 1]]);
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
        let (other, end) = self.open_scan_tree(local, width);
        let a_ptr = self.shared_planes(shared, pointer, other, scan.summary_words());
        let b_ptr = self.shared_planes(shared, pointer, local, scan.summary_words());
        let a = self.read_summary_words(&a_ptr);
        let b = self.read_summary_words(&b_ptr);
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
                self.select(u, result, has_right, *value, *left);
                *value = result;
            }
        }
        if down {
            self.write_summary_words(&a_ptr, &b);
        }
        self.write_summary_words(&b_ptr, &merged);
        self.close_scan_tree(end);
    }
    pub(super) fn segmented_add(&mut self, scan: &Scan) {
        let r = &scan.values;
        let scan = &self.segment(&scan.ty, scan.operator, false);
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[self.types[&Scalar::U32], group, self.index, width]);
        let prefix = self.summary_addresses(scan.summary_words(), self.scan_input_binding(r), group);
        let prefix = self.read_summary_words(&prefix);
        let target = self.summary_addresses(scan.summary_words(), r.target, self.index);
        let value = self.read_summary_words(&target);
        let merged = self.segment_combine(scan, &prefix, &value);
        let merged =
            if scan.ty.components[0].0.is_float() && scan.operator != CollectiveOperator::Sum {
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
                        self.select(u, result, first, prefix, value);
                        result
                    })
                    .collect::<Vec<_>>()
            } else {
                merged
            };
        self.write_summary_words(&target, &merged);
    }
}
