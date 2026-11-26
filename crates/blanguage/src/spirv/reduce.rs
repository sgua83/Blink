//! 全域数值归约：分量共用 64 路树的同步，递归结果在设备驻留。
use super::{scan::{TreeComponent, TreeWords}, *};
use std::ffi::c_void;

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
struct CollectiveComponent {
    scalar: ScalarCode,
    offset: u32,
}

#[repr(C)]
pub(super) struct CollectiveLayout {
    source: *const c_void,
    count: usize,
    component: unsafe extern "C" fn(*const c_void, usize) -> CollectiveComponent,
}

impl CollectiveLayout {
    pub(super) fn borrow(ty: &Ty) -> Self {
        Self { source: ty.components.as_ptr().cast(), count: ty.components.len(), component: layout_component }
    }
}

unsafe extern "C" fn layout_component(source: *const c_void, index: usize) -> CollectiveComponent {
    let (scalar, offset) = unsafe { *source.cast::<(Scalar, u32)>().add(index) };
    CollectiveComponent { scalar: scalar.into(), offset }
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
    #[link_name = "bootstrap.spirv_envelope::collective_fields"]
    fn emit_collective_fields(layout: CollectiveLayout, source_word: u32, has_source: u32,
        host: MemoryHost, fields: *mut CollectiveField);
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
        let components = self.tree_components(&reduction.ty, reduction.operator, shared, pointer);
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
        let count = ty.components.len();
        let mut fields = Vec::with_capacity(count);
        unsafe {
            emit_collective_fields(CollectiveLayout::borrow(ty), source.unwrap_or(0),
                u32::from(source.is_some()), self.memory_host(), fields.as_mut_ptr());
            fields.set_len(count);
        }
        fields
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

