module bootstrap.operator_types;
import bootstrap.ir_dataflow;
import bootstrap.c_function;
import bootstrap.scalar_fold;
// Scalar IDs are the original compiler Scalar order. These records borrow no
// native containers and carry only actual type facts and the final decision.
struct Operand {u64 components;u64 rows;u64 columns;u32 scalar;u32 flags;};
struct Plan {
    u32 status;u32 kind;u32 scalar;u32 cast_left;u32 cast_right;u32 comparison;u32 operation;
    u64 rows;u64 inner;u64 columns;u64 count;
};
struct Queries {
    address context;
    cfn<bool,address> same;
    cfn<bool,address,u32,u32,u64> builtin;
    cfn<bool,address,u32> numeric;
    cfn<bool,address,u32> views;
};
extern "C" Plan numeric(Operand a,bool a_literal,Operand b,bool b_literal) on cpu;
extern "C" Plan binary(address bytes,u64 length,Operand a,bool a_literal,Operand b,bool b_literal,Queries queries) on cpu
    requires nonzero(queries.same)
    requires nonzero(queries.builtin)
    requires nonzero(queries.numeric)
    requires nonzero(queries.views);
extern "C" Plan unary(address bytes,u64 length,Operand value,Queries queries) on cpu
    requires nonzero(queries.builtin)
    requires nonzero(queries.numeric);

struct MathArgument { u32 scalar; u32 flags; };
struct MathShape { u32 status; u32 scalar; usize width; };
struct MathQueries {
    address context;
    cfn<u32,address,usize> ensures memory(read) numeric;
    cfn<MathArgument,address,usize> ensures memory(read) argument;
    cfn<usize,address,usize> ensures memory(read) width;
};
extern "C" u32 integer_math(u32 extremum,usize count,MathQueries queries) on cpu
    requires nonzero(queries.numeric) requires nonzero(queries.argument);
extern "C" MathShape floating_math(usize count,MathQueries queries) on cpu
    requires nonzero(queries.argument) requires nonzero(queries.width);


enum GeometryBuiltin:u32 { Unknown=0; Dot=1; Length=2; Normalize=3; Cross=4; };
enum GeometryStatus:u32 { Valid=0; Arity=1; Same=2; DotType=3; LengthType=4; CrossType=5; };
struct GeometryPlan { GeometryStatus status; u32 side; usize width; usize result_width; };
struct GeometryQueries {
    MathQueries values;
    cfn<u32,address> ensures memory(read) same;
    cfn<u32,address,usize> ensures memory(read) float3;
};
extern "C" GeometryBuiltin source_geometry(address bytes,usize length) on cpu;
extern "C" GeometryPlan geometry_plan(GeometryBuiltin builtin,usize count,GeometryQueries queries) on cpu
    requires nonzero(queries.values.numeric) requires nonzero(queries.values.argument)
    requires nonzero(queries.values.width) requires nonzero(queries.same) requires nonzero(queries.float3);

enum GeometryAction:u32 { Copy=0; Add=1; Subtract=2; Multiply=3; Divide=4; Sqrt=5; };
struct GeometryOperand { u32 slot; usize component; u32 projected; u32 take; };
struct GeometryStep {
    GeometryAction action; GeometryOperand left; GeometryOperand right; u32 destination; usize width;
};
struct GeometryEmitter {
    address context;
    cfn<void,address,GeometryStep> emit;
    cfn<void,address,u32,u32> output;
};
extern "C" void expand_geometry(GeometryBuiltin builtin,usize width,GeometryEmitter emitter) on cpu
    requires nonzero(emitter.emit) requires nonzero(emitter.output);

enum FixedProjectionKind:u32 { Components=0; Vector=1; Array=2; };
struct FixedProjection { FixedProjectionKind kind; usize first; };
extern "C" FixedProjection fixed_projection(u32 array, usize width, u32 known, usize index) on cpu;

// The caller keeps the actual component sources alive until this synchronous lowering returns.
struct ComponentChoice { u32 key; array<usize,2> registers; };
struct SelectedComponents { array<usize,2> registers; };
struct ComponentEmitter {
    address context;
    cfn<ComponentChoice,address,usize> choice;
    cfn<usize,address,usize,u32> equal;
    cfn<usize,address,usize,usize,usize> select;
};
extern "C" SelectedComponents select_indexed_components(usize index, usize count, u32 paired, ComponentEmitter emitter) on cpu
    requires nonzero(emitter.choice) requires nonzero(emitter.equal) requires nonzero(emitter.select);

// The f32 arithmetic IDs agree with binary_instruction.
enum MatrixAction:u32 { Copy=0; Add=1; Subtract=2; Multiply=3; Divide=4; Negate=5; };
struct MatrixOperand { u32 slot; u32 take; };
struct MatrixStep {
    u32 action;
    MatrixOperand left;
    MatrixOperand right;
    u32 destination;
};
struct MatrixEmitter {
    address context;
    cfn<void,address,MatrixStep> emit;
    cfn<void,address,u32> output;
};
extern "C" void expand_square_matrix(u32 width, bool inverse, MatrixEmitter emitter) on cpu
    requires nonzero(emitter.emit) requires nonzero(emitter.output);
extern "C" void expand_matrix_binary(Plan plan,u32 left_count,u32 input_count,MatrixEmitter emitter) on cpu
    requires nonzero(emitter.emit) requires nonzero(emitter.output);

enum MatrixBuiltin:u32 { Unknown=0; Transpose=1; Determinant=2; Inverse=3; };
enum MatrixIntrinsicStatus:u32 { Valid=0; Arity=1; Matrix=2; Square=3; };
struct MatrixIntrinsicInput { u32 present; u32 rows; u32 columns; };
struct MatrixIntrinsicPlan {
    MatrixIntrinsicStatus status; u32 rows; u32 columns; u32 result_rows; u32 result_columns;
    u32 outputs; u32 transpose; u32 inverse;
};
extern "C" MatrixBuiltin source_matrix_intrinsic(address bytes,usize length) on cpu;
extern "C" MatrixIntrinsicPlan matrix_intrinsic(MatrixBuiltin builtin,usize count,address context,
    cfn<MatrixIntrinsicInput,address> ensures memory(read) inspect) on cpu requires nonzero(inspect);
extern "C" void expand_transpose(u32 rows,u32 columns,address context,
    cfn<void,address,u32> output) on cpu requires nonzero(output);

// Source operation IDs do not depend on either machine backend's opcode order.
enum AtomicSourceKind:u32 {
    Unknown=0; Load=1; Store=2; Add=3; Sub=4; Exchange=5; CompareExchange=6;
    Min=7; Max=8; And=9; Or=10; Xor=11;
};
struct AtomicBuiltin { AtomicSourceKind kind; Access access; u32 arity; u32 order_index; };
extern "C" AtomicBuiltin source_atomic(address bytes, usize length, bool statement) on cpu;
extern "C" AtomicBuiltin source_atomic_name(address bytes, usize length, bool statement) on cpu;
extern "C" u32 source_custom_reduce(address bytes, usize length) on cpu;
extern "C" bool atomic_order_allowed(AtomicSourceKind kind, u32 order, u32 failure) on cpu;

enum SubgroupRule:u32 { Unknown=0; Index=1; Elect=2; Vote=3; VoteCount=4; Transfer=5; Arithmetic=6; Bits=7; };
struct SubgroupBuiltin { SubgroupOperation operation; u32 arity; SubgroupRule rule; };
enum SubgroupFact:u32 { Shape=0; Numeric=1; Integer=2; };
enum SubgroupResult:u32 {
    U32=0; Bool=1; PredicateU32=2; PredicateBool=3; Operand=4; BadShape=5; BadBits=6;
};
extern "C" SubgroupBuiltin source_subgroup(address bytes, usize length) on cpu;
extern "C" SubgroupResult subgroup_result(SubgroupRule rule, address ty, cfn<u32,address,SubgroupFact> fact) on cpu
    requires nonzero(fact);

// The source form and operator are also consumed by the original compiler IR.
enum CollectiveForm:u32 { Unknown=0; TileReduce=1; TileScan=2; Reduce=3; SegmentedReduce=4; Scan=5; SegmentedScan=6; };
enum CollectiveOperator:u32 { Sum=0; Min=1; Max=2; BitAnd=3; BitOr=4; BitXor=5; };
struct CollectiveBuiltin { CollectiveForm form; CollectiveOperator operation; };
extern "C" CollectiveBuiltin source_collective(address bytes, usize length) on cpu;
extern "C" bool collective_accepts(CollectiveOperator operation, bool numeric, bool integer) on cpu;


enum ConstructEntry:u32 { Value=0; Matrix=1; };
enum ConstructFact:u32 { CFunction=0; Address=1; Handle=2; Variants=3; Enumeration=4; Array=5; Matrix=6; Views=7; Fields=8; Width=9; };
enum ConstructStatus:u32 {
    Valid=0; CFunctionTarget=1; CFunctionInput=2; AddressTarget=3; AddressInput=4;
    HandleTarget=5; HandleInput=6; Variants=7; Enumeration=8; Array=9; Matrix=10;
    Views=11; MatrixInput=12; Components=13; MatrixComponents=14;
};
enum ConstructKind:u32 { Zero=0; Identity=1; Enumeration=2; Numeric=3; Diagonal=4; Matrix=5; };
struct ConstructPlan { ConstructStatus status; ConstructKind kind; u32 scalar; u32 splat; usize width; usize rows; usize columns; };
struct ConstructQueries {
    address context;
    cfn<usize,address,usize,ConstructFact> ensures memory(read) fact;
    cfn<u32,address,usize> ensures memory(read) same;
    cfn<u32,address> ensures memory(read) scalar_identity;
};
extern "C" ConstructPlan construct_plan(ConstructEntry entry,Operand target,usize count,ConstructQueries queries) on cpu
    requires nonzero(queries.fact) requires nonzero(queries.same) requires nonzero(queries.scalar_identity);
enum ConstructArgument:u32 { Append=0; ReplaceProof=1; Matrix=2; Diagonal=3; };
enum ConstructComponent:u32 { Project=0; Diagonal=1; Zero=2; };
struct ConstructEmitter {
    address context;
    cfn<usize,address,u32,ConstructArgument> argument;
    cfn<void,address,ConstructComponent,usize,u32> component;
    cfn<void,address,usize> splat;
};
extern "C" void expand_construct(ConstructPlan plan,usize count,ConstructEmitter emitter) on cpu
    requires nonzero(emitter.argument) requires nonzero(emitter.component) requires nonzero(emitter.splat);


struct BitcastSignature { u32 present; u32 source; u32 target; usize maximum; };
enum BitcastStatus:u32 { Valid=0; Arity=1; Type=2; };
struct BitcastPlan { BitcastStatus status; u32 source; u32 target; usize width; };
extern "C" BitcastSignature source_bitcast(address bytes,usize length) on cpu;
extern "C" BitcastPlan bitcast_plan(BitcastSignature signature,usize count,address context,
    cfn<usize,address> ensures memory(read) width,
    cfn<u32,address,u32,usize> ensures memory(read) builtin) on cpu requires nonzero(width) requires nonzero(builtin);

enum ConversionMode:u32 { Numeric=0; Bitcast=1; };
enum ConversionSource:u32 { Type=0; Register=1; };
enum ConversionAction:u32 {
    DiscardConstants=0; CastProof=1; BitcastProof=2; ConstantValue=3;
    CastOrigin=4; BitcastOrigin=5; Reuse=6; Cast=7; Bitcast=8; ConstantBitcast=9; BeginConstants=10;
};
enum ConversionOutcome:u32 { Registers=0; Constant=1; };
struct ConversionInput {
    ConversionMode mode; ScalarEncoding target; usize types; usize registers;
    address constants; usize constant_count; u32 constant_present;
};
struct ConversionEmitter {
    address context;
    cfn<ScalarEncoding,address,usize,ConversionSource> ensures memory(read) source;
    cfn<void,address,u64> constant;
    cfn<usize,address,ConversionAction,usize> action;
};
extern "C" ConversionOutcome convert_value(ConversionInput input,ConversionEmitter emitter) on cpu
    requires nonzero(emitter.source) requires nonzero(emitter.constant) requires nonzero(emitter.action);


enum MathPath:u32 { Floating=0; Minimum=1; Maximum=2; };
struct MathPreparation { u32 status; MathPath path; u32 pair; };
extern "C" MathPreparation prepare_math(usize arity,u32 extremum,usize count,MathQueries queries) on cpu
    requires nonzero(queries.numeric) requires nonzero(queries.argument);
extern "C" MathShape math_shape(MathPath path,usize count,MathQueries queries) on cpu
    requires nonzero(queries.argument) requires nonzero(queries.width);
enum MathAction:u32 {
    IntegerProof=0; IntegerConstant=1; KnownOrigin=2; CompareMinimum=3; CompareMaximum=4;
    Select=5; FloatBegin=6; FloatInput=7; FloatEnd=8; FloatingProof=9;
};
struct MathValueFacts { usize width; u32 literal; u32 uniform; };
struct MathStep { MathAction action; usize component; usize left; usize right; };
struct MathEmitter {
    address context;
    cfn<MathValueFacts,address,usize> ensures memory(read) value;
    cfn<usize,address,MathStep> action;
};
struct MathOutcome { u32 constant; u32 literal; u32 uniform; };
extern "C" MathOutcome expand_math(MathPath path,usize count,usize width,MathEmitter emitter) on cpu
    requires nonzero(emitter.value) requires nonzero(emitter.action);


// operation is the original unary Plan instruction: 1 float negate, 2/3 integer negate,
// 4 logical not, 5 bit not. The identity plan returns the original value directly.
enum ValueUnaryAction:u32 { BeginConstants=0; Proof=1; ConstantValue=2; Origin=3; Emit=4; };
enum ValueUnaryOutcome:u32 { Registers=0; Constant=1; };
struct ValueUnaryInput { u32 operation; u32 width; usize registers; address constants; usize constant_count; u32 constant_present; };
struct ValueUnaryEmitter {
    address context;
    cfn<void,address,u64> constant;
    cfn<usize,address,ValueUnaryAction,usize> action;
};
extern "C" ValueUnaryOutcome expand_unary_value(ValueUnaryInput input,ValueUnaryEmitter emitter) on cpu
    requires nonzero(emitter.constant) requires nonzero(emitter.action);


// Borrowed original operands and divisor fields remain alive during this call.
struct BinaryWords { address constants; usize constant_count; usize registers; u32 constant_present; };
struct ValueBinaryInput {
    usize count; ScalarEncoding scalar; u32 operation; u32 comparison; u32 remainder;
    BinaryWords left; BinaryWords right; usize divisor_count; u32 divisors_present;
};
struct ValueDivisor { u32 constant; u64 bits; address nonzero; };
enum ValueBinaryAction:u32 {
    BeginConstants=0; DiscardConstants=1; ArithmeticProof=2; ComparisonProof=3;
    KnownComparison=4; FoldConstantValue=5; KnownConstantValue=6;
    ConstantOrigin=7; KnownOrigin=8; Binary=9; Quotient=10; Remainder=11;
    Negate=12; Zero=13; PreserveFloat=14; FinishRegisters=15; ObserveInteger=16;
};
enum ValueBinaryOutcome:u32 { Registers=0; Constant=1; };
struct ValueBinaryStep { ValueBinaryAction action; usize component; usize left; usize right; usize divisor; };
struct ValueBinaryEmitter {
    address context;
    cfn<ValueDivisor,address,usize> ensures memory(read) divisor;
    cfn<u64,address,u64,address> ensures memory(read) divide;
    cfn<void,address,u64> constant;
    cfn<usize,address,ValueBinaryStep> action;
};
extern "C" ValueBinaryOutcome expand_binary_value(ValueBinaryInput input,ValueBinaryEmitter emitter) on cpu
    requires nonzero(emitter.divisor) requires nonzero(emitter.divide)
    requires nonzero(emitter.constant) requires nonzero(emitter.action);


enum SourceBinaryAction:u32 {
    Identity=0; Handle=1; HandleLeft=2; HandleRight=3; DifferenceWithZero=4; Matrix=5;
    CastLeft=6; CastRight=7; IntegerDivisors=8; PendingDivisors=9;
    DeferredDivisors=10; MissingDivisors=11; Numeric=12;
};
enum SourceBinaryToken:u32 { Original=0; Xor=1; Or=2; };
struct SourceBinaryInput { Plan plan; u32 division; u32 left_literal; u32 right_literal; };
struct SourceBinaryEmitter {
    address context;
    cfn<u32,address,SourceBinaryAction,u32> action;
    cfn<u32,address,SourceBinaryToken> reenter;
};
extern "C" u32 lower_source_binary(SourceBinaryInput input,SourceBinaryEmitter emitter) on cpu
    requires nonzero(emitter.action) requires nonzero(emitter.reenter);


struct WordScalar { u32 code; ScalarEncoding encoding; };
struct WordComponent { WordScalar scalar; u32 offset; };
struct WordConstants { u64 left; u64 right; u32 left_present; u32 right_present; };
struct WordInput { usize components; u32 offset; WordScalar target; };
enum WordOperation:u32 { ShiftRight=0; And=1; ShiftLeft=2; Or=3; };
enum WordAction:u32 {
    BeginComponent=0; Bitcast=1; Cast=2; CastKeepProof=3; Project=4; RightConstant=5;
    TakeJoined=6; Emit=7; Accumulate=8; TakeAccumulated=9; Zero=10; FinalComponent=11;
    NonzeroBytes=12; MarkNonzero=13;
};
struct WordStep { WordAction action; usize index; u32 scalar; WordOperation operation; u32 present; u64 bits; };
struct WordEmitter {
    address context;
    cfn<WordComponent,address,usize> ensures memory(read) component;
    cfn<WordConstants,address> ensures memory(read) known;
    cfn<u32,address,WordStep> action;
};
extern "C" void lower_variant_component(WordScalar source,WordScalar target,usize index,WordEmitter emitter) on cpu
    requires nonzero(emitter.action);
extern "C" void lower_variant_word(WordInput input,WordEmitter emitter) on cpu
    requires nonzero(emitter.component) requires nonzero(emitter.known) requires nonzero(emitter.action);


struct VariantInput { usize components; u32 offset; u32 payload; u32 unit; u32 scalar; u64 discriminant; };
enum VariantSide:u32 { Storage=0; Payload=1; Selected=2; };
struct VariantField { u32 scalar; u32 offset; };
struct VariantWord { u64 bits; u32 constant; };
struct VariantView { usize component; u32 present; };
enum VariantAction:u32 {
    InitializeBuild=0; Tag=1; StoreBuild=2; PayloadFacts=3; RetainBuild=4; StartActive=5;
    AddActive=6; Inactive=7; EndActive=8; InitializeRead=9; StoreRead=10; Borrow=11;
    FinishRead=12; ExpectedTag=13; CompareTag=14;
};
struct VariantStep { VariantAction action; usize component; usize other; u32 scalar; u32 offset; u32 known; u64 bits; };
struct VariantEmitter {
    address context;
    cfn<VariantField,address,VariantSide,usize> ensures memory(read) field;
    cfn<VariantWord,address,u32,u32> load;
    cfn<void,address,VariantSide> views;
    cfn<VariantView,address> next;
    cfn<u32,address,usize> ensures memory(read) active;
    cfn<void,address,VariantStep> action;
};
extern "C" void build_variant_value(VariantInput input,VariantEmitter emitter) on cpu
    requires nonzero(emitter.field) requires nonzero(emitter.load) requires nonzero(emitter.views)
    requires nonzero(emitter.next) requires nonzero(emitter.active) requires nonzero(emitter.action);
extern "C" void read_variant_payload(VariantInput input,VariantEmitter emitter) on cpu
    requires nonzero(emitter.field) requires nonzero(emitter.load) requires nonzero(emitter.views)
    requires nonzero(emitter.next) requires nonzero(emitter.action);
extern "C" void test_variant_tag(VariantInput input,VariantEmitter emitter) on cpu
    requires nonzero(emitter.load) requires nonzero(emitter.action);

enum BuiltinValueKind:u32 {Other=0;FunctionAddress=1;UnsafeHandle=2;UnsafeAddress=3;AddressBits=4;Type=5;Bitcast=6;Math=7;Matrix=8;Geometry=9;Clamp=10;};
struct BuiltinValueCursor {u32 phase;BuiltinValueKind kind;};
struct BuiltinValueFacts {u32 present;u32 cpu;u64 count;u32 function;u32 low_known;u32 high_known;u32 low_uniform;u32 high_uniform;};
enum BuiltinValueNeed:u32 {Type=0;Bitcast=1;Math=2;Matrix=3;Geometry=4;Values=5;Empty=6;Unknown=7;Constructor=8;InvokeBitcast=9;InvokeMath=10;InvokeMatrix=11;InvokeGeometry=12;BadFunctionTarget=13;BadAddressTarget=14;BadFunctionCount=15;BadHandleCount=16;BadAddressCount=17;BadClampCount=18;Pop=19;FunctionKind=20;BadFunctionKind=21;FunctionToAddress=22;RequireU64=23;HandleLow=24;HandleHigh=25;HandlePackage=26;RequireAddress=27;RawToAddress=28;AddressToBits=29;ClampMax=30;ClampMin=31;Complete=32;};
struct BuiltinValueStep {BuiltinValueCursor cursor;BuiltinValueNeed need;u32 flags;};
extern "C" BuiltinValueCursor builtin_value_start(address bytes,u64 length) on cpu;
extern "C" BuiltinValueStep builtin_value_step(BuiltinValueCursor cursor,BuiltinValueFacts facts,address bytes,u64 length) on cpu;
struct FunctionFromAddressCursor {u32 phase;};
struct FunctionFromAddressFacts {u32 cpu;u32 explicit_type;u64 count;u32 function;u32 present;};
enum FunctionFromAddressNeed:u32 {BadTarget=0;BadArguments=1;Type=2;BadSignature=3;Argument=4;Empty=5;RequireAddress=6;Retype=7;Complete=8;};
struct FunctionFromAddressStep {FunctionFromAddressCursor cursor;FunctionFromAddressNeed need;};
extern "C" FunctionFromAddressStep function_from_address_step(FunctionFromAddressCursor cursor,FunctionFromAddressFacts facts) on cpu;

struct BuiltinCallCursor {u32 phase;CollectiveForm collective;usize index;};
struct BuiltinCallFacts {u32 present;u32 record;u32 in_function;usize count;};
enum BuiltinCallNeed:u32 {CFunction=0;ResultContext=1;Transfer=2;Println=3;TextureSample=4;Append=5;TileCollective=6;TilePublish=7;TileInput=8;Subgroup=9;SubgroupResult=10;RayQueryAny=11;UnsafeRead=12;Ordinal=13;IndexScope=14;Index=15;Unsupported=16;AtomicStore=17;Atomic=18;TileBarrier=19;RecordValues=20;RecordLookup=21;RecordExpression=22;AddressOf=23;Values=24;Argument=25;ArgumentValue=26;Empty=27;ArgumentsComplete=28;RayQueryClosest=29;};
struct BuiltinCallStep {BuiltinCallCursor cursor;BuiltinCallNeed need;AtomicBuiltin atomic;};
extern "C" BuiltinCallStep builtin_call_step(BuiltinCallCursor cursor,BuiltinCallFacts facts,address bytes,usize length) on cpu;

extern "C" u32 atomic_storage(u32 kind,u32 access) on cpu;
extern "C" u32 atomic_scalar(address context,cfn<u32,address,u32> ensures memory(read) same) on cpu requires nonzero(same);
enum AtomicOrder:u32 {Unknown=0;Relaxed=1;Acquire=2;Release=3;AcquireRelease=4;};
extern "C" AtomicOrder source_atomic_order(u32 named,address bytes,usize length) on cpu;
struct AtomicLocationCursor {u32 phase;};
struct AtomicLocationFacts {u32 valid;u32 kind;u32 offset;};
enum AtomicLocationNeed:u32 {Place=0;Type=1;BadType=2;Access=3;Effect=4;Position=5;Data=6;Tile=7;Array=8;Reference=9;Application=10;Complete=11;};
struct AtomicLocationStep {AtomicLocationCursor cursor;AtomicLocationNeed need;};
extern "C" AtomicLocationStep atomic_location_step(AtomicLocationCursor cursor,AtomicLocationFacts facts) on cpu;

struct AtomicCursor {u32 phase;};
struct AtomicFacts {u64 count;u32 present;u32 order;u32 failure;u32 location;u32 data;};
enum AtomicInstruction:u32 {Load=0;Store=1;Add=2;Sub=3;Exchange=4;CompareExchange=5;Min=6;Max=7;And=8;Or=9;Xor=10;};
enum AtomicNeed:u32 {BadStoreArity=0;BadArity=1;Location=2;Order=3;Failure=4;Value=5;Desired=6;ValueType=7;DesiredType=8;BadStoreOrder=9;BadLoadOrder=10;BadFailureOrder=11;BadOperation=12;Build=13;Result=14;Proof=15;Access=16;Read=17;Write=18;Cross=19;Emit=20;Empty=21;Complete=22;};
struct AtomicStep {AtomicCursor cursor;AtomicNeed need;AtomicInstruction operation;u32 stored;};
extern "C" AtomicStep atomic_step(AtomicCursor cursor,AtomicBuiltin builtin,AtomicFacts facts) on cpu;

extern "C" SubgroupBuiltin source_subgroup_name(address bytes,usize length) on cpu;
struct SubgroupCursor {u32 phase;SubgroupResult result;u64 index;};
struct SubgroupFacts {u64 count;u64 components;u32 gpu;u32 present;};
enum SubgroupNeed:u32 {BadTarget=0;BadArity=1;Value=2;Argument=3;ArgumentType=4;Predicate=5;U32=6;Bool=7;Operand=8;Component=9;CompleteScalar=10;CompleteOperand=11;Empty=12;BadShape=13;BadBits=14;};
struct SubgroupStep {SubgroupCursor cursor;SubgroupNeed need;};
extern "C" SubgroupStep subgroup_step(SubgroupCursor cursor,SubgroupBuiltin builtin,SubgroupFacts facts,address ty,
 cfn<u32,address,SubgroupFact> fact) on cpu requires nonzero(fact);
