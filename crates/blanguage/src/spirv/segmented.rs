//! 分段摘要携带尾段值与段头标记；归约额外携带段数，摘要始终留在设备。
use super::{scan::TreeWords, *};

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

#[repr(C)]
struct SegmentLevel {
    mask: u32,
    stride: u32,
}

#[repr(C)]
struct SegmentTree {
    merge: SegmentMerge,
    shared: u32,
    pointer: u32,
    lane: u32,
    remaining: u32,
    one: u32,
    scope: u32,
    semantics: u32,
    down: u32,
}

struct SegmentTreeLayout {
    levels: [SegmentLevel; 6],
    words: Vec<u32>,
}

#[repr(C)]
struct SegmentAdd {
    merge: SegmentMerge,
    target: PageAddress,
    prefix_descriptor: u32,
    width: u32,
    step: u32,
    active: u32,
    run: u32,
    end: u32,
}

#[repr(C)]
struct SegmentSourceField {
    word: u32,
    offset: u32,
    unit_type: u32,
    unit_bytes: u32,
    pointer_type: u32,
    step: u32,
}

#[repr(C)]
struct SegmentSource {
    source: PageAddress,
    bool_type: u32,
    head: u32,
    value_words: u32,
    counted: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::segment_source"]
    fn emit_segment_source(input: SegmentSource, fields: *const SegmentSourceField, count: u32,
        words: *mut u32, body: *mut u32) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::segment_add"]
    fn emit_segment_add(input: SegmentAdd, fields: *const SegmentField, count: u32,
        words: *mut u32, body: *mut u32, decorations: *mut u32) -> TreeWords;
    #[link_name = "bootstrap.spirv_envelope::segment_tree"]
    fn emit_segment_tree(input: SegmentTree, levels: *const SegmentLevel, fields: *const SegmentField,
        count: u32, words: *mut u32, body: *mut u32, decorations: *mut u32) -> TreeWords;
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
    fn segment_source(&mut self, scan: &Segment<'_>, reduction: &Reduction, head_word: u32) -> Vec<u32> {
        let fields: Vec<_> = scan.ty.components.iter().map(|&(scalar, offset)| {
            let unit = Self::physical_unit(scalar);
            let byte = reduction.source_word + offset;
            SegmentSourceField {
                word: offset / 4,
                offset: if byte == 0 { 0 } else { self.constant(Scalar::U64, byte) },
                unit_type: self.types[&unit],
                unit_bytes: unit.size(),
                pointer_type: self.physical_pointers[&unit],
                step: if scalar.size() == 8 { self.constant(Scalar::U64, 4) } else { 0 },
            }
        }).collect();
        let head = if head_word == 0 { 0 } else { self.constant(Scalar::U64, head_word) };
        let source = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[reduction.source]),
            self.index, None, 0);
        let mut words = vec![source.zero; scan.summary_words()];
        let input = SegmentSource {
            source, head,
            bool_type: self.types[&Scalar::Bool],
            value_words: scan.ty.size.div_ceil(4),
            counted: u32::from(scan.counted),
        };
        let at = self.body.len();
        self.body.reserve(145 + 34 * fields.len());
        unsafe {
            let result = emit_segment_source(input, fields.as_ptr(), fields.len() as u32,
                words.as_mut_ptr(), self.body.as_mut_ptr().add(at));
            self.body.set_len(at + result.words as usize);
            self.next = result.next;
        }
        words
    }
    fn segment_merge_input(&mut self, scan: &Segment<'_>) -> SegmentMerge {
        let zero = self.constant(Scalar::U32, 0);
        SegmentMerge {
            operator: scan.operator as u32,
            uint_type: self.types[&Scalar::U32],
            bool_type: self.types[&Scalar::Bool],
            zero,
            value_words: scan.ty.size.div_ceil(4),
            counted: u32::from(scan.counted),
            next: self.next,
        }
    }

    fn segment_combine(&mut self, scan: &Segment<'_>, left: &[u32], right: &[u32]) -> Vec<u32> {
        let input = self.segment_merge_input(scan);
        let mut values = vec![input.zero; scan.summary_words()];
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
        let input = if tail {
            let source=self.summary_addresses(scan.summary_words(),self.scan_input_binding(r),self.index);
            self.read_summary_words(&source)
        } else {
            self.segment_source(scan, r, head_word)
        };
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
        let mut layout = self.segment_tree_layout(scan);
        self.segment_tree(scan, &mut layout, local, shared, pointer, false, remaining);
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
        self.segment_tree(scan, &mut layout, local, shared, pointer, true, None);
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
            self.segment_source(scan, r, reduction.head_word)
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
        let mut layout = self.segment_tree_layout(scan);
        self.segment_tree(scan, &mut layout, local, shared, pointer, false, Some(remaining));
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
        self.segment_tree(scan, &mut layout, local, shared, pointer, true, None);
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
    fn segment_tree_layout(&mut self, scan: &Segment<'_>) -> SegmentTreeLayout {
        let levels = [1, 2, 4, 8, 16, 32].map(|width| SegmentLevel {
            mask: self.constant(Scalar::U32, width * 2 - 1),
            stride: self.constant(Scalar::U32, width),
        });
        let count = scan.summary_words();
        let mut words = vec![0; 6 * count];
        for (index, slot) in words[..count].iter_mut().enumerate().skip(1) {
            *slot = self.constant(Scalar::U32, index as u32 * 64);
        }
        SegmentTreeLayout { levels, words }
    }

    fn segment_tree(
        &mut self,
        scan: &Segment<'_>,
        layout: &mut SegmentTreeLayout,
        lane: u32,
        shared: u32,
        pointer: u32,
        down: bool,
        remaining: Option<u32>,
    ) {
        let one = remaining.map_or(0, |_| self.constant(Scalar::U32, 1));
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x108);
        let input = SegmentTree {
            merge: self.segment_merge_input(scan),
            shared, pointer, lane, one, scope, semantics,
            remaining: remaining.unwrap_or(0),
            down: u32::from(down),
        };
        let body = self.body.len();
        let decorations = self.decorations.len();
        let levels = layout.levels.len();
        self.body.reserve(levels * (57 + 48 * scan.summary_words() + 74 * scan.fields.len()));
        self.decorations.reserve(levels * 3 * scan.fields.len());
        unsafe {
            let result = emit_segment_tree(input, layout.levels.as_ptr(), scan.fields.as_ptr(),
                scan.fields.len() as u32, layout.words.as_mut_ptr(), self.body.as_mut_ptr().add(body),
                self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + result.body as usize);
            self.decorations.set_len(decorations + result.decorations as usize);
            self.next = result.next;
            self.current_label = result.label;
        }
    }
    pub(super) fn segmented_add(&mut self, scan: &Scan, active: u32, labels: [u32; 2]) {
        let segment = self.segment(&scan.ty, scan.operator, false);
        self.segment_add(&segment, &scan.values, active, labels);
    }

    pub(super) fn segmented_reduce_add(&mut self, reduction: &SegmentedReduction, active: u32, labels: [u32; 2]) {
        let values = &reduction.values;
        let segment = self.segment(&values.ty, values.operator, true);
        self.segment_add(&segment, &values.fields, active, labels);
    }

    fn segment_add(&mut self, scan: &Segment<'_>, fields: &Reduction, active: u32, [run, end]: [u32; 2]) {
        let target = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[fields.target]),
            self.index, Some(0), 0);
        let width = self.constant(Scalar::U32, 64);
        let step = self.constant(Scalar::U64, 4);
        let input = SegmentAdd {
            merge: self.segment_merge_input(scan),
            target,
            prefix_descriptor: self.buffers[self.scan_input_binding(fields)],
            width, step, active, run, end,
        };
        let count = scan.summary_words();
        let mut words = vec![0; 5 * count];
        let body = self.body.len();
        let decorations = self.decorations.len();
        self.body.reserve(243 + 41 * count + 74 * scan.fields.len());
        self.decorations.reserve(3 * scan.fields.len());
        unsafe {
            let result = emit_segment_add(input, scan.fields.as_ptr(), scan.fields.len() as u32,
                words.as_mut_ptr(), self.body.as_mut_ptr().add(body), self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + result.body as usize);
            self.decorations.set_len(decorations + result.decorations as usize);
            self.next = result.next;
            self.current_label = result.label;
        }
    }
}
