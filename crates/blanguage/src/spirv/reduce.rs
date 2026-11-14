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
    #[link_name = "bootstrap.spirv_envelope::choose_reduction_target"]
    fn emit_choose_reduction_target(uint_type: u32, bool_type: u32, push_pointer: u32, push: u32,
        index: u32, width: u32, zero: u32, one: u32, length_word: u32, target_bias: u32,
        first: u32, body: *mut u32);
    #[link_name = "bootstrap.spirv_envelope::reduction_record"]
    fn emit_reduction_record(input: PageAddress, body: *mut u32) -> PageAddressResult;
    #[link_name = "bootstrap.spirv_envelope::reduction_field"]
    fn emit_reduction_field(record: u32, offset: u32, pointer_type: u32, wide_type: u32,
        step: u32, first: u32, body: *mut u32) -> PageAddressResult;
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
        if ty.components.is_empty() {
            return;
        }

        let run = self.id();
        let end = self.id();
        self.emit_control(9, |body| unsafe { open_selection(end, first, run, end, body) });

        let width = self.constant(Scalar::U32, 64);
        let length_word = self.constant(Scalar::U32, 2 + r.target as u32);
        let bias = if r.target_word == 0 { 0 } else { self.constant(Scalar::U32, r.target_word) };
        let mut target = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[r.target]),
            self.index, None, 0);
        let u = target.u32_type;
        let b = self.types[&Scalar::Bool];
        let first = self.next;
        let words = 26 + if bias == 0 { 0 } else { 6 };
        let at = self.body.len();
        self.body.reserve(words);
        unsafe {
            emit_choose_reduction_target(u, b, self.ptr_push_u32, self.push, self.index,
                width, target.zero, target.one, length_word, bias, first, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + words);
        }
        target.index = first + 4;
        self.next = first + 5;
        if bias != 0 {
            target.extra = self.id();
        }
        target.next = self.next;

        let at = self.body.len();
        self.body.reserve(109);
        let record = unsafe {
            let emitted = emit_reduction_record(target, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            self.next = emitted.next;
            emitted.pointer
        };

        for (&(scalar, offset), source) in ty.components.iter().zip(own) {
            let source = source.as_ref();
            let value = if scalar.size() < 4 {
                let mut raw = self.load_raw(Scalar::U32, Scalar::U32, source, false);
                if offset % 4 != 0 {
                    let shift = self.constant(Scalar::U32, offset % 4 * 8);
                    let shifted = self.id();
                    self.emit(194, &[u, shifted, raw, shift]);
                    raw = shifted;
                }
                self.join_words(scalar, &[raw], Scalar::U32)
            } else {
                self.load_raw(scalar, Scalar::U32, source, false)
            };

            let unit = Self::physical_unit(scalar);
            let count = scalar.size().div_ceil(4) as usize;
            let offset = if offset == 0 { 0 } else { self.constant(Scalar::U64, offset) };
            let step = if count == 2 { self.constant(Scalar::U64, 4) } else { 0 };
            let at = self.body.len();
            self.body.reserve(18);
            let pointers = unsafe {
                let emitted = emit_reduction_field(record, offset, self.physical_pointers[&unit],
                    self.u64_type, step, self.next, self.body.as_mut_ptr().add(at));
                self.body.set_len(at + emitted.words as usize);
                self.next = emitted.next;
                [emitted.pointer, emitted.second]
            };
            self.store_raw(scalar, unit, &pointers[..count], value, true);
        }

        self.emit_control(4, |body| unsafe { next_control_block(end, end, 1, body) });
    }
}
