module bootstrap.source_call;
import bootstrap.ast_storage;
import bootstrap.signatures;

// Read-only offsets still refer to the original sealed source owner.
enum CallForm:u32 {Other=0;Named=1;Explicit=2;Indirect=3;};
struct CallSyntax {CallForm form;Text name;Optional<Slice> types;Slice arguments;ExprRef callee;};
struct SourceCallCursor {u32 phase;};
struct BoundArguments {address data;u64 count;};
struct CallFacts {u32 present;u32 continuing;u32 owned;};
struct CallHost {cfn<u32,address,u64> ensures memory(read) is_type;};
enum SpecializedNeed:u32 {Bind=0;UnsafeFunction=1;UnsafePosition=2;ReadPosition=3;View=4;VariantType=5;VariantValue=6;Result=7;RecordName=8;Record=9;FunctionName=10;Arguments=11;Instance=12;Invoke=13;LayoutType=14;LayoutSize=15;LayoutAlign=16;InvalidLayout=17;Unknown=18;Flow=19;Value=20;Empty=21;VariantOwned=22;};
extern "C" CallSyntax inspect(SignatureNode source) on cpu;
extern "C" SourceCallCursor specialized_start() on cpu;
extern "C" SpecializedNeed specialized_step(inout SourceCallCursor cursor,SignatureNode source,CallSyntax syntax,BoundArguments arguments,CallFacts facts,CallHost host) on cpu
 requires nonzero(host.is_type);

struct ExpectedFacts {u32 present;u32 result;u32 owned;};
enum ExpectedNeed:u32 {Bind=0;VariantType=1;VariantValue=2;ResultOk=3;ResultErr=4;MissingResult=5;Specialized=6;Expression=7;Selection=8;VariantOwned=9;};
extern "C" SourceCallCursor expected_start(SignatureNode source,CallSyntax syntax) on cpu;
extern "C" ExpectedNeed expected_step(inout SourceCallCursor cursor,SignatureNode source,CallSyntax syntax,ExpectedFacts facts) on cpu;

struct ReturnedFacts {u32 present;u32 kind;u32 expected;};
enum ReturnedNeed:u32 {Other=0;Task=1;TypeCall=2;AliasBuiltin=3;AliasType=4;Environment=5;Indirect=6;LocalFunction=7;Bind=8;VariantType=9;VariantValue=10;Dataset=11;ExpectedOwns=12;ExpectedOwnedResult=13;ResultType=14;OwnedResult=15;RecordName=16;RecordType=17;OwnedRecord=18;FunctionName=19;Arguments=20;Instance=21;Invoke=22;Ambiguous=23;Empty=24;Fallback=25;ExpectedResult=26;MissingResult=27;Resize=28;TakeLast=29;Own=30;IntoInner=31;Builtin=32;Propagate=33;VariantOwned=34;TaskType=35;RecordValue=36;};
extern "C" SourceCallCursor returned_start() on cpu;
extern "C" ReturnedNeed returned_step(inout SourceCallCursor cursor,SignatureNode source,CallSyntax syntax,ReturnedFacts facts) on cpu;
