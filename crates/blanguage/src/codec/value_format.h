module codec.value_format;
// Borrowed typed projections and construction slots for the shared value format.
// Callers retain original inputs, containers and the sole Reader position.

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

enum ReadStatus:u32 { Success=0; Overflow=1; Truncated=2; Boolean=3; Scalar=4; Access=5; Target=6; Shape=7; Argument=8; Expression=9; Predicate=10; Condition=11; RustError=12; Identity=13; Operand=14; IntegerOperation=15; IntegerExpression=16; Comparison=17; FloatingForm=18; IndexDomain=19; Fact=20; ConditionVersion=21; ConditionNode=22; ContractAddressRoot=23; ContractInitializationSource=24; ContractElementFacts=25; ContractDataLayout=26; ContractReturned=27; ContractTaskProjection=28; ContractExclusion=29; ContractViewAccess=30; ContractResidencyPart=31; ContractResidencyOrigin=32; ContractResidency=33; ContractResidencyOutputKind=34; ContractTextureDimension=35; ContractTextureFormat=36; ContractElementFact=37; ContractCallParameterKind=38; ContractContractKind=39; ContractRelationOp=40; ContractOwnership=41; ContractAddressIdentity=42; ContractBorrowSource=43; ResourceSortOrder=44; ResourceCollectiveOperator=45; ResourceOperation=46; ResourceImage=47; ResourceLinkedImage=48; ResourceModuleTarget=49; ResourceNativeSymbol=50; ResourceRangeResult=51; ResourceContinuationCode=52; ResourceCpuStageCode=53;  };

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

struct ContractPairFields {address first;address second;};

struct ContractPairWriter {cfn<void,address,address> first;cfn<void,address,address> second;};

struct ContractPairReader {cfn<ReadStatus,address,address> first;cfn<ReadStatus,address,address> second;};

struct ContractTextureDimensionVariants{cfn<ReadStatus,address,address> d2;cfn<ReadStatus,address,address> d3;};

struct ContractTextureFormatVariants{cfn<ReadStatus,address,address> rgba32float;cfn<ReadStatus,address,address> rgba8unorm;cfn<ReadStatus,address,address> rgba8srgb;cfn<ReadStatus,address,address> r8unorm;cfn<ReadStatus,address,address> rg16unorm;cfn<ReadStatus,address,address> rgb10a2unorm;cfn<ReadStatus,address,address> r11g11b10float;cfn<ReadStatus,address,address> r16float;};

struct ContractElementFactVariants{cfn<ReadStatus,address,address> range;cfn<ReadStatus,address,address> index;};

struct ContractAccessVariants{cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> read;cfn<ReadStatus,address,address> write;cfn<ReadStatus,address,address> inout;cfn<ReadStatus,address,address> atomic;};

struct ContractTargetVariants{cfn<ReadStatus,address,address> compute;cfn<ReadStatus,address,address> cpu;cfn<ReadStatus,address,address> gpu;};

struct ContractCallParameterKindVariants{cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> reference;};

struct ContractContractKindVariants{cfn<ReadStatus,address,address> requires;cfn<ReadStatus,address,address> ensures;};

struct ContractScalarVariants{cfn<ReadStatus,address,address> f32;cfn<ReadStatus,address,address> u32;cfn<ReadStatus,address,address> i32;cfn<ReadStatus,address,address> bool;cfn<ReadStatus,address,address> u64;cfn<ReadStatus,address,address> i64;cfn<ReadStatus,address,address> f64;cfn<ReadStatus,address,address> u8;cfn<ReadStatus,address,address> i8;cfn<ReadStatus,address,address> u16;cfn<ReadStatus,address,address> i16;};

struct SubgroupFields { u32 basic; u32 vote; u32 arithmetic; u32 ballot; u32 shuffle; u32 shuffle_relative; u32 extended_types; };

struct DeviceFields { u32 ray_query; u32 float64; u32 integer8; u32 integer16; u32 atomic64; u32 shared_atomic64; u32 float32_preserve; u32 float64_preserve; u32 sorting; u32 maximal_reconvergence; u32 texture_indexing; u32 position_fetch; SubgroupFields subgroup; };

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

struct ResourceSortOrderReader {cfn<void,address> ascending;cfn<void,address> descending;};

struct ResourceCollectiveOperatorReader {cfn<void,address> sum;cfn<void,address> min;cfn<void,address> max;cfn<void,address> bit_and;cfn<void,address> bit_or;cfn<void,address> bit_xor;};

struct ResourceOperationReader {cfn<ReadStatus,address,address> map;cfn<ReadStatus,address,address> reduce;cfn<ReadStatus,address,address> custom_reduce;cfn<ReadStatus,address,address> custom_scan;cfn<ReadStatus,address,address> custom_segmented_reduce;cfn<ReadStatus,address,address> segmented_reduce;cfn<ReadStatus,address,address> scan_exclusive;cfn<ReadStatus,address,address> compact;cfn<ReadStatus,address,address> stable_sort;};

struct ContractTripleFields {address first;address second;address third;};

struct ContractTripleWriter {cfn<void,address,address> first;cfn<void,address,address> second;cfn<void,address,address> third;};

struct ContractTripleReader {cfn<ReadStatus,address,address> first;cfn<ReadStatus,address,address> second;cfn<ReadStatus,address,address> third;};

struct ResourceExternalNameFields {address library;address symbol;};

struct ResourceExternalNameWriter {cfn<void,address,address> library;cfn<void,address,address> symbol;};

struct ResourceExternalNameReader {cfn<ReadStatus,address,address> library;cfn<ReadStatus,address,address> symbol;};

struct ResourceModuleCodeFields {address sections;address symbols;};

struct ResourceModuleCodeWriter {cfn<void,address,address> sections;cfn<void,address,address> symbols;};

struct ResourceModuleCodeReader {cfn<ReadStatus,address,address> sections;cfn<ReadStatus,address,address> symbols;};

struct ResourceModuleSectionFields {address name;address bytes;address flags;address relocations;};

struct ResourceModuleSectionWriter {cfn<void,address,address> name;cfn<void,address,address> bytes;cfn<void,address,address> flags;cfn<void,address,address> relocations;};

struct ResourceModuleSectionReader {cfn<ReadStatus,address,address> name;cfn<ReadStatus,address,address> bytes;cfn<ReadStatus,address,address> flags;cfn<ReadStatus,address,address> relocations;};

struct ResourceModuleRelocationFields {address offset;address kind;address target;};

struct ResourceModuleRelocationWriter {cfn<void,address,address> offset;cfn<void,address,address> kind;cfn<void,address,address> target;};

struct ResourceModuleRelocationReader {cfn<ReadStatus,address,address> offset;cfn<ReadStatus,address,address> kind;cfn<ReadStatus,address,address> target;};

struct ResourceNativeRelocationFields {address offset;address target;};

struct ResourceNativeRelocationWriter {cfn<void,address,address> offset;cfn<void,address,address> target;};

struct ResourceNativeRelocationReader {cfn<ReadStatus,address,address> offset;cfn<ReadStatus,address,address> target;};

struct ResourceNativeCodeFields {address bytes;address relocations;address stack_bytes;address calls;address function_end;address unwind;};

struct ResourceNativeCodeWriter {cfn<void,address,address> bytes;cfn<void,address,address> relocations;cfn<void,address,address> stack_bytes;cfn<void,address,address> calls;cfn<void,address,address> function_end;cfn<void,address,address> unwind;};

struct ResourceNativeCodeReader {cfn<ReadStatus,address,address> bytes;cfn<ReadStatus,address,address> relocations;cfn<ReadStatus,address,address> stack_bytes;cfn<ReadStatus,address,address> calls;cfn<ReadStatus,address,address> function_end;cfn<ReadStatus,address,address> unwind;};

struct ResourceSortCodeFields {address source;address target;address initialize;address histogram;address scatter;address gather;};

struct ResourceSortCodeWriter {cfn<void,address,address> source;cfn<void,address,address> target;cfn<void,address,address> initialize;cfn<void,address,address> histogram;cfn<void,address,address> scatter;cfn<void,address,address> gather;};

struct ResourceSortCodeReader {cfn<ReadStatus,address,address> source;cfn<ReadStatus,address,address> target;cfn<ReadStatus,address,address> initialize;cfn<ReadStatus,address,address> histogram;cfn<ReadStatus,address,address> scatter;cfn<ReadStatus,address,address> gather;};

struct ResourceShaderCodeFields {address words;address ranks;};

struct ResourceShaderCodeWriter {cfn<void,address,address> words;cfn<void,address,address> ranks;};

struct ResourceShaderCodeReader {cfn<ReadStatus,address,address> words;cfn<ReadStatus,address,address> ranks;};

struct ResourceGpuStageCodeFields {address main;address tail;address scan_add;address finish;address dispatch;address selector;address additional;};

struct ResourceGpuStageCodeWriter {cfn<void,address,address> main;cfn<void,address,address> tail;cfn<void,address,address> scan_add;cfn<void,address,address> finish;cfn<void,address,address> dispatch;cfn<void,address,address> selector;cfn<void,address,address> additional;};

struct ResourceGpuStageCodeReader {cfn<ReadStatus,address,address> main;cfn<ReadStatus,address,address> tail;cfn<ReadStatus,address,address> scan_add;cfn<ReadStatus,address,address> finish;cfn<ReadStatus,address,address> dispatch;cfn<ReadStatus,address,address> selector;cfn<ReadStatus,address,address> additional;};

struct ResourceModuleFields {address types;address images;};

struct ResourceModuleWriter {cfn<void,address,address> types;cfn<void,address,address> images;};

struct ResourceModuleReader {cfn<ReadStatus,address,address> types;cfn<ReadStatus,address,address> images;};

struct ResourceImageVariants {cfn<ReadStatus,address,address> cpu;cfn<ReadStatus,address,address> gpu;};

struct ResourceModuleTargetVariants {cfn<ReadStatus,address,address> section;cfn<ReadStatus,address,address> import;};

struct ResourceModuleTargetSectionFieldsFields {address section;address value;};

struct ResourceModuleTargetSectionFieldsWriter {cfn<void,address,address> section;cfn<void,address,address> value;};

struct ResourceModuleTargetSectionFieldsReader {cfn<ReadStatus,address,address> section;cfn<ReadStatus,address,address> value;};

struct ResourceNativeSymbolVariants {cfn<ReadStatus,address,address> print;cfn<ReadStatus,address,address> external;cfn<ReadStatus,address,address> function;};

struct ResourceRangeResultVariants {cfn<ReadStatus,address,address> none;cfn<ReadStatus,address,address> values;};

struct ResourceContinuationCodeVariants {cfn<ReadStatus,address,address> reduction_tail;cfn<ReadStatus,address,address> scan_add;cfn<ReadStatus,address,address> compact_scatter;cfn<ReadStatus,address,address> segmented_finish;cfn<ReadStatus,address,address> scan;};

struct ResourceContinuationCodeScanFieldsFields {address prefix;address apply;address source;address target;};

struct ResourceContinuationCodeScanFieldsWriter {cfn<void,address,address> prefix;cfn<void,address,address> apply;cfn<void,address,address> source;cfn<void,address,address> target;};

struct ResourceContinuationCodeScanFieldsReader {cfn<ReadStatus,address,address> prefix;cfn<ReadStatus,address,address> apply;cfn<ReadStatus,address,address> source;cfn<ReadStatus,address,address> target;};

struct ResourceCpuStageCodeVariants {cfn<ReadStatus,address,address> element;cfn<ReadStatus,address,address> sort;};

struct ResourceCpuStageCodeElementFieldsFields {address main;address continuation;address result;};

struct ResourceCpuStageCodeElementFieldsWriter {cfn<void,address,address> main;cfn<void,address,address> continuation;cfn<void,address,address> result;};

struct ResourceCpuStageCodeElementFieldsReader {cfn<ReadStatus,address,address> main;cfn<ReadStatus,address,address> continuation;cfn<ReadStatus,address,address> result;};

struct ResourceNativeImageFields {address plan;address stages;address functions;address resources;address module_code;};

struct ResourceNativeImageWriter {cfn<void,address,address> plan;cfn<void,address,address> stages;cfn<void,address,address> functions;cfn<void,address,address> resources;cfn<void,address,address> module_code;};

struct ResourceNativeImageReader {cfn<ReadStatus,address,address> plan;cfn<ReadStatus,address,address> stages;cfn<ReadStatus,address,address> functions;cfn<ReadStatus,address,address> resources;cfn<ReadStatus,address,address> module_code;};

struct ResourceGpuImageFields {address plan;address stages;};

struct ResourceGpuImageWriter {cfn<void,address,address> plan;cfn<void,address,address> stages;};

struct ResourceGpuImageReader {cfn<ReadStatus,address,address> plan;cfn<ReadStatus,address,address> stages;};

extern "C" void write_type(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant);

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

extern "C" void contract_write_sequence(u64 count,address iterator,address output,cfn<address,address> next,cfn<void,address,address> element,cfn<void,address,address,u64> append) on cpu requires nonzero(next) requires nonzero(element) requires nonzero(append);

extern "C" ReadStatus contract_read_sequence(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address> push) on cpu requires nonzero(push);

extern "C" void contract_write_option(address value,address output,cfn<void,address,address> element,cfn<void,address,address,u64> append) on cpu requires nonzero(element) requires nonzero(append);

extern "C" ReadStatus contract_read_option(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address> some,cfn<void,address> none) on cpu requires nonzero(some) requires nonzero(none);

extern "C" void contract_write_trimmed(Span values,address output,cfn<u32,address> is_empty,cfn<void,address,address> element,cfn<void,address,address,u64> append) on cpu requires nonzero(is_empty) requires nonzero(element) requires nonzero(append);

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

extern "C" void contract_write_texture_format_r16float(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

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

extern "C" void contract_write_pair(address output,ContractPairFields fields,ContractPairWriter host) on cpu requires nonzero(host.first) requires nonzero(host.second);

extern "C" ReadStatus contract_read_pair(address context,ContractPairFields fields,ContractPairReader host) on cpu requires nonzero(host.first) requires nonzero(host.second);

extern "C" ReadStatus contract_read_texture_dimension(Bytes input,inout u64 position,address context,address output,ContractTextureDimensionVariants host) on cpu requires nonzero(host.d2) requires nonzero(host.d3);

extern "C" ReadStatus contract_read_texture_format(Bytes input,inout u64 position,address context,address output,ContractTextureFormatVariants host) on cpu requires nonzero(host.rgba32float) requires nonzero(host.rgba8unorm) requires nonzero(host.rgba8srgb) requires nonzero(host.r8unorm) requires nonzero(host.rg16unorm) requires nonzero(host.rgb10a2unorm) requires nonzero(host.r11g11b10float) requires nonzero(host.r16float);

extern "C" ReadStatus contract_read_element_fact(Bytes input,inout u64 position,address context,address output,ContractElementFactVariants host) on cpu requires nonzero(host.range) requires nonzero(host.index);

extern "C" ReadStatus contract_read_access(Bytes input,inout u64 position,address context,address output,ContractAccessVariants host) on cpu requires nonzero(host.value) requires nonzero(host.read) requires nonzero(host.write) requires nonzero(host.inout) requires nonzero(host.atomic);

extern "C" ReadStatus contract_read_target(Bytes input,inout u64 position,address context,address output,ContractTargetVariants host) on cpu requires nonzero(host.compute) requires nonzero(host.cpu) requires nonzero(host.gpu);

extern "C" ReadStatus contract_read_call_parameter_kind(Bytes input,inout u64 position,address context,address output,ContractCallParameterKindVariants host) on cpu requires nonzero(host.value) requires nonzero(host.reference);

extern "C" ReadStatus contract_read_contract_kind(Bytes input,inout u64 position,address context,address output,ContractContractKindVariants host) on cpu requires nonzero(host.requires) requires nonzero(host.ensures);

extern "C" ReadStatus contract_read_scalar(Bytes input,inout u64 position,address context,address output,ContractScalarVariants host) on cpu requires nonzero(host.f32) requires nonzero(host.u32) requires nonzero(host.i32) requires nonzero(host.bool) requires nonzero(host.u64) requires nonzero(host.i64) requires nonzero(host.f64) requires nonzero(host.u8) requires nonzero(host.i8) requires nonzero(host.u16) requires nonzero(host.i16);

extern "C" void contract_write_ast_type(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant);

extern "C" void contract_write_ast_predicate(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant);

extern "C" void contract_write_ast_condition(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant);

extern "C" ReadStatus contract_read_ast_condition(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span);

extern "C" void contract_write_subgroup_capabilities(SubgroupFields value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus contract_read_subgroup_capabilities(Bytes input,inout u64 position,inout SubgroupFields output) on cpu;

extern "C" void contract_write_device_requirements(DeviceFields value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus contract_read_device_requirements(Bytes input,inout u64 position,inout DeviceFields output) on cpu;

extern "C" void resource_write_program_layout(address output,ResourceProgramLayoutFields fields,ResourceProgramLayoutWriter host) on cpu requires nonzero(host.write_regions) requires nonzero(host.name) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.accelerations) requires nonzero(host.textures) requires nonzero(host.uniforms) requires nonzero(host.uniform_bytes) requires nonzero(host.prefix_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_requirements) requires nonzero(host.element_guarantees) requires nonzero(host.length_requirements) requires nonzero(host.requirements);

extern "C" ReadStatus resource_read_program_layout(address context,ResourceProgramLayoutFields fields,ResourceProgramLayoutReader host) on cpu requires nonzero(host.write_regions) requires nonzero(host.name) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.accelerations) requires nonzero(host.textures) requires nonzero(host.uniforms) requires nonzero(host.uniform_bytes) requires nonzero(host.prefix_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_requirements) requires nonzero(host.element_guarantees) requires nonzero(host.length_requirements) requires nonzero(host.requirements);

extern "C" void resource_write_reduction(address output,ResourceReductionFields fields,ResourceReductionWriter host) on cpu requires nonzero(host.source) requires nonzero(host.source_word) requires nonzero(host.target) requires nonzero(host.target_word);

extern "C" ReadStatus resource_read_reduction(address context,ResourceReductionFields fields,ResourceReductionReader host) on cpu requires nonzero(host.source) requires nonzero(host.source_word) requires nonzero(host.target) requires nonzero(host.target_word);

extern "C" void resource_write_reduction_spec(address output,ResourceReductionSpecFields fields,ResourceReductionSpecWriter host) on cpu requires nonzero(host.fields) requires nonzero(host.ty) requires nonzero(host.operator);

extern "C" ReadStatus resource_read_reduction_spec(address context,ResourceReductionSpecFields fields,ResourceReductionSpecReader host) on cpu requires nonzero(host.fields) requires nonzero(host.ty) requires nonzero(host.operator);

extern "C" void resource_write_segmented_reduction(address output,ResourceSegmentedReductionFields fields,ResourceSegmentedReductionWriter host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count);

extern "C" ReadStatus resource_read_segmented_reduction(address context,ResourceSegmentedReductionFields fields,ResourceSegmentedReductionReader host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count);

extern "C" void resource_write_scan(address output,ResourceScanFields fields,ResourceScanWriter host) on cpu requires nonzero(host.values) requires nonzero(host.ty) requires nonzero(host.operator) requires nonzero(host.head_word);

extern "C" ReadStatus resource_read_scan(address context,ResourceScanFields fields,ResourceScanReader host) on cpu requires nonzero(host.values) requires nonzero(host.ty) requires nonzero(host.operator) requires nonzero(host.head_word);

extern "C" void resource_write_compaction(address output,ResourceCompactionFields fields,ResourceCompactionWriter host) on cpu requires nonzero(host.source) requires nonzero(host.flags) requires nonzero(host.flag_word) requires nonzero(host.target) requires nonzero(host.count) requires nonzero(host.count_word);

extern "C" ReadStatus resource_read_compaction(address context,ResourceCompactionFields fields,ResourceCompactionReader host) on cpu requires nonzero(host.source) requires nonzero(host.flags) requires nonzero(host.flag_word) requires nonzero(host.target) requires nonzero(host.count) requires nonzero(host.count_word);

extern "C" void resource_write_sorting(address output,ResourceSortingFields fields,ResourceSortingWriter host) on cpu requires nonzero(host.source) requires nonzero(host.keys) requires nonzero(host.target);

extern "C" ReadStatus resource_read_sorting(address context,ResourceSortingFields fields,ResourceSortingReader host) on cpu requires nonzero(host.source) requires nonzero(host.keys) requires nonzero(host.target);

extern "C" void resource_write_reduction_layout(address output,ResourceReductionLayoutFields fields,ResourceReductionLayoutWriter host) on cpu requires nonzero(host.fields) requires nonzero(host.ty);

extern "C" ReadStatus resource_read_reduction_layout(address context,ResourceReductionLayoutFields fields,ResourceReductionLayoutReader host) on cpu requires nonzero(host.fields) requires nonzero(host.ty);

extern "C" void resource_write_custom_scan(address output,ResourceCustomScanFields fields,ResourceCustomScanWriter host) on cpu requires nonzero(host.values) requires nonzero(host.head_word);

extern "C" ReadStatus resource_read_custom_scan(address context,ResourceCustomScanFields fields,ResourceCustomScanReader host) on cpu requires nonzero(host.values) requires nonzero(host.head_word);

extern "C" void resource_write_custom_segmented_reduction(address output,ResourceCustomSegmentedReductionFields fields,ResourceCustomSegmentedReductionWriter host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count);

extern "C" ReadStatus resource_read_custom_segmented_reduction(address context,ResourceCustomSegmentedReductionFields fields,ResourceCustomSegmentedReductionReader host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count);

extern "C" void resource_write_selection(address output,ResourceSelectionFields fields,ResourceSelectionWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.word);

extern "C" ReadStatus resource_read_selection(address context,ResourceSelectionFields fields,ResourceSelectionReader host) on cpu requires nonzero(host.binding) requires nonzero(host.word);

extern "C" void resource_write_execution_stage(address output,ResourceExecutionStageFields fields,ResourceExecutionStageWriter host) on cpu requires nonzero(host.domain) requires nonzero(host.prefix) requires nonzero(host.selection) requires nonzero(host.operation) requires nonzero(host.reads) requires nonzero(host.writes) requires nonzero(host.write_regions) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_guarantees) requires nonzero(host.inputs) requires nonzero(host.tile_scope) requires nonzero(host.width) requires nonzero(host.shared_bytes);

extern "C" ReadStatus resource_read_execution_stage(address context,ResourceExecutionStageFields fields,ResourceExecutionStageReader host) on cpu requires nonzero(host.domain) requires nonzero(host.prefix) requires nonzero(host.selection) requires nonzero(host.operation) requires nonzero(host.reads) requires nonzero(host.writes) requires nonzero(host.write_regions) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_guarantees) requires nonzero(host.inputs) requires nonzero(host.tile_scope) requires nonzero(host.width) requires nonzero(host.shared_bytes);

extern "C" void resource_write_execution_plan(address output,ResourceExecutionPlanFields fields,ResourceExecutionPlanWriter host) on cpu requires nonzero(host.interface) requires nonzero(host.stages) requires nonzero(host.indirect_buffer) requires nonzero(host.auxiliary_buffers) requires nonzero(host.native_frame);

extern "C" ReadStatus resource_read_execution_plan(address context,ResourceExecutionPlanFields fields,ResourceExecutionPlanReader host) on cpu requires nonzero(host.interface) requires nonzero(host.stages) requires nonzero(host.indirect_buffer) requires nonzero(host.auxiliary_buffers) requires nonzero(host.native_frame);

extern "C" void resource_write_sort_order_ascending(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void resource_write_sort_order_descending(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus resource_read_sort_order(Bytes input,inout u64 position,address output,ResourceSortOrderReader host) on cpu requires nonzero(host.ascending) requires nonzero(host.descending);

extern "C" void resource_write_collective_operator_sum(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void resource_write_collective_operator_min(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void resource_write_collective_operator_max(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void resource_write_collective_operator_bit_and(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void resource_write_collective_operator_bit_or(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void resource_write_collective_operator_bit_xor(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus resource_read_collective_operator(Bytes input,inout u64 position,address output,ResourceCollectiveOperatorReader host) on cpu requires nonzero(host.sum) requires nonzero(host.min) requires nonzero(host.max) requires nonzero(host.bit_and) requires nonzero(host.bit_or) requires nonzero(host.bit_xor);

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

extern "C" void contract_write_triple(address output,ContractTripleFields fields,ContractTripleWriter host) on cpu requires nonzero(host.first) requires nonzero(host.second) requires nonzero(host.third);

extern "C" ReadStatus contract_read_triple(address context,ContractTripleFields fields,ContractTripleReader host) on cpu requires nonzero(host.first) requires nonzero(host.second) requires nonzero(host.third);

extern "C" void resource_write_external_name(address output,ResourceExternalNameFields fields,ResourceExternalNameWriter host) on cpu requires nonzero(host.library) requires nonzero(host.symbol);

extern "C" ReadStatus resource_read_external_name(address context,ResourceExternalNameFields fields,ResourceExternalNameReader host) on cpu requires nonzero(host.library) requires nonzero(host.symbol);

extern "C" void resource_write_module_code(address output,ResourceModuleCodeFields fields,ResourceModuleCodeWriter host) on cpu requires nonzero(host.sections) requires nonzero(host.symbols);

extern "C" ReadStatus resource_read_module_code(address context,ResourceModuleCodeFields fields,ResourceModuleCodeReader host) on cpu requires nonzero(host.sections) requires nonzero(host.symbols);

extern "C" void resource_write_module_section(address output,ResourceModuleSectionFields fields,ResourceModuleSectionWriter host) on cpu requires nonzero(host.name) requires nonzero(host.bytes) requires nonzero(host.flags) requires nonzero(host.relocations);

extern "C" ReadStatus resource_read_module_section(address context,ResourceModuleSectionFields fields,ResourceModuleSectionReader host) on cpu requires nonzero(host.name) requires nonzero(host.bytes) requires nonzero(host.flags) requires nonzero(host.relocations);

extern "C" void resource_write_module_relocation(address output,ResourceModuleRelocationFields fields,ResourceModuleRelocationWriter host) on cpu requires nonzero(host.offset) requires nonzero(host.kind) requires nonzero(host.target);

extern "C" ReadStatus resource_read_module_relocation(address context,ResourceModuleRelocationFields fields,ResourceModuleRelocationReader host) on cpu requires nonzero(host.offset) requires nonzero(host.kind) requires nonzero(host.target);

extern "C" void resource_write_native_relocation(address output,ResourceNativeRelocationFields fields,ResourceNativeRelocationWriter host) on cpu requires nonzero(host.offset) requires nonzero(host.target);

extern "C" ReadStatus resource_read_native_relocation(address context,ResourceNativeRelocationFields fields,ResourceNativeRelocationReader host) on cpu requires nonzero(host.offset) requires nonzero(host.target);

extern "C" void resource_write_native_code(address output,ResourceNativeCodeFields fields,ResourceNativeCodeWriter host) on cpu requires nonzero(host.bytes) requires nonzero(host.relocations) requires nonzero(host.stack_bytes) requires nonzero(host.calls) requires nonzero(host.function_end) requires nonzero(host.unwind);

extern "C" ReadStatus resource_read_native_code(address context,ResourceNativeCodeFields fields,ResourceNativeCodeReader host) on cpu requires nonzero(host.bytes) requires nonzero(host.relocations) requires nonzero(host.stack_bytes) requires nonzero(host.calls) requires nonzero(host.function_end) requires nonzero(host.unwind);

extern "C" void resource_write_sort_code(address output,ResourceSortCodeFields fields,ResourceSortCodeWriter host) on cpu requires nonzero(host.source) requires nonzero(host.target) requires nonzero(host.initialize) requires nonzero(host.histogram) requires nonzero(host.scatter) requires nonzero(host.gather);

extern "C" ReadStatus resource_read_sort_code(address context,ResourceSortCodeFields fields,ResourceSortCodeReader host) on cpu requires nonzero(host.source) requires nonzero(host.target) requires nonzero(host.initialize) requires nonzero(host.histogram) requires nonzero(host.scatter) requires nonzero(host.gather);

extern "C" void resource_write_shader_code(address output,ResourceShaderCodeFields fields,ResourceShaderCodeWriter host) on cpu requires nonzero(host.words) requires nonzero(host.ranks);

extern "C" ReadStatus resource_read_shader_code(address context,ResourceShaderCodeFields fields,ResourceShaderCodeReader host) on cpu requires nonzero(host.words) requires nonzero(host.ranks);

extern "C" void resource_write_gpu_stage_code(address output,ResourceGpuStageCodeFields fields,ResourceGpuStageCodeWriter host) on cpu requires nonzero(host.main) requires nonzero(host.tail) requires nonzero(host.scan_add) requires nonzero(host.finish) requires nonzero(host.dispatch) requires nonzero(host.selector) requires nonzero(host.additional);

extern "C" ReadStatus resource_read_gpu_stage_code(address context,ResourceGpuStageCodeFields fields,ResourceGpuStageCodeReader host) on cpu requires nonzero(host.main) requires nonzero(host.tail) requires nonzero(host.scan_add) requires nonzero(host.finish) requires nonzero(host.dispatch) requires nonzero(host.selector) requires nonzero(host.additional);

extern "C" void resource_write_module(address output,ResourceModuleFields fields,ResourceModuleWriter host) on cpu requires nonzero(host.types) requires nonzero(host.images);

extern "C" ReadStatus resource_read_module(address context,ResourceModuleFields fields,ResourceModuleReader host) on cpu requires nonzero(host.types) requires nonzero(host.images);

extern "C" ReadStatus resource_read_image(Bytes input,inout u64 position,address context,address output,ResourceImageVariants host) on cpu requires nonzero(host.cpu) requires nonzero(host.gpu);

extern "C" void resource_write_image_cpu(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" void resource_write_image_gpu(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" ReadStatus resource_read_linked_image(Bytes input,inout u64 position,address context,address output,ResourceImageVariants host) on cpu requires nonzero(host.cpu) requires nonzero(host.gpu);

extern "C" void resource_write_linked_image_cpu(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" ReadStatus resource_read_module_target(Bytes input,inout u64 position,address context,address output,ResourceModuleTargetVariants host) on cpu requires nonzero(host.section) requires nonzero(host.import);

extern "C" void resource_write_module_target_section_fields(address output,ResourceModuleTargetSectionFieldsFields fields,ResourceModuleTargetSectionFieldsWriter host) on cpu requires nonzero(host.section) requires nonzero(host.value);

extern "C" ReadStatus resource_read_module_target_section_fields(address context,ResourceModuleTargetSectionFieldsFields fields,ResourceModuleTargetSectionFieldsReader host) on cpu requires nonzero(host.section) requires nonzero(host.value);

extern "C" void resource_write_module_target_section(address output,ResourceModuleTargetSectionFieldsFields fields,ResourceModuleTargetSectionFieldsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(append) requires nonzero(host.section) requires nonzero(host.value);

extern "C" void resource_write_module_target_import(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" ReadStatus resource_read_native_symbol(Bytes input,inout u64 position,address context,address output,ResourceNativeSymbolVariants host) on cpu requires nonzero(host.print) requires nonzero(host.external) requires nonzero(host.function);

extern "C" void resource_write_native_symbol_print(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" void resource_write_native_symbol_external(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" void resource_write_native_symbol_function(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" ReadStatus resource_read_range_result(Bytes input,inout u64 position,address context,address output,ResourceRangeResultVariants host) on cpu requires nonzero(host.none) requires nonzero(host.values);

extern "C" void resource_write_range_result_none(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void resource_write_range_result_values(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" ReadStatus resource_read_continuation_code(Bytes input,inout u64 position,address context,address output,ResourceContinuationCodeVariants host) on cpu requires nonzero(host.reduction_tail) requires nonzero(host.scan_add) requires nonzero(host.compact_scatter) requires nonzero(host.segmented_finish) requires nonzero(host.scan);

extern "C" void resource_write_continuation_code_reduction_tail(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" void resource_write_continuation_code_scan_add(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" void resource_write_continuation_code_compact_scatter(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" void resource_write_continuation_code_segmented_finish(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" void resource_write_continuation_code_scan_fields(address output,ResourceContinuationCodeScanFieldsFields fields,ResourceContinuationCodeScanFieldsWriter host) on cpu requires nonzero(host.prefix) requires nonzero(host.apply) requires nonzero(host.source) requires nonzero(host.target);

extern "C" ReadStatus resource_read_continuation_code_scan_fields(address context,ResourceContinuationCodeScanFieldsFields fields,ResourceContinuationCodeScanFieldsReader host) on cpu requires nonzero(host.prefix) requires nonzero(host.apply) requires nonzero(host.source) requires nonzero(host.target);

extern "C" void resource_write_continuation_code_scan(address output,ResourceContinuationCodeScanFieldsFields fields,ResourceContinuationCodeScanFieldsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(append) requires nonzero(host.prefix) requires nonzero(host.apply) requires nonzero(host.source) requires nonzero(host.target);

extern "C" ReadStatus resource_read_cpu_stage_code(Bytes input,inout u64 position,address context,address output,ResourceCpuStageCodeVariants host) on cpu requires nonzero(host.element) requires nonzero(host.sort);

extern "C" void resource_write_cpu_stage_code_element_fields(address output,ResourceCpuStageCodeElementFieldsFields fields,ResourceCpuStageCodeElementFieldsWriter host) on cpu requires nonzero(host.main) requires nonzero(host.continuation) requires nonzero(host.result);

extern "C" ReadStatus resource_read_cpu_stage_code_element_fields(address context,ResourceCpuStageCodeElementFieldsFields fields,ResourceCpuStageCodeElementFieldsReader host) on cpu requires nonzero(host.main) requires nonzero(host.continuation) requires nonzero(host.result);

extern "C" void resource_write_cpu_stage_code_element(address output,ResourceCpuStageCodeElementFieldsFields fields,ResourceCpuStageCodeElementFieldsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(append) requires nonzero(host.main) requires nonzero(host.continuation) requires nonzero(host.result);

extern "C" void resource_write_cpu_stage_code_sort(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append);

extern "C" void resource_write_native_image(address output,ResourceNativeImageFields fields,ResourceNativeImageWriter host) on cpu requires nonzero(host.plan) requires nonzero(host.stages) requires nonzero(host.functions) requires nonzero(host.resources) requires nonzero(host.module_code);

extern "C" ReadStatus resource_read_native_image(address context,ResourceNativeImageFields fields,ResourceNativeImageReader host) on cpu requires nonzero(host.plan) requires nonzero(host.stages) requires nonzero(host.functions) requires nonzero(host.resources) requires nonzero(host.module_code);

extern "C" void resource_write_gpu_image(address output,ResourceGpuImageFields fields,ResourceGpuImageWriter host) on cpu requires nonzero(host.plan) requires nonzero(host.stages);

extern "C" ReadStatus resource_read_gpu_image(address context,ResourceGpuImageFields fields,ResourceGpuImageReader host) on cpu requires nonzero(host.plan) requires nonzero(host.stages);

struct ReadValue { u64 value;ReadStatus status; };
struct ReadScalar { ScalarTag value;ReadStatus status; };
void byte(u8 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
ReadValue fixed_value(Bytes input,inout u64 position,u32 bytes) on cpu requires range(bytes,1,8);
address item(Span values,u64 index) on cpu;
ReadValue read_boolean(Bytes input,inout u64 position) on cpu;
ReadScalar read_scalar(Bytes input,inout u64 position) on cpu;
void wide(u64 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
void word(u32 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
