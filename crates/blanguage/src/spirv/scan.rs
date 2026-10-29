//! 64 线程 work-efficient Blelloch 独占扫描，工作组间由设备递归前缀传播。
use super::*;
#[derive(Clone, Copy)]
pub(super) struct SharedTree {
    pub shared: u32,
    pub pointer: u32,
    pub base_word: u32,
    pub scalar: Scalar,
    pub operator: CollectiveOperator,
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

    pub(super) fn shared_at(&mut self, pointer: u32, shared: u32, index: u32) -> u32 {
        let at = self.id();
        self.emit(65, &[pointer, at, shared, index]);
        at
    }
    pub(super) fn scan_barrier(&mut self) {
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x108);
        self.emit(224, &[scope, scope, semantics]);
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
                let group_begin = self.id();
                self.emit(130, &[u, group_begin, self.index, local]);
                let available = self.id();
                self.emit(130, &[u, available, count, group_begin]);
                available
            } else {
                0
            };
        let words = scan.ty.size.div_ceil(4);
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
                    words
                } else {
                    self.program.buffers[r.source].stride / 4
                },
                if tail {
                    offset/4
                } else {
                    r.source_word + offset / 4
                },
            );
            let value = if (matches!(self.stage.operation, Operation::Compact(_))
                || self.stage.selection.is_some())
                && !tail
            {
                let keep = self.id();
                self.emit(171, &[b, keep, value, zero]);
                let one = self.constant(Scalar::U32, 1);
                let selected = self.id();
                self.emit(169, &[u, selected, keep, one, zero]);
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
        for width in [1, 2, 4, 8, 16, 32] {
            self.scan_tree(&trees, local, width, false, remaining);
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
                words,
                offset/4,
                total,
            );
            self.tree_store(*tree,local,identity);
        }
        self.emit(249, &[saved]);
        self.emit(248, &[saved]);
        self.scan_barrier();
        for width in [32, 16, 8, 4, 2, 1] {
            self.scan_tree(&trees, local, width, true, remaining);
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
                    self.program.buffers[r.target].stride / 4
                } else {
                    words
                },
                if root {
                    r.target_word + offset / 4
                } else {
                    offset/4
                },
                result,
            );
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
    fn scan_tree(
        &mut self,
        trees: &[SharedTree],
        local: u32,
        width: u32,
        down: bool,
        remaining: u32,
    ) {
        let u = self.types[&Scalar::U32];
        let mask = self.constant(Scalar::U32, width * 2 - 1);
        let bits = self.id();
        self.emit(199, &[u, bits, local, mask]);
        let selected = self.id();
        self.emit(170, &[self.types[&Scalar::Bool], selected, bits, mask]);
        self.scan_tree_step(trees, local, width, selected, down, remaining);
    }
    pub(super) fn tree_parts(&mut self, tree: SharedTree, node: u32) -> Vec<u32> {
        let node = if tree.scalar.size()==4 {node} else {
            let words=self.constant(Scalar::U32,tree.scalar.size()/4);
            let scaled=self.id();self.emit(132,&[self.types[&Scalar::U32],scaled,node,words]);scaled
        };
        (0..tree.scalar.size()/4).map(|part| {
        let at = if tree.base_word + part == 0 {
            node
        } else {
            let base = self.constant(Scalar::U32, tree.base_word + part);
            let at = self.id();
            self.emit(128, &[self.types[&Scalar::U32], at, base, node]);
            at
        };
        self.shared_at(tree.pointer, tree.shared, at)
        }).collect()
    }
    pub(super) fn tree_load(&mut self, tree: SharedTree, node: u32) -> u32 {
        let pointers=self.tree_parts(tree,node);self.load_raw(tree.scalar,&pointers,false)
    }
    pub(super) fn tree_store(&mut self, tree: SharedTree, node: u32, value: u32) {
        let pointers=self.tree_parts(tree,node);self.store_raw(tree.scalar,&pointers,value,false);
    }
    pub(super) fn scan_tree_step(
        &mut self,
        trees: &[SharedTree],
        node: u32,
        width: u32,
        selected: u32,
        down: bool,
        remaining: u32,
    ) {
        let u = self.types[&Scalar::U32];
        let run = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[selected, run, end]);
        self.emit(248, &[run]);
        for tree in trees {
            let tree = *tree;
            let width = self.constant(Scalar::U32, width);
            let other = self.id();
            self.emit(130, &[u, other, node, width]);
            let a = self.tree_load(tree,other);
            let bv = self.tree_load(tree,node);
            let (left, right) = if down && tree.operator != CollectiveOperator::Sum {
                (bv, a)
            } else {
                (a, bv)
            };
            let combined = self.collective_pair(tree.scalar, tree.operator, left, right);
            let sum = if tree.scalar.is_float()
                && tree.operator != CollectiveOperator::Sum
                && !down
            {
                let one = self.constant(Scalar::U32, 1);
                let right_begin = self.id();
                self.emit(128, &[u, right_begin, other, one]);
                let has_right = self.id();
                self.emit(
                    176,
                    &[self.types[&Scalar::Bool], has_right, right_begin, remaining],
                );
                let sum = self.id();
                self.emit(169, &[self.raw_type(tree.scalar), sum, has_right, combined, left]);
                sum
            } else {
                combined
            };
            if down {
                self.tree_store(tree,other,bv);
            }
            self.tree_store(tree,node,sum);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        self.scan_barrier();
    }
    pub(super) fn scan_add(&mut self, scan: &Scan, root: bool) {
        let r = &scan.values;
        let u = self.types[&Scalar::U32];
        let words = scan.ty.size.div_ceil(4);
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
                words,
                offset/4,
            );
            let stride = if root {self.program.buffers[r.target].stride/4} else {words};
            let word = if root {r.target_word + offset/4} else {offset/4};
            let value = self.load_raw_buffer(*scalar,
                r.target,
                self.index,
                stride,word,
            );
            let combined = self.collective_pair(*scalar, scan.operator, prefix, value);
            let result = if let Some(first) = first {
                let result = self.id();
                self.emit(169, &[self.raw_type(*scalar), result, first, prefix, combined]);
                result
            } else {
                combined
            };
            self.store_raw_buffer(*scalar,r.target,self.index,stride,word,result);
        }
    }
}
