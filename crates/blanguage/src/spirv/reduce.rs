//! 全域数值归约：分量共用 64 路树的同步，递归结果在设备驻留。
use super::{scan::{SharedTree, TreeComponent, TreeWords}, *};

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

#[repr(C)]
pub(super) struct CollectiveField {
    source_offset: u32,
    target_offset: u32,
    unit_type: u32,
    unit_bytes: u32,
    pointer_type: u32,
    step: u32,
    shift: u32,
    slot: u32,
}

#[repr(C)]
struct ReductionWriteback {
    target: PageAddress,
    bool_type: u32,
    push_pointer: u32,
    push: u32,
    width: u32,
    length_word: u32,
    bias: u32,
}

#[repr(C)]
pub(super) struct CollectiveInput {
    pub(super) identity: [u32; 2],
    pub(super) source: u32,
    pub(super) lane: u32,
    pub(super) active: u32,
    pub(super) count: u32,
    pub(super) before: u32,
    pub(super) scope: u32,
    pub(super) semantics: u32,
}

#[repr(C)]
struct ReductionGroup {
    output: ReductionWriteback,
    lanes: CollectiveInput,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::reduction_group"]
    fn emit_reduction_group(input: ReductionGroup, strides: *const u32, components: *const TreeComponent,
        fields: *const CollectiveField, count: u32, workspace: *mut u32, body: *mut u32,
        decorations: *mut u32) -> TreeWords;
    #[link_name = "bootstrap.spirv_envelope::reduction_writeback"]
    fn emit_reduction_writeback(input: ReductionWriteback, lane: u32, root: u32, fields: *const CollectiveField,
        count: u32, pointers: *const u32, body: *mut u32) -> StageWords;
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
        lane: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) {
        let r = &reduction.fields;
        let scalar = reduction.ty.components[0].0;
        let identity = self.collective_identity(scalar, reduction.operator);
        let trees: Vec<_> = reduction.ty.components.iter().map(|&(scalar, offset)| SharedTree {
            shared, pointer, base_word: (offset / 4) * 64, scalar, operator: reduction.operator,
        }).collect();
        let components = self.tree_components(&trees);
        let fields = self.collective_fields(&reduction.ty, Some(if tail { 0 } else { r.source_word }));
        let mut workspace = vec![0; 4 * fields.len()];
        let strides = [32, 16, 8, 4, 2, 1].map(|stride| self.constant(Scalar::U32, stride));
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x108);
        let input = ReductionGroup {
            output: self.reduction_writeback_input(r),
            lanes: CollectiveInput {
                identity, source: self.buffers[r.source], lane, active, count,
                before: self.current_label, scope, semantics,
            },
        };
        let body = self.body.len();
        let decorations = self.decorations.len();
        self.body.reserve(6 * (42 + 176 * fields.len()) + 350 + 108 * fields.len());
        self.decorations.reserve(18 * fields.len());
        unsafe {
            let emitted = emit_reduction_group(input, strides.as_ptr(), components.as_ptr(),
                fields.as_ptr(), fields.len() as u32, workspace.as_mut_ptr(),
                self.body.as_mut_ptr().add(body), self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + emitted.body as usize);
            self.decorations.set_len(decorations + emitted.decorations as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
    }


    pub(super) fn collective_identity(&mut self, scalar: Scalar, operator: CollectiveOperator) -> [u32; 2] {
        let bits = operator.identity(scalar);
        [
            self.constant(Scalar::U32, bits as u32),
            if scalar.size() == 8 { self.constant(Scalar::U32, (bits >> 32) as u32) } else { 0 },
        ]
    }

    pub(super) fn collective_fields(&mut self, ty: &Ty, source: Option<u32>) -> Vec<CollectiveField> {
        ty.components.iter().enumerate().map(|(index, &(scalar, offset))| {
            let unit = Self::physical_unit(scalar);
            let source_offset = match source {
                Some(base) if base + offset != 0 => self.constant(Scalar::U64, base + offset),
                _ => 0,
            };
            CollectiveField {
                source_offset,
                target_offset: if offset == 0 { 0 } else { self.constant(Scalar::U64, offset) },
                unit_type: self.types[&unit],
                unit_bytes: unit.size(),
                pointer_type: self.physical_pointers[&unit],
                step: if scalar.size() == 8 { self.constant(Scalar::U64, 4) } else { 0 },
                shift: if scalar.size() < 4 && offset % 4 != 0 {
                    self.constant(Scalar::U32, offset % 4 * 8)
                } else { 0 },
                slot: if source.is_some() { 2 * index as u32 } else { offset / 4 },
            }
        }).collect()
    }

    fn reduction_writeback_input(&mut self, r: &Reduction) -> ReductionWriteback {
        let width = self.constant(Scalar::U32, 64);
        let length_word = self.constant(Scalar::U32, 2 + r.target as u32);
        let bias = if r.target_word == 0 { 0 } else { self.constant(Scalar::U32, r.target_word) };
        let target = self.page_input(Scalar::U32, self.buffers[r.target],
            self.index, None, 0);
        ReductionWriteback {
            target, bool_type: self.types[&Scalar::Bool], push_pointer: self.ptr_push_u32,
            push: self.push, width, length_word, bias,
        }
    }


    pub(super) fn reduction_output(&mut self, r: &Reduction, ty: &Ty, pointers: &[u32], lane: u32) {
        if ty.components.is_empty() {
            return;
        }
        let fields = self.collective_fields(ty, None);
        let root = self.constant(Scalar::U32, 63);
        let input = self.reduction_writeback_input(r);
        let body = self.body.len();
        self.body.reserve(180 + 44 * fields.len());
        unsafe {
            let emitted = emit_reduction_writeback(input, lane, root, fields.as_ptr(),
                fields.len() as u32, pointers.as_ptr(), self.body.as_mut_ptr().add(body));
            self.body.set_len(body + emitted.words as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
    }
}

