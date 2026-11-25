module bootstrap.source_writes;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.c_function;

enum WritePath:u32 { Direct=0; WhenTrue=1; WhenFalse=2; Unknown=3; TrueMoves=4; FalseMoves=5; Range=6; Elements=7; Loop=8; };
enum Truth:u32 { Absent=0; Conditional=1; False=2; True=3; };
enum Capture:u32 { Discard=0; Control=1; Right=2; };
enum ExpressionAction:u32 { Finish=0; Visit=1; Bool=2; NameEligible=3; Name=4; Comparison=5; Negate=6; And=7; Or=8; Call=9; Resource=10; Move=11; };
struct ExpressionStep {
    u32 phase;
    ExpressionAction action;
    u64 next_argument;
    ExprRef child;
    Access access;
    WritePath path;
    Capture capture;
    bootstrap.ast_storage::Text name;
};
extern "C" ExpressionStep expression_step(SignatureNode source, u32 phase, u64 argument,
    Access access, u32 current, u32 loop_scope, Truth truth, u32 fact) on cpu;

enum StatementAction:u32 { Finish=0; Expression=1; Block=2; Declare=3; Let=4; Element=5; MemoryQuery=6; Memory=7; Position=8; WhileBounds=9; Drain=10; RangeBounds=11; BindRange=12; BindElement=13; BindParallel=14; BindArm=15; Unbind=16; Advance=17; };
struct StatementCursor { u64 index; u64 arm; u32 phase; };
struct StatementStep {
    StatementCursor cursor;
    StatementAction action;
    WritePath path;
    Access access;
    ExprRef value;
    Optional<ExprRef> end;
    Slice body;
    bootstrap.ast_storage::Text name;
    bootstrap.ast_storage::Text variant;
    Optional<TypeRef> annotation;
    u32 local;
};
extern "C" StatementStep statement_step(StorageView storage, Slice body, StatementCursor cursor,
    u32 current, Truth truth, u32 present) on cpu;

enum CallForm:u32 { Named=0; Specialized=1; Cooperative=2; Indirect=3; };
struct CallSyntax { CallForm form; bootstrap.ast_storage::Text name; Slice arguments; ExprRef callee; };
extern "C" CallSyntax call_syntax(SignatureNode source) on cpu;
enum CallResolution:u32 { Missing=0; Unique=1; Ambiguous=2; };
enum CallAction:u32 {
    Finish=0; BoundType=1; Local=2; Target=3; Function=4; BadSignature=5; Declared=6; Instance=7;
    MemoryContract=8; ReadOnly=9; Parameter=10; Contents=11; Position=12; External=13; Visit=14;
    Worker=15; LengthNumber=16; LengthName=17; Factory=18; FactoryEffects=19; Generator=20; Advance=21;
    BuiltinType=22; RecordType=23;
};
struct CallCursor { u64 index; u64 parameter; u32 phase; u32 signature; Access default_access; };
struct CallFacts {
    u32 current;
    u32 bound_type;
    u32 builtin_type;
    u32 record;
    u32 typed;
    u32 function;
    u64 parameter_count;
    Access parameter_access;
    CallResolution resolution;
    SignatureNode declaration;
    u32 known_memory_write;
    u32 instantiated;
    u32 read_only;
    u32 contents;
    u32 empty;
    u32 initialized_length;
    SignatureNode length;
    u32 factory_owns;
};
struct CallStep {
    CallCursor cursor;
    CallAction action;
    ExprRef value;
    bootstrap.ast_storage::Text worker;
    Access access;
    u64 parameter;
    u32 contents;
    u32 local;
};
extern "C" CallStep call_step(SignatureNode source, CallSyntax call, CallCursor cursor, CallFacts facts) on cpu;
