//! 叶范围函数直接累计各数值分量，经第五个参数写出本范围的部分结果。
use super::*;
use crate::builtin_types::ScalarEncoding;

#[repr(C)]
pub(super) struct ReductionRange {
    pub(super) identity: u64,
    scalar: ScalarEncoding,
    source: u32,
    stride_offset: u32,
    source_word: u32,
    operation: u32,
    count: u32,
    offsets: [u32; 4],
}

#[repr(C)]
pub(super) struct ReductionCode {
    length: u64,
    literals: [u64; 4],
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::reduce_range"]
    fn reduce_range(input: ReductionRange, output: *mut u8) -> ReductionCode;
}

impl ReductionRange {
    pub(super) fn new(fields: &Reduction, ty: &Ty, operator: CollectiveOperator) -> Self {
        let scalar = ty.components[0].0;
        let identity = operator.identity(scalar);
        let mut offsets = [0; 4];
        for (slot, (_, offset)) in offsets.iter_mut().zip(&ty.components) {
            *slot = *offset;
        }
        Self {
            identity,
            scalar: scalar.into(),
            source: fields.source as u32 * BUFFER_BYTES,
            stride_offset: BUFFER_STRIDE,
            source_word: fields.source_word,
            operation: match operator {
                CollectiveOperator::Sum => 0,
                CollectiveOperator::Min => 1,
                CollectiveOperator::Max => 2,
                CollectiveOperator::BitAnd => 3,
                CollectiveOperator::BitOr => 4,
                CollectiveOperator::BitXor => 5,
            },
            count: ty.components.len() as u32,
            offsets,
        }
    }
}

pub(super) fn emit(program: &Program, reduction: &ReductionSpec) -> NativeCode {
    let mut e = Emitter {
        unwind: Vec::new(),
        prolog: 0,
        probe: None,
        code: Vec::new(),
        relocations: Vec::new(),
        calls: Vec::new(),
        operation_ranges: None,
        function_abi: None,
        literals: std::collections::BTreeMap::new(),
        stage: &program.stages[0],
        buffers: &program.buffers,
        textures: &program.textures,
        breaks: Vec::new(),
        exits: Vec::new(),
        continues: Vec::new(),
        allocation: Allocation::default(),
        frame: Frame::default(),
    };
    e.push_registers(&[12]);

    let input = ReductionRange::new(&reduction.fields, &reduction.ty, reduction.operator);
    e.emit_collective(256, input.identity, |output| unsafe { reduce_range(input, output) });
    e.finish(8)
}

impl Emitter<'_> {
    pub(super) fn emit_collective(&mut self, capacity: usize, identity: Bits, emit: impl FnOnce(*mut u8) -> ReductionCode) {
        let start = self.code.len();
        let mut literals = [0; 4];
        self.emit_bytes(capacity, |output| {
            let encoded = emit(output);
            literals = encoded.literals;
            encoded.length
        });
        for offset in literals.into_iter().filter(|&offset| offset != 0) {
            self.literals.entry((identity, false)).or_default().push(start + offset as usize);
        }
    }
}
