//! 原内置分段着色器从 Rust 借用布局、绑定和单位元。
use super::{reduce::{CollectiveLayout, WordScratch}, scan::TreeWords, *};

#[repr(C)]
struct SegmentLayout {
    fields: CollectiveLayout,
    operation: CollectiveOperator,
    value_bytes: u32,
    counted: u32,
    source_byte: u32,
    target_byte: u32,
    head_byte: u32,
    count_byte: u32,
}

#[repr(C)]
struct SegmentBindings {
    descriptors: *const u32,
    source: usize,
    prefix: usize,
    target: usize,
    summary: usize,
    counter: usize,
}

#[repr(C)]
pub(super) struct SegmentDispatch {
    pub(super) lane: u32,
    pub(super) shared: u32,
    pub(super) pointer: u32,
    pub(super) active: u32,
    pub(super) count: u32,
    pub(super) run: u32,
    pub(super) end: u32,
    pub(super) before: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_segmented"]
    fn emit_segmented(pass: EntryPass, single_group: u32, layout: SegmentLayout, bindings: SegmentBindings,
        dispatch: SegmentDispatch, page: PageInputs, scratch: WordScratch, host: MemoryHost) -> TreeWords;
}

impl Emitter<'_> {
    pub(super) fn segmented_scan(&mut self, scan: &Scan, head: u32, pass: EntryPass, dispatch: SegmentDispatch) {
        self.segmented(&scan.ty, scan.operator, &scan.values, head, None, pass, false, dispatch);
    }

    pub(super) fn segmented_reduce(&mut self, reduction: &SegmentedReduction, pass: EntryPass,
        single_group: bool, dispatch: SegmentDispatch) {
        let values = &reduction.values;
        self.segmented(&values.ty, values.operator, &values.fields, reduction.head_word, Some(&reduction.count),
            pass, single_group, dispatch);
    }

    fn segmented(&mut self, ty: &Ty, operator: CollectiveOperator, values: &Reduction, head: u32,
        count: Option<&PrefixCount>, pass: EntryPass, single_group: bool, dispatch: SegmentDispatch) {
        let layout = SegmentLayout {
            fields: CollectiveLayout::borrow(ty), operation: operator,
            value_bytes: ty.size, counted: u32::from(count.is_some()),
            source_byte: values.source_word, target_byte: values.target_word,
            head_byte: head, count_byte: count.map_or(0, |field| field.word),
        };
        let bindings = SegmentBindings {
            descriptors: self.buffers.as_ptr(), source: values.source,
            prefix: self.scan_input_binding(values.source, values.target), target: values.target,
            summary: self.program.buffers.len(), counter: count.map_or(0, |field| field.binding),
        };
        let page = self.page_inputs(0, self.index, None, 0);
        let mut words = Vec::new();
        let scratch = WordScratch::borrow(&mut words);
        let result = unsafe {
            emit_segmented(pass, u32::from(single_group), layout, bindings, dispatch, page, scratch, self.memory_host())
        };
        self.finish_tree(result);
    }
}
