//! Original operation origins stream into optional material; decoding retains only source-free facts.
use super::super::{array, object, text};
use super::source::{Definition as SourceOrigin, Site as SourceSite, Declaration as SourceDeclaration, Range};
use crate::{codec::{Encode, Writer, record, variants}, ir::{self, ReuseResult, ReductionInputRole, ReductionPartition, ReductionUniformOperation, IntegerKnownValueBasis}};

#[derive(Clone, Debug)]
pub(crate) struct OperationRange {
    pub(super) start: usize,
    pub(super) end: usize,
    includes_nested_operations: bool,
    origin: OperationOrigin,
}
record!(OperationRange { start, end, includes_nested_operations, origin });
impl Encode for crate::native::OperationRange {
    fn encode(&self, writer: &mut Writer) {
        self.start.encode(writer); self.end.encode(writer);
        self.includes_nested_operations.encode(writer); self.origin.encode(writer);
    }
}
pub(super) fn ranges_json(ranges: &[OperationRange]) -> String {
    array(ranges.iter().map(|range| object(&[
        ("start_byte", range.start.to_string()), ("end_byte", range.end.to_string()),
        ("byte_length", (range.end - range.start).to_string()),
        ("includes_nested_operations", range.includes_nested_operations.to_string()),
        ("origin", range.origin.ir_json()),
    ])))
}
variants!(ir::UnaryOp { 0=>FloatNegate, 1=>SignedNegate, 2=>UnsignedNegate, 3=>LogicalNot, 4=>BitNot, 5=>Bitcast });
variants!(ir::ReuseResult { 0=>Value, 1=>Quotient, 2=>Remainder });
variants!(ir::ReductionInputRole { 0=>Source, 1=>Left, 2=>Right });
variants!(ir::IntegerDivisionOp { 0=>Quotient, 1=>Remainder });
variants!(ir::ReductionPartition { 0=>Identity, 1=>Project, 2=>Combine });
variants!(ir::ReductionUniformOperation { 0=>Constant, 1=>Uniform, 2=>BufferStride, 3=>Unary, 4=>Binary, 5=>Cast, 6=>Math, 7=>Select, 8=>ConstantIntegerDivide });
variants!(ir::IntegerKnownValueBasis { 0=>SingletonRange, 1=>ZeroCondition });
fn division_results_json(results: &[ir::IntegerDivisionOp]) -> String {
    array(results.iter().map(|result| text(match result {
        ir::IntegerDivisionOp::Quotient => "quotient", ir::IntegerDivisionOp::Remainder => "remainder",
    })))
}

#[derive(Clone, Debug)]
pub(super) struct OperationOrigin {
    source_site: Option<Box<SourceSite>>,
    generated_origin: Option<Box<GeneratedOrigin>>,
}
record!(OperationOrigin { source_site, generated_origin });
impl Encode for ir::OperationOrigin {
    fn encode(&self, writer: &mut Writer) { self.source_site.encode(writer); self.generated_origin.encode(writer); }
}
impl OperationOrigin {
    pub fn ir_json(&self) -> String {
        format!("{{\"kind\":\"operation_origin\",\"source_site\":{},\"generated_origin\":{}}}",
            self.source_site.as_deref().map_or_else(|| "null".into(), SourceSite::json),
            self.generated_origin.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json))
    }
}

#[derive(Clone, Debug)]
struct StageForBound {
    expression: Option<Box<SourceSite>>,
    value: u32,
    evaluation: Option<Box<GeneratedOrigin>>,
}
record!(StageForBound { expression, value, evaluation });
impl Encode for ir::StageForBound {
    fn encode(&self, writer: &mut Writer) { self.expression.encode(writer); self.value.encode(writer); self.evaluation.encode(writer); }
}
impl StageForBound {
    pub fn ir_json(&self) -> String {
        format!("{{\"kind\":\"stage_for_bound\",\"expression\":{},\"value\":{},\"evaluation\":{}}}",
            self.expression.as_deref().map_or_else(|| "null".into(), SourceSite::json), self.value,
            self.evaluation.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json))
    }
}

#[derive(Clone, Debug)]
struct StageForIteration {
    index: u32,
    returned: bool,
    stages: Vec<OperationOrigin>,
    calls: Vec<OperationOrigin>,
    expansion: Option<Box<GeneratedOrigin>>,
}
record!(StageForIteration { index, returned, stages, calls, expansion });
impl Encode for ir::StageForIteration {
    fn encode(&self, writer: &mut Writer) { self.index.encode(writer); self.returned.encode(writer); self.stages.encode(writer); self.calls.encode(writer); self.expansion.encode(writer); }
}
impl StageForIteration {
    pub fn ir_json(&self) -> String {
        format!("{{\"kind\":\"stage_for_iteration\",\"index\":{},\"returned\":{},\"stages\":[{}],\"calls\":[{}],\"expansion\":{}}}",
            self.index, self.returned, self.stages.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(","),
            self.calls.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(","),
            self.expansion.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json))
    }
}

#[derive(Clone, Debug)]
struct ExpressionReuse {
    result: ReuseResult,
    paired_input: bool,
    origin: OperationOrigin,
}
record!(ExpressionReuse { result, paired_input, origin });
impl Encode for ir::ExpressionReuse {
    fn encode(&self, writer: &mut Writer) { self.result.encode(writer); self.paired_input.encode(writer); self.origin.encode(writer); }
}
impl ExpressionReuse {
    fn input_results(&self) -> &[ReuseResult] {
        if self.paired_input { &[ReuseResult::Quotient, ReuseResult::Remainder] }
        else { std::slice::from_ref(&self.result) }
    }
    pub fn ir_json(&self) -> String {
        let name = |result| match result { ReuseResult::Value => "value", ReuseResult::Quotient => "quotient", ReuseResult::Remainder => "remainder" };
        format!("{{\"kind\":\"expression_reuse_input\",\"result\":{},\"input_results\":[{}],\"origin\":{}}}",
            text(name(self.result)), self.input_results().iter().map(|r|text(name(*r))).collect::<Vec<_>>().join(","), self.origin.ir_json())
    }
}

#[derive(Clone, Debug)]
struct LocalReadForward {
    result_index: usize,
    result_count: usize,
    result: ReuseResult,
    origin: OperationOrigin,
}
record!(LocalReadForward { result_index, result_count, result, origin });
impl Encode for ir::LocalReadForward {
    fn encode(&self, writer: &mut Writer) { self.result_index.encode(writer); self.result_count.encode(writer); self.result.encode(writer); self.origin.encode(writer); }
}
impl LocalReadForward {
    pub fn ir_json(&self) -> String {
        let result = match self.result { ReuseResult::Value => "value", ReuseResult::Quotient => "quotient", ReuseResult::Remainder => "remainder" };
        format!("{{\"kind\":\"local_read_forward_input\",\"result_index\":{},\"result_count\":{},\"result\":{},\"origin\":{}}}",
            self.result_index, self.result_count, text(result), self.origin.ir_json())
    }
}

#[derive(Clone, Debug)]
struct IndexedStoreReadForward {
    result_index: usize,
    result_count: usize,
    result: ReuseResult,
    store: OperationOrigin,
    read: OperationOrigin,
}
record!(IndexedStoreReadForward { result_index, result_count, result, store, read });
impl Encode for ir::IndexedStoreReadForward {
    fn encode(&self, writer: &mut Writer) { self.result_index.encode(writer); self.result_count.encode(writer); self.result.encode(writer); self.store.encode(writer); self.read.encode(writer); }
}
impl IndexedStoreReadForward {
    pub fn ir_json(&self) -> String {
        let result = match self.result { ReuseResult::Value => "value", ReuseResult::Quotient => "quotient", ReuseResult::Remainder => "remainder" };
        format!("{{\"kind\":\"indexed_store_read_forward_input\",\"result_index\":{},\"result_count\":{},\"result\":{},\"store\":{},\"read\":{}}}",
            self.result_index, self.result_count, text(result), self.store.ir_json(), self.read.ir_json())
    }
}

#[derive(Clone, Debug)]
struct InputReadForward {
    input_index: usize,
    reads: Vec<OperationOrigin>,
}
record!(InputReadForward { input_index, reads });
impl Encode for ir::InputReadForward {
    fn encode(&self, writer: &mut Writer) { self.input_index.encode(writer); self.reads.encode(writer); }
}
impl InputReadForward {
    pub fn ir_json(&self) -> String {
        format!("{{\"kind\":\"input_read_forward\",\"input_index\":{},\"reads\":[{}]}}", self.input_index,
            self.reads.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(","))
    }
}

#[derive(Clone, Debug)]
struct ReductionInputReadForward {
    role: ReductionInputRole,
    component_index: usize,
    reads: Vec<OperationOrigin>,
}
record!(ReductionInputReadForward { role, component_index, reads });
impl Encode for ir::ReductionInputReadForward {
    fn encode(&self, writer: &mut Writer) { self.role.encode(writer); self.component_index.encode(writer); self.reads.encode(writer); }
}
impl ReductionInputReadForward {
    pub fn ir_json(&self) -> String {
        let role = match self.role { ReductionInputRole::Source => "source", ReductionInputRole::Left => "left", ReductionInputRole::Right => "right" };
        format!("{{\"kind\":\"reduction_input_read_forward\",\"role\":{},\"component_index\":{},\"reads\":[{}]}}",
            text(role), self.component_index, self.reads.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(","))
    }
}

#[derive(Clone, Debug)]
enum EliminatedOperation {
    ValueResults {
        result_types: Vec<ir::Scalar>,
        integer_division_results: Option<Vec<ir::IntegerDivisionOp>>,
        origin: OperationOrigin,
    },
    DirectPrivateWrite { scalar: ir::Scalar, origin: OperationOrigin },
    IndexedPrivateWrite {
        component_count: usize,
        index_type: ir::Scalar,
        byte_offset: u32,
        scalar: ir::Scalar,
        origin: OperationOrigin,
    },
}
variants!(EliminatedOperation {
    0=>ValueResults{ result_types, integer_division_results, origin },
    1=>DirectPrivateWrite{ scalar, origin },
    2=>IndexedPrivateWrite{ component_count, index_type, byte_offset, scalar, origin }
});
impl Encode for ir::EliminatedOperation {
    fn encode(&self, writer: &mut Writer) {
        match self {
            Self::ValueResults{ result_types, integer_division_results, origin } => { (0u32).encode(writer); result_types.encode(writer); integer_division_results.encode(writer); origin.encode(writer); },
            Self::DirectPrivateWrite{ scalar, origin } => { (1u32).encode(writer); scalar.encode(writer); origin.encode(writer); },
            Self::IndexedPrivateWrite{ component_count, index_type, byte_offset, scalar, origin } => { (2u32).encode(writer); component_count.encode(writer); index_type.encode(writer); byte_offset.encode(writer); scalar.encode(writer); origin.encode(writer); },
        }
    }
}
impl EliminatedOperation {
    pub fn ir_json(&self) -> String {
        match self {
            Self::ValueResults { result_types, integer_division_results, origin } => format!("{{\"kind\":\"eliminated_value\",\"result_types\":[{}],\"integer_division_results\":{},\"origin\":{}}}",
                result_types.iter().map(|value| text(&format!("{value:?}"))).collect::<Vec<_>>().join(","),
                integer_division_results.as_deref().map_or_else(|| "null".into(), division_results_json), origin.ir_json()),
            Self::DirectPrivateWrite { scalar, origin } => format!("{{\"kind\":\"eliminated_private_write\",\"target\":\"direct\",\"scalar\":{},\"origin\":{}}}",
                text(&format!("{scalar:?}")), origin.ir_json()),
            Self::IndexedPrivateWrite { component_count, index_type, byte_offset, scalar, origin } => format!("{{\"kind\":\"eliminated_private_write\",\"target\":\"indexed\",\"component_count\":{},\"index_type\":{},\"byte_offset\":{},\"scalar\":{},\"origin\":{}}}",
                component_count, text(&format!("{index_type:?}")), byte_offset, text(&format!("{scalar:?}")), origin.ir_json()),
        }
    }
}

#[derive(Clone, Debug)]
enum GenerationAnchor {
    Syntax(Box<SourceSite>),
    CpuTextureSample { sample: OperationOrigin },
    Definition { origin: Option<Box<SourceOrigin>>, instance: String },
    IntegerDivisionPair { quotient: OperationOrigin, remainder: OperationOrigin },
    ExpressionReuse { retained: OperationOrigin, reused: Vec<ExpressionReuse> },
    LocalReadForward { retained: OperationOrigin, reads: Vec<LocalReadForward> },
    IndexedStoreReadForward { retained: OperationOrigin, reads: Vec<IndexedStoreReadForward> },
    IndexedReadReuse { retained: OperationOrigin, reused: Vec<OperationOrigin> },
    LocalInputForward { retained: Option<Box<GeneratedOrigin>>, inputs: Vec<InputReadForward> },
    ReductionInputForward { retained: Option<Box<GeneratedOrigin>>, inputs: Vec<ReductionInputReadForward> },
    EliminatedOperations { retained: Option<Box<GeneratedOrigin>>, operations: Vec<EliminatedOperation> },
    EmptyMapRemoval { retained: Option<Box<GeneratedOrigin>>, stages: Vec<OperationOrigin> },
    IntegerDivisionResultPrune { retained: OperationOrigin, input_results: Vec<ir::IntegerDivisionOp>, output_results: Vec<ir::IntegerDivisionOp> },
    ReductionUniformExtract { retained: OperationOrigin, partition: ReductionPartition, operation: ReductionUniformOperation, uniform_operand_count: usize },
    UnaryConstantFold {
        expression: Option<Box<SourceSite>>, operation: ir::UnaryOp, scalar: ir::Scalar,
        input_bits: ir::Bits, result_bits: ir::Bits, component_index: usize, component_count: usize,
    },
    BinaryConstantFold {
        expression: Option<Box<SourceSite>>, operation: String, scalar: ir::Scalar,
        result_scalar: ir::Scalar, left_bits: ir::Bits, right_bits: ir::Bits, result_bits: ir::Bits,
        left_component_index: usize, left_component_count: usize,
        right_component_index: usize, right_component_count: usize,
        component_index: usize, component_count: usize, retained: Option<Box<GeneratedOrigin>>,
    },
    CastConstantFold {
        expression: Option<Box<SourceSite>>, source_scalar: ir::Scalar, target_scalar: ir::Scalar,
        input_bits: ir::Bits, result_bits: ir::Bits, component_index: usize, component_count: usize,
    },
    BitcastConstantFold {
        expression: Option<Box<SourceSite>>, source_scalar: ir::Scalar, target_scalar: ir::Scalar,
        bits: ir::Bits, component_index: usize, component_count: usize,
    },
    MinmaxKnownValue {
        expression: Option<Box<SourceSite>>, operation: String, scalar: ir::Scalar,
        left_ranges: Vec<(ir::Bits, ir::Bits)>, right_ranges: Vec<(ir::Bits, ir::Bits)>,
        left_constant_present: bool, right_constant_present: bool, same_value: bool,
        result_bits: ir::Bits, basis: IntegerKnownValueBasis,
        left_component_index: usize, left_component_count: usize,
        right_component_index: usize, right_component_count: usize,
        component_index: usize, component_count: usize,
    },
    ComparisonKnownValue {
        expression: Option<Box<SourceSite>>, operation: String, scalar: ir::Scalar,
        left_ranges: Option<Vec<(ir::Bits, ir::Bits)>>, right_ranges: Option<Vec<(ir::Bits, ir::Bits)>>,
        left_constant_present: bool, right_constant_present: bool, same_value: bool,
        result_bits: ir::Bits, false_possible: bool, true_possible: bool,
        left_component_index: usize, left_component_count: usize, right_component_index: usize, right_component_count: usize,
        component_index: usize, component_count: usize, retained: Option<Box<GeneratedOrigin>>,
    },
    ModuleConstantEvaluation {
        definition: Option<Box<SourceOrigin>>, constant: String, operations: Vec<OperationOrigin>,
    },
    StageConstantEvaluation { expression: Option<Box<SourceSite>>, operations: Vec<OperationOrigin> },
    StageIfSelection {
        condition: Option<Box<SourceSite>>, value: bool,
        evaluation: Option<Box<GeneratedOrigin>>, returned: bool,
        stages: Vec<OperationOrigin>, calls: Vec<OperationOrigin>,
        retained: Option<Box<GeneratedOrigin>>,
    },
    StageForExpansion {
        statement: Option<Box<SourceSite>>, counter: String,
        first: StageForBound, last: StageForBound, iterations: Vec<StageForIteration>,
        retained: Option<Box<GeneratedOrigin>>,
    },
    DomainEntry { context: Option<Box<GenerationAnchor>>, expansion: Option<Box<GeneratedOrigin>> },
    StageConstantReference { reference: Option<Box<SourceSite>>, retained: Option<Box<GeneratedOrigin>> },
    DomainCallLiteral { call: Option<Box<SourceSite>>, evaluation: Option<Box<GeneratedOrigin>> },
    DeclarationReference { declaration: Box<SourceDeclaration>, retained: OperationOrigin },
    ContractCondition { declaration: Box<SourceDeclaration>, span: Range, retained: OperationOrigin },
}
variants!(GenerationAnchor {
    0=>Syntax(site),
    1=>CpuTextureSample{ sample },
    2=>Definition{ origin, instance },
    3=>IntegerDivisionPair{ quotient, remainder },
    4=>ExpressionReuse{ retained, reused },
    5=>LocalReadForward{ retained, reads },
    6=>IndexedStoreReadForward{ retained, reads },
    7=>IndexedReadReuse{ retained, reused },
    8=>LocalInputForward{ retained, inputs },
    9=>ReductionInputForward{ retained, inputs },
    10=>EliminatedOperations{ retained, operations },
    11=>EmptyMapRemoval{ retained, stages },
    12=>IntegerDivisionResultPrune{ retained, input_results, output_results },
    13=>ReductionUniformExtract{ retained, partition, operation, uniform_operand_count },
    14=>UnaryConstantFold{ expression, operation, scalar, input_bits, result_bits, component_index, component_count },
    15=>BinaryConstantFold{ expression, operation, scalar, result_scalar, left_bits, right_bits, result_bits, left_component_index, left_component_count, right_component_index, right_component_count, component_index, component_count, retained },
    16=>CastConstantFold{ expression, source_scalar, target_scalar, input_bits, result_bits, component_index, component_count },
    17=>BitcastConstantFold{ expression, source_scalar, target_scalar, bits, component_index, component_count },
    18=>MinmaxKnownValue{ expression, operation, scalar, left_ranges, right_ranges, left_constant_present, right_constant_present, same_value, result_bits, basis, left_component_index, left_component_count, right_component_index, right_component_count, component_index, component_count },
    19=>ComparisonKnownValue{ expression, operation, scalar, left_ranges, right_ranges, left_constant_present, right_constant_present, same_value, result_bits, false_possible, true_possible, left_component_index, left_component_count, right_component_index, right_component_count, component_index, component_count, retained },
    20=>ModuleConstantEvaluation{ definition, constant, operations },
    21=>StageConstantEvaluation{ expression, operations },
    22=>StageIfSelection{ condition, value, evaluation, returned, stages, calls, retained },
    23=>StageForExpansion{ statement, counter, first, last, iterations, retained },
    24=>DomainEntry{ context, expansion },
    25=>StageConstantReference{ reference, retained },
    26=>DomainCallLiteral{ call, evaluation },
    27=>DeclarationReference{ declaration, retained },
    28=>ContractCondition{ declaration, span, retained }
});
impl Encode for ir::GenerationAnchor {
    fn encode(&self, writer: &mut Writer) {
        match self {
            Self::Syntax(site) => { (0u32).encode(writer); site.encode(writer); },
            Self::CpuTextureSample{ sample } => { (1u32).encode(writer); sample.encode(writer); },
            Self::Definition{ origin, instance } => { (2u32).encode(writer); origin.encode(writer); instance.to_string().encode(writer); },
            Self::IntegerDivisionPair{ quotient, remainder } => { (3u32).encode(writer); quotient.encode(writer); remainder.encode(writer); },
            Self::ExpressionReuse{ retained, reused } => { (4u32).encode(writer); retained.encode(writer); reused.encode(writer); },
            Self::LocalReadForward{ retained, reads } => { (5u32).encode(writer); retained.encode(writer); reads.encode(writer); },
            Self::IndexedStoreReadForward{ retained, reads } => { (6u32).encode(writer); retained.encode(writer); reads.encode(writer); },
            Self::IndexedReadReuse{ retained, reused } => { (7u32).encode(writer); retained.encode(writer); reused.encode(writer); },
            Self::LocalInputForward{ retained, inputs } => { (8u32).encode(writer); retained.encode(writer); inputs.encode(writer); },
            Self::ReductionInputForward{ retained, inputs } => { (9u32).encode(writer); retained.encode(writer); inputs.encode(writer); },
            Self::EliminatedOperations{ retained, operations } => { (10u32).encode(writer); retained.encode(writer); operations.encode(writer); },
            Self::EmptyMapRemoval{ retained, stages } => { (11u32).encode(writer); retained.encode(writer); stages.encode(writer); },
            Self::IntegerDivisionResultPrune{ retained, input_results, output_results } => { (12u32).encode(writer); retained.encode(writer); input_results.encode(writer); output_results.encode(writer); },
            Self::ReductionUniformExtract{ retained, partition, operation, uniform_operand_count } => { (13u32).encode(writer); retained.encode(writer); partition.encode(writer); operation.encode(writer); uniform_operand_count.encode(writer); },
            Self::UnaryConstantFold{ expression, operation, scalar, input_bits, result_bits, component_index, component_count } => { (14u32).encode(writer); expression.encode(writer); operation.encode(writer); scalar.encode(writer); input_bits.encode(writer); result_bits.encode(writer); component_index.encode(writer); component_count.encode(writer); },
            Self::BinaryConstantFold{ expression, operation, scalar, result_scalar, left_bits, right_bits, result_bits, left_component_index, left_component_count, right_component_index, right_component_count, component_index, component_count, retained } => { (15u32).encode(writer); expression.encode(writer); operation.to_string().encode(writer); scalar.encode(writer); result_scalar.encode(writer); left_bits.encode(writer); right_bits.encode(writer); result_bits.encode(writer); left_component_index.encode(writer); left_component_count.encode(writer); right_component_index.encode(writer); right_component_count.encode(writer); component_index.encode(writer); component_count.encode(writer); retained.encode(writer); },
            Self::CastConstantFold{ expression, source_scalar, target_scalar, input_bits, result_bits, component_index, component_count } => { (16u32).encode(writer); expression.encode(writer); source_scalar.encode(writer); target_scalar.encode(writer); input_bits.encode(writer); result_bits.encode(writer); component_index.encode(writer); component_count.encode(writer); },
            Self::BitcastConstantFold{ expression, source_scalar, target_scalar, bits, component_index, component_count } => { (17u32).encode(writer); expression.encode(writer); source_scalar.encode(writer); target_scalar.encode(writer); bits.encode(writer); component_index.encode(writer); component_count.encode(writer); },
            Self::MinmaxKnownValue{ expression, operation, scalar, left_ranges, right_ranges, left_constant_present, right_constant_present, same_value, result_bits, basis, left_component_index, left_component_count, right_component_index, right_component_count, component_index, component_count } => { (18u32).encode(writer); expression.encode(writer); operation.to_string().encode(writer); scalar.encode(writer); left_ranges.encode(writer); right_ranges.encode(writer); left_constant_present.encode(writer); right_constant_present.encode(writer); same_value.encode(writer); result_bits.encode(writer); basis.encode(writer); left_component_index.encode(writer); left_component_count.encode(writer); right_component_index.encode(writer); right_component_count.encode(writer); component_index.encode(writer); component_count.encode(writer); },
            Self::ComparisonKnownValue{ expression, operation, scalar, left_ranges, right_ranges, left_constant_present, right_constant_present, same_value, result_bits, false_possible, true_possible, left_component_index, left_component_count, right_component_index, right_component_count, component_index, component_count, retained } => { (19u32).encode(writer); expression.encode(writer); operation.to_string().encode(writer); scalar.encode(writer); left_ranges.encode(writer); right_ranges.encode(writer); left_constant_present.encode(writer); right_constant_present.encode(writer); same_value.encode(writer); result_bits.encode(writer); false_possible.encode(writer); true_possible.encode(writer); left_component_index.encode(writer); left_component_count.encode(writer); right_component_index.encode(writer); right_component_count.encode(writer); component_index.encode(writer); component_count.encode(writer); retained.encode(writer); },
            Self::ModuleConstantEvaluation{ definition, constant, operations } => { (20u32).encode(writer); definition.encode(writer); constant.to_string().encode(writer); operations.encode(writer); },
            Self::StageConstantEvaluation{ expression, operations } => { (21u32).encode(writer); expression.encode(writer); operations.encode(writer); },
            Self::StageIfSelection{ condition, value, evaluation, returned, stages, calls, retained } => { (22u32).encode(writer); condition.encode(writer); value.encode(writer); evaluation.encode(writer); returned.encode(writer); stages.encode(writer); calls.encode(writer); retained.encode(writer); },
            Self::StageForExpansion{ statement, counter, first, last, iterations, retained } => { (23u32).encode(writer); statement.encode(writer); counter.to_string().encode(writer); first.encode(writer); last.encode(writer); iterations.encode(writer); retained.encode(writer); },
            Self::DomainEntry{ context, expansion } => { (24u32).encode(writer); context.encode(writer); expansion.encode(writer); },
            Self::StageConstantReference{ reference, retained } => { (25u32).encode(writer); reference.encode(writer); retained.encode(writer); },
            Self::DomainCallLiteral{ call, evaluation } => { (26u32).encode(writer); call.encode(writer); evaluation.encode(writer); },
            Self::DeclarationReference{ declaration, retained } => { (27u32).encode(writer); declaration.encode(writer); retained.encode(writer); },
            Self::ContractCondition{ declaration, span, retained } => { (28u32).encode(writer); declaration.encode(writer); Range::capture(Some(&declaration.source().text), *span).encode(writer); retained.encode(writer); },
        }
    }
}
impl GenerationAnchor {
    pub fn ir_json(&self) -> String {
        match self {
            Self::Syntax(site) => site.json(),
            Self::CpuTextureSample { sample } => format!("{{\"kind\":\"cpu_texture_sample\",\"sample\":{}}}", sample.ir_json()),
            Self::ComparisonKnownValue { expression, operation, scalar, left_ranges, right_ranges,
                left_constant_present, right_constant_present, same_value, result_bits, false_possible, true_possible,
                left_component_index, left_component_count, right_component_index, right_component_count,
                component_index, component_count, retained } => {
                let ranges = |ranges: &Option<Vec<(ir::Bits, ir::Bits)>>| ranges.as_ref().map_or_else(|| "null".into(), |ranges|
                    format!("[{}]", ranges.iter().map(|(lower, upper)| format!("{{\"lower_bits\":\"0x{lower:016x}\",\"upper_bits\":\"0x{upper:016x}\"}}")).collect::<Vec<_>>().join(",")));
                let order = if scalar.is_float() { text("float_bit_order") } else if scalar.is_integer() { text("scalar_numeric") } else { "null".into() };
                format!("{{\"kind\":\"comparison_known_value\",\"expression\":{},\"operation\":{},\"scalar\":{},\"interval_order\":{},\"interval_bounds\":\"inclusive\",\"left_ranges\":{},\"right_ranges\":{},\"left_constant_present\":{},\"right_constant_present\":{},\"same_value\":{},\"result_bits\":\"0x{:016x}\",\"false_possible\":{},\"true_possible\":{},\"component_index_base\":0,\"left_component_index\":{},\"left_component_count\":{},\"right_component_index\":{},\"right_component_count\":{},\"component_index\":{},\"component_count\":{},\"retained\":{}}}",
                    expression.as_deref().map_or_else(|| "null".into(), SourceSite::json), text(operation), text(&format!("{scalar:?}")), order,
                    ranges(left_ranges), ranges(right_ranges), left_constant_present, right_constant_present, same_value, result_bits, false_possible, true_possible,
                    left_component_index, left_component_count, right_component_index, right_component_count, component_index, component_count,
                    retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json))
            },
            Self::MinmaxKnownValue { expression, operation, scalar, left_ranges, right_ranges,
                left_constant_present, right_constant_present, same_value, result_bits, basis,
                left_component_index, left_component_count, right_component_index, right_component_count,
                component_index, component_count } => {
                let ranges = |ranges: &[(ir::Bits, ir::Bits)]| format!("[{}]", ranges.iter().map(|(lower, upper)|
                    format!("{{\"lower_bits\":\"0x{lower:016x}\",\"upper_bits\":\"0x{upper:016x}\"}}")).collect::<Vec<_>>().join(","));
                format!("{{\"kind\":\"minmax_known_value\",\"expression\":{},\"operation\":{},\"scalar\":{},\"interval_order\":\"scalar_numeric\",\"interval_bounds\":\"inclusive\",\"left_ranges\":{},\"right_ranges\":{},\"left_constant_present\":{},\"right_constant_present\":{},\"same_value\":{},\"result_bits\":\"0x{:016x}\",\"basis\":{},\"component_index_base\":0,\"left_component_index\":{},\"left_component_count\":{},\"right_component_index\":{},\"right_component_count\":{},\"component_index\":{},\"component_count\":{}}}",
                    expression.as_deref().map_or_else(|| "null".into(), SourceSite::json), text(operation), text(&format!("{scalar:?}")),
                    ranges(left_ranges), ranges(right_ranges), left_constant_present, right_constant_present, same_value, result_bits,
                    text(basis.ir_name()), left_component_index, left_component_count, right_component_index, right_component_count, component_index, component_count)
            },
            Self::BitcastConstantFold { expression, source_scalar, target_scalar, bits, component_index, component_count } => format!("{{\"kind\":\"bitcast_constant_fold\",\"expression\":{},\"source_scalar\":{},\"target_scalar\":{},\"bits\":\"0x{:016x}\",\"component_index_base\":0,\"component_index\":{},\"component_count\":{}}}",
                expression.as_deref().map_or_else(|| "null".into(), SourceSite::json),
                text(&format!("{source_scalar:?}")), text(&format!("{target_scalar:?}")), bits, component_index, component_count),
            Self::CastConstantFold { expression, source_scalar, target_scalar, input_bits, result_bits, component_index, component_count } => format!("{{\"kind\":\"cast_constant_fold\",\"expression\":{},\"source_scalar\":{},\"target_scalar\":{},\"input_bits\":\"0x{:016x}\",\"result_bits\":\"0x{:016x}\",\"component_index_base\":0,\"component_index\":{},\"component_count\":{}}}",
                expression.as_deref().map_or_else(|| "null".into(), SourceSite::json),
                text(&format!("{source_scalar:?}")), text(&format!("{target_scalar:?}")), input_bits, result_bits, component_index, component_count),
            Self::BinaryConstantFold { expression, operation, scalar, result_scalar, left_bits, right_bits, result_bits,
                left_component_index, left_component_count, right_component_index, right_component_count,
                component_index, component_count, retained } => format!("{{\"kind\":\"binary_constant_fold\",\"expression\":{},\"operation\":{},\"scalar\":{},\"result_scalar\":{},\"left_bits\":\"0x{:016x}\",\"right_bits\":\"0x{:016x}\",\"result_bits\":\"0x{:016x}\",\"component_index_base\":0,\"left_component_index\":{},\"left_component_count\":{},\"right_component_index\":{},\"right_component_count\":{},\"component_index\":{},\"component_count\":{},\"retained\":{}}}",
                expression.as_deref().map_or_else(|| "null".into(), SourceSite::json), text(operation), text(&format!("{scalar:?}")),
                text(&format!("{result_scalar:?}")), left_bits, right_bits, result_bits,
                left_component_index, left_component_count, right_component_index, right_component_count,
                component_index, component_count, retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json)),
            Self::StageConstantReference { reference, retained } => format!("{{\"kind\":\"stage_constant_reference\",\"reference\":{},\"retained\":{}}}",
                reference.as_deref().map_or_else(|| "null".into(), SourceSite::json),
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json)),
            Self::StageForExpansion { statement, counter, first, last, iterations, retained } => format!("{{\"kind\":\"stage_for_expansion\",\"statement\":{},\"counter\":{},\"first\":{},\"last\":{},\"iterations\":[{}],\"retained\":{}}}",
                statement.as_deref().map_or_else(|| "null".into(), SourceSite::json), text(counter), first.ir_json(), last.ir_json(),
                iterations.iter().map(StageForIteration::ir_json).collect::<Vec<_>>().join(","),
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json)),
            Self::StageIfSelection { condition, value, evaluation, returned, stages, calls, retained } => format!("{{\"kind\":\"stage_if_selection\",\"condition\":{},\"value\":{},\"evaluation\":{},\"returned\":{},\"stages\":[{}],\"calls\":[{}],\"retained\":{}}}",
                condition.as_deref().map_or_else(|| "null".into(), SourceSite::json), value,
                evaluation.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json), returned,
                stages.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(","),
                calls.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(","),
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json)),
            Self::DomainEntry { context, expansion } => format!("{{\"kind\":\"domain_entry\",\"context\":{},\"expansion\":{}}}",
                context.as_deref().map_or_else(|| "null".into(), GenerationAnchor::ir_json),
                expansion.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json)),
            Self::DomainCallLiteral { call, evaluation } => format!("{{\"kind\":\"domain_call_literal\",\"call\":{},\"evaluation\":{}}}",
                call.as_deref().map_or_else(|| "null".into(), SourceSite::json), evaluation.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json)),
            Self::StageConstantEvaluation { expression, operations } => format!("{{\"kind\":\"stage_constant_evaluation\",\"expression\":{},\"operations\":[{}]}}",
                expression.as_deref().map(SourceSite::json).unwrap_or_else(|| "null".into()), operations.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(",")),
            Self::DeclarationReference { declaration, retained } => format!("{{\"kind\":\"declaration_reference\",\"declaration\":{},\"retained\":{}}}",
                declaration.json(), retained.ir_json()),
            Self::ContractCondition { declaration, span, retained } => {
                let mut fields = vec![("kind", text("contract_condition")),
                    ("declaration", declaration.json()), ("retained", retained.ir_json())];
                fields.extend(span.fields()); object(&fields)
            },
            Self::UnaryConstantFold { expression, operation, scalar, input_bits, result_bits, component_index, component_count } => format!("{{\"kind\":\"unary_constant_fold\",\"expression\":{},\"operation\":{},\"scalar\":{},\"input_bits\":\"0x{:016x}\",\"result_bits\":\"0x{:016x}\",\"component_index_base\":0,\"component_index\":{},\"component_count\":{}}}",
                expression.as_deref().map_or_else(|| "null".into(), SourceSite::json), text(&format!("{operation:?}")), text(&format!("{scalar:?}")), input_bits, result_bits, component_index, component_count),
            Self::ModuleConstantEvaluation { definition, constant, operations } => format!("{{\"kind\":\"module_constant_evaluation\",\"definition\":{},\"constant\":{},\"operations\":[{}]}}",
                definition.as_deref().map_or_else(|| "null".into(), SourceOrigin::json), text(constant), operations.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(",")),
            Self::Definition { origin, instance } => format!("{{\"kind\":\"definition_instance\",\"definition\":{},\"instance\":{}}}",
                origin.as_deref().map_or_else(|| "null".into(), SourceOrigin::json), text(instance)),
            Self::IntegerDivisionPair { quotient, remainder } => format!("{{\"kind\":\"integer_division_inputs\",\"quotient\":{},\"remainder\":{}}}",
                quotient.ir_json(), remainder.ir_json()),
            Self::ExpressionReuse { retained, reused } => format!("{{\"kind\":\"expression_reuse\",\"retained\":{},\"reused\":[{}]}}",
                retained.ir_json(), reused.iter().map(ExpressionReuse::ir_json).collect::<Vec<_>>().join(",")),
            Self::LocalReadForward { retained, reads } => format!("{{\"kind\":\"local_read_forward\",\"result_index_base\":0,\"retained\":{},\"reads\":[{}]}}",
                retained.ir_json(), reads.iter().map(LocalReadForward::ir_json).collect::<Vec<_>>().join(",")),
            Self::IndexedStoreReadForward { retained, reads } => format!("{{\"kind\":\"indexed_store_read_forward\",\"result_index_base\":0,\"retained\":{},\"reads\":[{}]}}",
                retained.ir_json(), reads.iter().map(IndexedStoreReadForward::ir_json).collect::<Vec<_>>().join(",")),
            Self::IndexedReadReuse { retained, reused } => format!("{{\"kind\":\"indexed_read_reuse\",\"retained\":{},\"reused\":[{}]}}",
                retained.ir_json(), reused.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(",")),
            Self::LocalInputForward { retained, inputs } => format!("{{\"kind\":\"local_input_forward\",\"input_index_base\":0,\"retained\":{},\"inputs\":[{}]}}",
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json), inputs.iter().map(InputReadForward::ir_json).collect::<Vec<_>>().join(",")),
            Self::ReductionInputForward { retained, inputs } => format!("{{\"kind\":\"reduction_input_forward\",\"component_index_base\":0,\"retained\":{},\"inputs\":[{}]}}",
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json), inputs.iter().map(ReductionInputReadForward::ir_json).collect::<Vec<_>>().join(",")),
            Self::EliminatedOperations { retained, operations } => format!("{{\"kind\":\"eliminated_operations\",\"retained\":{},\"operations\":[{}]}}",
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json), operations.iter().map(EliminatedOperation::ir_json).collect::<Vec<_>>().join(",")),
            Self::EmptyMapRemoval { retained, stages } => format!("{{\"kind\":\"empty_map_removal\",\"retained\":{},\"stages\":[{}]}}",
                retained.as_deref().map_or_else(|| "null".into(), GeneratedOrigin::ir_json), stages.iter().map(OperationOrigin::ir_json).collect::<Vec<_>>().join(",")),
            Self::ReductionUniformExtract { retained, partition, operation, uniform_operand_count } => format!("{{\"kind\":\"reduction_uniform_extract\",\"retained\":{},\"partition\":{},\"operation\":{},\"uniform_operand_count\":{}}}",
                retained.ir_json(), text(partition.ir_name()), text(operation.ir_name()), uniform_operand_count),
            Self::IntegerDivisionResultPrune { retained, input_results, output_results } => format!("{{\"kind\":\"integer_division_result_prune\",\"retained\":{},\"input_results\":{},\"output_results\":{}}}",
                retained.ir_json(), division_results_json(input_results), division_results_json(output_results)),
        }
    }
}

#[derive(Clone, Debug)]
struct GeneratedOrigin {
    reason: String,
    trigger: Option<GenerationAnchor>,
}
record!(GeneratedOrigin { reason, trigger });
impl Encode for ir::GeneratedOrigin {
    fn encode(&self, writer: &mut Writer) { self.reason.ir_name().to_owned().encode(writer); self.trigger.encode(writer); }
}
impl GeneratedOrigin {
    fn ir_json(&self) -> String {
        object(&[("kind", text("generated_origin")), ("reason", text(&self.reason)),
            ("trigger", self.trigger.as_ref().map_or_else(|| "null".into(), GenerationAnchor::ir_json))])
    }
}
