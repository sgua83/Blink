//! 全域数值归约：分量共用 64 路树的同步，递归结果在设备驻留。
use super::*;
impl Emitter<'_> {
    pub(super) fn collective_pair(
        &mut self,
        scalar: Scalar,
        operator: CollectiveOperator,
        a: u32,
        b: u32,
    ) -> u32 {
        let u = self.types[&Scalar::U32];
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
        let (av, bv) = if scalar == Scalar::U32 {
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
            if scalar == Scalar::F32 {
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
                (Scalar::F32, true) => 184,
                (Scalar::F32, false) => 186,
                (Scalar::I32, true) => 177,
                (Scalar::I32, false) => 173,
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
        let components = reduction.ty.components.len() as u32;
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let identity = self.constant(Scalar::U32, reduction.operator.identity(scalar));
        let own: Vec<_> = (0..components)
            .map(|component| {
                let base = self.constant(Scalar::U32, component * 64);
                let index = self.id();
                self.emit(128, &[u, index, base, local]);
                let at = self.shared_at(pointer, shared, index);
                self.emit(62, &[at, identity]);
                at
            })
            .collect();
        let run = self.id();
        let merged = self.id();
        self.emit(247, &[merged, 0]);
        self.emit(250, &[active, run, merged]);
        self.emit(248, &[run]);
        for (component, at) in own.iter().enumerate() {
            let address = self.address_scalar(
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
                    r.source_word + reduction.ty.components[component].1 / 4
                },
            );
            let raw = self.id();
            self.emit(61, &[u, raw, address, 2, 4]);
            self.emit(62, &[*at, raw]);
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
            let valid = if reduction.operator != CollectiveOperator::Sum && scalar == Scalar::F32 {
                let other_global = self.id();
                self.emit(128, &[u, other_global, self.index, width]);
                let valid = self.id();
                self.emit(176, &[b, valid, other_global, count]);
                Some(valid)
            } else {
                None
            };
            for (component, at) in own.iter().enumerate() {
                let base = self.constant(Scalar::U32, component as u32 * 64);
                let index = self.id();
                self.emit(128, &[u, index, base, other]);
                let other_at = self.shared_at(pointer, shared, index);
                let a = self.id();
                let bv = self.id();
                self.emit(61, &[u, a, *at]);
                self.emit(61, &[u, bv, other_at]);
                let value = self.collective_pair(scalar, reduction.operator, a, bv);
                let value = if let Some(valid) = valid {
                    let selected = self.id();
                    self.emit(169, &[u, selected, valid, value, a]);
                    selected
                } else {
                    value
                };
                self.emit(62, &[*at, value]);
            }
            self.emit(249, &[end]);
            self.emit(248, &[end]);
            self.scan_barrier();
        }
        let first = self.id();
        self.emit(170, &[b, first, local, zero]);
        self.reduction_output(r, &reduction.ty, &own, first);
    }
    pub(super) fn reduction_output(&mut self, r: &Reduction, ty: &Ty, own: &[u32], first: u32) {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let components = ty.components.len() as u32;
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
                let value = self.id();
                self.emit(61, &[u, value, *at]);
                let target = self.address_scalar(
                    r.target,
                    if final_pass { zero } else { group },
                    if final_pass {
                        self.program.buffers[r.target].stride / 4
                    } else {
                        components
                    },
                    if final_pass {
                        r.target_word + ty.components[component].1 / 4
                    } else {
                        component as u32
                    },
                );
                self.emit(62, &[target, value, 2, 4]);
            }
            self.emit(249, &[written]);
        }
        self.emit(248, &[written]);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
}
