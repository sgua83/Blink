//! 用户值函数共用有序共享树；归约、扫描及摘要传播不引入另一套函数执行器。
use super::{reduce::CollectiveField, scan::PhysicalField, *};

#[repr(C)]
struct CustomCollectiveSource {
    source: PageAddress,
    active: u32,
    bool_type: u32,
    value_words: u32,
    tag_words: u32,
    head_offset: u32,
    step: u32,
    tail: u32,
}

#[repr(C)]
struct CollectiveBranch {
    words: u64,
    next: u32,
    run: u32,
    end: u32,
}

#[repr(C)]
struct CustomCollectiveJoin {
    uint_type: u32,
    from: u32,
    inactive: u32,
    end: u32,
    next: u32,
    scope: u32,
    semantics: u32,
}

#[repr(C)]
struct CustomTreeInput {
    uint_type: u32,
    bool_type: u32,
    lane: u32,
    shared: u32,
    pointer: u32,
    mask: u32,
    stride: u32,
    remaining: u32,
    one: u32,
    down: u32,
    next: u32,
}

#[repr(C)]
struct CustomTreeOpen {
    words: u64,
    next: u32,
    run: u32,
    absent: u32,
    merged: u32,
    end: u32,
}

#[repr(C)]
struct CustomTreeClose {
    uint_type: u32,
    from: u32,
    absent: u32,
    merged: u32,
    end: u32,
    scope: u32,
    semantics: u32,
    down: u32,
    next: u32,
}

#[repr(C)]
struct CustomSegmentJoin {
    uint_type: u32,
    skipped: u32,
    combined: u32,
    end: u32,
    value_words: u32,
    counted: u32,
    next: u32,
}

#[repr(C)]
struct CustomSegmentPrefix {
    parent: PageAddress,
    bool_type: u32,
    active: u32,
    step: u32,
}

#[repr(C)]
struct CustomSegmentOutput {
    source: PageAddress,
    bool_type: u32,
    length: u32,
    from: u32,
    end: u32,
    target: u32,
    target_word: u32,
    count_target: u32,
    count_word: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::open_custom_segment_finish"]
    fn open_custom_segment_finish(input: CustomSegmentPrefix, own: *const u32,
        identity: *const u32, words: u32, workspace: *mut u32, body: *mut u32) -> CollectiveBranch;
    #[link_name = "bootstrap.spirv_envelope::close_custom_segment_finish"]
    fn close_custom_segment_finish(input: CustomSegmentOutput, fields: *const CollectiveField,
        field_count: u32, total: *const u32, segments: u32, body: *mut u32) -> StageWords;
    #[link_name = "bootstrap.spirv_envelope::open_custom_segment"]
    fn open_custom_segment(bool_type: u32, right_head: u32, zero: u32, next: u32,
        body: *mut u32) -> CollectiveBranch;
    #[link_name = "bootstrap.spirv_envelope::close_custom_segment"]
    fn close_custom_segment(input: CustomSegmentJoin, left: *const u32, right: *const u32,
        values: *mut u32, body: *mut u32) -> StageWords;
    #[link_name = "bootstrap.spirv_envelope::open_custom_tree"]
    fn emit_open_custom_tree(input: CustomTreeInput, offsets: *const u32, own: *const u32,
        count: u32, workspace: *mut u32, body: *mut u32) -> CustomTreeOpen;
    #[link_name = "bootstrap.spirv_envelope::close_custom_tree"]
    fn emit_close_custom_tree(input: CustomTreeClose, own: *const u32, left: *const u32,
        values: *mut u32, count: u32, body: *mut u32) -> StageWords;
    #[link_name = "bootstrap.spirv_envelope::read_collective_result"]
    fn read_collective_result(target: &PageAddress, fields: *const CollectiveField,
        count: u32, values: *mut u32, locations: *mut u32, body: *mut u32) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::write_collective_result"]
    fn write_collective_result(target: PageAddress, fields: *const CollectiveField,
        count: u32, values: *const u32, locations: *const u32, body: *mut u32) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::custom_collective_read"]
    fn emit_custom_collective_read(input: CustomCollectiveSource, fields: *const PhysicalField,
        count: u32, values: *mut u32, body: *mut u32) -> CollectiveBranch;
    #[link_name = "bootstrap.spirv_envelope::custom_collective_join"]
    fn emit_custom_collective_join(input: CustomCollectiveJoin, values: *mut u32,
        identity: *const u32, shared: *const u32, count: u32, body: *mut u32) -> StageWords;
}
#[derive(Clone, Copy)]
enum Summary {
    Plain,
    Segmented(u32),
    Counted(u32),
}
impl Summary {
    fn head(self) -> Option<u32> {
        match self {
            Self::Plain => None,
            Self::Segmented(h) | Self::Counted(h) => Some(h),
        }
    }
    fn words(self, k: usize) -> usize {
        k + match self {
            Self::Plain => 0,
            Self::Segmented(_) => 1,
            Self::Counted(_) => 2,
        }
    }
}
impl Emitter<'_> {
    fn custom_arguments(&mut self, ty: &Ty, registers: &[Reg], values: &[u32]) {
        for (reg, (scalar, offset)) in registers.iter().zip(&ty.components) {
            let at = (offset / 4) as usize;
            if offset % 4 != 0 {
                let shift = self.constant(Scalar::U32, offset % 4 * 8);
                let raw = self.id();
                self.emit(194, &[self.types[&Scalar::U32], raw, values[at], shift]);
                self.from_words(*reg, &[raw], Scalar::U32);
            } else {
                self.from_words(*reg, &values[at..at + scalar.size().div_ceil(4) as usize], Scalar::U32);
            }
        }
    }
    fn custom_pair(
        &mut self,
        r: &CustomReduction,
        kernel: &ReductionKernel,
        left: &[u32],
        right: &[u32],
    ) -> Result<Vec<u32>, &'static str> {
        for reg in
            kernel.stage.registers.len() - r.combine.registers.len()..kernel.stage.registers.len()
        {
            if !kernel.invariants.contains(&reg) {
                self.regs[reg] = self.id();
            }
        }
        self.custom_arguments(&r.ty, &kernel.left, left);
        self.custom_arguments(&r.ty, &kernel.right, right);
        self.block(&kernel.stage.code[kernel.project_end..], None)?;
        let values: Vec<_> = kernel.result.iter().map(|reg| self.raw_bits(*reg)).collect();
        Ok(self.pack_words(r.ty.size, r.ty.components.iter().zip(&values).map(|((scalar, byte), value)| (*scalar, *byte, *value))))
    }
    fn custom_input(
        &mut self,
        r: &CustomReduction,
        kernel: &ReductionKernel,
        tail: bool,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        summary: Summary,
    ) -> Result<(Vec<u32>, Vec<u32>, Vec<u32>), &'static str> {
        let k = r.ty.size.div_ceil(4) as usize;
        let words = summary.words(k);
        let tags = words - k;
        self.block(
            &kernel.stage.code[kernel.invariant_end..kernel.initialize_end],
            None,
        )?;
        let initial: Vec<_> = kernel.initial.iter().map(|reg| self.raw_bits(*reg)).collect();
        let mut identity = self.pack_words(r.ty.size, r.ty.components.iter().zip(&initial)
            .map(|((scalar, byte), value)| (*scalar, *byte, *value)));
        identity.resize(words, self.constant(Scalar::U32, 0));
        let own = self.shared_planes(shared, pointer, local, words);

        let (binding, fields) = if tail {
            (self.scan_input_binding(&r.fields), Vec::new())
        } else {
            (r.fields.source, self.physical_fields(&r.source_ty, r.fields.source_word))
        };
        let head = summary.head().unwrap_or(0);
        let head_offset = if !tail && head != 0 { self.constant(Scalar::U64, head) } else { 0 };
        let step = if tail && words > 1 { self.constant(Scalar::U64, 4) } else { 0 };
        let source = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[binding]),
            self.index, None, 0);
        let input = CustomCollectiveSource {
            source, active, bool_type: self.types[&Scalar::Bool],
            value_words: k as u32, tag_words: tags as u32, head_offset, step,
            tail: u32::from(tail),
        };
        let mut incoming = vec![0; if tail { words } else { fields.len() + tags }];
        let inactive = self.current_label;
        let body = self.body.len();
        self.body.reserve(155 + 74 * fields.len() + 14 * words);
        let read = unsafe {
            let read = emit_custom_collective_read(input, fields.as_ptr(), fields.len() as u32,
                incoming.as_mut_ptr(), self.body.as_mut_ptr().add(body));
            self.body.set_len(body + read.words as usize);
            read
        };
        self.next = read.next;
        self.current_label = read.run;


        let mut values = if tail {
            incoming
        } else {
            for (reg, raw) in kernel.source.iter().zip(&incoming) {
                let scalar = kernel.stage.registers[*reg];
                self.from_words(*reg, &[*raw], Self::raw_scalar(scalar));
            }
            self.block(&kernel.stage.code[kernel.initialize_end..kernel.project_end], None)?;
            for (dst, src) in kernel.right.iter().zip(&kernel.projected) {
                self.regs[*dst] = self.value(*src);
            }
            let projected: Vec<_> = kernel.right.iter().map(|reg| self.raw_bits(*reg)).collect();
            let mut packed = self.pack_words(r.ty.size, r.ty.components.iter().zip(&projected)
                .map(|((scalar, byte), value)| (*scalar, *byte, *value)));
            packed.extend_from_slice(&incoming[fields.len()..]);
            packed
        };

        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x108);
        let join = CustomCollectiveJoin {
            uint_type: self.types[&Scalar::U32], from: self.current_label, inactive,
            end: read.end, next: self.next, scope, semantics,
        };
        let body = self.body.len();
        self.body.reserve(8 + 12 * words);
        unsafe {
            let joined = emit_custom_collective_join(join, values.as_mut_ptr(), identity.as_ptr(),
                own.as_ptr(), words as u32, self.body.as_mut_ptr().add(body));
            self.body.set_len(body + joined.words as usize);
            self.next = joined.next;
            self.current_label = joined.label;
        }
        Ok((own, identity, values))
    }


    fn custom_merge(
        &mut self,
        r: &CustomReduction,
        kernel: &ReductionKernel,
        left: &[u32],
        right: &[u32],
        summary: Summary,
    ) -> Result<Vec<u32>, &'static str> {
        if matches!(summary, Summary::Plain) {
            return self.custom_pair(r, kernel, left, right);
        }
        let k = r.ty.size.div_ceil(4) as usize;
        let uint_type = self.types[&Scalar::U32];
        let bool_type = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let skipped = self.current_label;
        let body = self.body.len();
        self.body.reserve(14);
        let open = unsafe {
            let open = open_custom_segment(bool_type, right[k], zero, self.next,
                self.body.as_mut_ptr().add(body));
            self.body.set_len(body + open.words as usize);
            open
        };
        self.next = open.next;
        self.current_label = open.run;
        let mut values = self.custom_pair(r, kernel, &left[..k], &right[..k])?;
        values.resize(summary.words(k), 0);
        let join = CustomSegmentJoin {
            uint_type, skipped, combined: self.current_label, end: open.end,
            value_words: k as u32, counted: u32::from(matches!(summary, Summary::Counted(_))),
            next: self.next,
        };
        let body = self.body.len();
        self.body.reserve(14 + 7 * k);
        unsafe {
            let emitted = close_custom_segment(join, left.as_ptr(), right.as_ptr(),
                values.as_mut_ptr(), self.body.as_mut_ptr().add(body));
            self.body.set_len(body + emitted.words as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
        Ok(values)
    }
    fn custom_tree(
        &mut self,
        r: &CustomReduction,
        kernel: &ReductionKernel,
        local: u32,
        shared: u32,
        pointer: u32,
        remaining: u32,
        own: &[u32],
        down: bool,
        summary: Summary,
    ) -> Result<(), &'static str> {
        let words = own.len();
        let uint_type = self.types[&Scalar::U32];
        let bool_type = self.types[&Scalar::Bool];
        let one = self.constant(Scalar::U32, 1);
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x108);
        let mut offsets = vec![0; words];
        for (part, offset) in offsets.iter_mut().enumerate().skip(1) {
            *offset = self.constant(Scalar::U32, part as u32 * 64);
        }
        let mut workspace = vec![0; 3 * words];
        for depth in 0..6 {
            let width = if down { 32 >> depth } else { 1 << depth };
            let mask = self.constant(Scalar::U32, 2 * width - 1);
            let stride = self.constant(Scalar::U32, width);
            let input = CustomTreeInput {
                uint_type, bool_type, lane: local, shared, pointer, mask, stride,
                remaining, one, down: u32::from(down), next: self.next,
            };
            let body = self.body.len();
            self.body.reserve(48 + 25 * words);
            let open = unsafe {
                let open = emit_open_custom_tree(input, offsets.as_ptr(), own.as_ptr(),
                    words as u32, workspace.as_mut_ptr(), self.body.as_mut_ptr().add(body));
                self.body.set_len(body + open.words as usize);
                open
            };
            self.next = open.next;
            self.current_label = open.run;
            let (left, right) = workspace[words..].split_at(words);
            let (before, after) = if down { (right, left) } else { (left, right) };
            let mut values = self.custom_merge(r, kernel, before, after, summary)?;


            let close = CustomTreeClose {
                uint_type, from: self.current_label, absent: open.absent, merged: open.merged,
                end: open.end, scope, semantics, down: u32::from(down), next: self.next,
            };
            let body = self.body.len();
            self.body.reserve(12 + 12 * words);
            unsafe {
                let emitted = emit_close_custom_tree(close, own.as_ptr(), left.as_ptr(),
                    values.as_mut_ptr(), words as u32, self.body.as_mut_ptr().add(body));
                self.body.set_len(body + emitted.words as usize);
                self.next = emitted.next;
                self.current_label = emitted.label;
            }
        }
        Ok(())
    }


    pub(super) fn custom_reduce(
        &mut self,
        r: &CustomReduction,
        kernel: &ReductionKernel,
        tail: bool,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) -> Result<(), &'static str> {
        let (own, _, _) = self.custom_input(
            r,
            kernel,
            tail,
            local,
            shared,
            pointer,
            active,
            Summary::Plain,
        )?;
        let remaining = self.group_remaining(local, count);
        self.custom_tree(r, kernel, local, shared, pointer, remaining, &own, false, Summary::Plain)?;
        let (owner, _) = self.collective_writer(local, active, count);
        let last = self.constant(Scalar::U32, 63);
        let words = self.shared_planes(shared, pointer, last, r.ty.size.div_ceil(4) as usize);
        self.reduction_output(&r.fields, &r.ty, &words, owner);
        Ok(())
    }
    pub(super) fn custom_scan(
        &mut self,
        scan: &CustomScan,
        kernel: &ReductionKernel,
        tail: bool,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) -> Result<(), &'static str> {
        let r = &scan.values;
        let k = r.ty.size.div_ceil(4) as usize;
        let words = scan.summary_words();
        let summary = scan.head_word.map(Summary::Segmented).unwrap_or(Summary::Plain);
        let (own, identity, input) = self.custom_input(
            r, kernel, tail, local, shared, pointer, active, summary,
        )?;
        let head = scan.head_word.map(|_| input[k]);
        let remaining = self.group_remaining(local, count);
        self.custom_tree(r, kernel, local, shared, pointer, remaining, &own, false, summary)?;
        let (owner, _) = self.collective_writer(local, active, count);
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[self.types[&Scalar::U32], group, self.index, width]);
        let save = self.id();
        let saved = self.id();
        self.emit(247, &[saved, 0]);
        self.emit(250, &[owner, save, saved]);
        self.emit(248, &[save]);
        let last = self.constant(Scalar::U32, 63);
        let root = self.shared_planes(shared, pointer, last, words);
        let total = self.read_summary_words(&root);
        let target = self.summary_addresses(words, self.program.buffers.len(), group);
        self.write_summary_words(&target, &total);
        self.write_summary_words(&root, &identity);
        self.emit(249, &[saved]);
        self.emit(248, &[saved]);
        self.scan_barrier();
        self.custom_tree(r, kernel, local, shared, pointer, remaining, &own, true, summary)?;
        let values = self.read_summary_words(&own);
        let write = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, write, end]);
        self.emit(248, &[write]);
        if !tail && let Some(head) = head {
            let u = self.types[&Scalar::U32];
            let b = self.types[&Scalar::Bool];
            let zero = self.constant(Scalar::U32, 0);
            let is_head = self.id();
            self.emit(171, &[b, is_head, head, zero]);
            let result: Vec<_> = values
                .iter()
                .zip(&identity)
                .take(k)
                .map(|(v, i)| {
                    let result = self.id();
                    self.select(u, result, is_head, *i, *v);
                    result
                })
                .collect();
            self.custom_store_target(r, self.index, &result);
            // 根摘要的标记同时编码首段头 lane+1，供最终传播跳过已闭合的段。
            let no_previous = self.id();
            self.emit(170, &[b, no_previous, values[k], zero]);
            let first = self.id();
            self.emit(167, &[b, first, is_head, no_previous]);
            let save = self.id();
            let saved = self.id();
            self.emit(247, &[saved, 0]);
            self.emit(250, &[first, save, saved]);
            self.emit(248, &[save]);
            let one = self.constant(Scalar::U32, 1);
            let encoded = self.id();
            self.emit(128, &[u, encoded, local, one]);
            let target =
                self.address_scalar(Scalar::U32, self.program.buffers.len(), group, k as u32 * 4);
            self.write_summary_words(&[target], &[encoded]);
            self.emit(249, &[saved]);
            self.emit(248, &[saved]);
        } else if tail {
            let target = self.summary_addresses(words, r.fields.target, self.index);
            self.write_summary_words(&target, &values);
        } else {
            self.custom_store_target(r, self.index, &values);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        Ok(())
    }
    pub(super) fn custom_segmented_reduce(
        &mut self,
        reduction: &CustomSegmentedReduction,
        kernel: &ReductionKernel,
        pass: Pass,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) -> Result<(), &'static str> {
        let r = &reduction.values;
        let summary = Summary::Counted(reduction.head_word);
        let words = reduction.summary_words();
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[u, group, self.index, width]);
        if pass == Pass::ScanAdd {
            let run = self.id();
            let end = self.id();
            self.emit(247, &[end, 0]);
            self.emit(250, &[active, run, end]);
            self.emit(248, &[run]);
            let prefix = self.summary_addresses(words, self.scan_input_binding(&r.fields), group);
            let prefix = self.read_summary_words(&prefix);
            let target = self.summary_addresses(words, r.fields.target, self.index);
            let value = self.read_summary_words(&target);
            let total = self.custom_merge(r, kernel, &prefix, &value, summary)?;
            self.write_summary_words(&target, &total);
            self.emit(249, &[end]);
            self.emit(248, &[end]);
            return Ok(());
        }
        let (own, identity, input) = self.custom_input(
            r,
            kernel,
            pass == Pass::ScanTail,
            local,
            shared,
            pointer,
            active,
            summary,
        )?;
        let remaining = self.group_remaining(local, count);
        self.custom_tree(r, kernel, local, shared, pointer, remaining, &own, false, summary)?;
        let (owner, _) = self.collective_writer(local, active, count);
        let save = self.id();
        let saved = self.id();
        self.emit(247, &[saved, 0]);
        self.emit(250, &[owner, save, saved]);
        self.emit(248, &[save]);
        let last = self.constant(Scalar::U32, 63);
        let root = self.shared_planes(shared, pointer, last, words);
        if pass != Pass::ScanRootAdd {
            let total = self.read_summary_words(&root);
            let target = self.summary_addresses(words, self.program.buffers.len(), group);
            self.write_summary_words(&target, &total);
        } else {
            let empty = self.id();
            self.emit(170, &[b, empty, count, zero]);
            let write = self.id();
            let end = self.id();
            self.emit(247, &[end, 0]);
            self.emit(250, &[empty, write, end]);
            self.emit(248, &[write]);
            let target = self.address_scalar(Scalar::U32,
                reduction.count.binding,
                zero,
                reduction.count.word);
            self.write_summary_words(&[target], &[zero]);
            self.emit(249, &[end]);
            self.emit(248, &[end]);
        }
        if pass != Pass::Main {
            self.write_summary_words(&root, &identity);
        }
        self.emit(249, &[saved]);
        self.emit(248, &[saved]);
        if pass == Pass::Main {
            return Ok(());
        }
        self.scan_barrier();
        self.custom_tree(r, kernel, local, shared, pointer, remaining, &own, true, summary)?;
        if pass != Pass::ScanTail {
            return self.custom_segmented_finish(reduction, kernel, active, count, group, &own, &identity, &input);
        }
        let result = self.read_summary_words(&own);
        let write = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, write, end]);
        self.emit(248, &[write]);
        let target = self.summary_addresses(words, r.fields.target, self.index);
        self.write_summary_words(&target, &result);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        Ok(())
    }

    fn custom_segmented_finish(
        &mut self,
        reduction: &CustomSegmentedReduction,
        kernel: &ReductionKernel,
        active: u32,
        length: u32,
        group: u32,
        own: &[u32],
        identity: &[u32],
        item: &[u32],
    ) -> Result<(), &'static str> {
        let r = &reduction.values;
        let summary = Summary::Counted(reduction.head_word);
        let words = own.len();
        let bool_type = self.types[&Scalar::Bool];
        let step = self.constant(Scalar::U64, 4);
        let parent = self.page_input(Scalar::U32,
            PageSource::Dataset(self.buffers[self.program.buffers.len()]), group, Some(0), 0);
        let prefix = CustomSegmentPrefix { parent, bool_type, active, step };
        let mut workspace = vec![0; 2 * words];
        let body = self.body.len();
        self.body.reserve(136 + 28 * words);
        let open = unsafe {
            let open = open_custom_segment_finish(prefix, own.as_ptr(), identity.as_ptr(),
                words as u32, workspace.as_mut_ptr(), self.body.as_mut_ptr().add(body));
            self.body.set_len(body + open.words as usize);
            open
        };
        self.next = open.next;
        self.current_label = open.run;

        let (exclusive, prefix) = workspace.split_at(words);
        let inclusive = self.custom_merge(r, kernel, exclusive, item, summary)?;
        let total = self.custom_merge(r, kernel, prefix, &inclusive, summary)?;

        let fields = self.collective_fields(&r.ty, None);
        let target_word = if r.fields.target_word == 0 { 0 }
            else { self.constant(Scalar::U32, r.fields.target_word) };
        let count_word = self.constant(Scalar::U32, reduction.count.word);
        let source = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[r.fields.source]),
            self.index, Some(reduction.head_word), 0);
        let output = CustomSegmentOutput {
            source, bool_type, length, from: self.current_label, end: open.end,
            target: self.buffers[r.fields.target], target_word,
            count_target: self.buffers[reduction.count.binding], count_word,
        };
        let body = self.body.len();
        self.body.reserve(420 + 44 * fields.len());
        unsafe {
            let emitted = close_custom_segment_finish(output, fields.as_ptr(), fields.len() as u32,
                total.as_ptr(), total[words - 1], self.body.as_mut_ptr().add(body));
            self.body.set_len(body + emitted.words as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
        Ok(())
    }
    fn custom_result_target(&mut self, r: &CustomReduction, index: u32) -> (PageAddress, Vec<CollectiveField>) {
        let fields = self.collective_fields(&r.ty, None);
        let target = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[r.fields.target]),
            index, (r.fields.target_word != 0).then_some(r.fields.target_word), 0);
        (target, fields)
    }

    fn custom_result_read(&mut self, target: &PageAddress, fields: &[CollectiveField], words: usize)
        -> (Vec<u32>, Vec<u32>)
    {
        let mut values = vec![target.zero; words];
        let mut locations = vec![0; 2 * fields.len()];
        if !fields.is_empty() {
            let body = self.body.len();
            self.body.reserve(109 + 60 * fields.len());
            unsafe {
                let emitted = read_collective_result(target, fields.as_ptr(), fields.len() as u32,
                    values.as_mut_ptr(), locations.as_mut_ptr(), self.body.as_mut_ptr().add(body));
                self.body.set_len(body + emitted.words as usize);
                self.next = emitted.next;
            }
        }
        (values, locations)
    }


    fn custom_result_write(&mut self, mut target: PageAddress, fields: &[CollectiveField],
        values: &[u32], locations: Option<&[u32]>,
    ) {
        if fields.is_empty() {
            return;
        }
        target.next = self.next;
        let body = self.body.len();
        self.body.reserve(109 + 44 * fields.len());
        unsafe {
            let emitted = write_collective_result(target, fields.as_ptr(), fields.len() as u32,
                values.as_ptr(), locations.map_or(std::ptr::null(), |p| p.as_ptr()),
                self.body.as_mut_ptr().add(body));
            self.body.set_len(body + emitted.words as usize);
            self.next = emitted.next;
        }
    }

    fn custom_store_target(&mut self, r: &CustomReduction, index: u32, values: &[u32]) {
        if r.ty.components.is_empty() {
            return;
        }
        let (target, fields) = self.custom_result_target(r, index);
        self.custom_result_write(target, &fields, values, None);
    }


    pub(super) fn custom_scan_add(
        &mut self,
        scan: &CustomScan,
        kernel: &ReductionKernel,
        root: bool,
    ) -> Result<(), &'static str> {
        let r = &scan.values;
        let words = if root { r.ty.size.div_ceil(4) as usize } else { scan.summary_words() };
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[self.types[&Scalar::U32], group, self.index, width]);
        let end = if root && scan.head_word.is_some() {
            let u = self.types[&Scalar::U32];
            let b = self.types[&Scalar::Bool];
            let first = self.address_scalar(Scalar::U32,
                self.program.buffers.len(),
                group,
                r.ty.size.div_ceil(4) * 4);
            let first = self.read_summary_words(&[first])[0];
            let lane = self.id();
            self.emit(137, &[u, lane, self.index, width]);
            let one = self.constant(Scalar::U32, 1);
            let encoded = self.id();
            self.emit(128, &[u, encoded, lane, one]);
            let before = self.id();
            self.emit(176, &[b, before, encoded, first]);
            let zero = self.constant(Scalar::U32, 0);
            let no_head = self.id();
            self.emit(170, &[b, no_head, first, zero]);
            let needed = self.id();
            self.emit(166, &[b, needed, before, no_head]);
            let run = self.id();
            let end = self.id();
            self.emit(247, &[end, 0]);
            self.emit(250, &[needed, run, end]);
            self.emit(248, &[run]);
            Some(end)
        } else {
            None
        };
        let prefix = self.summary_addresses(words, self.scan_input_binding(&r.fields), group);
        let prefix = self.read_summary_words(&prefix);
        if root {
            let (target, fields) = self.custom_result_target(r, self.index);
            let (values, locations) = self.custom_result_read(&target, &fields, words);
            let values = self.custom_merge(r, kernel, &prefix, &values, Summary::Plain)?;
            self.custom_result_write(target, &fields, &values, Some(&locations));
        } else {
            let target = self.summary_addresses(words, r.fields.target, self.index);
            let values = self.read_summary_words(&target);
            let summary = scan.head_word.map(Summary::Segmented).unwrap_or(Summary::Plain);
            let values = self.custom_merge(r, kernel, &prefix, &values, summary)?;
            self.write_summary_words(&target, &values);
        }
        if let Some(end) = end {
            self.emit(249, &[end]);
            self.emit(248, &[end]);
        }
        Ok(())
    }
}
