//! 分段摘要携带尾段值与段头标记；归约额外携带段数，摘要始终留在设备。
use super::{scan::{ScanLevel, TreeWords}, *};

#[repr(C)]
struct SegmentField {
    scalar: ScalarCode,
    word: u32,
    raw_type: u32,
    value_type: u32,
    shift: u32,
}

#[repr(C)]
struct SegmentMerge {
    operator: u32,
    uint_type: u32,
    bool_type: u32,
    zero: u32,
    value_words: u32,
    counted: u32,
}

#[repr(C)]
struct SegmentAdd {
    merge: SegmentMerge,
    target: PageAddress,
    prefix_descriptor: u32,
    width: u32,
    step: u32,
    active: u32,
    run: u32,
    end: u32,
}

#[repr(C)]
struct SegmentStorageField {
    word: u32,
    offset: u32,
    unit_type: u32,
    unit_bytes: u32,
    pointer_type: u32,
    step: u32,
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SegmentPass {
    Main,
    Tail,
    Finish,
}

#[repr(C)]
struct SegmentGroup {
    merge: SegmentMerge,
    source: PageAddress,
    pass: SegmentPass,
    target: u32,
    summary: u32,
    head_offset: u32,
    head_word: u32,
    marker_word: u32,
    count_target: u32,
    count_word: u32,
    active: u32,
    count: u32,
    lane: u32,
    shared: u32,
    pointer: u32,
    width: u32,
    last: u32,
    scope: u32,
    semantics: u32,
    step: u32,
    before: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::segment_group"]
    fn emit_segment_group(input: SegmentGroup, levels: *const ScanLevel, fields: *const SegmentField,
        source_fields: *const SegmentStorageField, target_fields: *const SegmentStorageField,
        field_count: u32, identity: *const u32, words: *mut u32, body: *mut u32,
        decorations: *mut u32) -> TreeWords;
    #[link_name = "bootstrap.spirv_envelope::segment_add"]
    fn emit_segment_add(input: SegmentAdd, fields: *const SegmentField, count: u32,
        words: *mut u32, body: *mut u32, decorations: *mut u32) -> TreeWords;
}

struct Segment<'a> {
    ty: &'a Ty,
    operator: CollectiveOperator,
    counted: bool,
    fields: Vec<SegmentField>,
}
impl Segment<'_> {
    fn summary_words(&self) -> usize {
        self.ty.size.div_ceil(4) as usize + 1 + usize::from(self.counted)
    }
}
impl Emitter<'_> {
    fn segment<'a>(&mut self, ty: &'a Ty, operator: CollectiveOperator, counted: bool) -> Segment<'a> {
        let fields = ty.components.iter().map(|&(scalar, offset)| SegmentField {
            scalar: scalar.into(),
            word: offset / 4,
            raw_type: self.raw_type(scalar),
            value_type: self.types[&scalar],
            shift: if scalar.size() == 8 { self.constant(Scalar::U32, 32) } else { 0 },
        }).collect();
        Segment { ty, operator, counted, fields }
    }

    fn segment_identity(&mut self, scan: &Segment<'_>) -> Vec<u32> {
        let zero = self.constant(Scalar::U32, 0);
        let mut words=vec![zero;scan.summary_words()];
        for (scalar,offset) in &scan.ty.components {
            let value=self.raw_constant(*scalar,scan.operator.identity(*scalar));
            for (part,word) in self.split_words(*scalar,value, Scalar::U32).into_iter().enumerate() {
                words[(*offset/4) as usize+part]=word;
            }
        }
        words
    }
    fn segment_storage_fields(&mut self, scan: &Segment<'_>, first: u32) -> Vec<SegmentStorageField> {
        scan.ty.components.iter().map(|&(scalar, offset)| {
            let unit = Self::physical_unit(scalar);
            let byte = first + offset;
            SegmentStorageField {
                word: offset / 4,
                offset: if byte == 0 { 0 } else { self.constant(Scalar::U64, byte) },
                unit_type: self.types[&unit],
                unit_bytes: unit.size(),
                pointer_type: self.physical_pointers[&unit],
                step: if scalar.size() == 8 { self.constant(Scalar::U64, 4) } else { 0 },
            }
        }).collect()
    }

    fn segment_merge_input(&mut self, scan: &Segment<'_>) -> SegmentMerge {
        let zero = self.constant(Scalar::U32, 0);
        SegmentMerge {
            operator: scan.operator as u32,
            uint_type: self.types[&Scalar::U32],
            bool_type: self.types[&Scalar::Bool],
            zero,
            value_words: scan.ty.size.div_ceil(4),
            counted: u32::from(scan.counted),
        }
    }

    pub(super) fn segmented_scan(
        &mut self,
        scan: &Scan,
        head_word: u32,
        tail: bool,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) {
        let segment = self.segment(&scan.ty, scan.operator, false);
        let pass = if tail { SegmentPass::Tail } else { SegmentPass::Main };
        self.segment_group(&segment, &scan.values, head_word, pass, None, local, shared, pointer, active, count);
    }

    pub(super) fn segmented_reduce(
        &mut self,
        reduction: &SegmentedReduction,
        pass: Pass,
        local: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) {
        let values = &reduction.values;
        let segment = self.segment(&values.ty, values.operator, true);
        let pass = match pass {
            Pass::ScanTail => SegmentPass::Tail,
            Pass::ScanRootAdd => SegmentPass::Finish,
            _ => SegmentPass::Main,
        };
        let count_field = (pass == SegmentPass::Finish).then_some(&reduction.count);
        self.segment_group(&segment, &values.fields, reduction.head_word, pass, count_field,
            local, shared, pointer, active, count);
    }

    fn segment_group(
        &mut self,
        scan: &Segment<'_>,
        values: &Reduction,
        head: u32,
        pass: SegmentPass,
        count_field: Option<&crate::ir::PrefixCount>,
        lane: u32,
        shared: u32,
        pointer: u32,
        active: u32,
        count: u32,
    ) {
        let identity = self.segment_identity(scan);
        let merge = self.segment_merge_input(scan);
        let words = scan.summary_words();
        let levels = self.scan_levels();
        let mut workspace = vec![0; 8 * words];
        for (index, slot) in workspace[..words].iter_mut().enumerate().skip(1) {
            *slot = self.constant(Scalar::U32, index as u32 * 64);
        }

        let tail = pass == SegmentPass::Tail;
        let final_values = pass == SegmentPass::Finish || (pass == SegmentPass::Main && !scan.counted);
        let (source_binding, source_fields, head_offset) = if tail {
            (self.scan_input_binding(values), Vec::new(), 0)
        } else {
            let offset = if head == 0 { 0 } else { self.constant(Scalar::U64, head) };
            (values.source, self.segment_storage_fields(scan, values.source_word), offset)
        };
        let target_fields = if final_values {
            self.segment_storage_fields(scan, values.target_word)
        } else {
            Vec::new()
        };
        let (count_target, count_word) = count_field.map_or((0, 0), |field| {
            (self.buffers[field.binding], self.constant(Scalar::U32, field.word))
        });
        let head_word = if pass == SegmentPass::Finish { self.constant(Scalar::U32, head) } else { 0 };
        let marker_word = if final_values && !scan.counted {
            self.constant(Scalar::U32, merge.value_words * 4)
        } else {
            0
        };
        let width = self.constant(Scalar::U32, 64);
        let last = self.constant(Scalar::U32, 63);
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, 0x108);
        let step = self.constant(Scalar::U64, 4);
        let source = self.page_input(Scalar::U32,
            PageSource::Dataset(self.buffers[source_binding]), self.index, None, 0);
        let input = SegmentGroup {
            merge, source, pass, target: self.buffers[values.target],
            summary: self.buffers[self.program.buffers.len()],
            head_offset, head_word, marker_word, count_target, count_word,
            active, count, lane, shared, pointer, width, last, scope, semantics, step,
            before: self.current_label,
        };


        let fields = scan.fields.len();
        let layers = if scan.counted && pass == SegmentPass::Main { 6 } else { 12 };
        let final_merges = if pass == SegmentPass::Finish { 2 } else { 0 };
        let body = self.body.len();
        let decorations = self.decorations.len();
        self.body.reserve(layers * (57 + 48 * words + 74 * fields)
            + 1050 + 80 * words + 214 * fields);
        self.decorations.reserve(3 * fields * (layers + final_merges));
        unsafe {
            let emitted = emit_segment_group(input, levels.as_ptr(), scan.fields.as_ptr(),
                source_fields.as_ptr(), target_fields.as_ptr(), fields as u32, identity.as_ptr(),
                workspace.as_mut_ptr(), self.body.as_mut_ptr().add(body),
                self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + emitted.body as usize);
            self.decorations.set_len(decorations + emitted.decorations as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
    }

    pub(super) fn segmented_add(&mut self, scan: &Scan, active: u32, labels: [u32; 2]) {
        let segment = self.segment(&scan.ty, scan.operator, false);
        self.segment_add(&segment, &scan.values, active, labels);
    }

    pub(super) fn segmented_reduce_add(&mut self, reduction: &SegmentedReduction, active: u32, labels: [u32; 2]) {
        let values = &reduction.values;
        let segment = self.segment(&values.ty, values.operator, true);
        self.segment_add(&segment, &values.fields, active, labels);
    }

    fn segment_add(&mut self, scan: &Segment<'_>, fields: &Reduction, active: u32, [run, end]: [u32; 2]) {
        let width = self.constant(Scalar::U32, 64);
        let step = self.constant(Scalar::U64, 4);
        let merge = self.segment_merge_input(scan);
        let target = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[fields.target]),
            self.index, Some(0), 0);
        let input = SegmentAdd {
            merge,
            target,
            prefix_descriptor: self.buffers[self.scan_input_binding(fields)],
            width, step, active, run, end,
        };
        let count = scan.summary_words();
        let mut words = vec![0; 5 * count];
        let body = self.body.len();
        let decorations = self.decorations.len();
        self.body.reserve(243 + 41 * count + 74 * scan.fields.len());
        self.decorations.reserve(3 * scan.fields.len());
        unsafe {
            let result = emit_segment_add(input, scan.fields.as_ptr(), scan.fields.len() as u32,
                words.as_mut_ptr(), self.body.as_mut_ptr().add(body), self.decorations.as_mut_ptr().add(decorations));
            self.body.set_len(body + result.body as usize);
            self.decorations.set_len(decorations + result.decorations as usize);
            self.next = result.next;
            self.current_label = result.label;
        }
    }
}
