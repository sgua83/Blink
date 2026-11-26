//! 64 线程 work-efficient Blelloch 独占扫描，工作组间由设备递归前缀传播。
use super::{reduce::{CollectiveField, CollectiveInput, CollectiveLayout}, *};
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
pub(super) struct ScanLevel {
    mask: u32,
    stride: u32,
}

#[repr(C)]
struct ScanGroup {
    target: PageAddress,
    lanes: CollectiveInput,
    summary: u32,
    width: u32,
    last: u32,
    bool_type: u32,
    normalize: u32,
}

#[repr(C)]
struct CollectiveTree {
    shared: u32,
    pointer: u32,
    operation: CollectiveOperator,
}

#[repr(C)]
struct ScanPrefix {
    target: PageAddress,
    prefix_descriptor: u32,
    head_descriptor: u32,
    head_word: u32,
    width: u32,
    bool_type: u32,
    operation: u32,
    label: u32,
}

#[repr(C)]
pub(super) struct PhysicalField {
    scalar: ScalarCode,
    raw_type: u32,
    value_type: u32,
    unit_type: u32,
    unit_bytes: u32,
    pointer_type: u32,
    offset: u32,
    step: u32,
    shift: u32,
}


unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::collective_tree"]
    fn emit_collective_tree(layout: CollectiveLayout, tree: CollectiveTree, host: ScalarHost,
        components: *mut TreeComponent);
    #[link_name = "bootstrap.spirv_envelope::physical_fields"]
    fn emit_physical_fields(layout: CollectiveLayout, first: u32, host: MemoryHost, output: *mut PhysicalField);
    #[link_name = "bootstrap.spirv_envelope::scan_levels"]
    fn emit_scan_levels(host: ScalarHost, output: *mut ScanLevel);
    #[link_name = "bootstrap.spirv_envelope::scan_group"]
    fn emit_scan_group(input: ScanGroup, levels: *const ScanLevel, components: *const TreeComponent,
        fields: *const CollectiveField, count: u32, workspace: *mut u32, body: *mut u32,
        decorations: *mut u32) -> TreeWords;
    #[link_name = "bootstrap.spirv_envelope::add_scan_prefix"]
    fn add_scan_prefix(input: ScanPrefix, fields: *const PhysicalField, count: u32,
        body: *mut u32, decorations: *mut u32) -> TreeWords;
    #[link_name = "bootstrap.spirv_envelope::group_remaining"]
    fn emit_group_remaining(body: *mut u32, uint_type: u32, index: u32, lane: u32, count: u32, first: u32);
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

    pub(super) fn group_remaining(&mut self, lane: u32, count: u32) -> u32 {
        let at = self.body.len();
        let first = self.next;
        self.body.reserve(10);
        unsafe {
            emit_group_remaining(self.body.as_mut_ptr().add(at), self.types[&Scalar::U32],
                self.index, lane, count, first);
            self.body.set_len(at + 10);
        }
        self.next += 2;
        first + 1
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
        let identity = self.collective_identity(scan.ty.components[0].0, scan.operator);
        let components = self.tree_components(&scan.ty, scan.operator, shared, pointer);
        let fields = self.collective_fields(&scan.ty, Some(if tail { 0 } else { r.source_word }));
        let mut workspace = vec![0; 4 * fields.len()];
        let levels = self.scan_levels();
        let last = self.constant(Scalar::U32, 63);
        let width = self.constant(Scalar::U32, 64);
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x108);
        let source = if tail { self.scan_input_binding(r) } else { r.source };
        let root = !tail && matches!(self.stage.operation, Operation::ScanExclusive(_));
        let bias = (root && r.target_word != 0).then_some(r.target_word);
        let target = self.page_input(Scalar::U32, self.buffers[r.target],
            self.index, bias, 0);
        let normalize = !tail && (matches!(self.stage.operation, Operation::Compact(_))
            || self.stage.selection.is_some());
        let input = ScanGroup {
            target,
            lanes: CollectiveInput {
                identity, source: self.buffers[source], lane, active, count,
                before: self.current_label, scope, semantics,
            },
            summary: self.buffers[self.program.buffers.len()],
            width, last, bool_type: self.types[&Scalar::Bool], normalize: u32::from(normalize),
        };

        let count = fields.len();
        let body = self.body.len();
        let decorations = self.decorations.len();
        self.body.reserve(12 * (42 + 176 * count) + 430 + 128 * count);
        self.decorations.reserve(36 * count);
        unsafe {
            let emitted = emit_scan_group(input, levels.as_ptr(), components.as_ptr(), fields.as_ptr(),
                count as u32, workspace.as_mut_ptr(), self.body.as_mut_ptr().add(body),
                self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + emitted.body as usize);
            self.decorations.set_len(decorations + emitted.decorations as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
    }

    fn scan_levels(&mut self) -> [ScanLevel; 6] {
        let mut levels = std::mem::MaybeUninit::<[ScanLevel; 6]>::uninit();
        unsafe {
            emit_scan_levels(self.scalar_host(), levels.as_mut_ptr().cast());
            levels.assume_init()
        }
    }


    pub(super) fn tree_components(&mut self, ty: &Ty, operation: CollectiveOperator,
        shared: u32, pointer: u32) -> Vec<TreeComponent> {
        let tree = CollectiveTree { shared, pointer, operation };
        let count = ty.components.len();
        let mut components = Vec::with_capacity(count);
        unsafe {
            emit_collective_tree(CollectiveLayout::borrow(ty), tree, self.scalar_host(), components.as_mut_ptr());
            components.set_len(count);
        }
        components
    }

    pub(super) fn scan_add(&mut self, scan: &Scan, root: bool) {
        let target_word = if root { scan.values.target_word } else { 0 };
        self.emit_scan_prefix(scan, target_word, None);
    }

    pub(super) fn segmented_root_add(&mut self, scan: &Scan) {
        self.emit_scan_prefix(scan, scan.values.target_word, Some(scan.ty.size.div_ceil(4) * 4));
    }

    pub(super) fn physical_fields(&mut self, ty: &Ty, first: u32) -> Vec<PhysicalField> {
        let count = ty.components.len();
        let mut fields = Vec::with_capacity(count);
        unsafe {
            emit_physical_fields(CollectiveLayout::borrow(ty), first, self.memory_host(), fields.as_mut_ptr());
            fields.set_len(count);
        }
        fields
    }

    fn emit_scan_prefix(&mut self, scan: &Scan, target_word: u32, head: Option<u32>) {
        let r = &scan.values;
        let width = self.constant(Scalar::U32, 64);
        let fields = self.physical_fields(&scan.ty, 0);
        let (head_descriptor, head_word) = match head {
            Some(offset) => (self.buffers[self.program.buffers.len()], self.constant(Scalar::U32, offset)),
            None => (0, 0),
        };
        let target = self.page_input(Scalar::U32, self.buffers[r.target],
            self.index, (target_word != 0).then_some(target_word), 0);
        let input = ScanPrefix {
            target,
            prefix_descriptor: self.buffers[self.scan_input_binding(r)],
            head_descriptor,
            head_word,
            width,
            bool_type: self.types[&Scalar::Bool],
            operation: scan.operator as u32,
            label: self.current_label,
        };
        let body = self.body.len();
        let decorations = self.decorations.len();
        self.body.reserve(233 + 148 * usize::from(head.is_some()) + 144 * fields.len());
        self.decorations.reserve(3 * fields.len());
        let result = unsafe {
            let result = add_scan_prefix(input, fields.as_ptr(), fields.len() as u32,
                self.body.as_mut_ptr().add(body), self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + result.body as usize);
            self.decorations.set_len(decorations + result.decorations as usize);
            result
        };
        self.next = result.next;
        self.current_label = result.label;
    }
}
