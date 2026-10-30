//! 用户值函数共用有序共享树；归约、扫描及摘要传播不引入另一套函数执行器。
use super::*;
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
    fn custom_node(&mut self, k: usize, shared: u32, pointer: u32, node: u32) -> Vec<u32> {
        (0..k)
            .map(|i| {
                let base = self.constant(Scalar::U32, i as u32 * 64);
                let at = self.id();
                self.emit(128, &[self.types[&Scalar::U32], at, base, node]);
                self.shared_at(pointer, shared, at)
            })
            .collect()
    }
    fn custom_address(&mut self, words: usize, binding: usize, index: u32) -> Vec<u32> {
        (0..words as u32)
            .map(|word| self.address_scalar(Scalar::U32, binding, index, words as u32 * 4, word * 4))
            .collect()
    }
    fn custom_read(&mut self, pointers: &[u32]) -> Vec<u32> {
        pointers
            .iter()
            .map(|p| {
                let value = self.id();
                self.emit(61, &[self.types[&Scalar::U32], value, *p, 2, 4]);
                value
            })
            .collect()
    }
    fn custom_write(&mut self, pointers: &[u32], values: &[u32]) {
        for (p, v) in pointers.iter().zip(values) {
            self.emit(62, &[*p, *v, 2, 4]);
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
        let values: Vec<_> = kernel.result.iter().map(|reg| self.to_words(*reg, Self::raw_scalar(self.stage.registers[*reg]))[0]).collect();
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
        capture: bool,
    ) -> Result<(Vec<u32>, Vec<u32>, Vec<u32>), &'static str> {
        let k = r.ty.size.div_ceil(4) as usize;
        let words = summary.words(k);
        let head_word = summary.head();
        let fields = &r.fields;
        let u = self.types[&Scalar::U32];
        self.block(
            &kernel.stage.code[kernel.invariant_end..kernel.initialize_end],
            None,
        )?;
        let initial: Vec<_> = kernel.initial.iter().map(|reg| self.to_words(*reg, Self::raw_scalar(self.stage.registers[*reg]))[0]).collect();
        let mut identity = self.pack_words(r.ty.size, r.ty.components.iter().zip(&initial).map(|((scalar, byte), value)| (*scalar, *byte, *value)));
        for _ in k..words {
            identity.push(self.constant(Scalar::U32, 0));
        }
        let own = self.custom_node(words, shared, pointer, local);
        self.custom_write(&own, &identity);
        let read = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, read, end]);
        self.emit(248, &[read]);
        for (i, reg) in kernel.source.iter().enumerate() {
            let scalar = kernel.stage.registers[*reg];
            let word = if tail { r.ty.components[i].1 } else { fields.source_word + r.source_ty.components[i].1 };
            let raw = self.load_raw_buffer(scalar,
                if tail {
                    self.scan_input_binding(fields)
                } else {
                    fields.source
                },
                self.index,
                if tail {
                    words as u32 * 4
                } else {
                    self.program.buffers[fields.source].stride
                },
                word,
            );
            self.from_words(*reg, &[raw], Self::raw_scalar(scalar));
        }
        self.block(
            &kernel.stage.code[kernel.initialize_end..kernel.project_end],
            None,
        )?;
        for (dst, src) in kernel.right.iter().zip(&kernel.projected) {
            self.regs[*dst] = self.value(*src);
        }
        let components: Vec<_> = kernel.right.iter().map(|reg| self.to_words(*reg, Self::raw_scalar(self.stage.registers[*reg]))[0]).collect();
        let values = self.pack_words(r.ty.size, r.ty.components.iter().zip(&components).map(|((scalar, byte), value)| (*scalar, *byte, *value)));
        self.custom_write(&own[..k], &values);
        if let Some(head_word) = head_word {
            let source = self.address_scalar(Scalar::U32,
                if tail {
                    self.scan_input_binding(fields)
                } else {
                    fields.source
                },
                self.index,
                if tail {
                    words as u32 * 4
                } else {
                    self.program.buffers[fields.source].stride
                },
                if tail { k as u32 * 4 } else { head_word },
            );
            let raw = self.custom_read(&[source])[0];
            let mut head = self.id();
            let zero = self.constant(Scalar::U32, 0);
            let one = self.constant(Scalar::U32, 1);
            self.emit(171, &[self.types[&Scalar::Bool], head, raw, zero]);
            if matches!(summary, Summary::Counted(_)) && !tail {
                let first = self.id();
                self.emit(170, &[self.types[&Scalar::Bool], first, self.index, zero]);
                let explicit = head;
                head = self.id();
                self.emit(166, &[self.types[&Scalar::Bool], head, explicit, first]);
            }
            let flag = self.id();
            self.emit(169, &[u, flag, head, one, zero]);
            self.custom_write(&own[k..k + 1], &[flag]);
            if matches!(summary, Summary::Counted(_)) {
                let count = if tail {
                    let at = self.address_scalar(Scalar::U32,
                        self.scan_input_binding(fields),
                        self.index,
                        words as u32 * 4,
                        (k as u32 + 1) * 4,
                    );
                    self.custom_read(&[at])[0]
                } else {
                    flag
                };
                self.custom_write(&own[k + 1..], &[count]);
            }
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        let input = if capture {
            self.custom_read(&own)
        } else if matches!(summary, Summary::Segmented(_)) && !tail {
            self.custom_read(&own[k..k + 1])
        } else {
            Vec::new()
        };
        self.scan_barrier();
        Ok((own, identity, input))
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
        let u = self.types[&Scalar::U32];
        let zero = self.constant(Scalar::U32, 0);
        let head = self.id();
        self.emit(171, &[self.types[&Scalar::Bool], head, right[k], zero]);
        let keep = self.id();
        let combine = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[head, keep, combine]);
        self.emit(248, &[keep]);
        self.emit(249, &[end]);
        self.emit(248, &[combine]);
        let merged = self.custom_pair(r, kernel, &left[..k], &right[..k])?;
        let combined = self.current_label;
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        let mut values: Vec<_> = merged
            .iter()
            .zip(right)
            .map(|(m, r)| {
                let value = self.id();
                self.emit(245, &[u, value, *r, keep, *m, combined]);
                value
            })
            .collect();
        let flags = self.id();
        self.emit(197, &[u, flags, left[k], right[k]]);
        values.push(flags);
        if matches!(summary, Summary::Counted(_)) {
            let count = self.id();
            self.emit(128, &[u, count, left[k + 1], right[k + 1]]);
            values.push(count);
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
        width: u32,
        remaining: u32,
        down: bool,
        summary: Summary,
    ) -> Result<(), &'static str> {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let mask = self.constant(Scalar::U32, width * 2 - 1);
        let bits = self.id();
        self.emit(199, &[u, bits, local, mask]);
        let selected = self.id();
        self.emit(170, &[b, selected, bits, mask]);
        let run = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[selected, run, end]);
        self.emit(248, &[run]);
        let width = self.constant(Scalar::U32, width);
        let other = self.id();
        self.emit(130, &[u, other, local, width]);
        let one = self.constant(Scalar::U32, 1);
        let right_start = self.id();
        self.emit(128, &[u, right_start, other, one]);
        let valid = self.id();
        self.emit(176, &[b, valid, right_start, remaining]);
        let words = summary.words(r.ty.size.div_ceil(4) as usize);
        let a = self.custom_node(words, shared, pointer, other);
        let z = self.custom_node(words, shared, pointer, local);
        let left = self.custom_read(&a);
        let right = self.custom_read(&z);
        if down {
            self.custom_write(&a, &right);
        }
        let merge = self.id();
        let absent = self.id();
        let merged = self.id();
        self.emit(247, &[merged, 0]);
        self.emit(250, &[valid, merge, absent]);
        self.emit(248, &[merge]);
        let values = if down {
            self.custom_merge(r, kernel, &right, &left, summary)
        } else {
            self.custom_merge(r, kernel, &left, &right, summary)
        }?;
        self.custom_write(&z, &values);
        self.emit(249, &[merged]);
        self.emit(248, &[absent]);
        if !down {
            self.custom_write(&z, &left);
        }
        self.emit(249, &[merged]);
        self.emit(248, &[merged]);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        self.scan_barrier();
        Ok(())
    }
    fn custom_remaining(&mut self, local: u32, count: u32) -> u32 {
        let u = self.types[&Scalar::U32];
        let base = self.id();
        self.emit(130, &[u, base, self.index, local]);
        let remaining = self.id();
        self.emit(130, &[u, remaining, count, base]);
        remaining
    }
    fn custom_owner(&mut self, local: u32, active: u32, count: u32) -> u32 {
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let first = self.id();
        self.emit(170, &[b, first, local, zero]);
        let empty = self.id();
        self.emit(170, &[b, empty, count, zero]);
        let needed = self.id();
        self.emit(166, &[b, needed, active, empty]);
        let owner = self.id();
        self.emit(167, &[b, owner, first, needed]);
        owner
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
        self.custom_input(
            r,
            kernel,
            tail,
            local,
            shared,
            pointer,
            active,
            Summary::Plain,
            false,
        )?;
        let remaining = self.custom_remaining(local, count);
        for width in [1, 2, 4, 8, 16, 32] {
            self.custom_tree(
                r,
                kernel,
                local,
                shared,
                pointer,
                width,
                remaining,
                false,
                Summary::Plain,
            )?;
        }
        let owner = self.custom_owner(local, active, count);
        let last = self.constant(Scalar::U32, 63);
        let words = self.custom_node(r.ty.size.div_ceil(4) as usize, shared, pointer, last);
        let root:Vec<Vec<_>>=r.ty.components.iter().map(|(scalar,offset)|
            (0..scalar.size().div_ceil(4)).map(|part|words[(offset/4+part) as usize]).collect()).collect();
        self.reduction_output(&r.fields, &r.ty, &root, owner);
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
            r, kernel, tail, local, shared, pointer, active, summary, false,
        )?;
        let head = input.first().copied();
        let remaining = self.custom_remaining(local, count);
        for width in [1, 2, 4, 8, 16, 32] {
            self.custom_tree(
                r, kernel, local, shared, pointer, width, remaining, false, summary,
            )?;
        }
        let owner = self.custom_owner(local, active, count);
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[self.types[&Scalar::U32], group, self.index, width]);
        let save = self.id();
        let saved = self.id();
        self.emit(247, &[saved, 0]);
        self.emit(250, &[owner, save, saved]);
        self.emit(248, &[save]);
        let last = self.constant(Scalar::U32, 63);
        let root = self.custom_node(words, shared, pointer, last);
        let total = self.custom_read(&root);
        let target = self.custom_address(words, self.program.buffers.len(), group);
        self.custom_write(&target, &total);
        self.custom_write(&root, &identity);
        self.emit(249, &[saved]);
        self.emit(248, &[saved]);
        self.scan_barrier();
        for width in [32, 16, 8, 4, 2, 1] {
            self.custom_tree(
                r, kernel, local, shared, pointer, width, remaining, true, summary,
            )?;
        }
        let values = self.custom_read(&own);
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
                    self.emit(169, &[u, result, is_head, *i, *v]);
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
                self.address_scalar(Scalar::U32, self.program.buffers.len(), group, words as u32 * 4, k as u32 * 4);
            self.custom_write(&[target], &[encoded]);
            self.emit(249, &[saved]);
            self.emit(248, &[saved]);
        } else if tail {
            let target = self.custom_address(words, r.fields.target, self.index);
            self.custom_write(&target, &values);
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
        let k = r.ty.size.div_ceil(4) as usize;
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
            let prefix = self.custom_address(words, self.scan_input_binding(&r.fields), group);
            let prefix = self.custom_read(&prefix);
            let target = self.custom_address(words, r.fields.target, self.index);
            let value = self.custom_read(&target);
            let total = self.custom_merge(r, kernel, &prefix, &value, summary)?;
            self.custom_write(&target, &total);
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
            pass == Pass::ScanRootAdd,
        )?;
        let remaining = self.custom_remaining(local, count);
        for width in [1, 2, 4, 8, 16, 32] {
            self.custom_tree(
                r, kernel, local, shared, pointer, width, remaining, false, summary,
            )?;
        }
        let owner = self.custom_owner(local, active, count);
        let save = self.id();
        let saved = self.id();
        self.emit(247, &[saved, 0]);
        self.emit(250, &[owner, save, saved]);
        self.emit(248, &[save]);
        let last = self.constant(Scalar::U32, 63);
        let root = self.custom_node(words, shared, pointer, last);
        if pass != Pass::ScanRootAdd {
            let total = self.custom_read(&root);
            let target = self.custom_address(words, self.program.buffers.len(), group);
            self.custom_write(&target, &total);
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
                self.program.buffers[reduction.count.binding].stride,
                reduction.count.word,
            );
            self.custom_write(&[target], &[zero]);
            self.emit(249, &[end]);
            self.emit(248, &[end]);
        }
        if pass != Pass::Main {
            self.custom_write(&root, &identity);
        }
        self.emit(249, &[saved]);
        self.emit(248, &[saved]);
        if pass == Pass::Main {
            return Ok(());
        }
        self.scan_barrier();
        for width in [32, 16, 8, 4, 2, 1] {
            self.custom_tree(
                r, kernel, local, shared, pointer, width, remaining, true, summary,
            )?;
        }
        let result = self.custom_read(&own);
        let write = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, write, end]);
        self.emit(248, &[write]);
        if pass == Pass::ScanTail {
            let target = self.custom_address(words, r.fields.target, self.index);
            self.custom_write(&target, &result);
        } else {
            let inclusive = self.custom_merge(r, kernel, &result, &input, summary)?;
            let first_group = self.id();
            self.emit(170, &[b, first_group, group, zero]);
            let load = self.id();
            let loaded = self.id();
            let before = self.current_label;
            self.emit(247, &[loaded, 0]);
            self.emit(250, &[first_group, loaded, load]);
            self.emit(248, &[load]);
            let prefix = self.custom_address(words, self.program.buffers.len(), group);
            let prefix = self.custom_read(&prefix);
            self.emit(249, &[loaded]);
            self.emit(248, &[loaded]);
            let prefix: Vec<_> = prefix
                .iter()
                .zip(&identity)
                .map(|(v, i)| {
                    let p = self.id();
                    self.emit(245, &[u, p, *v, load, *i, before]);
                    p
                })
                .collect();
            let total = self.custom_merge(r, kernel, &prefix, &inclusive, summary)?;
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
                r.fields.source,
                next,
                self.program.buffers[r.fields.source].stride,
                reduction.head_word,
            );
            let next_head = self.custom_read(&[next_head])[0];
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
            let index = self.id();
            self.emit(130, &[u, index, total[k + 1], one]);
            self.custom_store_target(r, index, &total[..k]);
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
                self.program.buffers[reduction.count.binding].stride,
                reduction.count.word,
            );
            self.custom_write(&[target], &[total[k + 1]]);
            self.emit(249, &[counted]);
            self.emit(248, &[counted]);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        Ok(())
    }
    fn custom_store_target(&mut self, r: &CustomReduction, index: u32, values: &[u32]) {
        for (scalar, offset) in &r.ty.components {
            let at = (offset / 4) as usize;
            let raw = if offset % 4 != 0 {
                let shift = self.constant(Scalar::U32, offset % 4 * 8);
                let shifted = self.id();
                self.emit(194, &[self.types[&Scalar::U32], shifted, values[at], shift]);
                self.join_words(*scalar, &[shifted], Scalar::U32)
            } else { self.join_words(*scalar, &values[at..at + scalar.size().div_ceil(4) as usize], Scalar::U32) };
            self.store_raw_buffer(*scalar, r.fields.target, index, self.program.buffers[r.fields.target].stride, r.fields.target_word + offset, raw);
        }
    }
    pub(super) fn custom_scan_add(
        &mut self,
        scan: &CustomScan,
        kernel: &ReductionKernel,
        root: bool,
    ) -> Result<(), &'static str> {
        let r = &scan.values;
        let words = scan.summary_words();
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[self.types[&Scalar::U32], group, self.index, width]);
        let end = if root && scan.head_word.is_some() {
            let u = self.types[&Scalar::U32];
            let b = self.types[&Scalar::Bool];
            let first = self.address_scalar(Scalar::U32,
                self.program.buffers.len(),
                group,
                words as u32 * 4,
                r.ty.size.div_ceil(4) * 4,
            );
            let first = self.custom_read(&[first])[0];
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
        let prefix = self.custom_address(words, self.scan_input_binding(&r.fields), group);
        let prefix = self.custom_read(&prefix);
        let values = if root {
            let values: Vec<_> = r.ty.components.iter().map(|(scalar, offset)| self.load_raw_buffer(
                *scalar, r.fields.target, self.index, self.program.buffers[r.fields.target].stride, r.fields.target_word + offset,
            )).collect();
            self.pack_words(r.ty.size, r.ty.components.iter().zip(&values).map(|((scalar, byte), value)| (*scalar, *byte, *value)))
        } else {
            let target = self.custom_address(words, r.fields.target, self.index);
            self.custom_read(&target)
        };
        let values = self.custom_merge(
            r,
            kernel,
            &prefix,
            &values,
            if root {
                Summary::Plain
            } else {
                scan.head_word.map(Summary::Segmented).unwrap_or(Summary::Plain)
            },
        )?;
        if root {
            self.custom_store_target(r, self.index, &values);
        } else {
            let target = self.custom_address(words, r.fields.target, self.index);
            self.custom_write(&target, &values);
        }
        if let Some(end) = end {
            self.emit(249, &[end]);
            self.emit(248, &[end]);
        }
        Ok(())
    }
}
