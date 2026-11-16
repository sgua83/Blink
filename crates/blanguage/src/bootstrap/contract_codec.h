module bootstrap.contract_codec;
// Synchronous typed projections of the original contract values. They borrow
// actual fields only; no node registry, schema interpreter or copied type tree.
struct Bytes { address data; u64 count; };
struct Span { address data; u64 count; u64 stride; };
struct Component { u32 scalar; u32 word; };
struct Field { Bytes name; u32 word; address ty; };
struct Variant { Bytes name; u32 discriminant; address ty; };
struct Parameter { u32 access; address ty; };
struct Function { address result; Span parameters; Span contracts; };
struct Type {
 Bytes name; Span arguments; Span values; u32 size; u32 alignment;
 Span components; Span fields; address view; address array;
 u32 enumeration; u32 result; Span variants; address dataset;
 Bytes cleanup; u32 cleanup_present; address function;
};
struct TypeName {
 Bytes name; u32 canonical; Span arguments; u32 dataset; u32 view; u32 tile;
 address count; Span contracts; u32 contracts_present;
};
struct Argument { u32 kind; address value; };
struct Shape { u32 kind; u32 literal; Bytes name; address expression; };
struct Expression {
 u32 kind; u32 literal; Bytes name; address a; address b; address c;
 Span arguments; Span types; u32 types_present; address ty; u32 target;
 u64 start; u64 end;
};
struct Predicate {
 u32 kind; Bytes first; Bytes second; Bytes third; address condition;
 Span intervals; u32 access; u32 access_present;
};
struct Interval { address lower; address upper; };
struct Condition { u32 kind; u32 literal; Bytes name; address a; address b; address constant; };
struct Constant { address ty; Span bits; };
struct Host {
 cfn<void,address,address,u64> append;
 cfn<Type,address> ensures memory(read) ty;
 cfn<Component,address> ensures memory(read) component;
 cfn<Field,address> ensures memory(read) field;
 cfn<Variant,address> ensures memory(read) variant;
 cfn<Function,address> ensures memory(read) function;
 cfn<Parameter,address> ensures memory(read) parameter;
 cfn<TypeName,address> ensures memory(read) type_name;
 cfn<Argument,address> ensures memory(read) argument;
 cfn<Shape,address> ensures memory(read) shape;
 cfn<Expression,address> ensures memory(read) expression;
 cfn<Predicate,address> ensures memory(read) predicate;
 cfn<Interval,address> ensures memory(read) interval;
 cfn<Condition,address> ensures memory(read) condition;
 cfn<Constant,address> ensures memory(read) constant;
};
extern "C" void write_type(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant);

// The reader borrows the original input and exclusively updates the original
// Reader.position. Typed fields below are construction slots, not an IR/schema.
enum ReadStatus:u32 { Success=0; Overflow=1; Truncated=2; Boolean=3; Scalar=4; Access=5; Target=6; Shape=7; Argument=8; Expression=9; Predicate=10; Condition=11; RustError=12; };
enum ScalarTag:u32 { F32=0; U32=1; I32=2; Bool=3; U64=4; I64=5; F64=6; U8=7; I8=8; U16=9; I16=10; };
enum AccessTag:u32 { Value=0; Read=1; Write=2; InOut=3; Atomic=4; };
enum TargetTag:u32 { Compute=0; Cpu=1; Gpu=2; };
enum ArgumentTag:u32 { Type=0; Value=1; };
enum ExpressionTag:u32 { Number=0; StaticU32=1; Bool=2; Name=3; Member=4; Index=5; Call=6; Invoke=7; Construct=8; Specialize=9; FunctionRef=10; Cooperate=11; Target=12; Unary=13; Binary=14; Select=15; };
enum ConditionTag:u32 { Name=0; Number=1; Bool=2; Member=3; Unary=4; Binary=5; StaticU32=6; Constant=7; };
enum PredicateTag:u32 { Prefix=0; Nonzero=1; Nonempty=2; Contiguous=3; Memory=4; Index=5; Range=6; SameLength=7; Initialized=8; Cleanup=9; };
struct TypeFields {
 address name;address arguments;address values;address size;address alignment;
 address components;address fields;address view;address array;
 address enumeration;address result;address variants;address dataset;address cleanup;address function;
};
struct NameFields { address name;address canonical;address arguments;address dataset;address view;address tile;address count;address contracts; };
struct FunctionFields { address result;address parameters;address contracts; };
struct ConstantFields { address ty;address bits; };
struct ExpressionFields { address text;address a;address b;address c;address arguments;address types;address ty;address target;address literal; };
struct ConditionFields { address text;address a;address b;address literal;address constant; };
struct PredicateFields { address first;address second;address third;address condition;address intervals;address access; };
struct ReadHost {
 address context;
 cfn<TypeFields,address> ensures memory(read) type_fields;
 cfn<NameFields,address> ensures memory(read) name_fields;
 cfn<FunctionFields,address> ensures memory(read) function_fields;
 cfn<ConstantFields,address> ensures memory(read) constant_fields;
 cfn<ReadStatus,address,address,Bytes> text;
 cfn<ReadStatus,address,address,Bytes> string_option;
 cfn<void,address,u32> push_u32;
 cfn<void,address,u64> push_u64;
 cfn<void,address,ScalarTag,u32> push_component;
 cfn<ReadStatus,address,address> push_type;
 cfn<ReadStatus,address,address> push_field;
 cfn<ReadStatus,address,address> push_variant;
 cfn<ReadStatus,address,address> push_parameter;
 cfn<ReadStatus,address,address> push_interval;
 cfn<ReadStatus,address,address,ArgumentTag> push_argument;
 cfn<ReadStatus,address,address> push_expression;
 cfn<ReadStatus,address,address> push_predicate;
 cfn<ReadStatus,address,address> type_box;
 cfn<ReadStatus,address,address> type_name_box;
 cfn<ReadStatus,address,address> function_box;
 cfn<ReadStatus,address,address> shape_option;
 cfn<ReadStatus,address,address> predicates_option;
 cfn<ReadStatus,address,address> arguments_option;
 cfn<ReadStatus,address,address> shape_expression;
 cfn<void,address,u32> shape_literal;
 cfn<ReadStatus,address,address,Bytes> shape_name;
 cfn<ReadStatus,address,address,ExpressionTag> expression;
 cfn<ReadStatus,address,address,ConditionTag> condition;
 cfn<ReadStatus,address,address,PredicateTag> predicate;
 cfn<void,address,AccessTag> access;
 cfn<void,address,TargetTag> target;
 cfn<void,address,AccessTag> optional_access;
 cfn<void,address,u64,u64> expression_span;
};
extern "C" ReadStatus read_type(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_type_name(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_function(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_shape(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_expression(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_predicate(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_arguments(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_predicates(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_named_type(Bytes input,inout u64 position,address name,address word,address ty,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_parameter(Bytes input,inout u64 position,address access,address ty,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_interval(Bytes input,inout u64 position,address lower,address upper,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_expression_payload(ExpressionTag kind,ExpressionFields fields,Bytes input,inout u64 position,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_condition_payload(ConditionTag kind,ConditionFields fields,Bytes input,inout u64 position,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
extern "C" ReadStatus read_predicate_payload(PredicateTag kind,PredicateFields fields,Bytes input,inout u64 position,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);
