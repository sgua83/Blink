//! 64 线程 work-efficient Blelloch 独占扫描，工作组间由设备递归前缀传播。
use super::{reduce::{CollectiveLayout, NumericDispatch, NumericGroupInput, WordScratch}, *};
#[repr(C)]
pub(super) struct TreeComponent {
    shared: u32,
    pointer_type: u32,
    low: u32,
    high: u32,
    scale: u32,
    shift: u32,
    count: u32,
    operation: u32,
    scalar: ScalarCode,
    raw_type: u32,
    value_type: u32,
}

#[repr(C)]
pub(super) struct TreeWords {
    pub(super) body: u64,
    pub(super) decorations: u64,
    pub(super) next: u32,
    pub(super) label: u32,
}

#[repr(C)]
pub(super) struct CollectiveBindings {
    descriptors: *const u32,
    source: usize,
    prefix: usize,
    target: usize,
    summary: usize,
}

#[repr(C)]
struct NumericScanOptions {
    target_word: u32,
    root_operation: u32,
    normalized_input: u32,
}

#[repr(C)]
struct NumericPrefix {
    fields: CollectiveLayout,
    operation: u32,
    target_word: u32,
    value_bytes: u32,
    root: u32,
    segmented: u32,
    label: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_numeric_scan"]
    fn emit_numeric_scan(input: NumericGroupInput, bindings: CollectiveBindings, options: NumericScanOptions,
        page: PageInputs, temporary: WordScratch, host: MemoryHost) -> TreeWords;
    #[link_name = "bootstrap.spirv_envelope::emit_numeric_prefix"]
    fn emit_numeric_prefix(input: NumericPrefix, bindings: CollectiveBindings, page: PageInputs,
        temporary: WordScratch, host: MemoryHost) -> TreeWords;
}

impl Emitter<'_> {
    pub(super) fn scan_input_binding(&self, fields: &Reduction) -> usize {
        if fields.source == fields.target
            && matches!(
                self.stage.operation,
                Operation::ScanExclusive(_)
                    | Operation::CustomScan(_)
                    | Operation::SegmentedReduce(_)
                    | Operation::CustomSegmentedReduce(_)
            )
        {
            self.execution_buffers.scan_source()
        } else {
            fields.source
        }
    }

    pub(super) fn scan_barrier(&mut self) {
        self.workgroup_barrier(0x108);
    }
    pub(super) fn scan(
        &mut self,
        scan: &Scan,
        tail: bool,
        lane: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) {
        let r = &scan.values;
        let dispatch = NumericDispatch { lane, shared, pointer, active, count, before: self.current_label };
        let input = NumericGroupInput::borrow(&scan.ty, scan.operator, r.source_word, tail, dispatch);
        let options = NumericScanOptions {
            target_word: r.target_word,
            root_operation: u32::from(matches!(self.stage.operation, Operation::ScanExclusive(_))),
            normalized_input: u32::from(matches!(self.stage.operation, Operation::Compact(_))
                || self.stage.selection.is_some()),
        };
        let page = self.page_inputs(0, self.index, None, 0);
        let mut words = Vec::new();
        let result = unsafe {
            emit_numeric_scan(input, self.collective_bindings(r), options, page,
                WordScratch::borrow(&mut words), self.memory_host())
        };
        self.finish_tree(result);
    }

    pub(super) fn scan_add(&mut self, scan: &Scan, root: bool) {
        self.emit_scan_prefix(scan, root, false);
    }

    pub(super) fn segmented_root_add(&mut self, scan: &Scan) {
        self.emit_scan_prefix(scan, true, true);
    }

    pub(super) fn collective_bindings(&self, r: &Reduction) -> CollectiveBindings {
        CollectiveBindings {
            descriptors: self.buffers.as_ptr(), source: r.source, prefix: self.scan_input_binding(r),
            target: r.target, summary: self.program.buffers.len(),
        }
    }

    fn emit_scan_prefix(&mut self, scan: &Scan, root: bool, segmented: bool) {
        let input = NumericPrefix {
            fields: CollectiveLayout::borrow(&scan.ty),
            operation: scan.operator as u32,
            target_word: scan.values.target_word,
            value_bytes: scan.ty.size,
            root: u32::from(root),
            segmented: u32::from(segmented),
            label: self.current_label,
        };
        let page = self.page_inputs(0, self.index, None, 0);
        let mut words = Vec::new();
        let result = unsafe {
            emit_numeric_prefix(input, self.collective_bindings(&scan.values), page,
                WordScratch::borrow(&mut words), self.memory_host())
        };
        self.finish_tree(result);
    }
}
