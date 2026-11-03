//! Read-only queries over the IR values held by their original compiler consumers.
use super::*;
use std::fmt::Display;

fn text(value: &str) -> String {
    let mut out = String::new();
    crate::json::string(&mut out, value);
    out
}
fn name(value: &impl std::fmt::Debug) -> String { text(&format!("{value:?}")) }
fn number(value: impl Display) -> String { value.to_string() }
fn array(values: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", values.into_iter().collect::<Vec<_>>().join(","))
}
fn numbers<T: Display>(values: &[T]) -> String { array(values.iter().map(number)) }
fn optional<T>(value: Option<&T>, f: impl FnOnce(&T) -> String) -> String {
    value.map(f).unwrap_or_else(|| "null".into())
}
fn node(kind: &str, fields: &[(&str, String)]) -> String {
    let mut out = format!("{{\"kind\":{}", text(kind));
    for (key, value) in fields {
        out.push(','); out.push_str(&text(key)); out.push(':'); out.push_str(value);
    }
    out.push('}'); out
}
fn prefix(value: &PrefixCount) -> String {
    node("prefix_count", &[("binding", number(value.binding)), ("word", number(value.word))])
}
fn relation(value: &PrefixRelation) -> String {
    node("prefix_relation", &[("domain", number(value.domain)), ("count", prefix(&value.count))])
}
fn alias(value: &MemoryAlias) -> String {
    match value {
        MemoryAlias::Local { base, len } => node("local", &[("base", number(base)), ("len", number(len))]),
        MemoryAlias::Data(binding) => node("data", &[("binding", number(binding))]),
    }
}
fn indexed(value: &IndexedSource) -> String {
    match value {
        IndexedSource::Data { binding, index } => node("data", &[("binding", number(binding)), ("index", number(index))]),
        IndexedSource::View { address, index, stride } => node("view", &[("address", numbers(address)), ("index", number(index)), ("stride", number(stride))]),
        IndexedSource::Pointer { address, index, stride, aliases } => node("pointer", &[("address", numbers(address)), ("index", number(index)), ("stride", number(stride)), ("aliases", array(aliases.iter().map(alias)))]),
        IndexedSource::Tile { tile, index } => node("tile", &[("tile", number(tile)), ("index", number(index))]),
    }
}
fn reference(value: &CallReference) -> String {
    let storage = match &value.storage {
        CallStorage::Local { base, len } => node("local", &[("base", number(base)), ("len", number(len))]),
        CallStorage::Indexed(source) => indexed(source),
    };
    node("reference", &[("storage", storage), ("offset", optional(value.offset.as_ref(), |v| number(v))), ("word", number(value.word))])
}
fn target(value: &CallTarget) -> String {
    match value {
        CallTarget::External(value) => node("external", &[("library", text(&value.library)), ("symbol", text(&value.symbol))]),
        CallTarget::Defined(value) => node("defined", &[("name", text(value))]),
        CallTarget::Indirect(value) => node("indirect", &[("register", number(value))]),
    }
}
fn parameter(value: &CallParameter) -> String {
    match value {
        CallParameter::Value(ty) => node("value", &[("type", ty.layout_json())]),
        CallParameter::Reference { ty, access } => node("reference", &[("type", ty.layout_json()), ("access", name(access))]),
    }
}
fn argument(value: &CallArgument) -> String {
    match value {
        CallArgument::Value { ty, values } => node("value", &[("type", ty.layout_json()), ("values", numbers(values))]),
        CallArgument::Reference { ty, access, address } => node("reference", &[("type", ty.layout_json()), ("access", name(access)), ("address", reference(address))]),
    }
}
fn call(value: &Call) -> String {
    node("Call", &[
        ("target", target(&value.target)), ("arguments", array(value.arguments.iter().map(argument))),
        ("memory", array(value.memory.iter().map(|value| node("memory_access", &[("alias", alias(&value.alias)), ("access", name(&value.access))])))),
        ("memory_use", optional(value.memory_use.as_ref(), |v| number(v))),
        ("result", optional(value.result.as_ref(), Ty::layout_json)), ("values", numbers(&value.values)),
    ])
}
fn texture(value: &TextureSource) -> String {
    node("texture", &[("binding", number(value.binding)), ("element", optional(value.element.as_ref(), |v| number(v)))])
}
fn atomic_op(value: &AtomicOp) -> String {
    match value {
        AtomicOp::Load => node("Load", &[]),
        AtomicOp::Store(r) => node("Store", &[("value", number(r))]),
        AtomicOp::Add(r) => node("Add", &[("value", number(r))]),
        AtomicOp::Sub(r) => node("Sub", &[("value", number(r))]),
        AtomicOp::Exchange(r) => node("Exchange", &[("value", number(r))]),
        AtomicOp::Min(r) => node("Min", &[("value", number(r))]),
        AtomicOp::Max(r) => node("Max", &[("value", number(r))]),
        AtomicOp::And(r) => node("And", &[("value", number(r))]),
        AtomicOp::Or(r) => node("Or", &[("value", number(r))]),
        AtomicOp::Xor(r) => node("Xor", &[("value", number(r))]),
        AtomicOp::CompareExchange { expected, desired, failure } => node("CompareExchange", &[("expected", number(expected)), ("desired", number(desired)), ("failure", name(failure))]),
    }
}
fn code(values: &[Inst]) -> String { array(values.iter().map(instruction)) }
fn instruction(value: &Inst) -> String {
    use Inst::*;
    match value {
        TextureSample { result, texture: source, coordinate, lod } => node("TextureSample", &[("result", numbers(result)), ("texture", texture(source)), ("coordinate", numbers(coordinate)), ("lod", number(lod))]),
        TextureSize { result, texture: source, level } => node("TextureSize", &[("result", numbers(result)), ("texture", texture(source)), ("level", number(level))]),
        TextureLevels { dst, texture: source } => node("TextureLevels", &[("dst", number(dst)), ("texture", texture(source))]),
        TextureAddress { dst, texture: source } => node("TextureAddress", &[("dst", number(dst)), ("texture", texture(source))]),
        TextureFormat { dst, texture: source } => node("TextureFormat", &[("dst", number(dst)), ("texture", texture(source))]),
        TextureRead { dst, texture: source, coordinate, level, byte } => node("TextureRead", &[("dst", number(dst)), ("texture", texture(source)), ("coordinate", numbers(coordinate)), ("level", number(level)), ("byte", number(byte))]),
        TextureFetch { result, texture: source, coordinate, level } => node("TextureFetch", &[("result", numbers(result)), ("texture", texture(source)), ("coordinate", numbers(coordinate)), ("level", number(level))]),
        LoadIndexed { dst, source, offset, word } => node("LoadIndexed", &[("dst", number(dst)), ("source", indexed(source)), ("offset", number(offset)), ("word", number(word))]),
        StoreIndexed { source, offset, word, value } => node("StoreIndexed", &[("source", indexed(source)), ("offset", number(offset)), ("word", number(word)), ("value", number(value))]),
        Atomic(value) => node("Atomic", &[("result", optional(value.result.as_ref(), |v| number(v))), ("scalar", name(&value.scalar)), ("target", indexed(&value.target)), ("word", number(value.word)), ("offset", optional(value.offset.as_ref(), |v| number(v))), ("operation", atomic_op(&value.op)), ("order", name(&value.order))]),
        Append(value) => node("Append", &[("accepted", number(value.accepted)), ("target", number(value.target)), ("count", prefix(&value.count)), ("values", word_values(&value.values))]),
        TileBarrier => node("TileBarrier", &[]),
        TilePublish(value) => node("TilePublish", &[("tile", number(value.tile)), ("values", word_values(&value.values))]),
        TileCollective(value) => node("TileCollective", &[("operator", name(&value.op)), ("tile", number(value.tile)), ("results", word_values(&value.results))]),
        TileLength(dst) => node("TileLength", &[("dst", number(dst))]),
        TileLane(dst) => node("TileLane", &[("dst", number(dst))]),
        TileLoad { dst, tile, index, word } => node("TileLoad", &[("dst", number(dst)), ("tile", number(tile)), ("index", number(index)), ("word", number(word))]),
        Subgroup { dst, op, value, argument } => node("Subgroup", &[("dst", number(dst)), ("operator", name(op)), ("value", optional(value.as_ref(), |v| number(v))), ("argument", optional(argument.as_ref(), |v| number(v)))]),
        Print { dst, value } => node("Print", &[("dst", number(dst)), ("value", number(value))]),
        Call(value) => call(value),
        ViewLoad { dst, address, index, stride, word } => node("ViewLoad", &[("dst", number(dst)), ("address", numbers(address)), ("index", number(index)), ("stride", number(stride)), ("word", number(word))]),
        RayQuery { scene, origin, direction, minimum, maximum, result } => node("RayQuery", &[("scene", number(scene)), ("origin", numbers(origin)), ("direction", numbers(direction)), ("minimum", number(minimum)), ("maximum", number(maximum)), ("result", match result { RayResult::Any(r) => node("Any", &[("register", number(r))]), RayResult::Closest(r) => node("Closest", &[("registers", numbers(r))]) })]),
        Constant(dst, scalar, bits) => node("Constant", &[("dst", number(dst)), ("scalar", name(scalar)), ("bits", text(&format!("0x{bits:016x}")))]),
        FunctionAddress(dst, value) => node("FunctionAddress", &[("dst", number(dst)), ("target", target(value))]),
        PositionAddress { dst, address } => node("PositionAddress", &[("dst", number(dst)), ("address", reference(address))]),
        Index(dst) => node("Index", &[("dst", number(dst))]),
        Uniform(dst, offset) => node("Uniform", &[("dst", number(dst)), ("offset", number(offset))]),
        LoadLocal(dst, slot) => node("LoadLocal", &[("dst", number(dst)), ("slot", number(slot))]),
        StoreLocal(slot, value) => node("StoreLocal", &[("slot", number(slot)), ("value", number(value))]),
        LoadLocalIndexed { dst, base, len, index, word } => node("LoadLocalIndexed", &[("dst", number(dst)), ("base", number(base)), ("len", number(len)), ("index", number(index)), ("word", number(word))]),
        StoreLocalIndexed { base, len, index, word, value } => node("StoreLocalIndexed", &[("base", number(base)), ("len", number(len)), ("index", number(index)), ("word", number(word)), ("value", number(value))]),
        Load { dst, binding, index, word } => node("Load", &[("dst", number(dst)), ("binding", number(binding)), ("index", number(index)), ("word", number(word))]),
        Store { binding, index, word, value } => node("Store", &[("binding", number(binding)), ("index", number(index)), ("word", number(word)), ("value", number(value))]),
        Unary { dst, op, a } => node("Unary", &[("dst", number(dst)), ("operator", name(op)), ("a", number(a))]),
        Binary { dst, op, a, b } => node("Binary", &[("dst", number(dst)), ("operator", match op { BinaryOp::Compare(value) => node("Compare", &[("predicate", name(value))]), value => node(&format!("{value:?}"), &[]) }), ("a", number(a)), ("b", number(b))]),
        IntegerDivide { result, src, divisor } => node("IntegerDivide", &[("result", match result {
            IntegerDivisionResult::Quotient(r) => node("Quotient", &[("dst", number(r))]),
            IntegerDivisionResult::Remainder(r) => node("Remainder", &[("dst", number(r))]),
            IntegerDivisionResult::Both { quotient, remainder } => node("Both", &[("quotient", number(quotient)), ("remainder", number(remainder))]),
        }), ("src", number(src)), ("divisor", match divisor { IntegerDivisor::Constant(value) => node("constant", &[("bits", text(&format!("0x{:016x}", value.get())))]), IntegerDivisor::Value(value) => node("register", &[("register", number(value))]) })]),
        Cast { dst, src } => node("Cast", &[("dst", number(dst)), ("src", number(src))]),
        Math { dst, op, args } => node("Math", &[("dst", number(dst)), ("operator", name(op)), ("args", numbers(args))]),
        Select { dst, condition, a, b } => node("Select", &[("dst", number(dst)), ("condition", number(condition)), ("a", number(a)), ("b", number(b))]),
        If { condition, yes, no } => node("If", &[("condition", number(condition)), ("yes", code(yes)), ("no", code(no))]),
        Loop { condition, test, body } => node("Loop", &[("condition", code(condition)), ("test", number(test)), ("body", code(body))]),
        Break => node("Break", &[]), Continue => node("Continue", &[]),
        Scope(body) => node("Scope", &[("body", code(body))]), Exit => node("Exit", &[]),
    }
}
fn word_values(values: &[(u32, Reg)]) -> String {
    array(values.iter().map(|(word, value)| node("component", &[("word", number(word)), ("register", number(value))])))
}
fn locals(values: &BTreeMap<usize, Ty>) -> String {
    array(values.iter().map(|(end, ty)| node("local", &[("end_component", number(end)), ("type", ty.layout_json())])))
}
impl ValueCode {
    /// Serializes this value; its construction/optimization phase belongs to the caller.
    pub fn ir_json(&self) -> String {
        node("value_ir", &[("registers", array(self.registers.iter().map(name))), ("locals", locals(&self.locals)), ("inputs", numbers(&self.inputs)), ("outputs", numbers(&self.outputs)), ("code", code(&self.code))])
    }
}
impl FunctionBody {
    pub fn ir_json(&self) -> String {
        node("function_ir", &[("name", text(&self.name)), ("parameters", array(self.parameters.iter().map(parameter))), ("result", optional(self.result.as_ref(), Ty::layout_json)), ("body", self.body.ir_json())])
    }
}
fn reduction(value: &Reduction) -> String {
    node("reduction", &[("source", number(value.source)), ("source_word", number(value.source_word)), ("target", number(value.target)), ("target_word", number(value.target_word))])
}
fn reduction_spec(value: &ReductionSpec) -> String {
    node("reduction_spec", &[("fields", reduction(&value.fields)), ("type", value.ty.layout_json()), ("operator", name(&value.operator))])
}
fn custom(value: &CustomReduction) -> String {
    node("custom_reduction", &[("fields", reduction(&value.fields)), ("source_type", value.source_ty.layout_json()), ("type", value.ty.layout_json()), ("project", optional(value.project.as_ref(), ValueCode::ir_json)), ("identity", value.identity.ir_json()), ("combine", value.combine.ir_json())])
}
fn operation(value: &Operation) -> String {
    match value {
        Operation::Map => node("Map", &[]),
        Operation::Reduce(value) => node("Reduce", &[("values", reduction_spec(value))]),
        Operation::CustomReduce(value) => node("CustomReduce", &[("values", custom(value))]),
        Operation::CustomScan(value) => node("CustomScan", &[("values", custom(&value.values)), ("head_word", optional(value.head_word.as_ref(), |v| number(v)))]),
        Operation::CustomSegmentedReduce(value) => node("CustomSegmentedReduce", &[("values", custom(&value.values)), ("head_word", number(value.head_word)), ("count", prefix(&value.count))]),
        Operation::SegmentedReduce(value) => node("SegmentedReduce", &[("values", reduction_spec(&value.values)), ("head_word", number(value.head_word)), ("count", prefix(&value.count))]),
        Operation::ScanExclusive(value) => node("ScanExclusive", &[("values", reduction(&value.values)), ("type", value.ty.layout_json()), ("operator", name(&value.operator)), ("head_word", optional(value.head_word.as_ref(), |v| number(v)))]),
        Operation::Compact(value) => node("Compact", &[("source", number(value.source)), ("flags", number(value.flags)), ("flag_word", number(value.flag_word)), ("target", number(value.target)), ("count", number(value.count)), ("count_word", number(value.count_word))]),
        Operation::StableSort(value) => node("StableSort", &[("source", number(value.source)), ("target", number(value.target)), ("keys", array(value.keys.iter().map(|(word, scalar, order)| node("key", &[("word", number(word)), ("scalar", name(scalar)), ("order", name(order))]))))]),
    }
}
impl Stage {
    pub fn ir_json(&self) -> String {
        node("stage_ir", &[
            ("domain", number(self.domain)), ("prefix", optional(self.prefix.as_ref(), prefix)),
            ("selection", optional(self.selection.as_ref(), |value| node("selection", &[("binding", number(value.binding)), ("word", number(value.word))]))),
            ("operation", operation(&self.operation)), ("boundary_before", number(self.boundary_before)),
            ("registers", array(self.registers.iter().map(name))), ("locals", locals(&self.locals)), ("code", code(&self.code)),
            ("reads", numbers(&self.reads)), ("writes", numbers(&self.writes)), ("cross_access", numbers(&self.cross_access)),
            ("tiles", array(self.tiles.iter().map(|value| node("tile", &[("source", match value.source { TileSource::Input { binding, word } => node("input", &[("binding", number(binding)), ("word", number(word))]), TileSource::Published => node("published", &[]) }), ("type", value.ty.layout_json()), ("width", number(value.width))])))),
        ])
    }
}
#[cfg(feature = "compiler")]
impl Program {
    /// Borrows the actual compiler result. No optimization, lowering or codegen is performed.
    pub fn ir_json(&self) -> String {
        node("program_ir", &[
            ("interface", interface_layout_json(self)), ("source_stages", number(self.source_stages)),
            ("accelerations", array(self.accelerations.iter().map(|v| text(v)))),
            ("textures", array(self.textures.iter().map(|v| node("texture_binding", &[("name", text(&v.name)), ("count", optional(v.count.as_ref(), |v| number(v)))])))),
            ("prefix_requirements", array(self.prefix_requirements.iter().map(relation))),
            ("prefix_guarantees", array(self.prefix_guarantees.iter().map(relation))),
            ("length_requirements", array(self.length_requirements.iter().map(|(left, right)| node("same_length", &[("left", number(left)), ("right", number(right))])))),
            ("stages", array(self.stages.iter().map(Stage::ir_json))),
        ])
    }
}
#[cfg(feature = "compiler")]
fn resources(value: Option<&crate::application::Resources>) -> String {
    optional(value, |value| node("resources", &[
        ("types", array(value.types.iter().map(Ty::layout_json))),
        ("programs", array(value.programs.iter().map(|(target, program)| node("resource_program", &[("target", name(target)), ("program", program.ir_json())])))),
    ]))
}
#[cfg(feature = "compiler")]
impl crate::application::Application {
    pub fn ir_json(&self) -> String {
        node("application_ir", &[("name", text(&self.name)), ("body", self.body.ir_json()), ("functions", array(self.functions.iter().map(FunctionBody::ir_json))), ("imports", array(self.imports.keys().map(|v| text(v)))), ("resources", resources(self.resources.as_ref()))])
    }
    pub fn function_ir_json(&self, name: &str) -> Option<String> {
        self.functions.iter().find(|function| function.name == name).map(FunctionBody::ir_json)
    }
}
#[cfg(feature = "compiler")]
impl crate::module::Module {
    pub fn ir_json(&self) -> String {
        node("module_ir", &[("name", text(&self.name)), ("functions", array(self.functions.iter().map(FunctionBody::ir_json))), ("imports", array(self.imports.keys().map(|v| text(v)))), ("exports", array(self.exports.keys().map(|v| text(v)))), ("resources", resources(self.resources.as_ref()))])
    }
    pub fn function_ir_json(&self, name: &str) -> Option<String> {
        self.functions.iter().find(|function| function.name == name).map(FunctionBody::ir_json)
    }
}
