//! 64 线程 work-efficient Blelloch 独占扫描，工作组间由设备递归前缀传播。
use super::{reduce::{CollectiveField, CollectiveInput}, *};
#[repr(C)]
struct SharedAddress {
    index_type: u32,
    pointer_type: u32,
    variable: u32,
    node: u32,
    scale: u32,
    count: u32,
    next: u32,
}

#[repr(C)]
struct SharedAddressWords {
    words: u64,
    next: u32,
}

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
struct ScanTreeInput {
    uint_type: u32,
    bool_type: u32,
    lane: u32,
    mask: u32,
    stride: u32,
    first: u32,
    other: u32,
}

#[repr(C)]
struct ScanTreeOpen {
    words: u64,
    run: u32,
    end: u32,
}

#[repr(C)]
struct TreeLayer {
    uint_type: u32,
    bool_type: u32,
    lane: u32,
    stride: u32,
    selector: u32,
    tile_capacity: u32,
    remaining: u32,
    one: u32,
    scope: u32,
    semantics: u32,
    down: u32,
    next: u32,
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
struct ScanPrefix {
    target: PageAddress,
    prefix_descriptor: u32,
    head_descriptor: u32,
    head_word: u32,
    width: u32,
    bool_type: u32,
    one: u32,
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
    #[link_name = "bootstrap.spirv_envelope::scan_group"]
    fn emit_scan_group(input: ScanGroup, levels: *const ScanLevel, components: *const TreeComponent,
        fields: *const CollectiveField, count: u32, workspace: *mut u32, body: *mut u32,
        decorations: *mut u32) -> TreeWords;
    #[link_name = "bootstrap.spirv_envelope::add_scan_prefix"]
    fn add_scan_prefix(input: ScanPrefix, fields: *const PhysicalField, count: u32,
        body: *mut u32, decorations: *mut u32) -> TreeWords;
    #[link_name = "bootstrap.spirv_envelope::shared_addresses"]
    fn emit_shared_addresses(input: SharedAddress, offsets: *mut u32, body: *mut u32) -> SharedAddressWords;
    #[link_name = "bootstrap.spirv_envelope::group_remaining"]
    fn emit_group_remaining(body: *mut u32, uint_type: u32, index: u32, lane: u32, count: u32, first: u32);
    #[link_name = "bootstrap.spirv_envelope::collective_writer"]
    fn emit_collective_writer(body: *mut u32, bool_type: u32, lane: u32, active: u32, count: u32, zero: u32, first: u32);
    #[link_name = "bootstrap.spirv_envelope::open_scan_tree"]
    fn emit_open_scan_tree(input: ScanTreeInput, body: *mut u32) -> ScanTreeOpen;
    #[link_name = "bootstrap.spirv_envelope::tree_layer"]
    fn emit_tree_layer(input: TreeLayer, components: *const TreeComponent, count: u32,
        body: *mut u32, decorations: *mut u32) -> TreeWords;
}

#[derive(Clone, Copy)]
pub(super) struct SharedTree {
    pub shared: u32,
    pub pointer: u32,
    pub base_word: u32,
    pub scalar: Scalar,
    pub operator: CollectiveOperator,
}
struct TreeParts {
    ids: [u32; 2],
    count: usize,
}

impl AsRef<[u32]> for TreeParts {
    fn as_ref(&self) -> &[u32] {
        &self.ids[..self.count]
    }
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
            self.program.scan_source_buffer()
        } else {
            fields.source
        }
    }

    fn shared_addresses(&mut self, shared: u32, pointer: u32, node: u32, stride: u32, offsets: &mut [u32]) {
        let scale = if stride == 1 { 0 } else { self.constant(Scalar::U32, stride) };
        for offset in offsets.iter_mut() {
            if *offset != 0 {
                *offset = self.constant(Scalar::U32, *offset);
            }
        }
        let input = SharedAddress {
            index_type: self.types[&Scalar::U32],
            pointer_type: pointer,
            variable: shared,
            node,
            scale,
            count: offsets.len() as u32,
            next: self.next,
        };
        let start = self.body.len();
        self.body.reserve(5 + 10 * offsets.len());
        self.next = unsafe {
            let result = emit_shared_addresses(input, offsets.as_mut_ptr(), self.body.as_mut_ptr().add(start));
            self.body.set_len(start + result.words as usize);
            result.next
        };
    }

    pub(super) fn shared_planes(&mut self, shared: u32, pointer: u32, node: u32, count: usize) -> Vec<u32> {
        let mut offsets = (0..count as u32).map(|word| word * 64).collect::<Vec<_>>();
        self.shared_addresses(shared, pointer, node, 1, &mut offsets);
        offsets
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

    pub(super) fn collective_writer(&mut self, lane: u32, active: u32, count: u32) -> (u32, u32) {
        let zero = self.constant(Scalar::U32, 0);
        let at = self.body.len();
        let first = self.next;
        self.body.reserve(20);
        unsafe {
            emit_collective_writer(self.body.as_mut_ptr().add(at), self.types[&Scalar::Bool],
                lane, active, count, zero, first);
            self.body.set_len(at + 20);
        }
        self.next += 4;
        (first + 3, first + 1)
    }

    pub(super) fn open_scan_tree(&mut self, lane: u32, width: u32) -> (u32, u32) {
        let mask = self.constant(Scalar::U32, width * 2 - 1);
        let first = self.next;
        self.next += 4;
        let stride = self.constant(Scalar::U32, width);
        let other = self.id();
        let input = ScanTreeInput {
            uint_type: self.types[&Scalar::U32],
            bool_type: self.types[&Scalar::Bool],
            lane, mask, stride, first, other,
        };
        let at = self.body.len();
        self.body.reserve(24);
        let branch = unsafe {
            let branch = emit_open_scan_tree(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + branch.words as usize);
            branch
        };
        self.current_label = branch.run;
        (other, branch.end)
    }

    pub(super) fn close_scan_tree(&mut self, end: u32) {
        self.emit_control(4, |body| unsafe { next_control_block(end, end, 1, body) });
        self.scan_barrier();
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
        let trees: Vec<_> = scan.ty.components.iter().map(|&(scalar, offset)| SharedTree {
            shared, pointer, base_word: (offset / 4) * 64, scalar, operator: scan.operator,
        }).collect();
        let components = self.tree_components(&trees);
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
        let target = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[r.target]),
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

    pub(super) fn scan_levels(&mut self) -> [ScanLevel; 6] {
        [1, 2, 4, 8, 16, 32].map(|width| ScanLevel {
            mask: self.constant(Scalar::U32, width * 2 - 1),
            stride: self.constant(Scalar::U32, width),
        })
    }


    fn tree_parts(&mut self, tree: SharedTree, node: u32) -> TreeParts {
        let count = tree.scalar.size().div_ceil(4) as usize;
        let mut ids = [tree.base_word, tree.base_word + 1];
        self.shared_addresses(tree.shared, tree.pointer, node, count as u32, &mut ids[..count]);
        TreeParts { ids, count }
    }

    pub(super) fn tree_load(&mut self, tree: SharedTree, node: u32) -> u32 {
        let pointers = self.tree_parts(tree, node);
        self.load_raw(tree.scalar, Scalar::U32, pointers.as_ref(), false)
    }

    pub(super) fn tree_store(&mut self, tree: SharedTree, node: u32, value: u32) {
        let pointers = self.tree_parts(tree, node);
        self.store_raw(tree.scalar, Scalar::U32, pointers.as_ref(), value, false);
    }

    pub(super) fn tree_components(&mut self, trees: &[SharedTree]) -> Vec<TreeComponent> {
        trees.iter().map(|tree| {
            let count = tree.scalar.size().div_ceil(4);
            let low = if tree.base_word == 0 { 0 } else { self.constant(Scalar::U32, tree.base_word) };
            let (high, scale, shift) = if count == 2 {
                (self.constant(Scalar::U32, tree.base_word + 1),
                 self.constant(Scalar::U32, 2), self.constant(Scalar::U32, 32))
            } else {
                (0, 0, 0)
            };
            TreeComponent {
                shared: tree.shared,
                pointer_type: tree.pointer,
                low, high, scale, shift, count,
                operation: tree.operator as u32,
                scalar: tree.scalar.into(),
                raw_type: self.raw_type(tree.scalar),
                value_type: self.types[&tree.scalar],
            }
        }).collect()
    }

    pub(super) fn tile_tree_layer(&mut self, components: &[TreeComponent], lane: u32,
        width: u32, padded: u32, down: bool, remaining: u32,
    ) {
        let selector = self.constant(Scalar::U32, width * 2);
        let tile_capacity = self.constant(Scalar::U32, padded);
        let input = TreeLayer {
            uint_type: self.types[&Scalar::U32],
            bool_type: self.types[&Scalar::Bool],
            lane,
            stride: self.constant(Scalar::U32, width),
            selector,
            tile_capacity,
            remaining,
            one: self.constant(Scalar::U32, 1),
            scope: self.constant(Scalar::U32, 2),
            semantics: self.constant(Scalar::U32, 0x108),
            down: u32::from(down),
            next: self.next,
        };
        let body = self.body.len();
        let decorations = self.decorations.len();
        self.body.reserve(42 + 176 * components.len());
        self.decorations.reserve(3 * components.len());
        let result = unsafe {
            let result = emit_tree_layer(input, components.as_ptr(), components.len() as u32,
                self.body.as_mut_ptr().add(body), self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + result.body as usize);
            self.decorations.set_len(decorations + result.decorations as usize);
            result
        };
        self.next = result.next;
        self.current_label = result.label;
    }

    pub(super) fn scan_add(&mut self, scan: &Scan, root: bool) {
        let target_word = if root { scan.values.target_word } else { 0 };
        self.emit_scan_prefix(scan, target_word, None);
    }

    pub(super) fn segmented_root_add(&mut self, scan: &Scan) {
        self.emit_scan_prefix(scan, scan.values.target_word, Some(scan.ty.size.div_ceil(4) * 4));
    }

    pub(super) fn physical_fields(&mut self, ty: &Ty, first: u32) -> Vec<PhysicalField> {
        ty.components.iter().map(|&(scalar, offset)| {
            let unit = Self::physical_unit(scalar);
            let (step, shift) = if scalar.size() == 8 {
                (self.constant(Scalar::U64, 4), self.constant(Scalar::U32, 32))
            } else {
                (0, 0)
            };
            let offset = first + offset;
            PhysicalField {
                scalar: scalar.into(),
                raw_type: self.raw_type(scalar),
                value_type: self.types[&scalar],
                unit_type: self.types[&unit],
                unit_bytes: unit.size(),
                pointer_type: self.physical_pointers[&unit],
                offset: if offset == 0 { 0 } else { self.constant(Scalar::U64, offset) },
                step,
                shift,
            }
        }).collect()
    }

    fn emit_scan_prefix(&mut self, scan: &Scan, target_word: u32, head: Option<u32>) {
        let r = &scan.values;
        let width = self.constant(Scalar::U32, 64);
        let fields = self.physical_fields(&scan.ty, 0);
        let (head_descriptor, head_word, one) = match head {
            Some(offset) => (
                self.buffers[self.program.buffers.len()],
                self.constant(Scalar::U32, offset),
                self.constant(Scalar::U32, 1),
            ),
            None => (0, 0, 0),
        };
        let target = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[r.target]),
            self.index, (target_word != 0).then_some(target_word), 0);
        let input = ScanPrefix {
            target,
            prefix_descriptor: self.buffers[self.scan_input_binding(r)],
            head_descriptor,
            head_word,
            width,
            bool_type: self.types[&Scalar::Bool],
            one,
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
