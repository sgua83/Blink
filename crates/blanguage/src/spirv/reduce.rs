//! 全域数值归约：分量共用 64 路树的同步，递归结果在设备驻留。
use super::{scan::TreeWords, *};
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
pub(super) struct WordScratch {
    context: *mut c_void,
    reserve: unsafe extern "C" fn(*mut c_void, usize) -> *mut u32,
}

impl WordScratch {
    pub(super) fn borrow(words: &mut Vec<u32>) -> Self {
        Self { context: std::ptr::from_mut(words).cast(), reserve: reserve_words }
    }
}

unsafe extern "C" fn reserve_words(context: *mut c_void, count: usize) -> *mut u32 {
    let words = unsafe { &mut *context.cast::<Vec<u32>>() };
    words.reserve_exact(count);
    words.as_mut_ptr()
}

#[repr(C)]
pub(super) struct NumericDispatch {
    pub(super) lane: u32,
    pub(super) shared: u32,
    pub(super) pointer: u32,
    pub(super) active: u32,
    pub(super) count: u32,
    pub(super) before: u32,
}

#[repr(C)]
pub(super) struct NumericGroupInput {
    fields: CollectiveLayout,
    identity: u64,
    operation: u32,
    source_word: u32,
    tail: u32,
    dispatch: NumericDispatch,
}

impl NumericGroupInput {
    pub(super) fn borrow(ty: &Ty, operator: CollectiveOperator, source_word: u32,
        tail: bool, dispatch: NumericDispatch) -> Self {
        Self {
            fields: CollectiveLayout::borrow(ty),
            identity: operator.identity(ty.components[0].0),
            operation: operator as u32,
            source_word,
            tail: u32::from(tail),
            dispatch,
        }
    }
}

#[repr(C)]
struct NumericOutput {
    binding: usize,
    word: u32,
    push_pointer: u32,
    push: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::collective_fields"]
    fn emit_collective_fields(layout: CollectiveLayout, source_word: u32, has_source: u32,
        host: MemoryHost, fields: *mut CollectiveField);
    #[link_name = "bootstrap.spirv_envelope::emit_numeric_reduction"]
    fn emit_numeric_reduction(input: NumericGroupInput, source: u32, output: NumericOutput,
        page: PageInputs, temporary: WordScratch, host: MemoryHost) -> TreeWords;
    #[link_name = "bootstrap.spirv_envelope::emit_numeric_writeback"]
    fn emit_numeric_writeback(fields: CollectiveLayout, output: NumericOutput, page: PageInputs,
        pointers: *const u32, lane: u32, temporary: WordScratch, host: MemoryHost) -> TreeWords;
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
        let dispatch = NumericDispatch { lane, shared, pointer, active, count, before: self.current_label };
        let input = NumericGroupInput::borrow(&reduction.ty, reduction.operator, r.source_word, tail, dispatch);
        let page = self.page_inputs(self.buffers[r.target], self.index, None, 0);
        let mut words = Vec::new();
        let result = unsafe {
            emit_numeric_reduction(input, self.buffers[r.source], self.numeric_output(r), page,
                WordScratch::borrow(&mut words), self.memory_host())
        };
        self.finish_tree(result);
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

    fn numeric_output(&self, r: &Reduction) -> NumericOutput {
        NumericOutput { binding: r.target, word: r.target_word, push_pointer: self.ptr_push_u32, push: self.push }
    }

    pub(super) fn reduction_output(&mut self, r: &Reduction, ty: &Ty, pointers: &[u32], lane: u32) {
        if ty.components.is_empty() {
            return;
        }
        let page = self.page_inputs(self.buffers[r.target], self.index, None, 0);
        let mut words = Vec::new();
        let result = unsafe {
            emit_numeric_writeback(CollectiveLayout::borrow(ty), self.numeric_output(r), page,
                pointers.as_ptr(), lane, WordScratch::borrow(&mut words), self.memory_host())
        };
        self.finish_tree(result);
    }

    pub(super) fn finish_tree(&mut self, result: TreeWords) {
        unsafe {
            finish_value(std::ptr::from_mut(self).cast(), ScalarWords {
                body: result.body, decorations: result.decorations,
            });
        }
        self.next = result.next;
        self.current_label = result.label;
    }
}
