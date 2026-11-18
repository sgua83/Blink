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
enum ReadStatus:u32 { Success=0; Overflow=1; Truncated=2; Boolean=3; Scalar=4; Access=5; Target=6; Shape=7; Argument=8; Expression=9; Predicate=10; Condition=11; RustError=12; Identity=13; Operand=14; IntegerOperation=15; IntegerExpression=16; Comparison=17; FloatingForm=18; IndexDomain=19; Fact=20; ConditionVersion=21; ConditionNode=22; ContractAddressRoot=23; ContractInitializationSource=24; ContractElementFacts=25; ContractDataLayout=26; ContractReturned=27; ContractTaskProjection=28; ContractExclusion=29; ContractViewAccess=30; ContractResidencyPart=31; ContractResidencyOrigin=32; ContractResidency=33; ContractResidencyOutputKind=34; ContractTextureDimension=35; ContractTextureFormat=36; ContractElementFact=37; ContractCallParameterKind=38; ContractContractKind=39; ContractRelationOp=40; ContractOwnership=41; ContractAddressIdentity=42; ContractBorrowSource=43; ResourceSortOrder=44; ResourceCollectiveOperator=45; ResourceOperation=46; };
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

// Original numerical identities/domains/facts; fixed synchronous C ABI values.
enum IdentityKind:u32 { Register=0; DataLength=1; TileLength=2; ParameterContent=3; ResourceParameter=4; ParameterStorage=5; ElementComponent=6; ElementParent=7; ElementIndex=8; DatasetContent=9; MemoryValue=10; LoopValue=11; TaskResult=12; TaskCompletion=13; TaskWorkCompletion=14; TaskControl=15; TaskReturnSource=16; };
enum OperandKind:u32 { Value=0; Constant=1; };
enum IntegerKind:u32 { Add=0; Subtract=1; Multiply=2; Divide=3; Remainder=4; And=5; Or=6; Xor=7; ShiftLeft=8; ShiftRight=9; };
enum ComparisonKind:u32 { Equal=0; NotEqual=1; Less=2; Greater=3; LessEqual=4; GreaterEqual=5; };
enum FloatingKind:u32 { Cast=0; Upper=1; };
enum DomainKind:u32 { Data=0; Fixed=1; Tile=2; DataLength=3; TileLength=4; View=5; ViewLength=6; ApplicationData=7; ApplicationLength=8; Upper=9; Lower=10; Room=11; Sum=12; Offset=13; Shifted=14; Computed=15; CommonRange=16; KnownBits=17; Floating=18; };
enum FactKind:u32 { Equal=0; Index=1; Minimum=2; Maximum=3; NonZero=4; Boolean=5; Below=6; Outside=7; Zero=8; NotEqual=9; Predicate=10; };
struct NumericIdentity { IdentityKind kind; u64 first; u64 second; u64 third; };
struct NumericOperand { OperandKind kind; NumericIdentity identity; ScalarTag scalar; u64 bits; };
struct NumericExpression { IntegerKind operation; ScalarTag scalar; NumericOperand left; NumericOperand right; };
struct NumericFloating { NumericIdentity source; u64 lower; u64 upper; array<u64,3> terms; ScalarTag scalar; FloatingKind form; };
struct NumericPredicate { ComparisonKind operation; ScalarTag scalar; NumericOperand left; NumericOperand right; };
struct NumericInterval { u64 lower; u64 upper; };
struct RangeWriter { cfn<void,address,address,u64> append; cfn<NumericInterval,address> ensures memory(read) interval; };
extern "C" void numeric_write_identity(NumericIdentity value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus numeric_read_identity(Bytes input,inout u64 position,inout NumericIdentity output) on cpu;
extern "C" void numeric_write_operand(NumericOperand value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus numeric_read_operand(Bytes input,inout u64 position,inout NumericOperand output) on cpu;
extern "C" void numeric_write_integer(IntegerKind value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus numeric_read_integer(Bytes input,inout u64 position,inout IntegerKind output) on cpu;
extern "C" void numeric_write_expression(NumericExpression value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus numeric_read_expression(Bytes input,inout u64 position,inout NumericExpression output) on cpu;
extern "C" void numeric_write_comparison(ComparisonKind value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus numeric_read_comparison(Bytes input,inout u64 position,inout ComparisonKind output) on cpu;
extern "C" void numeric_write_floating_form(FloatingKind value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus numeric_read_floating_form(Bytes input,inout u64 position,inout FloatingKind output) on cpu;
extern "C" void numeric_write_floating(NumericFloating value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus numeric_read_floating(Bytes input,inout u64 position,inout NumericFloating output) on cpu;
extern "C" void numeric_write_predicate(NumericPredicate value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus numeric_read_predicate(Bytes input,inout u64 position,inout NumericPredicate output) on cpu;
extern "C" void numeric_write_ranges(Span values,address output,RangeWriter host) on cpu requires nonzero(host.append) requires nonzero(host.interval);
extern "C" ReadStatus numeric_read_ranges(Bytes input,inout u64 position,address output,cfn<void,address,u64,u64> push) on cpu requires nonzero(push);

struct DomainFields {DomainKind kind; address first; address second; address third; u64 ordinal; u32 distance; array<ScalarTag,4> scalars; u64 mask; u64 bits; address expression; address floating;};
struct FactFields {FactKind kind; address first; address second; address domain; ScalarTag scalar; u64 bits; u32 truth; address predicate;};
struct NumericWriter {
 cfn<void,address,address,u64> append;
 cfn<NumericIdentity,address> ensures memory(read) identity;
 cfn<NumericExpression,address> ensures memory(read) expression;
 cfn<NumericFloating,address> ensures memory(read) floating;
 cfn<NumericPredicate,address> ensures memory(read) predicate;
 cfn<DomainFields,address> ensures memory(read) domain;
 cfn<FactFields,address> ensures memory(read) fact;
};
struct NumericReader {address context;cfn<ReadStatus,address,address,DomainKind> domain;cfn<ReadStatus,address,address,FactKind> fact;};
extern "C" void numeric_write_domain(address source,address output,NumericWriter host) on cpu requires nonzero(host.append) requires nonzero(host.identity) requires nonzero(host.expression) requires nonzero(host.floating) requires nonzero(host.domain);
extern "C" ReadStatus numeric_read_domain(Bytes input,inout u64 position,address output,NumericReader host) on cpu requires nonzero(host.domain);
extern "C" void numeric_write_fact(address source,address output,NumericWriter host) on cpu requires nonzero(host.append) requires nonzero(host.identity) requires nonzero(host.expression) requires nonzero(host.floating) requires nonzero(host.domain) requires nonzero(host.fact) requires nonzero(host.predicate);
extern "C" ReadStatus numeric_read_fact(Bytes input,inout u64 position,address output,NumericReader host) on cpu requires nonzero(host.fact);
extern "C" ReadStatus numeric_read_payload_ordinal(Bytes input,inout u64 position,inout u64 field0) on cpu;
extern "C" ReadStatus numeric_read_payload_count(Bytes input,inout u64 position,inout u32 field0) on cpu;
extern "C" ReadStatus numeric_read_payload_view_identities(Bytes input,inout u64 position,inout array<NumericIdentity,3> field0) on cpu;
extern "C" ReadStatus numeric_read_payload_identity_distance(Bytes input,inout u64 position,inout NumericIdentity field0,inout u32 field1) on cpu;
extern "C" ReadStatus numeric_read_payload_identity_pair(Bytes input,inout u64 position,inout NumericIdentity field0,inout NumericIdentity field1) on cpu;
extern "C" ReadStatus numeric_read_payload_shifted_identities(Bytes input,inout u64 position,inout NumericIdentity field0,inout NumericIdentity field1,inout u32 field2) on cpu;
extern "C" ReadStatus numeric_read_payload_common_range(Bytes input,inout u64 position,inout NumericIdentity field0,inout ScalarTag field1,inout ScalarTag field2,inout ScalarTag field3,inout ScalarTag field4) on cpu;
extern "C" ReadStatus numeric_read_payload_known_bits(Bytes input,inout u64 position,inout ScalarTag field0,inout u64 field1,inout u64 field2) on cpu;
extern "C" ReadStatus numeric_read_payload_identity_domain(Bytes input,inout u64 position,inout NumericIdentity field0,address field1,NumericReader host) on cpu requires nonzero(host.domain);
extern "C" ReadStatus numeric_read_payload_identity_scalar_bits(Bytes input,inout u64 position,inout NumericIdentity field0,inout ScalarTag field1,inout u64 field2) on cpu;
extern "C" ReadStatus numeric_read_payload_identity_truth(Bytes input,inout u64 position,inout NumericIdentity field0,inout u32 field1) on cpu;
extern "C" ReadStatus numeric_read_payload_ordered_identities(Bytes input,inout u64 position,inout NumericIdentity field0,inout NumericIdentity field1,inout u32 field2,inout ScalarTag field3) on cpu;
extern "C" ReadStatus numeric_read_payload_predicate_truth(Bytes input,inout u64 position,inout NumericPredicate field0,inout u32 field1) on cpu;

// Shared canonical condition identities and the original Proof.conditions.
enum NodeKind:u32 { Impossible=0; Facts=1; Both=2; Either=3; };
enum ClauseKind:u32 { Impossible=0; Facts=1; Both=2; Either=3; Branch=4; };
struct ClauseFields {u32 present;ClauseKind kind;u64 key;address facts;u64 first;u64 second;};
struct NodeFields {NodeKind kind;address facts;u64 first;u64 second;};
struct RootFields {u32 present;u64 first;u64 second;};
struct Inserted {u64 index;u32 inserted;};
struct CanonicalHost {
 cfn<ClauseFields,address> next;
 cfn<u64,address,u64> ensures memory(read) index;
 cfn<u64,address> ensures memory(read) len;
 cfn<Inserted,address,NodeKind,address,u64,u64,u64> intern;
 cfn<void,address,NodeKind,address,u64,u64> record;
 cfn<void,address,u64,u64> bind;
};
extern "C" void condition_node_ids(address context,CanonicalHost host) on cpu requires nonzero(host.next) requires nonzero(host.index) requires nonzero(host.len) requires nonzero(host.intern) requires nonzero(host.record) requires nonzero(host.bind);
struct ConditionWriter {
 cfn<void,address,address,u64> append;
 cfn<RootFields,address,u64> ensures memory(read) condition;
 cfn<NodeFields,address,u64> ensures memory(read) node;
 cfn<u64,address,u64> ensures memory(read) index;
 cfn<void,address,address> facts;
};
extern "C" void write_proof_conditions(address context,u64 length,u64 count,address output,ConditionWriter host) on cpu requires nonzero(host.append) requires nonzero(host.condition) requires nonzero(host.node) requires nonzero(host.index) requires nonzero(host.facts);
extern "C" void write_fact_sequence(u64 count,address iterator,address output,NumericWriter host,cfn<address,address> next) on cpu requires nonzero(host.append) requires nonzero(host.identity) requires nonzero(host.expression) requires nonzero(host.floating) requires nonzero(host.domain) requires nonzero(host.fact) requires nonzero(host.predicate) requires nonzero(next);
struct ConditionReader {
 address context;
 cfn<void,address> impossible;
 cfn<ReadStatus,address,address> facts;
 cfn<void,address,u64,u64> both;
 cfn<void,address,u64,u64> either;
};
extern "C" ReadStatus read_condition_nodes(Bytes input,inout u64 position,inout u32 present,address output,ConditionReader host) on cpu requires nonzero(host.impossible) requires nonzero(host.facts) requires nonzero(host.both) requires nonzero(host.either);
extern "C" ReadStatus read_fact_sequence(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address> push) on cpu requires nonzero(push);
extern "C" ReadStatus read_condition_roots(Bytes input,inout u64 position,address output,cfn<void,address,u32,u64,u64> push) on cpu requires nonzero(push);

struct ContractValueFields {address proof;address literal;address ty;address regs;address uniform;address constant;};
struct ContractValueWriter {cfn<void,address,address> proof;cfn<void,address,address> literal;cfn<void,address,address> ty;cfn<void,address,address> regs;cfn<void,address,address> uniform;cfn<void,address,address> constant;};
struct ContractValueReader {cfn<ReadStatus,address,address> proof;cfn<ReadStatus,address,address> literal;cfn<ReadStatus,address,address> ty;cfn<ReadStatus,address,address> regs;cfn<ReadStatus,address,address> uniform;cfn<ReadStatus,address,address> constant;};

struct ContractAddressLayoutFields {address size;address align;address parts;};
struct ContractAddressLayoutWriter {cfn<void,address,address> size;cfn<void,address,address> align;cfn<void,address,address> parts;};
struct ContractAddressLayoutReader {cfn<ReadStatus,address,address> size;cfn<ReadStatus,address,address> align;cfn<ReadStatus,address,address> parts;};

struct ContractAddressRootLocalFields {address base;address len;};
struct ContractAddressRootLocalWriter {cfn<void,address,address> base;cfn<void,address,address> len;};
struct ContractAddressRootLocalReader {cfn<ReadStatus,address,address> base;cfn<ReadStatus,address,address> len;};

struct ContractAddressRootParameterFields {address index;};
struct ContractAddressRootParameterWriter {cfn<void,address,address> index;};
struct ContractAddressRootParameterReader {cfn<ReadStatus,address,address> index;};

struct ContractAddressRootResourceFields {address borrow;address index;};
struct ContractAddressRootResourceWriter {cfn<void,address,address> borrow;cfn<void,address,address> index;};
struct ContractAddressRootResourceReader {cfn<ReadStatus,address,address> borrow;cfn<ReadStatus,address,address> index;};

struct ContractAddressRootBufferFields {address binding;};
struct ContractAddressRootBufferWriter {cfn<void,address,address> binding;};
struct ContractAddressRootBufferReader {cfn<ReadStatus,address,address> binding;};

struct ContractAddressRootDerivedFields {address borrow;};
struct ContractAddressRootDerivedWriter {cfn<void,address,address> borrow;};
struct ContractAddressRootDerivedReader {cfn<ReadStatus,address,address> borrow;};

struct ContractAddressRootExternalFields {address identity;};
struct ContractAddressRootExternalWriter {cfn<void,address,address> identity;};
struct ContractAddressRootExternalReader {cfn<ReadStatus,address,address> identity;};

struct ContractAddressRootDatasetFields {address borrow;address index;};
struct ContractAddressRootDatasetWriter {cfn<void,address,address> borrow;cfn<void,address,address> index;};
struct ContractAddressRootDatasetReader {cfn<ReadStatus,address,address> borrow;cfn<ReadStatus,address,address> index;};

struct ContractInitializationSourcePositionFields {address origin;};
struct ContractInitializationSourcePositionWriter {cfn<void,address,address> origin;};
struct ContractInitializationSourcePositionReader {cfn<ReadStatus,address,address> origin;};

struct ContractInitializationSourceTaskFields {address borrow;};
struct ContractInitializationSourceTaskWriter {cfn<void,address,address> borrow;};
struct ContractInitializationSourceTaskReader {cfn<ReadStatus,address,address> borrow;};

struct ContractInitializationFields {address source;address when;};
struct ContractInitializationWriter {cfn<void,address,address> source;cfn<void,address,address> when;};
struct ContractInitializationReader {cfn<ReadStatus,address,address> source;cfn<ReadStatus,address,address> when;};

struct ContractDataPayloadFields {address root;address ty;address length;address ownership;address access;address count;address residency;address layout;address elements;address prefixes;};
struct ContractDataPayloadWriter {cfn<void,address,address> root;cfn<void,address,address> ty;cfn<void,address,address> length;cfn<void,address,address> ownership;cfn<void,address,address> access;cfn<void,address,address> count;cfn<void,address,address> residency;cfn<void,address,address> layout;cfn<void,address,address> elements;cfn<void,address,address> prefixes;};
struct ContractDataPayloadReader {cfn<ReadStatus,address,address> root;cfn<ReadStatus,address,address> ty;cfn<ReadStatus,address,address> length;cfn<ReadStatus,address,address> ownership;cfn<ReadStatus,address,address> access;cfn<ReadStatus,address,address> count;cfn<ReadStatus,address,address> residency;cfn<ReadStatus,address,address> layout;cfn<ReadStatus,address,address> elements;cfn<ReadStatus,address,address> prefixes;};

struct ContractElementFactsKnownFields {address value;};
struct ContractElementFactsKnownWriter {cfn<void,address,address> value;};
struct ContractElementFactsKnownReader {cfn<ReadStatus,address,address> value;};

struct ContractElementFactsCleanupFields {address when;};
struct ContractElementFactsCleanupWriter {cfn<void,address,address> when;};
struct ContractElementFactsCleanupReader {cfn<ReadStatus,address,address> when;};

struct ContractElementFactsCommonFields {address a;address b;};
struct ContractElementFactsCommonWriter {cfn<void,address,address> a;cfn<void,address,address> b;};
struct ContractElementFactsCommonReader {cfn<ReadStatus,address,address> a;cfn<ReadStatus,address,address> b;};

struct ContractElementFactsSelectedFields {address when;address a;address b;};
struct ContractElementFactsSelectedWriter {cfn<void,address,address> when;cfn<void,address,address> a;cfn<void,address,address> b;};
struct ContractElementFactsSelectedReader {cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> a;cfn<ReadStatus,address,address> b;};

struct ContractElementFactsInputFields {address when;};
struct ContractElementFactsInputWriter {cfn<void,address,address> when;};
struct ContractElementFactsInputReader {cfn<ReadStatus,address,address> when;};

struct ContractDataLayoutParameterFields {address parameter;address stride;address offset;};
struct ContractDataLayoutParameterWriter {cfn<void,address,address> parameter;cfn<void,address,address> stride;cfn<void,address,address> offset;};
struct ContractDataLayoutParameterReader {cfn<ReadStatus,address,address> parameter;cfn<ReadStatus,address,address> stride;cfn<ReadStatus,address,address> offset;};

struct ContractReturnedValueFields {address value;};
struct ContractReturnedValueWriter {cfn<void,address,address> value;};
struct ContractReturnedValueReader {cfn<ReadStatus,address,address> value;};

struct ContractReturnedDatasetFields {address data;};
struct ContractReturnedDatasetWriter {cfn<void,address,address> data;};
struct ContractReturnedDatasetReader {cfn<ReadStatus,address,address> data;};

struct ContractReturnedOwnedValueFields {address value;};
struct ContractReturnedOwnedValueWriter {cfn<void,address,address> value;};
struct ContractReturnedOwnedValueReader {cfn<ReadStatus,address,address> value;};

struct ContractOwnedValuePayloadFields {address value;address payloads;address ownership;};
struct ContractOwnedValuePayloadWriter {cfn<void,address,address> value;cfn<void,address,address> payloads;cfn<void,address,address> ownership;};
struct ContractOwnedValuePayloadReader {cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> payloads;cfn<ReadStatus,address,address> ownership;};

struct ContractTaskGuardFields {address target;address ty;address clauses;address result;address model;address arguments;address returned;address inputs;};
struct ContractTaskGuardWriter {cfn<void,address,address> target;cfn<void,address,address> ty;cfn<void,address,address> clauses;cfn<void,address,address> result;cfn<void,address,address> model;cfn<void,address,address> arguments;cfn<void,address,address> returned;cfn<void,address,address> inputs;};
struct ContractTaskGuardReader {cfn<ReadStatus,address,address> target;cfn<ReadStatus,address,address> ty;cfn<ReadStatus,address,address> clauses;cfn<ReadStatus,address,address> result;cfn<ReadStatus,address,address> model;cfn<ReadStatus,address,address> arguments;cfn<ReadStatus,address,address> returned;cfn<ReadStatus,address,address> inputs;};

struct ContractTaskCompletionFields {address offset;address result;address layout;address task;address parent;};
struct ContractTaskCompletionWriter {cfn<void,address,address> offset;cfn<void,address,address> result;cfn<void,address,address> layout;cfn<void,address,address> task;cfn<void,address,address> parent;};
struct ContractTaskCompletionReader {cfn<ReadStatus,address,address> offset;cfn<ReadStatus,address,address> result;cfn<ReadStatus,address,address> layout;cfn<ReadStatus,address,address> task;cfn<ReadStatus,address,address> parent;};

struct ContractTaskProjectionReturnedFields {address component;address source;};
struct ContractTaskProjectionReturnedWriter {cfn<void,address,address> component;cfn<void,address,address> source;};
struct ContractTaskProjectionReturnedReader {cfn<ReadStatus,address,address> component;cfn<ReadStatus,address,address> source;};

struct ContractTaskProjectionInitializedFields {address returned;};
struct ContractTaskProjectionInitializedWriter {cfn<void,address,address> returned;};
struct ContractTaskProjectionInitializedReader {cfn<ReadStatus,address,address> returned;};

struct ContractExclusionPositionFields {address access;};
struct ContractExclusionPositionWriter {cfn<void,address,address> access;};
struct ContractExclusionPositionReader {cfn<ReadStatus,address,address> access;};

struct ContractPositionAccessFields {address layout;address written;address writable;address initialized;};
struct ContractPositionAccessWriter {cfn<void,address,address> layout;cfn<void,address,address> written;cfn<void,address,address> writable;cfn<void,address,address> initialized;};
struct ContractPositionAccessReader {cfn<ReadStatus,address,address> layout;cfn<ReadStatus,address,address> written;cfn<ReadStatus,address,address> writable;cfn<ReadStatus,address,address> initialized;};

struct ContractViewAccessResourceFields {address resources;address borrows;address kind;address when;};
struct ContractViewAccessResourceWriter {cfn<void,address,address> resources;cfn<void,address,address> borrows;cfn<void,address,address> kind;cfn<void,address,address> when;};
struct ContractViewAccessResourceReader {cfn<ReadStatus,address,address> resources;cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> kind;cfn<ReadStatus,address,address> when;};

struct ContractViewAccessPositionFields {address borrows;address when;address position;};
struct ContractViewAccessPositionWriter {cfn<void,address,address> borrows;cfn<void,address,address> when;cfn<void,address,address> position;};
struct ContractViewAccessPositionReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> position;};

struct ContractViewAccessMemoryFields {address borrows;address when;address access;};
struct ContractViewAccessMemoryWriter {cfn<void,address,address> borrows;cfn<void,address,address> when;cfn<void,address,address> access;};
struct ContractViewAccessMemoryReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> access;};

struct ContractViewAccessDatasetFields {address borrows;address when;address index;address access;address atomic;};
struct ContractViewAccessDatasetWriter {cfn<void,address,address> borrows;cfn<void,address,address> when;cfn<void,address,address> index;cfn<void,address,address> access;cfn<void,address,address> atomic;};
struct ContractViewAccessDatasetReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> index;cfn<ReadStatus,address,address> access;cfn<ReadStatus,address,address> atomic;};

struct ContractViewAccessInitializedFields {address borrows;address initialized;address when;};
struct ContractViewAccessInitializedWriter {cfn<void,address,address> borrows;cfn<void,address,address> initialized;cfn<void,address,address> when;};
struct ContractViewAccessInitializedReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> initialized;cfn<ReadStatus,address,address> when;};

struct ContractViewAccessStoredFields {address borrows;address word;address value;address when;};
struct ContractViewAccessStoredWriter {cfn<void,address,address> borrows;cfn<void,address,address> word;cfn<void,address,address> value;cfn<void,address,address> when;};
struct ContractViewAccessStoredReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> word;cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> when;};

struct ContractResidencyPartFieldFields {address field;};
struct ContractResidencyPartFieldWriter {cfn<void,address,address> field;};
struct ContractResidencyPartFieldReader {cfn<ReadStatus,address,address> field;};

struct ContractResidencyPartSelectedFields {address index;};
struct ContractResidencyPartSelectedWriter {cfn<void,address,address> index;};
struct ContractResidencyPartSelectedReader {cfn<ReadStatus,address,address> index;};

struct ContractResidencyInputFields {address parameter;address fields;};
struct ContractResidencyInputWriter {cfn<void,address,address> parameter;cfn<void,address,address> fields;};
struct ContractResidencyInputReader {cfn<ReadStatus,address,address> parameter;cfn<ReadStatus,address,address> fields;};

struct ContractResidencyOriginInputFields {address input;};
struct ContractResidencyOriginInputWriter {cfn<void,address,address> input;};
struct ContractResidencyOriginInputReader {cfn<ReadStatus,address,address> input;};

struct ContractResidencyOriginOutputFields {address output;};
struct ContractResidencyOriginOutputWriter {cfn<void,address,address> output;};
struct ContractResidencyOriginOutputReader {cfn<ReadStatus,address,address> output;};

struct ContractResidencyOriginsFields {address origins;};
struct ContractResidencyOriginsWriter {cfn<void,address,address> origins;};
struct ContractResidencyOriginsReader {cfn<ReadStatus,address,address> origins;};

struct ContractResidencySelectedFields {address when;address a;address b;};
struct ContractResidencySelectedWriter {cfn<void,address,address> when;cfn<void,address,address> a;cfn<void,address,address> b;};
struct ContractResidencySelectedReader {cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> a;cfn<ReadStatus,address,address> b;};

struct ContractResidencyOutputKindWrittenFields {address parameter;};
struct ContractResidencyOutputKindWrittenWriter {cfn<void,address,address> parameter;};
struct ContractResidencyOutputKindWrittenReader {cfn<ReadStatus,address,address> parameter;};

struct ContractResidencyOutputFields {address function;address kind;address fields;address arguments;address bindings;};
struct ContractResidencyOutputWriter {cfn<void,address,address> function;cfn<void,address,address> kind;cfn<void,address,address> fields;cfn<void,address,address> arguments;cfn<void,address,address> bindings;};
struct ContractResidencyOutputReader {cfn<ReadStatus,address,address> function;cfn<ReadStatus,address,address> kind;cfn<ReadStatus,address,address> fields;cfn<ReadStatus,address,address> arguments;cfn<ReadStatus,address,address> bindings;};

struct ContractDomainOutputFields {address binding;address domain;address words;};
struct ContractDomainOutputWriter {cfn<void,address,address> binding;cfn<void,address,address> domain;cfn<void,address,address> words;};
struct ContractDomainOutputReader {cfn<ReadStatus,address,address> binding;cfn<ReadStatus,address,address> domain;cfn<ReadStatus,address,address> words;};

struct ContractDomainInterfaceFields {address instance;address target;address buffers;address textures;address write_regions;address uniforms;address length_requirements;address element_requirements;address prefix_guarantees;address outputs;address elements;};
struct ContractDomainInterfaceWriter {cfn<void,address,address> instance;cfn<void,address,address> target;cfn<void,address,address> buffers;cfn<void,address,address> textures;cfn<void,address,address> write_regions;cfn<void,address,address> uniforms;cfn<void,address,address> length_requirements;cfn<void,address,address> element_requirements;cfn<void,address,address> prefix_guarantees;cfn<void,address,address> outputs;cfn<void,address,address> elements;};
struct ContractDomainInterfaceReader {cfn<ReadStatus,address,address> instance;cfn<ReadStatus,address,address> target;cfn<ReadStatus,address,address> buffers;cfn<ReadStatus,address,address> textures;cfn<ReadStatus,address,address> write_regions;cfn<ReadStatus,address,address> uniforms;cfn<ReadStatus,address,address> length_requirements;cfn<ReadStatus,address,address> element_requirements;cfn<ReadStatus,address,address> prefix_guarantees;cfn<ReadStatus,address,address> outputs;cfn<ReadStatus,address,address> elements;};

struct ContractBufferFields {address name;address access;address ty;address stride;address count;};
struct ContractBufferWriter {cfn<void,address,address> name;cfn<void,address,address> access;cfn<void,address,address> ty;cfn<void,address,address> stride;cfn<void,address,address> count;};
struct ContractBufferReader {cfn<ReadStatus,address,address> name;cfn<ReadStatus,address,address> access;cfn<ReadStatus,address,address> ty;cfn<ReadStatus,address,address> stride;cfn<ReadStatus,address,address> count;};

struct ContractUniformFields {address name;address ty;address offset;};
struct ContractUniformWriter {cfn<void,address,address> name;cfn<void,address,address> ty;cfn<void,address,address> offset;};
struct ContractUniformReader {cfn<ReadStatus,address,address> name;cfn<ReadStatus,address,address> ty;cfn<ReadStatus,address,address> offset;};

struct ContractTextureBindingFields {address name;address count;address dimension;address format;};
struct ContractTextureBindingWriter {cfn<void,address,address> name;cfn<void,address,address> count;cfn<void,address,address> dimension;cfn<void,address,address> format;};
struct ContractTextureBindingReader {cfn<ReadStatus,address,address> name;cfn<ReadStatus,address,address> count;cfn<ReadStatus,address,address> dimension;cfn<ReadStatus,address,address> format;};

struct ContractPrefixCountFields {address binding;address word;};
struct ContractPrefixCountWriter {cfn<void,address,address> binding;cfn<void,address,address> word;};
struct ContractPrefixCountReader {cfn<ReadStatus,address,address> binding;cfn<ReadStatus,address,address> word;};

struct ContractPrefixRelationFields {address domain;address domain_region;address count;};
struct ContractPrefixRelationWriter {cfn<void,address,address> domain;cfn<void,address,address> domain_region;cfn<void,address,address> count;};
struct ContractPrefixRelationReader {cfn<ReadStatus,address,address> domain;cfn<ReadStatus,address,address> domain_region;cfn<ReadStatus,address,address> count;};

struct ContractElementRangeFields {address binding;address offset;address scalar;address intervals;address known_bits;};
struct ContractElementRangeWriter {cfn<void,address,address> binding;cfn<void,address,address> offset;cfn<void,address,address> scalar;cfn<void,address,address> intervals;cfn<void,address,address> known_bits;};
struct ContractElementRangeReader {cfn<ReadStatus,address,address> binding;cfn<ReadStatus,address,address> offset;cfn<ReadStatus,address,address> scalar;cfn<ReadStatus,address,address> intervals;cfn<ReadStatus,address,address> known_bits;};

struct ContractElementFactRangeFields {address range;};
struct ContractElementFactRangeWriter {cfn<void,address,address> range;};
struct ContractElementFactRangeReader {cfn<ReadStatus,address,address> range;};

struct ContractElementFactIndexFields {address binding;address offset;address target;};
struct ContractElementFactIndexWriter {cfn<void,address,address> binding;cfn<void,address,address> offset;cfn<void,address,address> target;};
struct ContractElementFactIndexReader {cfn<ReadStatus,address,address> binding;cfn<ReadStatus,address,address> offset;cfn<ReadStatus,address,address> target;};

struct ContractCallParameterKindValueFields {address ty;};
struct ContractCallParameterKindValueWriter {cfn<void,address,address> ty;};
struct ContractCallParameterKindValueReader {cfn<ReadStatus,address,address> ty;};

struct ContractCallParameterKindReferenceFields {address ty;address access;};
struct ContractCallParameterKindReferenceWriter {cfn<void,address,address> ty;cfn<void,address,address> access;};
struct ContractCallParameterKindReferenceReader {cfn<ReadStatus,address,address> ty;cfn<ReadStatus,address,address> access;};

struct ContractRelationOpPositionFields {address origin;};
struct ContractRelationOpPositionWriter {cfn<void,address,address> origin;};
struct ContractRelationOpPositionReader {cfn<ReadStatus,address,address> origin;};

struct ContractRelationOpReadFields {address word;address frame;};
struct ContractRelationOpReadWriter {cfn<void,address,address> word;cfn<void,address,address> frame;};
struct ContractRelationOpReadReader {cfn<ReadStatus,address,address> word;cfn<ReadStatus,address,address> frame;};

struct ContractRelationOpElementFields {address fields;address index;};
struct ContractRelationOpElementWriter {cfn<void,address,address> fields;cfn<void,address,address> index;};
struct ContractRelationOpElementReader {cfn<ReadStatus,address,address> fields;cfn<ReadStatus,address,address> index;};

struct ContractRelationOpProjectionFields {address fields;};
struct ContractRelationOpProjectionWriter {cfn<void,address,address> fields;};
struct ContractRelationOpProjectionReader {cfn<ReadStatus,address,address> fields;};

struct ContractRelationOpSelectFields {address indices;};
struct ContractRelationOpSelectWriter {cfn<void,address,address> indices;};
struct ContractRelationOpSelectReader {cfn<ReadStatus,address,address> indices;};

struct ContractRelationOpInitialFields {address origin;address frame;};
struct ContractRelationOpInitialWriter {cfn<void,address,address> origin;cfn<void,address,address> frame;};
struct ContractRelationOpInitialReader {cfn<ReadStatus,address,address> origin;cfn<ReadStatus,address,address> frame;};

struct ContractRelationEdgeFields {address from;address op;address when;};
struct ContractRelationEdgeWriter {cfn<void,address,address> from;cfn<void,address,address> op;cfn<void,address,address> when;};
struct ContractRelationEdgeReader {cfn<ReadStatus,address,address> from;cfn<ReadStatus,address,address> op;cfn<ReadStatus,address,address> when;};

struct ContractRelationNodeFields {address seeds;address edges;};
struct ContractRelationNodeWriter {cfn<void,address,address> seeds;cfn<void,address,address> edges;};
struct ContractRelationNodeReader {cfn<ReadStatus,address,address> seeds;cfn<ReadStatus,address,address> edges;};

struct ContractRelationWriteFields {address target;address fields;address when;};
struct ContractRelationWriteWriter {cfn<void,address,address> target;cfn<void,address,address> fields;cfn<void,address,address> when;};
struct ContractRelationWriteReader {cfn<ReadStatus,address,address> target;cfn<ReadStatus,address,address> fields;cfn<ReadStatus,address,address> when;};

struct ContractRelationFrameFields {address parent;address writes;};
struct ContractRelationFrameWriter {cfn<void,address,address> parent;cfn<void,address,address> writes;};
struct ContractRelationFrameReader {cfn<ReadStatus,address,address> parent;cfn<ReadStatus,address,address> writes;};

struct ContractRelationGraphFields {address nodes;address variables;address frames;};
struct ContractRelationGraphWriter {cfn<void,address,address> nodes;cfn<void,address,address> variables;cfn<void,address,address> frames;};
struct ContractRelationGraphReader {cfn<ReadStatus,address,address> nodes;cfn<ReadStatus,address,address> variables;cfn<ReadStatus,address,address> frames;};

struct ContractAddressIdentityConstantFields {address bits;};
struct ContractAddressIdentityConstantWriter {cfn<void,address,address> bits;};
struct ContractAddressIdentityConstantReader {cfn<ReadStatus,address,address> bits;};

struct ContractAddressIdentityValueFields {address ids;};
struct ContractAddressIdentityValueWriter {cfn<void,address,address> ids;};
struct ContractAddressIdentityValueReader {cfn<ReadStatus,address,address> ids;};

struct ContractAddressIdentityStorageFields {address borrow;};
struct ContractAddressIdentityStorageWriter {cfn<void,address,address> borrow;};
struct ContractAddressIdentityStorageReader {cfn<ReadStatus,address,address> borrow;};

struct ContractAddressOriginFields {address root;address layout;address base;address words;address writable;address initialized;};
struct ContractAddressOriginWriter {cfn<void,address,address> root;cfn<void,address,address> layout;cfn<void,address,address> base;cfn<void,address,address> words;cfn<void,address,address> writable;cfn<void,address,address> initialized;};
struct ContractAddressOriginReader {cfn<ReadStatus,address,address> root;cfn<ReadStatus,address,address> layout;cfn<ReadStatus,address,address> base;cfn<ReadStatus,address,address> words;cfn<ReadStatus,address,address> writable;cfn<ReadStatus,address,address> initialized;};

struct ContractBorrowSourceDataFields {address id;};
struct ContractBorrowSourceDataWriter {cfn<void,address,address> id;};
struct ContractBorrowSourceDataReader {cfn<ReadStatus,address,address> id;};

struct ContractBorrowSourceParameterFields {address id;address indices;};
struct ContractBorrowSourceParameterWriter {cfn<void,address,address> id;cfn<void,address,address> indices;};
struct ContractBorrowSourceParameterReader {cfn<ReadStatus,address,address> id;cfn<ReadStatus,address,address> indices;};

struct ContractBorrowSourcePositionFields {address origin;};
struct ContractBorrowSourcePositionWriter {cfn<void,address,address> origin;};
struct ContractBorrowSourcePositionReader {cfn<ReadStatus,address,address> origin;};

struct ContractBorrowSourceElementFields {address parent;address fields;address index;};
struct ContractBorrowSourceElementWriter {cfn<void,address,address> parent;cfn<void,address,address> fields;cfn<void,address,address> index;};
struct ContractBorrowSourceElementReader {cfn<ReadStatus,address,address> parent;cfn<ReadStatus,address,address> fields;cfn<ReadStatus,address,address> index;};

struct ContractBorrowSourceProjectionFields {address parent;address fields;};
struct ContractBorrowSourceProjectionWriter {cfn<void,address,address> parent;cfn<void,address,address> fields;};
struct ContractBorrowSourceProjectionReader {cfn<ReadStatus,address,address> parent;cfn<ReadStatus,address,address> fields;};

struct ContractBorrowSourceContentsFields {address origin;address word;};
struct ContractBorrowSourceContentsWriter {cfn<void,address,address> origin;cfn<void,address,address> word;};
struct ContractBorrowSourceContentsReader {cfn<ReadStatus,address,address> origin;cfn<ReadStatus,address,address> word;};

struct ContractBorrowSourceRelationFields {address graph;};
struct ContractBorrowSourceRelationWriter {cfn<void,address,address> graph;};
struct ContractBorrowSourceRelationReader {cfn<ReadStatus,address,address> graph;};

struct ContractBorrowSourceTaskFields {address captures;address outputs;address initialized;address guards;address completions;};
struct ContractBorrowSourceTaskWriter {cfn<void,address,address> captures;cfn<void,address,address> outputs;cfn<void,address,address> initialized;cfn<void,address,address> guards;cfn<void,address,address> completions;};
struct ContractBorrowSourceTaskReader {cfn<ReadStatus,address,address> captures;cfn<ReadStatus,address,address> outputs;cfn<ReadStatus,address,address> initialized;cfn<ReadStatus,address,address> guards;cfn<ReadStatus,address,address> completions;};

struct ContractBorrowSourceTaskOutputFields {address task;address projection;};
struct ContractBorrowSourceTaskOutputWriter {cfn<void,address,address> task;cfn<void,address,address> projection;};
struct ContractBorrowSourceTaskOutputReader {cfn<ReadStatus,address,address> task;cfn<ReadStatus,address,address> projection;};

struct ContractResidencyContractFields {address required;address calls;};
struct ContractResidencyContractWriter {cfn<void,address,address> required;cfn<void,address,address> calls;};
struct ContractResidencyContractReader {cfn<ReadStatus,address,address> required;cfn<ReadStatus,address,address> calls;};

struct ContractResidencyCallFields {address function;address arguments;address bindings;address path;};
struct ContractResidencyCallWriter {cfn<void,address,address> function;cfn<void,address,address> arguments;cfn<void,address,address> bindings;cfn<void,address,address> path;};
struct ContractResidencyCallReader {cfn<ReadStatus,address,address> function;cfn<ReadStatus,address,address> arguments;cfn<ReadStatus,address,address> bindings;cfn<ReadStatus,address,address> path;};

struct ContractOrdinaryContractFields {address parameters;address result;address inputs;address element_inputs;address returned;address written;address requires;address target;address frame;address fallible;address domain;address residency;address view_access;address initialization;};
struct ContractOrdinaryContractWriter {cfn<void,address,address> parameters;cfn<void,address,address> result;cfn<void,address,address> inputs;cfn<void,address,address> element_inputs;cfn<void,address,address> returned;cfn<void,address,address> written;cfn<void,address,address> requires;cfn<void,address,address> target;cfn<void,address,address> frame;cfn<void,address,address> fallible;cfn<void,address,address> domain;cfn<void,address,address> residency;cfn<void,address,address> view_access;cfn<void,address,address> initialization;};
struct ContractOrdinaryContractReader {cfn<ReadStatus,address,address> parameters;cfn<ReadStatus,address,address> result;cfn<ReadStatus,address,address> inputs;cfn<ReadStatus,address,address> element_inputs;cfn<ReadStatus,address,address> returned;cfn<ReadStatus,address,address> written;cfn<ReadStatus,address,address> requires;cfn<ReadStatus,address,address> target;cfn<ReadStatus,address,address> frame;cfn<ReadStatus,address,address> fallible;cfn<ReadStatus,address,address> domain;cfn<ReadStatus,address,address> residency;cfn<ReadStatus,address,address> view_access;cfn<ReadStatus,address,address> initialization;};

struct ContractProofFields {address origins;address conditions;address ranges;address nonzero;address domains;address views;address addresses;};
struct ContractProofWriter {cfn<void,address,address> origins;cfn<void,address,address> conditions;cfn<void,address,address> ranges;cfn<void,address,address> nonzero;cfn<void,address,address> domains;cfn<void,address,address> views;cfn<void,address,address> addresses;};
struct ContractProofReader {cfn<ReadStatus,address,address> origins;cfn<ReadStatus,address,address> conditions;cfn<ReadStatus,address,address> ranges;cfn<ReadStatus,address,address> nonzero;cfn<ReadStatus,address,address> domains;cfn<ReadStatus,address,address> views;cfn<ReadStatus,address,address> addresses;};

struct ContractPairFields {address first;address second;};
struct ContractPairWriter {cfn<void,address,address> first;cfn<void,address,address> second;};
struct ContractPairReader {cfn<ReadStatus,address,address> first;cfn<ReadStatus,address,address> second;};

struct ContractAddressRootVariants{cfn<ReadStatus,address,address> local;cfn<ReadStatus,address,address> parameter;cfn<ReadStatus,address,address> resource;cfn<ReadStatus,address,address> buffer;cfn<ReadStatus,address,address> derived;cfn<ReadStatus,address,address> external;cfn<ReadStatus,address,address> dataset;};
struct ContractInitializationSourceVariants{cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> task;};
struct ContractElementFactsVariants{cfn<ReadStatus,address,address> unknown;cfn<ReadStatus,address,address> known;cfn<ReadStatus,address,address> cleanup;cfn<ReadStatus,address,address> common;cfn<ReadStatus,address,address> selected;cfn<ReadStatus,address,address> input;};
struct ContractDataLayoutVariants{cfn<ReadStatus,address,address> declared;cfn<ReadStatus,address,address> parameter;};
struct ContractReturnedVariants{cfn<ReadStatus,address,address> unit;cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> dataset;cfn<ReadStatus,address,address> owned_value;};
struct ContractTaskProjectionVariants{cfn<ReadStatus,address,address> returned;cfn<ReadStatus,address,address> writes;cfn<ReadStatus,address,address> places;cfn<ReadStatus,address,address> initialized;};
struct ContractExclusionVariants{cfn<ReadStatus,address,address> resource;cfn<ReadStatus,address,address> ownership;cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> view;};
struct ContractViewAccessVariants{cfn<ReadStatus,address,address> resource;cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> memory;cfn<ReadStatus,address,address> dataset;cfn<ReadStatus,address,address> initialized;cfn<ReadStatus,address,address> stored;};
struct ContractResidencyPartVariants{cfn<ReadStatus,address,address> field;cfn<ReadStatus,address,address> elements;cfn<ReadStatus,address,address> selected;};
struct ContractResidencyOriginVariants{cfn<ReadStatus,address,address> cpu;cfn<ReadStatus,address,address> gpu;cfn<ReadStatus,address,address> input;cfn<ReadStatus,address,address> output;cfn<ReadStatus,address,address> task_transfer;};
struct ContractResidencyVariants{cfn<ReadStatus,address,address> absent;cfn<ReadStatus,address,address> origins;cfn<ReadStatus,address,address> selected;};
struct ContractResidencyOutputKindVariants{cfn<ReadStatus,address,address> returned;cfn<ReadStatus,address,address> written;};
struct ContractTextureDimensionVariants{cfn<ReadStatus,address,address> d2;cfn<ReadStatus,address,address> d3;};
struct ContractTextureFormatVariants{cfn<ReadStatus,address,address> rgba32float;cfn<ReadStatus,address,address> rgba8unorm;cfn<ReadStatus,address,address> rgba8srgb;cfn<ReadStatus,address,address> r8unorm;cfn<ReadStatus,address,address> rg16unorm;cfn<ReadStatus,address,address> rgb10a2unorm;cfn<ReadStatus,address,address> r11g11b10float;};
struct ContractElementFactVariants{cfn<ReadStatus,address,address> range;cfn<ReadStatus,address,address> index;};
struct ContractAccessVariants{cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> read;cfn<ReadStatus,address,address> write;cfn<ReadStatus,address,address> inout;cfn<ReadStatus,address,address> atomic;};
struct ContractTargetVariants{cfn<ReadStatus,address,address> compute;cfn<ReadStatus,address,address> cpu;cfn<ReadStatus,address,address> gpu;};
struct ContractCallParameterKindVariants{cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> reference;};
struct ContractContractKindVariants{cfn<ReadStatus,address,address> requires;cfn<ReadStatus,address,address> ensures;};
struct ContractRelationOpVariants{cfn<ReadStatus,address,address> identity;cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> read;cfn<ReadStatus,address,address> element;cfn<ReadStatus,address,address> projection;cfn<ReadStatus,address,address> resources;cfn<ReadStatus,address,address> owners;cfn<ReadStatus,address,address> select;cfn<ReadStatus,address,address> initial;};
struct ContractScalarVariants{cfn<ReadStatus,address,address> f32;cfn<ReadStatus,address,address> u32;cfn<ReadStatus,address,address> i32;cfn<ReadStatus,address,address> bool;cfn<ReadStatus,address,address> u64;cfn<ReadStatus,address,address> i64;cfn<ReadStatus,address,address> f64;cfn<ReadStatus,address,address> u8;cfn<ReadStatus,address,address> i8;cfn<ReadStatus,address,address> u16;cfn<ReadStatus,address,address> i16;};
struct ContractOwnershipVariants{cfn<ReadStatus,address,address> owned;cfn<ReadStatus,address,address> borrowed;cfn<ReadStatus,address,address> moved;};
struct ContractAddressIdentityVariants{cfn<ReadStatus,address,address> constant;cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> storage;};
struct ContractBorrowSourceVariants{cfn<ReadStatus,address,address> data;cfn<ReadStatus,address,address> parameter;cfn<ReadStatus,address,address> unresolved;cfn<ReadStatus,address,address> inactive;cfn<ReadStatus,address,address> external;cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> element;cfn<ReadStatus,address,address> projection;cfn<ReadStatus,address,address> contents;cfn<ReadStatus,address,address> relation;cfn<ReadStatus,address,address> task;cfn<ReadStatus,address,address> task_output;};
extern "C" void contract_write_sequence(u64 count,address iterator,address output,cfn<address,address> next,cfn<void,address,address> element,cfn<void,address,address,u64> append) on cpu requires nonzero(next) requires nonzero(element) requires nonzero(append);
extern "C" ReadStatus contract_read_sequence(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address> push) on cpu requires nonzero(push);
extern "C" void contract_write_option(address value,address output,cfn<void,address,address> element,cfn<void,address,address,u64> append) on cpu requires nonzero(element) requires nonzero(append);
extern "C" ReadStatus contract_read_option(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address> some,cfn<void,address> none) on cpu requires nonzero(some) requires nonzero(none);
extern "C" void contract_write_trimmed(Span values,address output,cfn<u32,address> is_empty,cfn<void,address,address> element,cfn<void,address,address,u64> append) on cpu requires nonzero(is_empty) requires nonzero(element) requires nonzero(append);
extern "C" void contract_write_requirement(address key,address condition,address output,cfn<void,address,address> key_write,cfn<void,address,address> condition_write,cfn<void,address,address,u64> append) on cpu requires nonzero(key_write) requires nonzero(condition_write) requires nonzero(append);
extern "C" void contract_write_text(Bytes value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus contract_read_text(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address,Bytes> text) on cpu requires nonzero(text);
extern "C" void contract_write_u8(u8 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus contract_read_u8(Bytes input,inout u64 position,inout u8 output) on cpu;
extern "C" void contract_write_u16(u16 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus contract_read_u16(Bytes input,inout u64 position,inout u16 output) on cpu;
extern "C" void contract_write_u32(u32 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus contract_read_u32(Bytes input,inout u64 position,inout u32 output) on cpu;
extern "C" void contract_write_u64(u64 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus contract_read_u64(Bytes input,inout u64 position,inout u64 output) on cpu;
extern "C" void contract_write_usize(u64 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus contract_read_usize(Bytes input,inout u64 position,inout u64 output) on cpu;
extern "C" void contract_write_bool(u32 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus contract_read_bool(Bytes input,inout u64 position,inout u32 output) on cpu;
extern "C" void contract_write_value(address output,ContractValueFields fields,ContractValueWriter host) on cpu requires nonzero(host.proof) requires nonzero(host.literal) requires nonzero(host.ty) requires nonzero(host.regs) requires nonzero(host.uniform) requires nonzero(host.constant);
extern "C" ReadStatus contract_read_value(address context,ContractValueFields fields,ContractValueReader host) on cpu requires nonzero(host.proof) requires nonzero(host.literal) requires nonzero(host.ty) requires nonzero(host.regs) requires nonzero(host.uniform) requires nonzero(host.constant);
extern "C" void contract_write_address_layout(address output,ContractAddressLayoutFields fields,ContractAddressLayoutWriter host) on cpu requires nonzero(host.size) requires nonzero(host.align) requires nonzero(host.parts);
extern "C" ReadStatus contract_read_address_layout(address context,ContractAddressLayoutFields fields,ContractAddressLayoutReader host) on cpu requires nonzero(host.size) requires nonzero(host.align) requires nonzero(host.parts);
extern "C" void contract_write_address_root_local(address output,ContractAddressRootLocalFields fields,ContractAddressRootLocalWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.base) requires nonzero(host.len) requires nonzero(append);
extern "C" ReadStatus contract_read_address_root_local(address context,ContractAddressRootLocalFields fields,ContractAddressRootLocalReader host) on cpu requires nonzero(host.base) requires nonzero(host.len);
extern "C" void contract_write_address_root_parameter(address output,ContractAddressRootParameterFields fields,ContractAddressRootParameterWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.index) requires nonzero(append);
extern "C" ReadStatus contract_read_address_root_parameter(address context,ContractAddressRootParameterFields fields,ContractAddressRootParameterReader host) on cpu requires nonzero(host.index);
extern "C" void contract_write_address_root_resource(address output,ContractAddressRootResourceFields fields,ContractAddressRootResourceWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(host.index) requires nonzero(append);
extern "C" ReadStatus contract_read_address_root_resource(address context,ContractAddressRootResourceFields fields,ContractAddressRootResourceReader host) on cpu requires nonzero(host.borrow) requires nonzero(host.index);
extern "C" void contract_write_address_root_buffer(address output,ContractAddressRootBufferFields fields,ContractAddressRootBufferWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.binding) requires nonzero(append);
extern "C" ReadStatus contract_read_address_root_buffer(address context,ContractAddressRootBufferFields fields,ContractAddressRootBufferReader host) on cpu requires nonzero(host.binding);
extern "C" void contract_write_address_root_derived(address output,ContractAddressRootDerivedFields fields,ContractAddressRootDerivedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(append);
extern "C" ReadStatus contract_read_address_root_derived(address context,ContractAddressRootDerivedFields fields,ContractAddressRootDerivedReader host) on cpu requires nonzero(host.borrow);
extern "C" void contract_write_address_root_external(address output,ContractAddressRootExternalFields fields,ContractAddressRootExternalWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.identity) requires nonzero(append);
extern "C" ReadStatus contract_read_address_root_external(address context,ContractAddressRootExternalFields fields,ContractAddressRootExternalReader host) on cpu requires nonzero(host.identity);
extern "C" void contract_write_address_root_dataset(address output,ContractAddressRootDatasetFields fields,ContractAddressRootDatasetWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(host.index) requires nonzero(append);
extern "C" ReadStatus contract_read_address_root_dataset(address context,ContractAddressRootDatasetFields fields,ContractAddressRootDatasetReader host) on cpu requires nonzero(host.borrow) requires nonzero(host.index);
extern "C" void contract_write_initialization_source_position(address output,ContractInitializationSourcePositionFields fields,ContractInitializationSourcePositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(append);
extern "C" ReadStatus contract_read_initialization_source_position(address context,ContractInitializationSourcePositionFields fields,ContractInitializationSourcePositionReader host) on cpu requires nonzero(host.origin);
extern "C" void contract_write_initialization_source_task(address output,ContractInitializationSourceTaskFields fields,ContractInitializationSourceTaskWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(append);
extern "C" ReadStatus contract_read_initialization_source_task(address context,ContractInitializationSourceTaskFields fields,ContractInitializationSourceTaskReader host) on cpu requires nonzero(host.borrow);
extern "C" void contract_write_initialization(address output,ContractInitializationFields fields,ContractInitializationWriter host) on cpu requires nonzero(host.source) requires nonzero(host.when);
extern "C" ReadStatus contract_read_initialization(address context,ContractInitializationFields fields,ContractInitializationReader host) on cpu requires nonzero(host.source) requires nonzero(host.when);
extern "C" void contract_write_data_payload(address output,ContractDataPayloadFields fields,ContractDataPayloadWriter host) on cpu requires nonzero(host.root) requires nonzero(host.ty) requires nonzero(host.length) requires nonzero(host.ownership) requires nonzero(host.access) requires nonzero(host.count) requires nonzero(host.residency) requires nonzero(host.layout) requires nonzero(host.elements) requires nonzero(host.prefixes);
extern "C" ReadStatus contract_read_data_payload(address context,ContractDataPayloadFields fields,ContractDataPayloadReader host) on cpu requires nonzero(host.root) requires nonzero(host.ty) requires nonzero(host.length) requires nonzero(host.ownership) requires nonzero(host.access) requires nonzero(host.count) requires nonzero(host.residency) requires nonzero(host.layout) requires nonzero(host.elements) requires nonzero(host.prefixes);
extern "C" void contract_write_element_facts_unknown(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_element_facts_known(address output,ContractElementFactsKnownFields fields,ContractElementFactsKnownWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.value) requires nonzero(append);
extern "C" ReadStatus contract_read_element_facts_known(address context,ContractElementFactsKnownFields fields,ContractElementFactsKnownReader host) on cpu requires nonzero(host.value);
extern "C" void contract_write_element_facts_cleanup(address output,ContractElementFactsCleanupFields fields,ContractElementFactsCleanupWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(append);
extern "C" ReadStatus contract_read_element_facts_cleanup(address context,ContractElementFactsCleanupFields fields,ContractElementFactsCleanupReader host) on cpu requires nonzero(host.when);
extern "C" void contract_write_element_facts_common(address output,ContractElementFactsCommonFields fields,ContractElementFactsCommonWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.a) requires nonzero(host.b) requires nonzero(append);
extern "C" ReadStatus contract_read_element_facts_common(address context,ContractElementFactsCommonFields fields,ContractElementFactsCommonReader host) on cpu requires nonzero(host.a) requires nonzero(host.b);
extern "C" void contract_write_element_facts_selected(address output,ContractElementFactsSelectedFields fields,ContractElementFactsSelectedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b) requires nonzero(append);
extern "C" ReadStatus contract_read_element_facts_selected(address context,ContractElementFactsSelectedFields fields,ContractElementFactsSelectedReader host) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b);
extern "C" void contract_write_element_facts_input(address output,ContractElementFactsInputFields fields,ContractElementFactsInputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(append);
extern "C" ReadStatus contract_read_element_facts_input(address context,ContractElementFactsInputFields fields,ContractElementFactsInputReader host) on cpu requires nonzero(host.when);
extern "C" void contract_write_data_layout_declared(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_data_layout_parameter(address output,ContractDataLayoutParameterFields fields,ContractDataLayoutParameterWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parameter) requires nonzero(host.stride) requires nonzero(host.offset) requires nonzero(append);
extern "C" ReadStatus contract_read_data_layout_parameter(address context,ContractDataLayoutParameterFields fields,ContractDataLayoutParameterReader host) on cpu requires nonzero(host.parameter) requires nonzero(host.stride) requires nonzero(host.offset);
extern "C" void contract_write_returned_unit(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_returned_value(address output,ContractReturnedValueFields fields,ContractReturnedValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.value) requires nonzero(append);
extern "C" ReadStatus contract_read_returned_value(address context,ContractReturnedValueFields fields,ContractReturnedValueReader host) on cpu requires nonzero(host.value);
extern "C" void contract_write_returned_dataset(address output,ContractReturnedDatasetFields fields,ContractReturnedDatasetWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.data) requires nonzero(append);
extern "C" ReadStatus contract_read_returned_dataset(address context,ContractReturnedDatasetFields fields,ContractReturnedDatasetReader host) on cpu requires nonzero(host.data);
extern "C" void contract_write_returned_owned_value(address output,ContractReturnedOwnedValueFields fields,ContractReturnedOwnedValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.value) requires nonzero(append);
extern "C" ReadStatus contract_read_returned_owned_value(address context,ContractReturnedOwnedValueFields fields,ContractReturnedOwnedValueReader host) on cpu requires nonzero(host.value);
extern "C" void contract_write_owned_value_payload(address output,ContractOwnedValuePayloadFields fields,ContractOwnedValuePayloadWriter host) on cpu requires nonzero(host.value) requires nonzero(host.payloads) requires nonzero(host.ownership);
extern "C" ReadStatus contract_read_owned_value_payload(address context,ContractOwnedValuePayloadFields fields,ContractOwnedValuePayloadReader host) on cpu requires nonzero(host.value) requires nonzero(host.payloads) requires nonzero(host.ownership);
extern "C" void contract_write_task_guard(address output,ContractTaskGuardFields fields,ContractTaskGuardWriter host) on cpu requires nonzero(host.target) requires nonzero(host.ty) requires nonzero(host.clauses) requires nonzero(host.result) requires nonzero(host.model) requires nonzero(host.arguments) requires nonzero(host.returned) requires nonzero(host.inputs);
extern "C" ReadStatus contract_read_task_guard(address context,ContractTaskGuardFields fields,ContractTaskGuardReader host) on cpu requires nonzero(host.target) requires nonzero(host.ty) requires nonzero(host.clauses) requires nonzero(host.result) requires nonzero(host.model) requires nonzero(host.arguments) requires nonzero(host.returned) requires nonzero(host.inputs);
extern "C" void contract_write_task_completion(address output,ContractTaskCompletionFields fields,ContractTaskCompletionWriter host) on cpu requires nonzero(host.offset) requires nonzero(host.result) requires nonzero(host.layout) requires nonzero(host.task) requires nonzero(host.parent);
extern "C" ReadStatus contract_read_task_completion(address context,ContractTaskCompletionFields fields,ContractTaskCompletionReader host) on cpu requires nonzero(host.offset) requires nonzero(host.result) requires nonzero(host.layout) requires nonzero(host.task) requires nonzero(host.parent);
extern "C" void contract_write_task_projection_returned(address output,ContractTaskProjectionReturnedFields fields,ContractTaskProjectionReturnedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.component) requires nonzero(host.source) requires nonzero(append);
extern "C" ReadStatus contract_read_task_projection_returned(address context,ContractTaskProjectionReturnedFields fields,ContractTaskProjectionReturnedReader host) on cpu requires nonzero(host.component) requires nonzero(host.source);
extern "C" void contract_write_task_projection_writes(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_task_projection_places(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_task_projection_initialized(address output,ContractTaskProjectionInitializedFields fields,ContractTaskProjectionInitializedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.returned) requires nonzero(append);
extern "C" ReadStatus contract_read_task_projection_initialized(address context,ContractTaskProjectionInitializedFields fields,ContractTaskProjectionInitializedReader host) on cpu requires nonzero(host.returned);
extern "C" void contract_write_exclusion_resource(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_exclusion_ownership(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_exclusion_position(address output,ContractExclusionPositionFields fields,ContractExclusionPositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.access) requires nonzero(append);
extern "C" ReadStatus contract_read_exclusion_position(address context,ContractExclusionPositionFields fields,ContractExclusionPositionReader host) on cpu requires nonzero(host.access);
extern "C" void contract_write_exclusion_view(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_position_access(address output,ContractPositionAccessFields fields,ContractPositionAccessWriter host) on cpu requires nonzero(host.layout) requires nonzero(host.written) requires nonzero(host.writable) requires nonzero(host.initialized);
extern "C" ReadStatus contract_read_position_access(address context,ContractPositionAccessFields fields,ContractPositionAccessReader host) on cpu requires nonzero(host.layout) requires nonzero(host.written) requires nonzero(host.writable) requires nonzero(host.initialized);
extern "C" void contract_write_view_access_resource(address output,ContractViewAccessResourceFields fields,ContractViewAccessResourceWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.resources) requires nonzero(host.borrows) requires nonzero(host.kind) requires nonzero(host.when) requires nonzero(append);
extern "C" ReadStatus contract_read_view_access_resource(address context,ContractViewAccessResourceFields fields,ContractViewAccessResourceReader host) on cpu requires nonzero(host.resources) requires nonzero(host.borrows) requires nonzero(host.kind) requires nonzero(host.when);
extern "C" void contract_write_view_access_position(address output,ContractViewAccessPositionFields fields,ContractViewAccessPositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.position) requires nonzero(append);
extern "C" ReadStatus contract_read_view_access_position(address context,ContractViewAccessPositionFields fields,ContractViewAccessPositionReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.position);
extern "C" void contract_write_view_access_memory(address output,ContractViewAccessMemoryFields fields,ContractViewAccessMemoryWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.access) requires nonzero(append);
extern "C" ReadStatus contract_read_view_access_memory(address context,ContractViewAccessMemoryFields fields,ContractViewAccessMemoryReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.access);
extern "C" void contract_write_view_access_dataset(address output,ContractViewAccessDatasetFields fields,ContractViewAccessDatasetWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.index) requires nonzero(host.access) requires nonzero(host.atomic) requires nonzero(append);
extern "C" ReadStatus contract_read_view_access_dataset(address context,ContractViewAccessDatasetFields fields,ContractViewAccessDatasetReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.index) requires nonzero(host.access) requires nonzero(host.atomic);
extern "C" void contract_write_view_access_initialized(address output,ContractViewAccessInitializedFields fields,ContractViewAccessInitializedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.initialized) requires nonzero(host.when) requires nonzero(append);
extern "C" ReadStatus contract_read_view_access_initialized(address context,ContractViewAccessInitializedFields fields,ContractViewAccessInitializedReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.initialized) requires nonzero(host.when);
extern "C" void contract_write_view_access_stored(address output,ContractViewAccessStoredFields fields,ContractViewAccessStoredWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.word) requires nonzero(host.value) requires nonzero(host.when) requires nonzero(append);
extern "C" ReadStatus contract_read_view_access_stored(address context,ContractViewAccessStoredFields fields,ContractViewAccessStoredReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.word) requires nonzero(host.value) requires nonzero(host.when);
extern "C" void contract_write_residency_part_field(address output,ContractResidencyPartFieldFields fields,ContractResidencyPartFieldWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.field) requires nonzero(append);
extern "C" ReadStatus contract_read_residency_part_field(address context,ContractResidencyPartFieldFields fields,ContractResidencyPartFieldReader host) on cpu requires nonzero(host.field);
extern "C" void contract_write_residency_part_elements(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_residency_part_selected(address output,ContractResidencyPartSelectedFields fields,ContractResidencyPartSelectedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.index) requires nonzero(append);
extern "C" ReadStatus contract_read_residency_part_selected(address context,ContractResidencyPartSelectedFields fields,ContractResidencyPartSelectedReader host) on cpu requires nonzero(host.index);
extern "C" void contract_write_residency_input(address output,ContractResidencyInputFields fields,ContractResidencyInputWriter host) on cpu requires nonzero(host.parameter) requires nonzero(host.fields);
extern "C" ReadStatus contract_read_residency_input(address context,ContractResidencyInputFields fields,ContractResidencyInputReader host) on cpu requires nonzero(host.parameter) requires nonzero(host.fields);
extern "C" void contract_write_residency_origin_cpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_residency_origin_gpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_residency_origin_input(address output,ContractResidencyOriginInputFields fields,ContractResidencyOriginInputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.input) requires nonzero(append);
extern "C" ReadStatus contract_read_residency_origin_input(address context,ContractResidencyOriginInputFields fields,ContractResidencyOriginInputReader host) on cpu requires nonzero(host.input);
extern "C" void contract_write_residency_origin_output(address output,ContractResidencyOriginOutputFields fields,ContractResidencyOriginOutputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.output) requires nonzero(append);
extern "C" ReadStatus contract_read_residency_origin_output(address context,ContractResidencyOriginOutputFields fields,ContractResidencyOriginOutputReader host) on cpu requires nonzero(host.output);
extern "C" void contract_write_residency_origin_task_transfer(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_residency_absent(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_residency_origins(address output,ContractResidencyOriginsFields fields,ContractResidencyOriginsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origins) requires nonzero(append);
extern "C" ReadStatus contract_read_residency_origins(address context,ContractResidencyOriginsFields fields,ContractResidencyOriginsReader host) on cpu requires nonzero(host.origins);
extern "C" void contract_write_residency_selected(address output,ContractResidencySelectedFields fields,ContractResidencySelectedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b) requires nonzero(append);
extern "C" ReadStatus contract_read_residency_selected(address context,ContractResidencySelectedFields fields,ContractResidencySelectedReader host) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b);
extern "C" void contract_write_residency_output_kind_returned(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_residency_output_kind_written(address output,ContractResidencyOutputKindWrittenFields fields,ContractResidencyOutputKindWrittenWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parameter) requires nonzero(append);
extern "C" ReadStatus contract_read_residency_output_kind_written(address context,ContractResidencyOutputKindWrittenFields fields,ContractResidencyOutputKindWrittenReader host) on cpu requires nonzero(host.parameter);
extern "C" void contract_write_residency_output(address output,ContractResidencyOutputFields fields,ContractResidencyOutputWriter host) on cpu requires nonzero(host.function) requires nonzero(host.kind) requires nonzero(host.fields) requires nonzero(host.arguments) requires nonzero(host.bindings);
extern "C" ReadStatus contract_read_residency_output(address context,ContractResidencyOutputFields fields,ContractResidencyOutputReader host) on cpu requires nonzero(host.function) requires nonzero(host.kind) requires nonzero(host.fields) requires nonzero(host.arguments) requires nonzero(host.bindings);
extern "C" void contract_write_domain_output(address output,ContractDomainOutputFields fields,ContractDomainOutputWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.domain) requires nonzero(host.words);
extern "C" ReadStatus contract_read_domain_output(address context,ContractDomainOutputFields fields,ContractDomainOutputReader host) on cpu requires nonzero(host.binding) requires nonzero(host.domain) requires nonzero(host.words);
extern "C" void contract_write_domain_interface(address output,ContractDomainInterfaceFields fields,ContractDomainInterfaceWriter host) on cpu requires nonzero(host.instance) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.textures) requires nonzero(host.write_regions) requires nonzero(host.uniforms) requires nonzero(host.length_requirements) requires nonzero(host.element_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.outputs) requires nonzero(host.elements);
extern "C" ReadStatus contract_read_domain_interface(address context,ContractDomainInterfaceFields fields,ContractDomainInterfaceReader host) on cpu requires nonzero(host.instance) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.textures) requires nonzero(host.write_regions) requires nonzero(host.uniforms) requires nonzero(host.length_requirements) requires nonzero(host.element_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.outputs) requires nonzero(host.elements);
extern "C" void contract_write_buffer(address output,ContractBufferFields fields,ContractBufferWriter host) on cpu requires nonzero(host.name) requires nonzero(host.access) requires nonzero(host.ty) requires nonzero(host.stride) requires nonzero(host.count);
extern "C" ReadStatus contract_read_buffer(address context,ContractBufferFields fields,ContractBufferReader host) on cpu requires nonzero(host.name) requires nonzero(host.access) requires nonzero(host.ty) requires nonzero(host.stride) requires nonzero(host.count);
extern "C" void contract_write_uniform(address output,ContractUniformFields fields,ContractUniformWriter host) on cpu requires nonzero(host.name) requires nonzero(host.ty) requires nonzero(host.offset);
extern "C" ReadStatus contract_read_uniform(address context,ContractUniformFields fields,ContractUniformReader host) on cpu requires nonzero(host.name) requires nonzero(host.ty) requires nonzero(host.offset);
extern "C" void contract_write_texture_dimension_d2(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_texture_dimension_d3(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_texture_format_rgba32float(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_texture_format_rgba8unorm(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_texture_format_rgba8srgb(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_texture_format_r8unorm(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_texture_format_rg16unorm(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_texture_format_rgb10a2unorm(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_texture_format_r11g11b10float(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_texture_binding(address output,ContractTextureBindingFields fields,ContractTextureBindingWriter host) on cpu requires nonzero(host.name) requires nonzero(host.count) requires nonzero(host.dimension) requires nonzero(host.format);
extern "C" ReadStatus contract_read_texture_binding(address context,ContractTextureBindingFields fields,ContractTextureBindingReader host) on cpu requires nonzero(host.name) requires nonzero(host.count) requires nonzero(host.dimension) requires nonzero(host.format);
extern "C" void contract_write_prefix_count(address output,ContractPrefixCountFields fields,ContractPrefixCountWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.word);
extern "C" ReadStatus contract_read_prefix_count(address context,ContractPrefixCountFields fields,ContractPrefixCountReader host) on cpu requires nonzero(host.binding) requires nonzero(host.word);
extern "C" void contract_write_prefix_relation(address output,ContractPrefixRelationFields fields,ContractPrefixRelationWriter host) on cpu requires nonzero(host.domain) requires nonzero(host.domain_region) requires nonzero(host.count);
extern "C" ReadStatus contract_read_prefix_relation(address context,ContractPrefixRelationFields fields,ContractPrefixRelationReader host) on cpu requires nonzero(host.domain) requires nonzero(host.domain_region) requires nonzero(host.count);
extern "C" void contract_write_element_range(address output,ContractElementRangeFields fields,ContractElementRangeWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.offset) requires nonzero(host.scalar) requires nonzero(host.intervals) requires nonzero(host.known_bits);
extern "C" ReadStatus contract_read_element_range(address context,ContractElementRangeFields fields,ContractElementRangeReader host) on cpu requires nonzero(host.binding) requires nonzero(host.offset) requires nonzero(host.scalar) requires nonzero(host.intervals) requires nonzero(host.known_bits);
extern "C" void contract_write_element_fact_range(address output,ContractElementFactRangeFields fields,ContractElementFactRangeWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.range) requires nonzero(append);
extern "C" ReadStatus contract_read_element_fact_range(address context,ContractElementFactRangeFields fields,ContractElementFactRangeReader host) on cpu requires nonzero(host.range);
extern "C" void contract_write_element_fact_index(address output,ContractElementFactIndexFields fields,ContractElementFactIndexWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.binding) requires nonzero(host.offset) requires nonzero(host.target) requires nonzero(append);
extern "C" ReadStatus contract_read_element_fact_index(address context,ContractElementFactIndexFields fields,ContractElementFactIndexReader host) on cpu requires nonzero(host.binding) requires nonzero(host.offset) requires nonzero(host.target);
extern "C" void contract_write_access_value(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_access_read(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_access_write(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_access_inout(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_access_atomic(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_target_compute(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_target_cpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_target_gpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_call_parameter_kind_value(address output,ContractCallParameterKindValueFields fields,ContractCallParameterKindValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.ty) requires nonzero(append);
extern "C" ReadStatus contract_read_call_parameter_kind_value(address context,ContractCallParameterKindValueFields fields,ContractCallParameterKindValueReader host) on cpu requires nonzero(host.ty);
extern "C" void contract_write_call_parameter_kind_reference(address output,ContractCallParameterKindReferenceFields fields,ContractCallParameterKindReferenceWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.ty) requires nonzero(host.access) requires nonzero(append);
extern "C" ReadStatus contract_read_call_parameter_kind_reference(address context,ContractCallParameterKindReferenceFields fields,ContractCallParameterKindReferenceReader host) on cpu requires nonzero(host.ty) requires nonzero(host.access);
extern "C" void contract_write_contract_kind_requires(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_contract_kind_ensures(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_relation_op_identity(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_relation_op_position(address output,ContractRelationOpPositionFields fields,ContractRelationOpPositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(append);
extern "C" ReadStatus contract_read_relation_op_position(address context,ContractRelationOpPositionFields fields,ContractRelationOpPositionReader host) on cpu requires nonzero(host.origin);
extern "C" void contract_write_relation_op_read(address output,ContractRelationOpReadFields fields,ContractRelationOpReadWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.word) requires nonzero(host.frame) requires nonzero(append);
extern "C" ReadStatus contract_read_relation_op_read(address context,ContractRelationOpReadFields fields,ContractRelationOpReadReader host) on cpu requires nonzero(host.word) requires nonzero(host.frame);
extern "C" void contract_write_relation_op_element(address output,ContractRelationOpElementFields fields,ContractRelationOpElementWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.fields) requires nonzero(host.index) requires nonzero(append);
extern "C" ReadStatus contract_read_relation_op_element(address context,ContractRelationOpElementFields fields,ContractRelationOpElementReader host) on cpu requires nonzero(host.fields) requires nonzero(host.index);
extern "C" void contract_write_relation_op_projection(address output,ContractRelationOpProjectionFields fields,ContractRelationOpProjectionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.fields) requires nonzero(append);
extern "C" ReadStatus contract_read_relation_op_projection(address context,ContractRelationOpProjectionFields fields,ContractRelationOpProjectionReader host) on cpu requires nonzero(host.fields);
extern "C" void contract_write_relation_op_resources(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_relation_op_owners(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_relation_op_select(address output,ContractRelationOpSelectFields fields,ContractRelationOpSelectWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.indices) requires nonzero(append);
extern "C" ReadStatus contract_read_relation_op_select(address context,ContractRelationOpSelectFields fields,ContractRelationOpSelectReader host) on cpu requires nonzero(host.indices);
extern "C" void contract_write_relation_op_initial(address output,ContractRelationOpInitialFields fields,ContractRelationOpInitialWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(host.frame) requires nonzero(append);
extern "C" ReadStatus contract_read_relation_op_initial(address context,ContractRelationOpInitialFields fields,ContractRelationOpInitialReader host) on cpu requires nonzero(host.origin) requires nonzero(host.frame);
extern "C" void contract_write_relation_edge(address output,ContractRelationEdgeFields fields,ContractRelationEdgeWriter host) on cpu requires nonzero(host.from) requires nonzero(host.op) requires nonzero(host.when);
extern "C" ReadStatus contract_read_relation_edge(address context,ContractRelationEdgeFields fields,ContractRelationEdgeReader host) on cpu requires nonzero(host.from) requires nonzero(host.op) requires nonzero(host.when);
extern "C" void contract_write_relation_node(address output,ContractRelationNodeFields fields,ContractRelationNodeWriter host) on cpu requires nonzero(host.seeds) requires nonzero(host.edges);
extern "C" ReadStatus contract_read_relation_node(address context,ContractRelationNodeFields fields,ContractRelationNodeReader host) on cpu requires nonzero(host.seeds) requires nonzero(host.edges);
extern "C" void contract_write_relation_write(address output,ContractRelationWriteFields fields,ContractRelationWriteWriter host) on cpu requires nonzero(host.target) requires nonzero(host.fields) requires nonzero(host.when);
extern "C" ReadStatus contract_read_relation_write(address context,ContractRelationWriteFields fields,ContractRelationWriteReader host) on cpu requires nonzero(host.target) requires nonzero(host.fields) requires nonzero(host.when);
extern "C" void contract_write_relation_frame(address output,ContractRelationFrameFields fields,ContractRelationFrameWriter host) on cpu requires nonzero(host.parent) requires nonzero(host.writes);
extern "C" ReadStatus contract_read_relation_frame(address context,ContractRelationFrameFields fields,ContractRelationFrameReader host) on cpu requires nonzero(host.parent) requires nonzero(host.writes);
extern "C" void contract_write_relation_graph(address output,ContractRelationGraphFields fields,ContractRelationGraphWriter host) on cpu requires nonzero(host.nodes) requires nonzero(host.variables) requires nonzero(host.frames);
extern "C" ReadStatus contract_read_relation_graph(address context,ContractRelationGraphFields fields,ContractRelationGraphReader host) on cpu requires nonzero(host.nodes) requires nonzero(host.variables) requires nonzero(host.frames);
extern "C" void contract_write_scalar_f32(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_u32(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_i32(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_bool(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_u64(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_i64(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_f64(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_u8(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_i8(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_u16(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_scalar_i16(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_ownership_owned(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_ownership_borrowed(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_ownership_moved(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_address_identity_constant(address output,ContractAddressIdentityConstantFields fields,ContractAddressIdentityConstantWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.bits) requires nonzero(append);
extern "C" ReadStatus contract_read_address_identity_constant(address context,ContractAddressIdentityConstantFields fields,ContractAddressIdentityConstantReader host) on cpu requires nonzero(host.bits);
extern "C" void contract_write_address_identity_value(address output,ContractAddressIdentityValueFields fields,ContractAddressIdentityValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.ids) requires nonzero(append);
extern "C" ReadStatus contract_read_address_identity_value(address context,ContractAddressIdentityValueFields fields,ContractAddressIdentityValueReader host) on cpu requires nonzero(host.ids);
extern "C" void contract_write_address_identity_storage(address output,ContractAddressIdentityStorageFields fields,ContractAddressIdentityStorageWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(append);
extern "C" ReadStatus contract_read_address_identity_storage(address context,ContractAddressIdentityStorageFields fields,ContractAddressIdentityStorageReader host) on cpu requires nonzero(host.borrow);
extern "C" void contract_write_address_origin(address output,ContractAddressOriginFields fields,ContractAddressOriginWriter host) on cpu requires nonzero(host.root) requires nonzero(host.layout) requires nonzero(host.base) requires nonzero(host.words) requires nonzero(host.writable) requires nonzero(host.initialized);
extern "C" ReadStatus contract_read_address_origin(address context,ContractAddressOriginFields fields,ContractAddressOriginReader host) on cpu requires nonzero(host.root) requires nonzero(host.layout) requires nonzero(host.base) requires nonzero(host.words) requires nonzero(host.writable) requires nonzero(host.initialized);
extern "C" void contract_write_borrow_source_data(address output,ContractBorrowSourceDataFields fields,ContractBorrowSourceDataWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.id) requires nonzero(append);
extern "C" ReadStatus contract_read_borrow_source_data(address context,ContractBorrowSourceDataFields fields,ContractBorrowSourceDataReader host) on cpu requires nonzero(host.id);
extern "C" void contract_write_borrow_source_parameter(address output,ContractBorrowSourceParameterFields fields,ContractBorrowSourceParameterWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.id) requires nonzero(host.indices) requires nonzero(append);
extern "C" ReadStatus contract_read_borrow_source_parameter(address context,ContractBorrowSourceParameterFields fields,ContractBorrowSourceParameterReader host) on cpu requires nonzero(host.id) requires nonzero(host.indices);
extern "C" void contract_write_borrow_source_unresolved(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_borrow_source_inactive(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_borrow_source_external(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void contract_write_borrow_source_position(address output,ContractBorrowSourcePositionFields fields,ContractBorrowSourcePositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(append);
extern "C" ReadStatus contract_read_borrow_source_position(address context,ContractBorrowSourcePositionFields fields,ContractBorrowSourcePositionReader host) on cpu requires nonzero(host.origin);
extern "C" void contract_write_borrow_source_element(address output,ContractBorrowSourceElementFields fields,ContractBorrowSourceElementWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parent) requires nonzero(host.fields) requires nonzero(host.index) requires nonzero(append);
extern "C" ReadStatus contract_read_borrow_source_element(address context,ContractBorrowSourceElementFields fields,ContractBorrowSourceElementReader host) on cpu requires nonzero(host.parent) requires nonzero(host.fields) requires nonzero(host.index);
extern "C" void contract_write_borrow_source_projection(address output,ContractBorrowSourceProjectionFields fields,ContractBorrowSourceProjectionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parent) requires nonzero(host.fields) requires nonzero(append);
extern "C" ReadStatus contract_read_borrow_source_projection(address context,ContractBorrowSourceProjectionFields fields,ContractBorrowSourceProjectionReader host) on cpu requires nonzero(host.parent) requires nonzero(host.fields);
extern "C" void contract_write_borrow_source_contents(address output,ContractBorrowSourceContentsFields fields,ContractBorrowSourceContentsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(host.word) requires nonzero(append);
extern "C" ReadStatus contract_read_borrow_source_contents(address context,ContractBorrowSourceContentsFields fields,ContractBorrowSourceContentsReader host) on cpu requires nonzero(host.origin) requires nonzero(host.word);
extern "C" void contract_write_borrow_source_relation(address output,ContractBorrowSourceRelationFields fields,ContractBorrowSourceRelationWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.graph) requires nonzero(append);
extern "C" ReadStatus contract_read_borrow_source_relation(address context,ContractBorrowSourceRelationFields fields,ContractBorrowSourceRelationReader host) on cpu requires nonzero(host.graph);
extern "C" void contract_write_borrow_source_task(address output,ContractBorrowSourceTaskFields fields,ContractBorrowSourceTaskWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.captures) requires nonzero(host.outputs) requires nonzero(host.initialized) requires nonzero(host.guards) requires nonzero(host.completions) requires nonzero(append);
extern "C" ReadStatus contract_read_borrow_source_task(address context,ContractBorrowSourceTaskFields fields,ContractBorrowSourceTaskReader host) on cpu requires nonzero(host.captures) requires nonzero(host.outputs) requires nonzero(host.initialized) requires nonzero(host.guards) requires nonzero(host.completions);
extern "C" void contract_write_borrow_source_task_output(address output,ContractBorrowSourceTaskOutputFields fields,ContractBorrowSourceTaskOutputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.task) requires nonzero(host.projection) requires nonzero(append);
extern "C" ReadStatus contract_read_borrow_source_task_output(address context,ContractBorrowSourceTaskOutputFields fields,ContractBorrowSourceTaskOutputReader host) on cpu requires nonzero(host.task) requires nonzero(host.projection);
extern "C" void contract_write_residency_contract(address output,ContractResidencyContractFields fields,ContractResidencyContractWriter host) on cpu requires nonzero(host.required) requires nonzero(host.calls);
extern "C" ReadStatus contract_read_residency_contract(address context,ContractResidencyContractFields fields,ContractResidencyContractReader host) on cpu requires nonzero(host.required) requires nonzero(host.calls);
extern "C" void contract_write_residency_call(address output,ContractResidencyCallFields fields,ContractResidencyCallWriter host) on cpu requires nonzero(host.function) requires nonzero(host.arguments) requires nonzero(host.bindings) requires nonzero(host.path);
extern "C" ReadStatus contract_read_residency_call(address context,ContractResidencyCallFields fields,ContractResidencyCallReader host) on cpu requires nonzero(host.function) requires nonzero(host.arguments) requires nonzero(host.bindings) requires nonzero(host.path);
extern "C" void contract_write_ordinary_contract(address output,ContractOrdinaryContractFields fields,ContractOrdinaryContractWriter host) on cpu requires nonzero(host.parameters) requires nonzero(host.result) requires nonzero(host.inputs) requires nonzero(host.element_inputs) requires nonzero(host.returned) requires nonzero(host.written) requires nonzero(host.requires) requires nonzero(host.target) requires nonzero(host.frame) requires nonzero(host.fallible) requires nonzero(host.domain) requires nonzero(host.residency) requires nonzero(host.view_access) requires nonzero(host.initialization);
extern "C" ReadStatus contract_read_ordinary_contract(address context,ContractOrdinaryContractFields fields,ContractOrdinaryContractReader host) on cpu requires nonzero(host.parameters) requires nonzero(host.result) requires nonzero(host.inputs) requires nonzero(host.element_inputs) requires nonzero(host.returned) requires nonzero(host.written) requires nonzero(host.requires) requires nonzero(host.target) requires nonzero(host.frame) requires nonzero(host.fallible) requires nonzero(host.domain) requires nonzero(host.residency) requires nonzero(host.view_access) requires nonzero(host.initialization);
extern "C" void contract_write_proof(address output,ContractProofFields fields,ContractProofWriter host) on cpu requires nonzero(host.origins) requires nonzero(host.conditions) requires nonzero(host.ranges) requires nonzero(host.nonzero) requires nonzero(host.domains) requires nonzero(host.views) requires nonzero(host.addresses);
extern "C" ReadStatus contract_read_proof(address context,ContractProofFields fields,ContractProofReader host) on cpu requires nonzero(host.origins) requires nonzero(host.conditions) requires nonzero(host.ranges) requires nonzero(host.nonzero) requires nonzero(host.domains) requires nonzero(host.views) requires nonzero(host.addresses);
extern "C" void contract_write_pair(address output,ContractPairFields fields,ContractPairWriter host) on cpu requires nonzero(host.first) requires nonzero(host.second);
extern "C" ReadStatus contract_read_pair(address context,ContractPairFields fields,ContractPairReader host) on cpu requires nonzero(host.first) requires nonzero(host.second);
extern "C" ReadStatus contract_read_address_root(Bytes input,inout u64 position,address context,address output,ContractAddressRootVariants host) on cpu requires nonzero(host.local) requires nonzero(host.parameter) requires nonzero(host.resource) requires nonzero(host.buffer) requires nonzero(host.derived) requires nonzero(host.external) requires nonzero(host.dataset);
extern "C" ReadStatus contract_read_initialization_source(Bytes input,inout u64 position,address context,address output,ContractInitializationSourceVariants host) on cpu requires nonzero(host.position) requires nonzero(host.task);
extern "C" ReadStatus contract_read_element_facts(Bytes input,inout u64 position,address context,address output,ContractElementFactsVariants host) on cpu requires nonzero(host.unknown) requires nonzero(host.known) requires nonzero(host.cleanup) requires nonzero(host.common) requires nonzero(host.selected) requires nonzero(host.input);
extern "C" ReadStatus contract_read_data_layout(Bytes input,inout u64 position,address context,address output,ContractDataLayoutVariants host) on cpu requires nonzero(host.declared) requires nonzero(host.parameter);
extern "C" ReadStatus contract_read_returned(Bytes input,inout u64 position,address context,address output,ContractReturnedVariants host) on cpu requires nonzero(host.unit) requires nonzero(host.value) requires nonzero(host.dataset) requires nonzero(host.owned_value);
extern "C" ReadStatus contract_read_task_projection(Bytes input,inout u64 position,address context,address output,ContractTaskProjectionVariants host) on cpu requires nonzero(host.returned) requires nonzero(host.writes) requires nonzero(host.places) requires nonzero(host.initialized);
extern "C" ReadStatus contract_read_exclusion(Bytes input,inout u64 position,address context,address output,ContractExclusionVariants host) on cpu requires nonzero(host.resource) requires nonzero(host.ownership) requires nonzero(host.position) requires nonzero(host.view);
extern "C" ReadStatus contract_read_view_access(Bytes input,inout u64 position,address context,address output,ContractViewAccessVariants host) on cpu requires nonzero(host.resource) requires nonzero(host.position) requires nonzero(host.memory) requires nonzero(host.dataset) requires nonzero(host.initialized) requires nonzero(host.stored);
extern "C" ReadStatus contract_read_residency_part(Bytes input,inout u64 position,address context,address output,ContractResidencyPartVariants host) on cpu requires nonzero(host.field) requires nonzero(host.elements) requires nonzero(host.selected);
extern "C" ReadStatus contract_read_residency_origin(Bytes input,inout u64 position,address context,address output,ContractResidencyOriginVariants host) on cpu requires nonzero(host.cpu) requires nonzero(host.gpu) requires nonzero(host.input) requires nonzero(host.output) requires nonzero(host.task_transfer);
extern "C" ReadStatus contract_read_residency(Bytes input,inout u64 position,address context,address output,ContractResidencyVariants host) on cpu requires nonzero(host.absent) requires nonzero(host.origins) requires nonzero(host.selected);
extern "C" ReadStatus contract_read_residency_output_kind(Bytes input,inout u64 position,address context,address output,ContractResidencyOutputKindVariants host) on cpu requires nonzero(host.returned) requires nonzero(host.written);
extern "C" ReadStatus contract_read_texture_dimension(Bytes input,inout u64 position,address context,address output,ContractTextureDimensionVariants host) on cpu requires nonzero(host.d2) requires nonzero(host.d3);
extern "C" ReadStatus contract_read_texture_format(Bytes input,inout u64 position,address context,address output,ContractTextureFormatVariants host) on cpu requires nonzero(host.rgba32float) requires nonzero(host.rgba8unorm) requires nonzero(host.rgba8srgb) requires nonzero(host.r8unorm) requires nonzero(host.rg16unorm) requires nonzero(host.rgb10a2unorm) requires nonzero(host.r11g11b10float);
extern "C" ReadStatus contract_read_element_fact(Bytes input,inout u64 position,address context,address output,ContractElementFactVariants host) on cpu requires nonzero(host.range) requires nonzero(host.index);
extern "C" ReadStatus contract_read_access(Bytes input,inout u64 position,address context,address output,ContractAccessVariants host) on cpu requires nonzero(host.value) requires nonzero(host.read) requires nonzero(host.write) requires nonzero(host.inout) requires nonzero(host.atomic);
extern "C" ReadStatus contract_read_target(Bytes input,inout u64 position,address context,address output,ContractTargetVariants host) on cpu requires nonzero(host.compute) requires nonzero(host.cpu) requires nonzero(host.gpu);
extern "C" ReadStatus contract_read_call_parameter_kind(Bytes input,inout u64 position,address context,address output,ContractCallParameterKindVariants host) on cpu requires nonzero(host.value) requires nonzero(host.reference);
extern "C" ReadStatus contract_read_contract_kind(Bytes input,inout u64 position,address context,address output,ContractContractKindVariants host) on cpu requires nonzero(host.requires) requires nonzero(host.ensures);
extern "C" ReadStatus contract_read_relation_op(Bytes input,inout u64 position,address context,address output,ContractRelationOpVariants host) on cpu requires nonzero(host.identity) requires nonzero(host.position) requires nonzero(host.read) requires nonzero(host.element) requires nonzero(host.projection) requires nonzero(host.resources) requires nonzero(host.owners) requires nonzero(host.select) requires nonzero(host.initial);
extern "C" ReadStatus contract_read_scalar(Bytes input,inout u64 position,address context,address output,ContractScalarVariants host) on cpu requires nonzero(host.f32) requires nonzero(host.u32) requires nonzero(host.i32) requires nonzero(host.bool) requires nonzero(host.u64) requires nonzero(host.i64) requires nonzero(host.f64) requires nonzero(host.u8) requires nonzero(host.i8) requires nonzero(host.u16) requires nonzero(host.i16);
extern "C" ReadStatus contract_read_ownership(Bytes input,inout u64 position,address context,address output,ContractOwnershipVariants host) on cpu requires nonzero(host.owned) requires nonzero(host.borrowed) requires nonzero(host.moved);
extern "C" ReadStatus contract_read_address_identity(Bytes input,inout u64 position,address context,address output,ContractAddressIdentityVariants host) on cpu requires nonzero(host.constant) requires nonzero(host.value) requires nonzero(host.storage);
extern "C" ReadStatus contract_read_borrow_source(Bytes input,inout u64 position,address context,address output,ContractBorrowSourceVariants host) on cpu requires nonzero(host.data) requires nonzero(host.parameter) requires nonzero(host.unresolved) requires nonzero(host.inactive) requires nonzero(host.external) requires nonzero(host.position) requires nonzero(host.element) requires nonzero(host.projection) requires nonzero(host.contents) requires nonzero(host.relation) requires nonzero(host.task) requires nonzero(host.task_output);
extern "C" void contract_write_ast_type(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant);
extern "C" void contract_write_ast_predicate(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant);
extern "C" void contract_write_ast_condition(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant);
extern "C" ReadStatus contract_read_ast_condition(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);

// Device requirements in the original resource image; only synchronous scalar fields.
struct SubgroupFields { u32 basic; u32 vote; u32 arithmetic; u32 ballot; u32 shuffle; u32 shuffle_relative; u32 extended_types; };
extern "C" void contract_write_subgroup_capabilities(SubgroupFields value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus contract_read_subgroup_capabilities(Bytes input,inout u64 position,inout SubgroupFields output) on cpu;
struct DeviceFields { u32 ray_query; u32 float64; u32 integer8; u32 integer16; u32 atomic64; u32 shared_atomic64; u32 float32_preserve; u32 float64_preserve; u32 sorting; u32 maximal_reconvergence; u32 texture_indexing; u32 position_fetch; SubgroupFields subgroup; };
extern "C" void contract_write_device_requirements(DeviceFields value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus contract_read_device_requirements(Bytes input,inout u64 position,inout DeviceFields output) on cpu;

// Resource layout projections borrow original fields for one synchronous call.
struct ResourceProgramLayoutFields {
 address write_regions;
 address name;
 address target;
 address buffers;
 address accelerations;
 address textures;
 address uniforms;
 address uniform_bytes;
 address prefix_requirements;
 address prefix_guarantees;
 address element_requirements;
 address element_guarantees;
 address length_requirements;
 address requirements;
};
struct ResourceProgramLayoutWriter {
 cfn<void,address,address> write_regions;
 cfn<void,address,address> name;
 cfn<void,address,address> target;
 cfn<void,address,address> buffers;
 cfn<void,address,address> accelerations;
 cfn<void,address,address> textures;
 cfn<void,address,address> uniforms;
 cfn<void,address,address> uniform_bytes;
 cfn<void,address,address> prefix_requirements;
 cfn<void,address,address> prefix_guarantees;
 cfn<void,address,address> element_requirements;
 cfn<void,address,address> element_guarantees;
 cfn<void,address,address> length_requirements;
 cfn<void,address,address> requirements;
};
struct ResourceProgramLayoutReader {
 cfn<ReadStatus,address,address> write_regions;
 cfn<ReadStatus,address,address> name;
 cfn<ReadStatus,address,address> target;
 cfn<ReadStatus,address,address> buffers;
 cfn<ReadStatus,address,address> accelerations;
 cfn<ReadStatus,address,address> textures;
 cfn<ReadStatus,address,address> uniforms;
 cfn<ReadStatus,address,address> uniform_bytes;
 cfn<ReadStatus,address,address> prefix_requirements;
 cfn<ReadStatus,address,address> prefix_guarantees;
 cfn<ReadStatus,address,address> element_requirements;
 cfn<ReadStatus,address,address> element_guarantees;
 cfn<ReadStatus,address,address> length_requirements;
 cfn<ReadStatus,address,address> requirements;
};
extern "C" void resource_write_program_layout(address output,ResourceProgramLayoutFields fields,ResourceProgramLayoutWriter host) on cpu requires nonzero(host.write_regions) requires nonzero(host.name) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.accelerations) requires nonzero(host.textures) requires nonzero(host.uniforms) requires nonzero(host.uniform_bytes) requires nonzero(host.prefix_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_requirements) requires nonzero(host.element_guarantees) requires nonzero(host.length_requirements) requires nonzero(host.requirements);
extern "C" ReadStatus resource_read_program_layout(address context,ResourceProgramLayoutFields fields,ResourceProgramLayoutReader host) on cpu requires nonzero(host.write_regions) requires nonzero(host.name) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.accelerations) requires nonzero(host.textures) requires nonzero(host.uniforms) requires nonzero(host.uniform_bytes) requires nonzero(host.prefix_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_requirements) requires nonzero(host.element_guarantees) requires nonzero(host.length_requirements) requires nonzero(host.requirements);

// Actual execution-plan fields borrowed for one synchronous format call.
struct ResourceReductionFields {
 address source;
 address source_word;
 address target;
 address target_word;
};
struct ResourceReductionWriter {
 cfn<void,address,address> source;
 cfn<void,address,address> source_word;
 cfn<void,address,address> target;
 cfn<void,address,address> target_word;
};
struct ResourceReductionReader {
 cfn<ReadStatus,address,address> source;
 cfn<ReadStatus,address,address> source_word;
 cfn<ReadStatus,address,address> target;
 cfn<ReadStatus,address,address> target_word;
};
extern "C" void resource_write_reduction(address output,ResourceReductionFields fields,ResourceReductionWriter host) on cpu requires nonzero(host.source) requires nonzero(host.source_word) requires nonzero(host.target) requires nonzero(host.target_word);
extern "C" ReadStatus resource_read_reduction(address context,ResourceReductionFields fields,ResourceReductionReader host) on cpu requires nonzero(host.source) requires nonzero(host.source_word) requires nonzero(host.target) requires nonzero(host.target_word);
struct ResourceReductionSpecFields {
 address fields;
 address ty;
 address operator;
};
struct ResourceReductionSpecWriter {
 cfn<void,address,address> fields;
 cfn<void,address,address> ty;
 cfn<void,address,address> operator;
};
struct ResourceReductionSpecReader {
 cfn<ReadStatus,address,address> fields;
 cfn<ReadStatus,address,address> ty;
 cfn<ReadStatus,address,address> operator;
};
extern "C" void resource_write_reduction_spec(address output,ResourceReductionSpecFields fields,ResourceReductionSpecWriter host) on cpu requires nonzero(host.fields) requires nonzero(host.ty) requires nonzero(host.operator);
extern "C" ReadStatus resource_read_reduction_spec(address context,ResourceReductionSpecFields fields,ResourceReductionSpecReader host) on cpu requires nonzero(host.fields) requires nonzero(host.ty) requires nonzero(host.operator);
struct ResourceSegmentedReductionFields {
 address values;
 address head_word;
 address count;
};
struct ResourceSegmentedReductionWriter {
 cfn<void,address,address> values;
 cfn<void,address,address> head_word;
 cfn<void,address,address> count;
};
struct ResourceSegmentedReductionReader {
 cfn<ReadStatus,address,address> values;
 cfn<ReadStatus,address,address> head_word;
 cfn<ReadStatus,address,address> count;
};
extern "C" void resource_write_segmented_reduction(address output,ResourceSegmentedReductionFields fields,ResourceSegmentedReductionWriter host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count);
extern "C" ReadStatus resource_read_segmented_reduction(address context,ResourceSegmentedReductionFields fields,ResourceSegmentedReductionReader host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count);
struct ResourceScanFields {
 address values;
 address ty;
 address operator;
 address head_word;
};
struct ResourceScanWriter {
 cfn<void,address,address> values;
 cfn<void,address,address> ty;
 cfn<void,address,address> operator;
 cfn<void,address,address> head_word;
};
struct ResourceScanReader {
 cfn<ReadStatus,address,address> values;
 cfn<ReadStatus,address,address> ty;
 cfn<ReadStatus,address,address> operator;
 cfn<ReadStatus,address,address> head_word;
};
extern "C" void resource_write_scan(address output,ResourceScanFields fields,ResourceScanWriter host) on cpu requires nonzero(host.values) requires nonzero(host.ty) requires nonzero(host.operator) requires nonzero(host.head_word);
extern "C" ReadStatus resource_read_scan(address context,ResourceScanFields fields,ResourceScanReader host) on cpu requires nonzero(host.values) requires nonzero(host.ty) requires nonzero(host.operator) requires nonzero(host.head_word);
struct ResourceCompactionFields {
 address source;
 address flags;
 address flag_word;
 address target;
 address count;
 address count_word;
};
struct ResourceCompactionWriter {
 cfn<void,address,address> source;
 cfn<void,address,address> flags;
 cfn<void,address,address> flag_word;
 cfn<void,address,address> target;
 cfn<void,address,address> count;
 cfn<void,address,address> count_word;
};
struct ResourceCompactionReader {
 cfn<ReadStatus,address,address> source;
 cfn<ReadStatus,address,address> flags;
 cfn<ReadStatus,address,address> flag_word;
 cfn<ReadStatus,address,address> target;
 cfn<ReadStatus,address,address> count;
 cfn<ReadStatus,address,address> count_word;
};
extern "C" void resource_write_compaction(address output,ResourceCompactionFields fields,ResourceCompactionWriter host) on cpu requires nonzero(host.source) requires nonzero(host.flags) requires nonzero(host.flag_word) requires nonzero(host.target) requires nonzero(host.count) requires nonzero(host.count_word);
extern "C" ReadStatus resource_read_compaction(address context,ResourceCompactionFields fields,ResourceCompactionReader host) on cpu requires nonzero(host.source) requires nonzero(host.flags) requires nonzero(host.flag_word) requires nonzero(host.target) requires nonzero(host.count) requires nonzero(host.count_word);
struct ResourceSortingFields {
 address source;
 address keys;
 address target;
};
struct ResourceSortingWriter {
 cfn<void,address,address> source;
 cfn<void,address,address> keys;
 cfn<void,address,address> target;
};
struct ResourceSortingReader {
 cfn<ReadStatus,address,address> source;
 cfn<ReadStatus,address,address> keys;
 cfn<ReadStatus,address,address> target;
};
extern "C" void resource_write_sorting(address output,ResourceSortingFields fields,ResourceSortingWriter host) on cpu requires nonzero(host.source) requires nonzero(host.keys) requires nonzero(host.target);
extern "C" ReadStatus resource_read_sorting(address context,ResourceSortingFields fields,ResourceSortingReader host) on cpu requires nonzero(host.source) requires nonzero(host.keys) requires nonzero(host.target);
struct ResourceReductionLayoutFields {
 address fields;
 address ty;
};
struct ResourceReductionLayoutWriter {
 cfn<void,address,address> fields;
 cfn<void,address,address> ty;
};
struct ResourceReductionLayoutReader {
 cfn<ReadStatus,address,address> fields;
 cfn<ReadStatus,address,address> ty;
};
extern "C" void resource_write_reduction_layout(address output,ResourceReductionLayoutFields fields,ResourceReductionLayoutWriter host) on cpu requires nonzero(host.fields) requires nonzero(host.ty);
extern "C" ReadStatus resource_read_reduction_layout(address context,ResourceReductionLayoutFields fields,ResourceReductionLayoutReader host) on cpu requires nonzero(host.fields) requires nonzero(host.ty);
struct ResourceCustomScanFields {
 address values;
 address head_word;
};
struct ResourceCustomScanWriter {
 cfn<void,address,address> values;
 cfn<void,address,address> head_word;
};
struct ResourceCustomScanReader {
 cfn<ReadStatus,address,address> values;
 cfn<ReadStatus,address,address> head_word;
};
extern "C" void resource_write_custom_scan(address output,ResourceCustomScanFields fields,ResourceCustomScanWriter host) on cpu requires nonzero(host.values) requires nonzero(host.head_word);
extern "C" ReadStatus resource_read_custom_scan(address context,ResourceCustomScanFields fields,ResourceCustomScanReader host) on cpu requires nonzero(host.values) requires nonzero(host.head_word);
struct ResourceCustomSegmentedReductionFields {
 address values;
 address head_word;
 address count;
};
struct ResourceCustomSegmentedReductionWriter {
 cfn<void,address,address> values;
 cfn<void,address,address> head_word;
 cfn<void,address,address> count;
};
struct ResourceCustomSegmentedReductionReader {
 cfn<ReadStatus,address,address> values;
 cfn<ReadStatus,address,address> head_word;
 cfn<ReadStatus,address,address> count;
};
extern "C" void resource_write_custom_segmented_reduction(address output,ResourceCustomSegmentedReductionFields fields,ResourceCustomSegmentedReductionWriter host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count);
extern "C" ReadStatus resource_read_custom_segmented_reduction(address context,ResourceCustomSegmentedReductionFields fields,ResourceCustomSegmentedReductionReader host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count);
struct ResourceSelectionFields {
 address binding;
 address word;
};
struct ResourceSelectionWriter {
 cfn<void,address,address> binding;
 cfn<void,address,address> word;
};
struct ResourceSelectionReader {
 cfn<ReadStatus,address,address> binding;
 cfn<ReadStatus,address,address> word;
};
extern "C" void resource_write_selection(address output,ResourceSelectionFields fields,ResourceSelectionWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.word);
extern "C" ReadStatus resource_read_selection(address context,ResourceSelectionFields fields,ResourceSelectionReader host) on cpu requires nonzero(host.binding) requires nonzero(host.word);
struct ResourceExecutionStageFields {
 address domain;
 address prefix;
 address selection;
 address operation;
 address reads;
 address writes;
 address write_regions;
 address prefix_guarantees;
 address element_guarantees;
 address inputs;
 address tile_scope;
 address width;
 address shared_bytes;
};
struct ResourceExecutionStageWriter {
 cfn<void,address,address> domain;
 cfn<void,address,address> prefix;
 cfn<void,address,address> selection;
 cfn<void,address,address> operation;
 cfn<void,address,address> reads;
 cfn<void,address,address> writes;
 cfn<void,address,address> write_regions;
 cfn<void,address,address> prefix_guarantees;
 cfn<void,address,address> element_guarantees;
 cfn<void,address,address> inputs;
 cfn<void,address,address> tile_scope;
 cfn<void,address,address> width;
 cfn<void,address,address> shared_bytes;
};
struct ResourceExecutionStageReader {
 cfn<ReadStatus,address,address> domain;
 cfn<ReadStatus,address,address> prefix;
 cfn<ReadStatus,address,address> selection;
 cfn<ReadStatus,address,address> operation;
 cfn<ReadStatus,address,address> reads;
 cfn<ReadStatus,address,address> writes;
 cfn<ReadStatus,address,address> write_regions;
 cfn<ReadStatus,address,address> prefix_guarantees;
 cfn<ReadStatus,address,address> element_guarantees;
 cfn<ReadStatus,address,address> inputs;
 cfn<ReadStatus,address,address> tile_scope;
 cfn<ReadStatus,address,address> width;
 cfn<ReadStatus,address,address> shared_bytes;
};
extern "C" void resource_write_execution_stage(address output,ResourceExecutionStageFields fields,ResourceExecutionStageWriter host) on cpu requires nonzero(host.domain) requires nonzero(host.prefix) requires nonzero(host.selection) requires nonzero(host.operation) requires nonzero(host.reads) requires nonzero(host.writes) requires nonzero(host.write_regions) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_guarantees) requires nonzero(host.inputs) requires nonzero(host.tile_scope) requires nonzero(host.width) requires nonzero(host.shared_bytes);
extern "C" ReadStatus resource_read_execution_stage(address context,ResourceExecutionStageFields fields,ResourceExecutionStageReader host) on cpu requires nonzero(host.domain) requires nonzero(host.prefix) requires nonzero(host.selection) requires nonzero(host.operation) requires nonzero(host.reads) requires nonzero(host.writes) requires nonzero(host.write_regions) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_guarantees) requires nonzero(host.inputs) requires nonzero(host.tile_scope) requires nonzero(host.width) requires nonzero(host.shared_bytes);
struct ResourceExecutionPlanFields {
 address interface;
 address stages;
 address indirect_buffer;
 address auxiliary_buffers;
 address native_frame;
};
struct ResourceExecutionPlanWriter {
 cfn<void,address,address> interface;
 cfn<void,address,address> stages;
 cfn<void,address,address> indirect_buffer;
 cfn<void,address,address> auxiliary_buffers;
 cfn<void,address,address> native_frame;
};
struct ResourceExecutionPlanReader {
 cfn<ReadStatus,address,address> interface;
 cfn<ReadStatus,address,address> stages;
 cfn<ReadStatus,address,address> indirect_buffer;
 cfn<ReadStatus,address,address> auxiliary_buffers;
 cfn<ReadStatus,address,address> native_frame;
};
extern "C" void resource_write_execution_plan(address output,ResourceExecutionPlanFields fields,ResourceExecutionPlanWriter host) on cpu requires nonzero(host.interface) requires nonzero(host.stages) requires nonzero(host.indirect_buffer) requires nonzero(host.auxiliary_buffers) requires nonzero(host.native_frame);
extern "C" ReadStatus resource_read_execution_plan(address context,ResourceExecutionPlanFields fields,ResourceExecutionPlanReader host) on cpu requires nonzero(host.interface) requires nonzero(host.stages) requires nonzero(host.indirect_buffer) requires nonzero(host.auxiliary_buffers) requires nonzero(host.native_frame);
struct ResourceSortOrderReader {cfn<void,address> ascending;cfn<void,address> descending;};
extern "C" void resource_write_sort_order_ascending(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void resource_write_sort_order_descending(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus resource_read_sort_order(Bytes input,inout u64 position,address output,ResourceSortOrderReader host) on cpu requires nonzero(host.ascending) requires nonzero(host.descending);
struct ResourceCollectiveOperatorReader {cfn<void,address> sum;cfn<void,address> min;cfn<void,address> max;cfn<void,address> bit_and;cfn<void,address> bit_or;cfn<void,address> bit_xor;};
extern "C" void resource_write_collective_operator_sum(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void resource_write_collective_operator_min(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void resource_write_collective_operator_max(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void resource_write_collective_operator_bit_and(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void resource_write_collective_operator_bit_or(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void resource_write_collective_operator_bit_xor(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus resource_read_collective_operator(Bytes input,inout u64 position,address output,ResourceCollectiveOperatorReader host) on cpu requires nonzero(host.sum) requires nonzero(host.min) requires nonzero(host.max) requires nonzero(host.bit_and) requires nonzero(host.bit_or) requires nonzero(host.bit_xor);
struct ResourceOperationReader {cfn<ReadStatus,address,address> map;cfn<ReadStatus,address,address> reduce;cfn<ReadStatus,address,address> custom_reduce;cfn<ReadStatus,address,address> custom_scan;cfn<ReadStatus,address,address> custom_segmented_reduce;cfn<ReadStatus,address,address> segmented_reduce;cfn<ReadStatus,address,address> scan_exclusive;cfn<ReadStatus,address,address> compact;cfn<ReadStatus,address,address> stable_sort;};
extern "C" void resource_write_operation_map(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" void resource_write_operation_reduce(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);
extern "C" void resource_write_operation_custom_reduce(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);
extern "C" void resource_write_operation_custom_scan(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);
extern "C" void resource_write_operation_custom_segmented_reduce(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);
extern "C" void resource_write_operation_segmented_reduce(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);
extern "C" void resource_write_operation_scan_exclusive(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);
extern "C" void resource_write_operation_compact(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);
extern "C" void resource_write_operation_stable_sort(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);
extern "C" ReadStatus resource_read_operation(Bytes input,inout u64 position,address context,address output,ResourceOperationReader host) on cpu requires nonzero(host.map) requires nonzero(host.reduce) requires nonzero(host.custom_reduce) requires nonzero(host.custom_scan) requires nonzero(host.custom_segmented_reduce) requires nonzero(host.segmented_reduce) requires nonzero(host.scan_exclusive) requires nonzero(host.compact) requires nonzero(host.stable_sort);
struct ContractTripleFields {address first;address second;address third;};
struct ContractTripleWriter {cfn<void,address,address> first;cfn<void,address,address> second;cfn<void,address,address> third;};
struct ContractTripleReader {cfn<ReadStatus,address,address> first;cfn<ReadStatus,address,address> second;cfn<ReadStatus,address,address> third;};
extern "C" void contract_write_triple(address output,ContractTripleFields fields,ContractTripleWriter host) on cpu requires nonzero(host.first) requires nonzero(host.second) requires nonzero(host.third);
extern "C" ReadStatus contract_read_triple(address context,ContractTripleFields fields,ContractTripleReader host) on cpu requires nonzero(host.first) requires nonzero(host.second) requires nonzero(host.third);
