//! 全域数值归约：分量共用 64 路树的同步，递归结果在设备驻留。
use super::{scan::{SharedTree, TreeDomain}, *};

#[repr(C)]
struct CollectivePair {
    operation: u32,
    scalar: ScalarCode,
    raw_type: u32,
    value_type: u32,
    bool_type: u32,
    left: u32,
    right: u32,
    next: u32,
}

#[repr(C)]
struct CollectiveWords {
    body: u64,
    decorations: u64,
    next: u32,
    value: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::collective_pair"]
    fn emit_collective_pair(input: CollectivePair, body: *mut u32, decorations: *mut u32) -> CollectiveWords;
}

impl Emitter<'_> {
    pub(super) fn collective_pair(
        &mut self, scalar: Scalar, operator: CollectiveOperator, left: u32, right: u32,
    ) -> u32 {
        let input = CollectivePair {
            operation: operator as u32,
            scalar: scalar.into(),
            raw_type: self.raw_type(scalar),
            value_type: self.types[&scalar],
            bool_type: self.types[&Scalar::Bool],
            left,
            right,
            next: self.next,
        };
        let body = self.body.len();
        let decorations = self.decorations.len();
        self.body.reserve(19);
        self.decorations.reserve(3);
        let result = unsafe {
            let result = emit_collective_pair(input, self.body.as_mut_ptr().add(body),
                self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + result.body as usize);
            self.decorations.set_len(decorations + result.decorations as usize);
            result
        };
        self.next = result.next;
        result.value
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
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
        let identity = self.raw_constant(scalar, reduction.operator.identity(scalar));
        let trees:Vec<_>=reduction.ty.components.iter().map(|(scalar,offset)|SharedTree {
            shared,pointer,base_word:(offset/4)*64,scalar:*scalar,operator:reduction.operator,
        }).collect();
        let own: Vec<_> = trees.iter()
            .map(|tree| {
                let at = self.tree_parts(*tree,local);
                self.store_raw(tree.scalar, Scalar::U32, at.as_ref(), identity, false);
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
                    offset
                } else {
                    r.source_word + offset
                });
            self.store_raw(scalar, Scalar::U32, at.as_ref(), raw, false);
        }
        self.emit(249, &[merged]);
        self.emit(248, &[merged]);
        self.scan_barrier();
        let components = self.tree_components(&trees);
        for width in [32, 16, 8, 4, 2, 1] {
            self.tree_layer(&components, local, width,
                TreeDomain::Reduction { index: self.index, own: &own }, false, count);
        }
        let first = self.id();
        self.emit(170, &[b, first, local, zero]);
        self.reduction_output(r, &reduction.ty, &own, first);
    }
    pub(super) fn reduction_output(&mut self, r: &Reduction, ty: &Ty, own: &[impl AsRef<[u32]>], first: u32) {
        let u = self.types[&Scalar::U32];
        let b = self.types[&Scalar::Bool];
        let zero = self.constant(Scalar::U32, 0);
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
                let at = at.as_ref();
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
                        r.target_word + offset
                    } else {
                        offset
                    },
                    value);
            }
            self.emit(249, &[written]);
        }
        self.emit(248, &[written]);
        self.emit(249, &[end]);
        self.emit(248, &[end]);
    }
}
