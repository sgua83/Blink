//! 用户值函数共用有序共享树；归约、扫描及摘要传播不引入另一套函数执行器。
use super::{reduce::CollectiveField, scan::PhysicalField, *};
use crate::ir::dataflow::Span;

#[repr(C)]
struct CustomInput {
    source: PageInputs,
    summary_descriptor: u32,
    active: u32,
    lane: u32,
    shared: u32,
    pointer: u32,
    value_words: u32,
    tag_words: u32,
    head_word: u32,
    tail: u32,
}

#[repr(C)]
struct CustomInputHost {
    context: *mut std::ffi::c_void,
    identity: unsafe extern "C" fn(*mut std::ffi::c_void, *mut u32, u32) -> u32,
    fields: unsafe extern "C" fn(*mut std::ffi::c_void) -> Span,
    project: unsafe extern "C" fn(*mut std::ffi::c_void, *mut u32, u32, u32, u32) -> u32,
}

#[repr(C)]
struct CollectiveBranch {
    words: u64,
    next: u32,
    run: u32,
    end: u32,
}

#[repr(C)]
struct CustomTree {
    lane: u32,
    shared: u32,
    pointer: u32,
    remaining: u32,
    value_words: u32,
    tag_words: u32,
    down: u32,
}

#[repr(C)]
struct CustomMerge {
    value_words: u32,
    tag_words: u32,
    label: u32,
}

#[repr(C)]
struct CustomCombine {
    context: *mut std::ffi::c_void,
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, *const u32, *const u32, *mut u32, u32, u32) -> u32,
}

struct CheckedCollective<'a, 'stage> {
    emitter: *mut Emitter<'stage>,
    reduction: &'a CustomReduction,
    kernel: &'a ReductionKernel,
    error: Option<&'static str>,
}

struct InputContext<'a, 'stage> {
    checked: CheckedCollective<'a, 'stage>,
    fields: Vec<PhysicalField>,
}

unsafe extern "C" fn emit_checked_identity(
    context: *mut std::ffi::c_void, output: *mut u32, count: u32,
) -> u32 {
    let context = unsafe { &mut *context.cast::<InputContext<'_, '_>>() };
    let emitter = unsafe { &mut *context.checked.emitter };
    let kernel = context.checked.kernel;
    if let Err(error) = emitter.block(&kernel.stage.code[kernel.invariant_end..kernel.initialize_end]) {
        context.checked.error = Some(error);
        return 0;
    }
    let output = unsafe { std::slice::from_raw_parts_mut(output, count as usize) };
    emitter.pack_custom_values(&context.checked.reduction.ty, &kernel.initial, output);
    emitter.current_label
}

unsafe extern "C" fn prepare_input_fields(context: *mut std::ffi::c_void) -> Span {
    let context = unsafe { &mut *context.cast::<InputContext<'_, '_>>() };
    let emitter = unsafe { &mut *context.checked.emitter };
    let reduction = context.checked.reduction;
    context.fields = emitter.physical_fields(&reduction.source_ty, reduction.fields.source_word);
    Span::slice(&context.fields)
}

unsafe extern "C" fn emit_checked_projection(
    context: *mut std::ffi::c_void, values: *mut u32, fields: u32, count: u32, label: u32,
) -> u32 {
    let context = unsafe { &mut *context.cast::<InputContext<'_, '_>>() };
    let emitter = unsafe { &mut *context.checked.emitter };
    let kernel = context.checked.kernel;
    let (fields, count) = (fields as usize, count as usize);
    let values = unsafe { std::slice::from_raw_parts_mut(values, fields.max(count)) };
    emitter.current_label = label;
    for (reg, raw) in kernel.source.iter().zip(&values[..fields]) {
        let scalar = kernel.stage.registers[*reg];
        emitter.from_words(*reg, &[*raw], Emitter::raw_scalar(scalar));
    }
    if let Err(error) = emitter.block(&kernel.stage.code[kernel.initialize_end..kernel.project_end]) {
        context.checked.error = Some(error);
        return 0;
    }
    for (dst, src) in kernel.right.iter().zip(&kernel.projected) {
        emitter.regs[*dst] = emitter.value(*src);
    }
    emitter.pack_custom_values(&context.checked.reduction.ty, &kernel.right, &mut values[..count]);
    emitter.current_label
}

unsafe extern "C" fn emit_checked_combine(
    context: *mut std::ffi::c_void, left: *const u32, right: *const u32,
    output: *mut u32, count: u32, label: u32,
) -> u32 {
    let context = unsafe { &mut *context.cast::<CheckedCollective<'_, '_>>() };
    let emitter = unsafe { &mut *context.emitter };
    let count = count as usize;
    let left = unsafe { std::slice::from_raw_parts(left, count) };
    let right = unsafe { std::slice::from_raw_parts(right, count) };
    let output = unsafe { std::slice::from_raw_parts_mut(output, count) };
    emitter.current_label = label;
    match emitter.custom_pair(context.reduction, context.kernel, left, right, output) {
        Ok(()) => emitter.current_label,
        Err(error) => {
            context.error = Some(error);
            0
        }
    }
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

#[repr(u32)]
enum SummaryRootMode {
    Publish = 0,
    PublishReset = 1,
    EmptyCountReset = 2,
}

#[repr(C)]
struct CustomSummaryRoot {
    target: PageAddress,
    mode: SummaryRootMode,
    bool_type: u32,
    zero: u32,
    lane: u32,
    active: u32,
    length: u32,
    width: u32,
    last: u32,
    shared: u32,
    pointer: u32,
    step: u32,
    scope: u32,
    semantics: u32,
}

#[repr(C)]
struct SummaryRootWords {
    words: u64,
    next: u32,
    label: u32,
    group: u32,
}

#[repr(C)]
struct CustomScanWrite {
    target: PageAddress,
    active: u32,
    bool_type: u32,
    head: u32,
    step: u32,
    tail: u32,
}

#[repr(C)]
struct CustomPrefix {
    target: PageAddress,
    prefix_descriptor: u32,
    head_descriptor: u32,
    head_word: u32,
    width: u32,
    bool_type: u32,
    active: u32,
    step: u32,
    packed: u32,
    words: u32,
    field_count: u32,
}

#[repr(C)]
struct CustomPrefixOpen {
    words: u64,
    next: u32,
    label: u32,
    end: u32,
    head_end: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::open_custom_prefix"]
    fn open_custom_prefix(input: &CustomPrefix, fields: *const CollectiveField,
        workspace: *mut u32, body: *mut u32) -> CustomPrefixOpen;
    #[link_name = "bootstrap.spirv_envelope::close_custom_prefix"]
    fn close_custom_prefix(input: &CustomPrefix, opened: CustomPrefixOpen,
        fields: *const CollectiveField, locations: *const u32, values: *const u32,
        next: u32, body: *mut u32) -> StageWords;
    #[link_name = "bootstrap.spirv_envelope::custom_scan_write"]
    fn emit_custom_scan_write(input: CustomScanWrite, own: *const u32, identity: *const u32,
        words: u32, fields: *const CollectiveField, field_count: u32,
        workspace: *mut u32, body: *mut u32) -> StageWords;
    #[link_name = "bootstrap.spirv_envelope::custom_summary_root"]
    fn emit_custom_summary_root(input: CustomSummaryRoot, identity: *const u32,
        words: u32, workspace: *mut u32, body: *mut u32) -> SummaryRootWords;
    #[link_name = "bootstrap.spirv_envelope::open_custom_segment_finish"]
    fn open_custom_segment_finish(input: CustomSegmentPrefix, own: *const u32,
        identity: *const u32, words: u32, workspace: *mut u32, body: *mut u32) -> CollectiveBranch;
    #[link_name = "bootstrap.spirv_envelope::close_custom_segment_finish"]
    fn close_custom_segment_finish(input: CustomSegmentOutput, fields: *const CollectiveField,
        field_count: u32, total: *const u32, segments: u32, body: *mut u32) -> StageWords;
    #[link_name = "bootstrap.spirv_envelope::emit_custom_merge"]
    fn emit_custom_merge(input: CustomMerge, left: *const u32, right: *const u32,
        result: *mut u32, memory: MemoryHost, combine: CustomCombine) -> u32;
    #[link_name = "bootstrap.spirv_envelope::emit_custom_tree"]
    fn emit_custom_tree(input: CustomTree, own: *const u32, workspace: *mut u32,
        memory: MemoryHost, combine: CustomCombine) -> u32;
    #[link_name = "bootstrap.spirv_envelope::emit_custom_input"]
    fn emit_custom_input(input: CustomInput, own: *mut u32, identity: *mut u32,
        values: *mut u32, memory: MemoryHost, host: CustomInputHost) -> u32;
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
                let ty = self.types[&Scalar::U32];
                self.emit_body(5, |body| unsafe { unpack_summary_word(ty, raw, values[at], shift, body) });
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
        output: &mut [u32],
    ) -> Result<(), &'static str> {
        for reg in
            kernel.stage.registers.len() - r.combine.registers.len()..kernel.stage.registers.len()
        {
            if !kernel.invariants.contains(&reg) {
                self.regs[reg] = self.id();
            }
        }
        self.custom_arguments(&r.ty, &kernel.left, left);
        self.custom_arguments(&r.ty, &kernel.right, right);
        self.block(&kernel.stage.code[kernel.project_end..])?;
        self.pack_custom_values(&r.ty, &kernel.result, output);
        Ok(())
    }

    fn pack_custom_values(&mut self, ty: &Ty, registers: &[Reg], output: &mut [u32]) {
        let values: Vec<_> = registers.iter().map(|reg| self.raw_bits(*reg)).collect();
        let zero = self.constant(Scalar::U32, 0);
        output.fill(zero);
        self.pack_word_fields(output, zero, ty.components.iter().zip(&values)
            .map(|((scalar, byte), value)| (*scalar, *byte, *value)));
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
        let value_words = r.ty.size.div_ceil(4) as usize;
        let words = summary.words(value_words);
        let (summary_descriptor, fields) = if tail {
            (self.buffers[self.scan_input_binding(&r.fields)], 0)
        } else {
            (0, r.source_ty.components.len())
        };
        let mut own = vec![0; words];
        let mut identity = vec![0; words];
        let mut values = vec![0; summary.words(value_words.max(fields))];
        let input = CustomInput {
            source: self.page_inputs(self.buffers[r.fields.source], self.index, None, 0),
            summary_descriptor,
            active,
            lane: local,
            shared,
            pointer,
            value_words: value_words as u32,
            tag_words: (words - value_words) as u32,
            head_word: summary.head().unwrap_or(0),
            tail: u32::from(tail),
        };
        let memory = self.memory_host();
        let mut context = InputContext {
            checked: CheckedCollective {
                emitter: memory.scalars.context.cast(), reduction: r, kernel, error: None,
            },
            fields: Vec::new(),
        };
        let host = CustomInputHost {
            context: std::ptr::from_mut(&mut context).cast(),
            identity: emit_checked_identity,
            fields: prepare_input_fields,
            project: emit_checked_projection,
        };
        let label = unsafe {
            emit_custom_input(input, own.as_mut_ptr(), identity.as_mut_ptr(), values.as_mut_ptr(), memory, host)
        };
        if let Some(error) = context.checked.error {
            return Err(error);
        }
        self.current_label = label;
        values.truncate(words);
        Ok((own, identity, values))
    }

    fn with_custom_combine(
        &mut self,
        reduction: &CustomReduction,
        kernel: &ReductionKernel,
        emit: impl FnOnce(MemoryHost, CustomCombine) -> u32,
    ) -> Result<(), &'static str> {
        let memory = self.memory_host();
        let mut context = CheckedCollective {
            emitter: memory.scalars.context.cast(), reduction, kernel, error: None,
        };
        let combine = CustomCombine {
            context: std::ptr::from_mut(&mut context).cast(),
            emit: emit_checked_combine,
        };
        let label = emit(memory, combine);
        if let Some(error) = context.error {
            return Err(error);
        }
        self.current_label = label;
        Ok(())
    }

    fn custom_merge(
        &mut self,
        r: &CustomReduction,
        kernel: &ReductionKernel,
        left: &[u32],
        right: &[u32],
        summary: Summary,
    ) -> Result<Vec<u32>, &'static str> {
        let value_words = r.ty.size.div_ceil(4);
        let tag_words = summary.words(0) as u32;
        let mut values = vec![0; (value_words + tag_words) as usize];
        let input = CustomMerge { value_words, tag_words, label: self.current_label };
        self.with_custom_combine(r, kernel, |memory, combine| unsafe {
            emit_custom_merge(input, left.as_ptr(), right.as_ptr(), values.as_mut_ptr(), memory, combine)
        })?;
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
        let value_words = r.ty.size.div_ceil(4);
        let tag_words = summary.words(0) as u32;
        let input = CustomTree { lane: local, shared, pointer, remaining, value_words, tag_words, down: u32::from(down) };
        let mut workspace = vec![0; 5 * own.len()];
        self.with_custom_combine(r, kernel, |memory, combine| unsafe {
            emit_custom_tree(input, own.as_ptr(), workspace.as_mut_ptr(), memory, combine)
        })
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
        self.reduction_output(&r.fields, &r.ty, &own, local);
        Ok(())
    }
    fn custom_summary_root(
        &mut self,
        binding: usize,
        word: u32,
        mode: SummaryRootMode,
        lane: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        length: u32,
        identity: &[u32],
    ) -> u32 {
        let words = identity.len();
        let mut workspace = vec![0; 3 * words];
        for (part, slot) in workspace[..words].iter_mut().enumerate().skip(1) {
            *slot = self.constant(Scalar::U32, (part as u32) * 64);
        }
        let width = self.constant(Scalar::U32, 64);
        let last = self.constant(Scalar::U32, 63);
        let step = self.constant(Scalar::U64, 4);
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x108);
        let zero = self.constant(Scalar::U32, 0);
        let target = self.page_input(Scalar::U32, self.buffers[binding],
            self.index, Some(word), 0);
        let input = CustomSummaryRoot {
            target, mode, bool_type: self.types[&Scalar::Bool], zero, lane, active, length,
            width, last, shared, pointer, step, scope, semantics,
        };
        let body = self.body.len();
        self.body.reserve(169 + 36 * words);
        unsafe {
            let emitted = emit_custom_summary_root(input, identity.as_ptr(), words as u32,
                workspace.as_mut_ptr(), self.body.as_mut_ptr().add(body));
            self.body.set_len(body + emitted.words as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
            emitted.group
        }
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
        let remaining = self.group_remaining(local, count);
        self.custom_tree(r, kernel, local, shared, pointer, remaining, &own, false, summary)?;
        self.custom_summary_root(self.program.buffers.len(), 0, SummaryRootMode::PublishReset,
            local, shared, pointer, active, count, &identity);
        self.custom_tree(r, kernel, local, shared, pointer, remaining, &own, true, summary)?;
        let step = if tail && words > 1 { self.constant(Scalar::U64, 4) } else { 0 };
        let (target, fields) = if tail {
            let target = self.page_input(Scalar::U32, self.buffers[r.fields.target],
                self.index, Some(0), 0);
            (target, Vec::new())
        } else {
            self.custom_result_target(r, self.index)
        };
        let head = if !tail && scan.head_word.is_some() { input[k] } else { 0 };
        let request = CustomScanWrite {
            target, active, bool_type: self.types[&Scalar::Bool], head, step, tail: u32::from(tail),
        };
        let mut workspace = vec![0; 2 * words];
        let body = self.body.len();
        self.body.reserve(128 + 20 * words + 44 * fields.len());
        unsafe {
            let emitted = emit_custom_scan_write(request, own.as_ptr(), identity.as_ptr(), words as u32,
                fields.as_ptr(), fields.len() as u32, workspace.as_mut_ptr(), self.body.as_mut_ptr().add(body));
            self.body.set_len(body + emitted.words as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
        Ok(())
    }
    pub(super) fn custom_segmented_reduce(
        &mut self,
        reduction: &CustomSegmentedReduction,
        kernel: &ReductionKernel,
        pass: EntryPass,
        single_group: bool,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) -> Result<(), &'static str> {
        let r = &reduction.values;
        let summary = Summary::Counted(reduction.head_word);
        let words = reduction.summary_words();
        if pass == EntryPass::ScanAdd {
            return self.custom_prefix_add(r, kernel, false, summary, active);
        }
        let (own, identity, input) = self.custom_input(
            r,
            kernel,
            pass == EntryPass::ScanTail,
            local,
            shared,
            pointer,
            active,
            summary,
        )?;
        let remaining = self.group_remaining(local, count);
        self.custom_tree(r, kernel, local, shared, pointer, remaining, &own, false, summary)?;
        let (binding, word, mode) = match pass {
            EntryPass::Main => (self.program.buffers.len(), 0, SummaryRootMode::Publish),
            EntryPass::ScanTail => (self.program.buffers.len(), 0, SummaryRootMode::PublishReset),
            _ => (reduction.count.binding, reduction.count.word, SummaryRootMode::EmptyCountReset),
        };
        let group = self.custom_summary_root(binding, word, mode, local, shared, pointer,
            active, count, &identity);
        if pass == EntryPass::Main {
            return Ok(());
        }
        self.custom_tree(r, kernel, local, shared, pointer, remaining, &own, true, summary)?;
        if pass != EntryPass::ScanTail {
            return self.custom_segmented_finish(
                reduction, kernel, active, count, group, single_group, &own, &identity, &input,
            );
        }
        let result = self.read_summary_words(&own);
        let write = self.id();
        let end = self.id();
        self.open_active(active, write, end);
        let target = self.summary_addresses(words, r.fields.target, self.index);
        self.write_summary_words(&target, &result);
        self.merge_active(end, true);
        Ok(())
    }

    fn custom_segmented_finish(
        &mut self,
        reduction: &CustomSegmentedReduction,
        kernel: &ReductionKernel,
        active: u32,
        length: u32,
        group: u32,
        single_group: bool,
        own: &[u32],
        identity: &[u32],
        item: &[u32],
    ) -> Result<(), &'static str> {
        let r = &reduction.values;
        let summary = Summary::Counted(reduction.head_word);
        let words = own.len();
        let bool_type = self.types[&Scalar::Bool];
        let step = self.constant(Scalar::U64, 4);
        let summary_descriptor = if single_group { 0 } else { self.buffers[self.program.buffers.len()] };
        let parent = self.page_input(Scalar::U32,
            summary_descriptor, group, Some(0), 0);
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
        let source = self.page_input(Scalar::U32, self.buffers[r.fields.source],
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
        let target = self.page_input(Scalar::U32, self.buffers[r.fields.target],
            index, (r.fields.target_word != 0).then_some(r.fields.target_word), 0);
        (target, fields)
    }

    pub(super) fn custom_scan_add(
        &mut self,
        scan: &CustomScan,
        kernel: &ReductionKernel,
        root: bool,
        active: u32,
    ) -> Result<(), &'static str> {
        let summary = scan.head_word.map(Summary::Segmented).unwrap_or(Summary::Plain);
        self.custom_prefix_add(&scan.values, kernel, root, summary, active)
    }

    fn custom_prefix_add(
        &mut self,
        r: &CustomReduction,
        kernel: &ReductionKernel,
        root: bool,
        summary: Summary,
        active: u32,
    ) -> Result<(), &'static str> {
        let value_words = r.ty.size.div_ceil(4) as usize;
        let merged_summary = if root { Summary::Plain } else { summary };
        let words = merged_summary.words(value_words);
        let width = self.constant(Scalar::U32, 64);
        let step = if words > 1 { self.constant(Scalar::U64, 4) } else { 0 };
        let (head_descriptor, head_word) = if root && summary.head().is_some() {
            (self.buffers[self.program.buffers.len()], self.constant(Scalar::U32, value_words as u32 * 4))
        } else {
            (0, 0)
        };
        let (target, fields) = if root {
            self.custom_result_target(r, self.index)
        } else {
            let target = self.page_input(Scalar::U32, self.buffers[r.fields.target],
                self.index, Some(0), 0);
            (target, Vec::new())
        };
        let mut workspace = vec![target.zero; 2 * words + if root { 2 * fields.len() } else { words }];
        let request = CustomPrefix {
            target, prefix_descriptor: self.buffers[self.scan_input_binding(&r.fields)],
            head_descriptor, head_word, width, bool_type: self.types[&Scalar::Bool], active, step,
            packed: u32::from(!root), words: words as u32, field_count: fields.len() as u32,
        };
        let body = self.body.len();
        self.body.reserve(375 + 30 * words + 60 * fields.len());
        let opened = unsafe {
            let opened = open_custom_prefix(&request, fields.as_ptr(),
                workspace.as_mut_ptr(), self.body.as_mut_ptr().add(body));
            self.body.set_len(body + opened.words as usize);
            opened
        };
        self.next = opened.next;
        self.current_label = opened.label;

        let (prefix, rest) = workspace.split_at(words);
        let (target, locations) = rest.split_at(words);
        let values = self.custom_merge(r, kernel, prefix, target, merged_summary)?;
        let body = self.body.len();
        self.body.reserve(8 + 5 * words + 44 * fields.len());
        unsafe {
            let emitted = close_custom_prefix(&request, opened, fields.as_ptr(), locations.as_ptr(),
                values.as_ptr(), self.next, self.body.as_mut_ptr().add(body));
            self.body.set_len(body + emitted.words as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
        Ok(())
    }
}
