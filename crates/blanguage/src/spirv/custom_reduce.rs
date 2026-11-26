//! 已检查的用户值函数由原 IR 发射器借给 B 集合阶段。
use super::{reduce::{CollectiveLayout, NumericDispatch, WordScratch}, scan::CollectiveBindings, *};
use std::ffi::c_void;

#[repr(C)]
struct CustomCollective {
    source_layout: CollectiveLayout,
    value_layout: CollectiveLayout,
    value_bytes: u32,
    pass: EntryPass,
    step: BodyStep,
    tags: u32,
    single_group: u32,
    source_word: u32,
    target_word: u32,
    head_word: u32,
    counter_word: u32,
    counter_binding: usize,
    push_pointer: u32,
    push: u32,
}

#[repr(C)]
struct CustomCollectiveHost {
    context: *mut c_void,
    identity: unsafe extern "C" fn(*mut c_void, *mut u32, u32) -> u32,
    project: unsafe extern "C" fn(*mut c_void, *mut u32, u32, u32, u32) -> u32,
    combine: unsafe extern "C" fn(*mut c_void, *const u32, *const u32, *mut u32, u32, u32) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_custom_collective"]
    fn emit_custom_collective(input: CustomCollective, bindings: CollectiveBindings, dispatch: NumericDispatch,
        page: PageInputs, temporary: WordScratch, memory: MemoryHost, host: CustomCollectiveHost) -> u32;
}

struct CheckedCollective<'a, 'stage> {
    emitter: *mut Emitter<'stage>,
    reduction: &'a CustomReduction,
    kernel: &'a PreparedReductionKernel,
    definitions: Vec<u8>,
    error: Option<&'static str>,
}

unsafe extern "C" fn emit_checked_identity(
    context: *mut std::ffi::c_void, output: *mut u32, count: u32,
) -> u32 {
    let context = unsafe { &mut *context.cast::<CheckedCollective<'_, '_>>() };
    let emitter = unsafe { &mut *context.emitter };
    let kernel = context.kernel;
    if let Err(error) = emitter.block(&kernel.stage.code[kernel.invariant_end..kernel.initialize_end]) {
        context.error = Some(error);
        return 0;
    }
    let output = unsafe { std::slice::from_raw_parts_mut(output, count as usize) };
    emitter.pack_custom_values(&context.reduction.ty, &kernel.initial, output);
    emitter.current_label
}

unsafe extern "C" fn emit_checked_projection(
    context: *mut std::ffi::c_void, values: *mut u32, fields: u32, count: u32, label: u32,
) -> u32 {
    let context = unsafe { &mut *context.cast::<CheckedCollective<'_, '_>>() };
    let emitter = unsafe { &mut *context.emitter };
    let kernel = context.kernel;
    let (fields, count) = (fields as usize, count as usize);
    let values = unsafe { std::slice::from_raw_parts_mut(values, fields.max(count)) };
    emitter.current_label = label;
    for (reg, raw) in kernel.source.iter().zip(&values[..fields]) {
        let scalar = kernel.stage.registers[*reg];
        emitter.from_words(*reg, &[*raw], Emitter::raw_scalar(scalar));
    }
    if let Err(error) = emitter.block(&kernel.stage.code[kernel.initialize_end..kernel.project_end]) {
        context.error = Some(error);
        return 0;
    }
    for (dst, src) in kernel.right.iter().zip(&kernel.projected) {
        emitter.regs[*dst] = emitter.value(*src);
    }
    emitter.pack_custom_values(&context.reduction.ty, &kernel.right, &mut values[..count]);
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
    match emitter.custom_pair(context.reduction, context.kernel, &context.definitions, left, right, output) {
        Ok(()) => emitter.current_label,
        Err(error) => {
            context.error = Some(error);
            0
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
        kernel: &PreparedReductionKernel,
        definitions: &[u8],
        left: &[u32],
        right: &[u32],
        output: &mut [u32],
    ) -> Result<(), &'static str> {
        for reg in
            kernel.stage.registers.len() - r.combine.registers.len()..kernel.stage.registers.len()
        {
            if definitions[reg] == 0 {
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
    pub(super) fn custom_collective(
        &mut self, step: BodyStep, operation: *const c_void, kernel: &PreparedReductionKernel,
        pass: EntryPass, single_group: bool, dispatch: NumericDispatch,
    ) -> Result<(), &'static str> {
        let (reduction, head, counter) = match step {
            BodyStep::CustomReduce => (unsafe { &*operation.cast::<CustomReduction>() }, None, None),
            BodyStep::CustomSegmented => {
                let reduction = unsafe { &*operation.cast::<CustomSegmentedReduction>() };
                (&reduction.values, Some(reduction.head_word), Some(&reduction.count))
            }
            _ => {
                let scan = unsafe { &*operation.cast::<CustomScan>() };
                (&scan.values, scan.head_word, None)
            }
        };
        let fields = &reduction.fields;
        let input = CustomCollective {
            source_layout: CollectiveLayout::borrow(&reduction.source_ty),
            value_layout: CollectiveLayout::borrow(&reduction.ty),
            value_bytes: reduction.ty.size,
            pass, step, tags: u32::from(head.is_some()) + u32::from(counter.is_some()),
            single_group: u32::from(single_group),
            source_word: fields.source_word, target_word: fields.target_word, head_word: head.unwrap_or(0),
            counter_word: counter.map_or(0, |field| field.word),
            counter_binding: counter.map_or(0, |field| field.binding),
            push_pointer: self.ptr_push_u32, push: self.push,
        };
        let bindings = self.collective_bindings(fields);
        let page = self.page_inputs(0, self.index, None, 0);
        let memory = self.memory_host();
        let mut context = CheckedCollective {
            emitter: memory.scalars.context.cast(), reduction, kernel, definitions: kernel.prefix_definitions(), error: None,
        };
        let host = CustomCollectiveHost {
            context: std::ptr::from_mut(&mut context).cast(),
            identity: emit_checked_identity,
            project: emit_checked_projection,
            combine: emit_checked_combine,
        };
        let mut words = Vec::new();
        let label = unsafe {
            emit_custom_collective(input, bindings, dispatch, page, WordScratch::borrow(&mut words), memory, host)
        };
        if let Some(error) = context.error {
            return Err(error);
        }
        self.current_label = label;
        Ok(())
    }
}
