//! 64 线程 work-efficient Blelloch 独占扫描，工作组间由设备递归前缀传播。
use super::*;
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
    index: u32,
    remaining: u32,
    one: u32,
    scope: u32,
    semantics: u32,
    down: u32,
    next: u32,
}

#[repr(C)]
struct TreeWords {
    body: u64,
    decorations: u64,
    next: u32,
    label: u32,
}

unsafe extern "C" {
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
        own: *const TreeParts, body: *mut u32, decorations: *mut u32) -> TreeWords;
}

#[derive(Clone, Copy)]
pub(super) struct SharedTree {
    pub shared: u32,
    pub pointer: u32,
    pub base_word: u32,
    pub scalar: Scalar,
    pub operator: CollectiveOperator,
}
pub(super) enum TreeDomain<'a> {
    Scan,
    Tile(u32),
    Reduction { index: u32, own: &'a [TreeParts] },
}

#[repr(C)]
pub(super) struct TreeParts {
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
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) {
        let r = &scan.values;
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        // 全域扫描的尾组与 tile 共用树；有效长度属于当前实际工作域。
        let remaining =
            if scan.ty.components[0].0.is_float() && scan.operator != CollectiveOperator::Sum {
                self.group_remaining(local, count)
            } else {
                0
            };
        let trees: Vec<_> = scan
            .ty
            .components
            .iter()
            .map(|(scalar, offset)| SharedTree {
                shared,
                pointer,
                base_word: (offset/4) * 64,
                scalar: *scalar,
                operator: scan.operator,
            })
            .collect();
        let identity = self.raw_constant(scan.ty.components[0].0, scan.operator.identity(scan.ty.components[0].0));
        let before = self.current_label;
        let read = self.id();
        let read_end = self.id();
        self.emit(247, &[read_end, 0]);
        self.emit(250, &[active, read, read_end]);
        self.emit(248, &[read]);
        let mut values = Vec::new();
        for (scalar, offset) in &scan.ty.components {
            let value = self.load_raw_buffer(*scalar,
                if tail {
                    self.scan_input_binding(r)
                } else {
                    r.source
                },
                self.index,
                if tail {
                    *offset
                } else {
                    r.source_word + offset
                });
            let value = if (matches!(self.stage.operation, Operation::Compact(_))
                || self.stage.selection.is_some())
                && !tail
            {
                let keep = self.id();
                self.emit(171, &[b, keep, value, zero]);
                let one = self.constant(Scalar::U32, 1);
                let selected = self.id();
                self.select(u, selected, keep, one, zero);
                selected
            } else {
                value
            };
            values.push(value);
        }
        self.emit(249, &[read_end]);
        self.emit(248, &[read_end]);
        let inputs: Vec<_> = values
            .into_iter()
            .zip(&scan.ty.components)
            .map(|(value,(scalar,_))| {
                let input = self.id();
                self.emit(245, &[self.raw_type(*scalar), input, value, read, identity, before]);
                input
            })
            .collect();
        for (tree,input) in trees.iter().zip(inputs) {self.tree_store(*tree,local,input);}
        self.scan_barrier();
        let components = self.tree_components(&trees);
        for width in [1, 2, 4, 8, 16, 32] {
            self.tree_layer(&components, local, width, TreeDomain::Scan, false, remaining);
        }
        let last = self.constant(Scalar::U32, 63);
        let is_last = self.id();
        self.emit(170, &[b, is_last, local, last]);
        let save = self.id();
        let saved = self.id();
        self.emit(247, &[saved, 0]);
        self.emit(250, &[is_last, save, saved]);
        self.emit(248, &[save]);
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[u, group, self.index, width]);
        for (tree, (_,offset)) in trees.iter().zip(&scan.ty.components) {
            let total = self.tree_load(*tree,local);
            self.store_raw_buffer(tree.scalar,
                self.program.buffers.len(),
                group,
                *offset,
                total);
            self.tree_store(*tree,local,identity);
        }
        self.emit(249, &[saved]);
        self.emit(248, &[saved]);
        self.scan_barrier();
        for width in [32, 16, 8, 4, 2, 1] {
            self.tree_layer(&components, local, width, TreeDomain::Scan, true, remaining);
        }
        let write = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[active, write, end]);
        self.emit(248, &[write]);
        let root = !tail && matches!(self.stage.operation, Operation::ScanExclusive(_));
        for (tree, (_,offset)) in trees.iter().zip(&scan.ty.components) {
            let result = self.tree_load(*tree,local);
            self.store_raw_buffer(tree.scalar,
                r.target,
                self.index,
                if root {
                    r.target_word + offset
                } else {
                    *offset
                },
                result);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
    pub(super) fn tree_parts(&mut self, tree: SharedTree, node: u32) -> TreeParts {
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

    pub(super) fn tree_layer(&mut self, components: &[TreeComponent], lane: u32,
        width: u32, domain: TreeDomain<'_>, down: bool, remaining: u32,
    ) {
        let (selector, tile_capacity, index, own) = match domain {
            TreeDomain::Scan => (self.constant(Scalar::U32, width * 2 - 1), 0, 0, std::ptr::null()),
            TreeDomain::Tile(capacity) => (self.constant(Scalar::U32, width * 2),
                self.constant(Scalar::U32, capacity), 0, std::ptr::null()),
            TreeDomain::Reduction { index, own } => (0, 0, index, own.as_ptr()),
        };
        let input = TreeLayer {
            uint_type: self.types[&Scalar::U32],
            bool_type: self.types[&Scalar::Bool],
            lane,
            stride: self.constant(Scalar::U32, width),
            selector,
            tile_capacity,
            index,
            remaining,
            one: if own.is_null() && (tile_capacity != 0 || (!down && remaining != 0)) {
                self.constant(Scalar::U32, 1)
            } else { 0 },
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
                own, self.body.as_mut_ptr().add(body), self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + result.body as usize);
            self.decorations.set_len(decorations + result.decorations as usize);
            result
        };
        self.next = result.next;
        self.current_label = result.label;
    }

    pub(super) fn scan_add(&mut self, scan: &Scan, root: bool) {
        let r = &scan.values;
        let u = self.types[&Scalar::U32];
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[u, group, self.index, width]);
        let first =
            if scan.ty.components[0].0.is_float() && scan.operator != CollectiveOperator::Sum {
                let local = self.id();
                self.emit(137, &[u, local, self.index, width]);
                let zero = self.constant(Scalar::U32, 0);
                let first = self.id();
                self.emit(170, &[self.types[&Scalar::Bool], first, local, zero]);
                Some(first)
            } else {
                None
            };
        for (scalar, offset) in &scan.ty.components {
            let prefix = self.load_raw_buffer(*scalar,
                self.scan_input_binding(r),
                group,
                *offset);
            let word = if root {r.target_word + offset} else {*offset};
            let value = self.load_raw_buffer(*scalar,
                r.target,
                self.index,word);
            let combined = self.collective_pair(*scalar, scan.operator, prefix, value);
            let result = if let Some(first) = first {
                let result = self.id();
                self.select(self.raw_type(*scalar), result, first, prefix, combined);
                result
            } else {
                combined
            };
            self.store_raw_buffer(*scalar,r.target,self.index,word,result);
        }
    }
}
