module bootstrap.operator_types;
import bootstrap.ir_dataflow;
import bootstrap.c_function;
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
