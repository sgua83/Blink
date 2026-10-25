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
            if scan.ty.components[0].0 == Scalar::F32 && scan.operator != CollectiveOperator::Sum {
                let group_begin = self.id();
                self.emit(130, &[u, group_begin, self.index, local]);
                let available = self.id();
                self.emit(130, &[u, available, count, group_begin]);
                available
            } else {
                0
            };
        let components = scan.ty.components.len() as u32;
        let trees: Vec<_> = scan
            .ty
            .components
            .iter()
            .enumerate()
            .map(|(component, (scalar, _))| SharedTree {
                shared,
                pointer,
                base_word: component as u32 * 64,
                scalar: *scalar,
                operator: scan.operator,
            })
            .collect();
        let identity = self.constant(Scalar::U32, scan.operator.identity(scan.ty.components[0].0));
        let before = self.current_label;
        let read = self.id();
        let read_end = self.id();
        self.emit(247, &[read_end, 0]);
        self.emit(250, &[active, read, read_end]);
        self.emit(248, &[read]);
        let mut values = Vec::new();
        for (component, (_, offset)) in scan.ty.components.iter().enumerate() {
            let at = self.address_scalar(
                r.source,
                self.index,
                if tail {
                    components
                } else {
                    self.program.buffers[r.source].stride / 4
                },
                if tail {
                    component as u32
                } else {
                    r.source_word + offset / 4
                },
            );
            let value = self.id();
            self.emit(61, &[u, value, at, 2, 4]);
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
            .map(|value| {
                let input = self.id();
                self.emit(245, &[u, input, value, read, identity, before]);
                input
            })
            .collect();
        let own: Vec<_> = trees
            .iter()
            .zip(inputs)
            .map(|(tree, input)| {
                let at = self.tree_at(*tree, local);
                self.emit(62, &[at, input]);
                at
            })
            .collect();
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
        for (component, at) in own.iter().enumerate() {
            let total = self.id();
            self.emit(61, &[u, total, *at]);
            let target = self.address_scalar(
                self.program.buffers.len(),
                group,
                components,
                component as u32,
            );
            self.emit(62, &[target, total, 2, 4]);
            self.emit(62, &[*at, identity]);
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
        for (component, at) in own.iter().enumerate() {
            let result = self.id();
            self.emit(61, &[u, result, *at]);
            let target = self.address_scalar(
                r.target,
                self.index,
                if root {
                    self.program.buffers[r.target].stride / 4
                } else {
                    components
                },
                if root {
                    r.target_word + scan.ty.components[component].1 / 4
                } else {
                    component as u32
                },
            );
            self.emit(62, &[target, result, 2, 4]);
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
    pub(super) fn tree_at(&mut self, tree: SharedTree, node: u32) -> u32 {
        let at = if tree.base_word == 0 {
            node
        } else {
            let base = self.constant(Scalar::U32, tree.base_word);
            let at = self.id();
            self.emit(128, &[self.types[&Scalar::U32], at, base, node]);
            at
        };
        self.shared_at(tree.pointer, tree.shared, at)
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
            let a_ptr = self.tree_at(tree, other);
            let b_ptr = self.tree_at(tree, node);
            let a = self.id();
            let bv = self.id();
            self.emit(61, &[u, a, a_ptr]);
            self.emit(61, &[u, bv, b_ptr]);
            let (left, right) = if down && tree.operator != CollectiveOperator::Sum {
                (bv, a)
            } else {
                (a, bv)
            };
            let combined = self.collective_pair(tree.scalar, tree.operator, left, right);
            let sum = if tree.scalar == Scalar::F32
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
                self.emit(169, &[u, sum, has_right, combined, left]);
                sum
            } else {
                combined
            };
            if down {
                self.emit(62, &[a_ptr, bv]);
            }
            self.emit(62, &[b_ptr, sum]);
        }
        self.emit(249, &[end]);
        self.emit(248, &[end]);
        self.scan_barrier();
    }
    pub(super) fn scan_add(&mut self, scan: &Scan, root: bool) {
        let r = &scan.values;
        let u = self.types[&Scalar::U32];
        let components = scan.ty.components.len() as u32;
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[u, group, self.index, width]);
        let first =
            if scan.ty.components[0].0 == Scalar::F32 && scan.operator != CollectiveOperator::Sum {
                let local = self.id();
                self.emit(137, &[u, local, self.index, width]);
                let zero = self.constant(Scalar::U32, 0);
                let first = self.id();
                self.emit(170, &[self.types[&Scalar::Bool], first, local, zero]);
                Some(first)
            } else {
                None
            };
        for (component, (scalar, offset)) in scan.ty.components.iter().enumerate() {
            let source = self.address_scalar(r.source, group, components, component as u32);
            let prefix = self.id();
            self.emit(61, &[u, prefix, source, 2, 4]);
            let target = self.address_scalar(
                r.target,
                self.index,
                if root {
                    self.program.buffers[r.target].stride / 4
                } else {
                    components
                },
                if root {
                    r.target_word + offset / 4
                } else {
                    component as u32
                },
            );
            let value = self.id();
            self.emit(61, &[u, value, target, 2, 4]);
            let combined = self.collective_pair(*scalar, scan.operator, prefix, value);
            let result = if let Some(first) = first {
                let result = self.id();
                self.emit(169, &[u, result, first, prefix, combined]);
                result
            } else {
                combined
            };
            self.emit(62, &[target, result, 2, 4]);
        }
    }
}
