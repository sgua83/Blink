//! 已检查的用户值函数由原 IR 发射器借给 B 集合阶段。
use super::{reduce::{CollectiveLayout, NumericDispatch, WordScratch}, scan::CollectiveBindings, *};
use std::ffi::c_void;

#[repr(C)]
struct CustomCollective {
    source_layout: CollectiveLayout,
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
    registers: *mut u32,
    definitions: *const u8,
    combine_first: usize,
    combine_end: usize,
    layout: CollectiveLayout,
    identity_code: Span,
    project_code: Span,
    combine_code: Span,
    initial: Span,
    source: Span,
    projected: Span,
    left: Span,
    right: Span,
    result: Span,
    scalar: unsafe extern "C" fn(*mut c_void, Reg) -> ScalarCode,
    emit: unsafe extern "C" fn(*mut c_void, Span, u32) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_custom_collective"]
    fn emit_custom_collective(input: CustomCollective, bindings: CollectiveBindings, dispatch: NumericDispatch,
        page: PageInputs, temporary: WordScratch, memory: MemoryHost, host: CustomCollectiveHost) -> u32;
}

struct CheckedCollective<'stage> {
    emitter: *mut Emitter<'stage>,
    error: Option<&'static str>,
}

unsafe extern "C" fn emit_checked_block(context: *mut c_void, code: Span, label: u32) -> u32 {
    let context = unsafe { &mut *context.cast::<CheckedCollective<'_>>() };
    let emitter = unsafe { &mut *context.emitter };
    emitter.current_label = label;
    match emitter.block(unsafe { code.as_slice() }) {
        Ok(_) => emitter.current_label,
        Err(error) => {
            context.error = Some(error);
            0
        }
    }
}

impl Emitter<'_> {
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
            value_bytes: reduction.ty.size,
            pass, step, tags: u32::from(head.is_some()) + u32::from(counter.is_some()),
            single_group: u32::from(single_group),
            source_word: fields.source_word, target_word: fields.target_word, head_word: head.unwrap_or(0),
            counter_word: counter.map_or(0, |field| field.word),
            counter_binding: counter.map_or(0, |field| field.binding),
            push_pointer: self.ptr_push_u32, push: self.push,
        };
        let bindings = self.collective_bindings(fields.source, fields.target);
        let page = self.page_inputs(0, self.index, None, 0);
        let registers = self.regs.as_mut_ptr();
        let memory = self.memory_host();
        let definitions = kernel.prefix_definitions();
        let code = &kernel.stage.code;
        let mut context = CheckedCollective { emitter: memory.scalars.context.cast(), error: None };
        let host = CustomCollectiveHost {
            context: std::ptr::from_mut(&mut context).cast(), registers,
            definitions: definitions.as_ptr(),
            combine_first: kernel.stage.registers.len() - reduction.combine.registers.len(),
            combine_end: kernel.stage.registers.len(),
            layout: CollectiveLayout::borrow(&reduction.ty),
            identity_code: Span::slice(&code[kernel.invariant_end..kernel.initialize_end]),
            project_code: Span::slice(&code[kernel.initialize_end..kernel.project_end]),
            combine_code: Span::slice(&code[kernel.project_end..]),
            initial: Span::slice(&kernel.initial), source: Span::slice(&kernel.source),
            projected: Span::slice(&kernel.projected), left: Span::slice(&kernel.left),
            right: Span::slice(&kernel.right), result: Span::slice(&kernel.result),
            scalar: value_scalar, emit: emit_checked_block,
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
