module bootstrap.constant_evaluation;
import bootstrap.ir_dataflow;

// Borrowed original names and generated-source identities, not another syntax
// tree or source registry. All fields live through this synchronous operation.
struct Name {address data;usize count;};
struct Origin {u32 present;address source;address generated;};
struct OriginHost {
 cfn<Origin,address> ensures memory(read) inspect;
 cfn<void,address,address> append;
};
extern "C" u32 declaration_type(Name name,u32 wrappers) on cpu;
extern "C" u32 constant_result(u32 facts,Span code,Host ir) on cpu
 requires nonzero(ir.inspect);
extern "C" void source_operations(Span code,usize first,Span used,address output,OriginHost origin,Host ir) on cpu
 requires nonzero(origin.inspect) requires nonzero(origin.append) requires nonzero(ir.inspect);

enum ModuleEntry:u32 {Cached=0;Evaluate=1;};
enum ModuleNeed:u32 {Instance=0;ReturnType=1;Body=2;InvalidType=3;};
struct ModuleCursor {u32 phase;};
extern "C" ModuleEntry module_entry(u32 cached) on cpu;
extern "C" ModuleCursor module_start() on cpu;
extern "C" ModuleNeed module_step(inout ModuleCursor cursor,u32 type_allowed) on cpu;

enum ExpressionKind:u32 {Number=0;StaticU32=1;Bool=2;Name=3;Member=4;Index=5;Call=6;Invoke=7;Construct=8;Specialize=9;FunctionRef=10;Cooperate=11;Target=12;Unary=13;Binary=14;Select=15;};
struct ExpressionView {
 ExpressionKind kind;Name text;address first;address second;address third;address ty;
 address types;usize type_count;address args;usize arg_count;u64 bits;
};
struct ExpressionHost {cfn<ExpressionView,address> ensures memory(read) inspect;};
enum SyntaxNeed:u32 {Number=0;StaticU32=1;Bool=2;Name=3;First=4;Second=5;Member=6;IndexType=7;Index=8;InvalidIndex=9;Negative=10;Unary=11;Binary=12;ShortCircuit=13;Select=14;Type=15;Construct=16;Call=17;FunctionRef=18;Cooperate=19;Target=20;Invoke=21;Empty=22;};
struct SyntaxCursor {u32 phase;};
extern "C" SyntaxCursor syntax_start() on cpu;
extern "C" SyntaxNeed syntax_step(inout SyntaxCursor cursor,ExpressionView expression,u32 facts,ExpressionHost host) on cpu
 requires nonzero(host.inspect);
enum ValueNeed:u32 {Expected=0;Read=1;Validate=2;Reject=3;Sources=4;Literal=5;Reference=6;};
struct ValueCursor {u32 phase;u32 flags;};
extern "C" ValueCursor value_start(u32 flags) on cpu;
extern "C" ValueNeed value_step(inout ValueCursor cursor,u32 facts,Span code,Host ir) on cpu
 requires nonzero(ir.inspect);

struct PlaceHost {
 cfn<ExpressionView,address> ensures memory(read) inspect;
 cfn<u32,address,Name> ensures memory(read) binding;
};
extern "C" u32 is_place(address expression,address context,PlaceHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.binding);
enum PlaceNeed:u32 {Name=0;External=1;Parent=2;Member=3;BaseIsPlace=4;BaseValue=5;IndexedType=6;ArrayType=7;Materialize=8;IndexValue=9;ArrayIndex=10;ReadPlace=11;ViewAt=12;Empty=13;Invalid=14;};
struct PlaceCursor {u32 phase;};
extern "C" PlaceCursor place_start() on cpu;
extern "C" PlaceNeed place_step(inout PlaceCursor cursor,ExpressionView expression,u32 facts) on cpu;
enum ArgumentNeed:u32 {PlaceTest=0;Place=1;AtomicPlace=2;MutablePlace=3;Expected=4;Read=5;Flow=6;MutableReturned=7;AtomicError=8;Resource=9;Empty=10;};
struct ArgumentCursor {u32 phase;u32 access;};
extern "C" ArgumentCursor argument_start(u32 access) on cpu;
extern "C" ArgumentNeed argument_step(inout ArgumentCursor cursor,u32 facts) on cpu;
enum ArgumentsNeed:u32 {Read=0;Append=1;Empty=2;Finish=3;};
struct ArgumentsCursor {usize index;usize count;u32 phase;};
extern "C" ArgumentsCursor arguments_start(usize count) on cpu;
extern "C" ArgumentsNeed arguments_step(inout ArgumentsCursor cursor,u32 present) on cpu;

enum ShapeNeed:u32 {Absent=0;Literal=1;Binding=2;Bound=3;Resolve=4;Unknown=5;Evaluate=6;Constant=7;InvalidType=8;Expression=9;};
struct ShapeCursor {u32 phase;u32 kind;u32 source;};
extern "C" ShapeCursor shape_start(u32 kind,u32 source) on cpu;
extern "C" ShapeNeed shape_step(inout ShapeCursor cursor,u32 present) on cpu;
enum IntegerNeed:u32 {Read=0;Validate=1;Reject=2;Finish=3;};
struct IntegerCursor {u32 phase;};
extern "C" IntegerCursor integer_start() on cpu;
extern "C" IntegerNeed integer_step(inout IntegerCursor cursor,u32 facts,Span code,Host ir) on cpu
 requires nonzero(ir.inspect);

struct CallInput {Name name;u32 explicit;address types;usize type_count;usize arg_count;u32 expected_result;};
struct CallFacts {u32 present;u32 owns;u32 resolution;usize parameters;};
struct CallHost {cfn<u32,address,usize> ensures memory(read) type_argument;};
enum CallNeed:u32 {VariantLookup=0;Owns=1;Variant=2;ResourceError=3;ResultType=4;ExpectedResult=5;InvalidResultTypes=6;MissingResultContext=7;FunctionFrom=8;PositionFrom=9;ReadPosition=10;Empty=11;InvalidConst=12;ResolveType=13;ExplicitType=14;CheckType=15;ResolveFunction=16;ParameterCount=17;Arguments=18;Instance=19;Invoke=20;Ambiguous=21;InvalidArgumentCount=22;LayoutType=23;LayoutSize=24;LayoutAlign=25;InvalidLayoutArguments=26;UnknownGeneric=27;CFunction=28;Println=29;Subgroup=30;SubgroupValue=31;ReadView=32;UnsafeRead=33;InvalidReadArguments=34;FinalType=35;Construct=36;IndexContext=37;OrdinalContext=38;Builtin=39;CheckOwns=40;};
struct CallCursor {u32 phase;u32 type_known;u32 type_visible;usize variant;};
extern "C" CallCursor call_start() on cpu;
extern "C" CallNeed call_step(inout CallCursor cursor,CallInput input,CallFacts facts,CallHost host) on cpu
 requires nonzero(host.type_argument);
enum CReferenceNeed:u32 {Count=0;Name=1;FunctionRef=2;Invalid=3;};
extern "C" CReferenceNeed c_reference(Span args,ExpressionHost host) on cpu requires nonzero(host.inspect);
