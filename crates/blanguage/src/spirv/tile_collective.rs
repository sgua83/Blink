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
            let mut base_word = self.tile_data_words(Scalar::U32);
            let trees: Vec<_> = results
                .iter()
                .map(|(_, result)| {
                    let scalar=self.stage.registers[*result];
                    let tree=SharedTree {
                    shared: self.tile_shared,
                    pointer: self.tile_pointer,
                    base_word,
                    scalar,
                    operator: collective.op.operator(),
                    };base_word+=padded*scalar.size().div_ceil(4);tree
                })
                .collect();
            for ((word, _), tree) in results.iter().zip(&trees) {
                let tree = *tree;
                let identity = self.raw_constant(tree.scalar, tree.operator.identity(tree.scalar));
                let active = self.id();
                let before = self.current_label;
                let read = self.id();
                let read_end = self.id();
                self.open_index_branch(active, self.tile_lane, self.tile_length, true, read, read_end);
                let value = self.tile_read(tree.scalar, collective.tile, self.tile_lane, *word, None);
                self.merge_active(read_end, true);
                let input = self.id();
                self.join_two(self.raw_type(tree.scalar), input, [value, read], [identity, before]);
                self.tree_store(tree,self.tile_lane,input);
                if padded > width {
                    let capacity = self.constant(Scalar::U32, padded);
                    let size = self.constant(Scalar::U32, width);
                    let index = self.id();
                    let fill = self.id();
                    let run = self.id();
                    let end = self.id();
                    let lane = self.tile_lane;
                    self.emit_control(19, |body| unsafe {
                        open_tile_padding(u, b, lane, size, capacity, index, fill, run, end, body)
                    });
                    self.tree_store(tree,index,identity);
                    self.merge_active(end, true);
                }
            }
            self.tile_barrier();
            let components = self.tree_components(&trees);
            let mut stride = 1;
            while stride < padded {
                self.tile_tree_layer(&components, self.tile_lane, stride, padded, false, self.tile_length);
                stride *= 2;
            }
            let final_index = if collective.op.exclusive() {
                let first = self.id();
                let run = self.id();
                let end = self.id();
                self.open_index_branch(first, self.tile_lane, zero, false, run, end);
                let last = self.constant(Scalar::U32, padded - 1);
                for tree in &trees {
                    let identity = self.raw_constant(tree.scalar, tree.operator.identity(tree.scalar));
                    self.tree_store(*tree,last,identity);
                }
                self.merge_active(end, true);
                self.scan_barrier();
                let mut stride = padded / 2;
                while stride != 0 {
                    self.tile_tree_layer(&components, self.tile_lane, stride, padded, true, self.tile_length);
                    stride /= 2;
                }
                self.tile_lane
            } else {
                self.constant(Scalar::U32, padded - 1)
            };
            for ((_, result), tree) in results.iter().zip(&trees) {
                let raw = self.tree_load(*tree, final_index);
                let words = self.split_words(tree.scalar,raw, Scalar::U32);
                self.from_words(*result,&words, Scalar::U32);
            }
            // 各 lane 已取得本批所有分量，后续集合才可重用同一工作区。
            self.tile_barrier();
        }
    }
}
