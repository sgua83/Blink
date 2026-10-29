//! 全域数值归约：分量共用 64 路树的同步，递归结果在设备驻留。
use super::{scan::SharedTree, *};
impl Emitter<'_> {
    pub(super) fn collective_pair(
        &mut self,
        scalar: Scalar,
        operator: CollectiveOperator,
        a: u32,
        b: u32,
    ) -> u32 {
        let u = self.raw_type(scalar);
        let bitwise = match operator {
            CollectiveOperator::BitAnd => Some(199),
            CollectiveOperator::BitOr => Some(197),
            CollectiveOperator::BitXor => Some(198),
            CollectiveOperator::Sum | CollectiveOperator::Min | CollectiveOperator::Max => None,
        };
        if let Some(opcode) = bitwise {
            let result = self.id();
            self.emit(opcode, &[u, result, a, b]);
            return result;
        }
        let (av, bv) = if scalar == Self::raw_scalar(scalar) {
            (a, b)
        } else {
            let ty = self.types[&scalar];
            let av = self.id();
            let bv = self.id();
            self.emit(124, &[ty, av, a]);
            self.emit(124, &[ty, bv, b]);
            (av, bv)
        };
        let result = self.id();
        if operator == CollectiveOperator::Sum {
            if scalar.is_float() {
                let sum = self.id();
                self.emit(129, &[self.types[&scalar], sum, av, bv]);
                op(&mut self.decorations, 71, &[sum, 42]);
                self.emit(124, &[u, result, sum]);
            } else {
                self.emit(128, &[u, result, a, b]);
            }
        } else {
            let min = operator == CollectiveOperator::Min;
            let opcode = match (scalar, min) {
                (Scalar::F32 | Scalar::F64, true) => 184,
                (Scalar::F32 | Scalar::F64, false) => 186,
                (_, true) if scalar.is_signed() => 177,
                (_, false) if scalar.is_signed() => 173,
                (_, true) => 176,
                (_, false) => 172,
            };
            let choose = self.id();
            self.emit(opcode, &[self.types[&Scalar::Bool], choose, av, bv]);
            self.emit(169, &[u, result, choose, a, b]);
        }
        result
    }
    pub(super) fn reduce(
        &mut self,
        reduction: &ReductionSpec,
        tail: bool,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) {
        let r = &reduction.fields;
        let scalar = reduction.ty.components[0].0;
        let words = reduction.ty.size.div_ceil(4);
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let identity = self.raw_constant(scalar, reduction.operator.identity(scalar));
        let trees:Vec<_>=reduction.ty.components.iter().map(|(scalar,offset)|SharedTree {
            shared,pointer,base_word:(offset/4)*64,scalar:*scalar,operator:reduction.operator,
        }).collect();
        let own: Vec<_> = trees.iter()
            .map(|tree| {
                let at = self.tree_parts(*tree,local);
                self.store_raw(tree.scalar, Scalar::U32,&at,identity,false);
                at
            })
            .collect();
        let run = self.id();
        let merged = self.id();
        self.emit(247, &[merged, 0]);
        self.emit(250, &[active, run, merged]);
        self.emit(248, &[run]);
        for (component, at) in own.iter().enumerate() {
            let offset=reduction.ty.components[component].1;
            let raw = self.load_raw_buffer(scalar,
                r.source,
                self.index,
                if tail {
                    words * 4
                } else {
                    self.program.buffers[r.source].stride
                },
                if tail {
                    offset
                } else {
                    r.source_word + offset
                },
            );
            self.store_raw(scalar, Scalar::U32,at,raw,false);
        }
        self.emit(249, &[merged]);
        self.emit(248, &[merged]);
        self.scan_barrier();
        for width in [32, 16, 8, 4, 2, 1] {
            let width = self.constant(Scalar::U32, width);
            let selected = self.id();
            self.emit(176, &[b, selected, local, width]);
            let run = self.id();
            let end = self.id();
            self.emit(247, &[end, 0]);
            self.emit(250, &[selected, run, end]);
            self.emit(248, &[run]);
            let other = self.id();
            self.emit(128, &[u, other, local, width]);
            let valid = if reduction.operator != CollectiveOperator::Sum && scalar.is_float() {
                let other_global = self.id();
                self.emit(128, &[u, other_global, self.index, width]);
                let valid = self.id();
                self.emit(176, &[b, valid, other_global, count]);
                Some(valid)
            } else {
                None
            };
            for (component, at) in own.iter().enumerate() {
                let a = self.load_raw(scalar, Scalar::U32,at,false);
                let bv = self.tree_load(trees[component],other);
                let value = self.collective_pair(scalar, reduction.operator, a, bv);
                let value = if let Some(valid) = valid {
                    let selected = self.id();
                    self.emit(169, &[self.raw_type(scalar), selected, valid, value, a]);
                    selected
                } else {
                    value
                };
                self.store_raw(scalar, Scalar::U32,at,value,false);
            }
            self.emit(249, &[end]);
            self.emit(248, &[end]);
            self.scan_barrier();
        }
        let first = self.id();
        self.emit(170, &[b, first, local, zero]);
        self.reduction_output(r, &reduction.ty, &own, first);
    }
    pub(super) fn reduction_output(&mut self, r: &Reduction, ty: &Ty, own: &[Vec<u32>], first: u32) {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let words = ty.size.div_ceil(4);
        let write = self.id();
        let end = self.id();
        self.emit(247, &[end, 0]);
        self.emit(250, &[first, write, end]);
        self.emit(248, &[write]);
        let width = self.constant(Scalar::U32, 64);
        let group = self.id();
        self.emit(134, &[u, group, self.index, width]);
        // 目标长度描述当前绑定的部分结果域；有效前缀可能早于绑定容量缩到单组。
        let target_length_word = self.constant(Scalar::U32, 2 + r.target as u32);
        let target_length_at = self.id();
        self.emit(
            65,
            &[
                self.ptr_push_u32,
                target_length_at,
                self.push,
                zero,
                target_length_word,
            ],
        );
        let target_length = self.id();
        self.emit(61, &[u, target_length, target_length_at]);
        let one = self.constant(Scalar::U32, 1);
        let final_pass = self.id();
        self.emit(170, &[b, final_pass, target_length, one]);
        let final_write = self.id();
        let partial_write = self.id();
        let written = self.id();
        self.emit(247, &[written, 0]);
        self.emit(250, &[final_pass, final_write, partial_write]);
        for (label, final_pass) in [(final_write, true), (partial_write, false)] {
            self.emit(248, &[label]);
            for (component, at) in own.iter().enumerate() {
                let (scalar,offset)=ty.components[component];
                let value = if scalar.size() < 4 {
                    let mut raw = self.load_raw(Scalar::U32, Scalar::U32, at, false);
                    if offset % 4 != 0 {
                        let shift = self.constant(Scalar::U32, offset % 4 * 8);
                        let shifted = self.id();
                        self.emit(194, &[u, shifted, raw, shift]);
                        raw = shifted;
                    }
                    self.join_words(scalar, &[raw], Scalar::U32)
                } else { self.load_raw(scalar, Scalar::U32, at, false) };
                self.store_raw_buffer(scalar,
                    r.target,
                    if final_pass { zero } else { group },
                    if final_pass {
                        self.program.buffers[r.target].stride
                    } else {
                        words * 4
                    },
                    if final_pass {
                        r.target_word + offset
                    } else {
                        offset
                    },
                    value,
                );
            }
            self.emit(249, &[written]);
        }
        self.emit(248, &[written]);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
}
