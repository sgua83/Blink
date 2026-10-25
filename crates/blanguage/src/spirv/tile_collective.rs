//! 同一向量的极值分量共用树的同步层；求和继续复用单流工作区。
use super::{scan::SharedTree, *};
impl Emitter<'_> {
    pub(super) fn tile_collective(&mut self, collective: &TileCollective) {
        if collective.results.is_empty() {
            self.tile_barrier();
            return;
        }
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let width = self.stage.workgroup_width();
        let padded = (width as u64).next_power_of_two() as u32;
        let batch = if collective.op.operator() == CollectiveOperator::Sum {
            1
        } else {
            collective.results.len()
        };
        for results in collective.results.chunks(batch) {
            let trees: Vec<_> = results
                .iter()
                .enumerate()
                .map(|(component, (_, result))| SharedTree {
                    shared: self.tile_shared,
                    pointer: self.tile_pointer,
                    base_word: (self.stage.tile_data_bytes() / 4) as u32
                        + component as u32 * padded,
                    scalar: self.stage.registers[*result],
                    operator: collective.op.operator(),
                })
                .collect();
            for ((word, _), tree) in results.iter().zip(&trees) {
                let tree = *tree;
                let identity = self.constant(Scalar::U32, tree.operator.identity(tree.scalar));
                let own = self.tree_at(tree, self.tile_lane);
                let active = self.id();
                self.emit(176, &[b, active, self.tile_lane, self.tile_length]);
                let before = self.current_label;
                let read = self.id();
                let read_end = self.id();
                self.emit(247, &[read_end, 0]);
                self.emit(250, &[active, read, read_end]);
                self.emit(248, &[read]);
                let source = self.tile_word_address(
                    self.stage.shared_word(collective.tile, *word),
                    self.tile_lane,
                );
                let value = self.id();
                self.emit(61, &[u, value, source]);
                self.emit(249, &[read_end]);
                self.emit(248, &[read_end]);
                let input = self.id();
                self.emit(245, &[u, input, value, read, identity, before]);
                self.emit(62, &[own, input]);
                if padded > width {
                    let capacity = self.constant(Scalar::U32, padded);
                    let size = self.constant(Scalar::U32, width);
                    let index = self.id();
                    self.emit(128, &[u, index, self.tile_lane, size]);
                    let fill = self.id();
                    self.emit(176, &[b, fill, index, capacity]);
                    let run = self.id();
                    let end = self.id();
                    self.emit(247, &[end, 0]);
                    self.emit(250, &[fill, run, end]);
                    self.emit(248, &[run]);
                    let at = self.tree_at(tree, index);
                    self.emit(62, &[at, identity]);
                    self.emit(249, &[end]);
                    self.emit(248, &[end]);
                }
            }
            self.tile_barrier();
            let mut stride = 1;
            while stride < padded {
                self.tile_tree_step(&trees, stride, padded, false);
                stride *= 2;
            }
            let final_index = if collective.op.exclusive() {
                let first = self.id();
                self.emit(170, &[b, first, self.tile_lane, zero]);
                let run = self.id();
                let end = self.id();
                self.emit(247, &[end, 0]);
                self.emit(250, &[first, run, end]);
                self.emit(248, &[run]);
                let last = self.constant(Scalar::U32, padded - 1);
                for tree in &trees {
                    let identity = self.constant(Scalar::U32, tree.operator.identity(tree.scalar));
                    let root = self.tree_at(*tree, last);
                    self.emit(62, &[root, identity]);
                }
                self.emit(249, &[end]);
                self.emit(248, &[end]);
                self.scan_barrier();
                let mut stride = padded / 2;
                while stride != 0 {
                    self.tile_tree_step(&trees, stride, padded, true);
                    stride /= 2;
                }
                self.tile_lane
            } else {
                self.constant(Scalar::U32, padded - 1)
            };
            for ((_, result), tree) in results.iter().zip(&trees) {
                let at = self.tree_at(*tree, final_index);
                let raw = self.id();
                self.emit(61, &[u, raw, at]);
                self.from_word(*result, raw);
            }
            // 各 lane 已取得本批所有分量，后续集合才可重用同一工作区。
            self.tile_barrier();
        }
    }
    fn tile_tree_step(&mut self, trees: &[SharedTree], stride: u32, padded: u32, down: bool) {
        let u = self.types[&Scalar::U32];
        let one = self.constant(Scalar::U32, 1);
        let scale = self.constant(Scalar::U32, stride * 2);
        let plus = self.id();
        self.emit(128, &[u, plus, self.tile_lane, one]);
        let next = self.id();
        self.emit(132, &[u, next, plus, scale]);
        let node = self.id();
        self.emit(130, &[u, node, next, one]);
        let count = self.constant(Scalar::U32, padded);
        let selected = self.id();
        self.emit(176, &[self.types[&Scalar::Bool], selected, node, count]);
        self.scan_tree_step(trees, node, stride, selected, down, self.tile_length);
    }
}
