module bootstrap.source_writes;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.c_function;
import bootstrap.scalar_fold;
import bootstrap.proof_facts;

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
    BuiltinType=22; RecordType=23; ReadPlace=24;
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

enum NumericKind:u32 { Missing=0; Unknown=1; Ordered=2; Assign=3; Reflect=4; Shift=5; };
struct NumericTransfer {
    NumericKind kind;
    u32 flags;
    bootstrap.scalar_fold::IntegerWide low;
    bootstrap.scalar_fold::IntegerWide high;
    bootstrap.scalar_fold::IntegerWide least;
    bootstrap.scalar_fold::IntegerWide most;
};
enum NumericProofAction:u32 { None=0; Left=1; Right=2; Join=3; Retain=4; Shift=5; Reflect=6; };
struct NumericCombination { NumericTransfer effect; NumericProofAction proof; };
struct NumericRange { u32 lower; u32 upper; };
struct NumericIterations { u32 present; u64 count; };
struct NumericRanges { u32 present; NumericRange body; NumericRange header; };
struct NumericRetention { u32 flags; NumericProofAction proof; };
extern "C" NumericTransfer numeric_shift(u32 amount, u32 subtract) on cpu;
extern "C" bool numeric_identity(NumericTransfer effect) on cpu;
extern "C" bool numeric_zero_delta(NumericTransfer effect) on cpu;
extern "C" NumericCombination numeric_then(NumericTransfer left, NumericTransfer right) on cpu;
extern "C" NumericCombination numeric_join(NumericTransfer left, NumericTransfer right) on cpu;
extern "C" NumericTransfer numeric_nested_order(NumericTransfer effect, NumericRange guard, NumericRange assigned) on cpu;
extern "C" NumericIterations numeric_iterations(NumericTransfer effect, NumericRange input, NumericRange guard) on cpu;
extern "C" NumericRanges numeric_ranges(NumericTransfer effect, NumericRange input, NumericIterations iterations) on cpu;
extern "C" NumericRetention numeric_retention(NumericTransfer effect, NumericRange input, NumericRange guard, u32 bounded) on cpu;

// Local slot records reuse the compiler's ordered tree storage.
extern "C" u32 has_loop_slot(address slots, usize slot) on cpu;
extern "C" void insert_loop_slot(inout OrderedTree slots, usize slot) on cpu;

enum LoopWriteKind:u32 { Move=0; Position=1; Contents=2; Memory=3; External=4; };
enum StorageWriteKind:u32 { Descendants=0; Frame=1; Bytes=2; };
struct LoopBytes { u64 begin; u64 end; };
struct StorageWrite { usize root; LoopBytes range; StorageWriteKind kind; };
struct LoopStorage { address data; usize count; usize capacity; };
extern "C" void drop_loop_storage(inout LoopStorage storage) on cpu;
struct LoopStorageSite { usize root; u32 descendants; u32 declared; };
struct LoopStoragePart { u32 present; u32 word; u32 width; };
struct LoopStorageHost {
    address context;
    cfn<LoopStorageSite,address,usize> ensures memory(read) site;
    cfn<u32,address,usize> start_parts;
    cfn<LoopStoragePart,address> next_part;
};
extern "C" void project_loop_storage(inout LoopStorage output, LoopStorageHost host, usize count, u32 constant_index, u32 index) on cpu
    requires nonzero(host.site) requires nonzero(host.start_parts) requires nonzero(host.next_part);
extern "C" LoopBytes loop_array_elements(u64 length, u32 exact, u32 index) on cpu;
extern "C" u32 loop_part_matches(u32 source_scalar, u32 source_word, u32 target_scalar, u32 target_word, u32 offset) on cpu;
enum LoopProjectionAction:u32 { Done=0; Memory=1; Position=2; };
struct LoopProjectionCursor { usize next; u32 unknown_storage; u32 external; };
struct LoopProjectionRequest { usize index; LoopProjectionAction action; };
struct LoopWriteInfo { LoopWriteKind kind; u32 address_write; u32 iterator; };
struct LoopContentsInfo { usize root; u32 found; u32 owns_resources; };
struct LoopProjectionHost {
    address context;
    cfn<LoopWriteInfo,address,usize> write;
    cfn<LoopContentsInfo,address,usize> ensures memory(read) contents;
};
extern "C" LoopProjectionRequest advance_loop_projection(inout LoopProjectionCursor cursor,
    inout LoopStorage storage, usize count, LoopProjectionHost host) on cpu
    requires nonzero(host.write) requires nonzero(host.contents);

enum LoopPosition:u32 { Missing=0; Local=1; Reference=2; Other=3; };
enum LoopMemoryFact:u32 { Unresolved=0; Resource=1; OtherResource=2; Owner=3; };
extern "C" void record_loop_memory(inout LoopProjectionCursor cursor, LoopMemoryFact fact,
    LoopPosition position, u32 present) on cpu;
extern "C" void record_loop_position(inout LoopProjectionCursor cursor, LoopPosition position,
    u32 written, u32 has_binding, u32 direct) on cpu;
enum LoopInvalidation:u32 { Elements=0; Rebind=1; Select=2; Apply=3; Done=4; };
extern "C" LoopInvalidation loop_invalidation_next(LoopInvalidation phase) on cpu;
enum LoopElementWrite:u32 { None=0; Bytes=1; Unknown=2; Contents=3; Position=4; };
struct LoopElementEffect { LoopElementWrite action; u32 raw; u32 deferred; };
extern "C" LoopElementEffect loop_element_effect(LoopWriteKind kind, u32 address_write, u32 resolved) on cpu;
struct LoopIndexRange { u32 low; u32 high; };
extern "C" LoopIndexRange loop_index_range(u32 constant, u32 index) on cpu;

enum LoopRebind:u32 { Keep=0; CheckBorrow=1; Scan=2; Forget=3; };
extern "C" LoopRebind loop_rebind(u32 held, u32 identity, u32 borrowed, u32 checked, u32 written) on cpu;
enum LoopComponentWrite:u32 { None=0; All=1; Memory=2; OutsideElement=3; Contents=4; Position=5; };
extern "C" u32 loop_scalar_header(u32 conditional, u32 owned, u32 unsigned_scalar, LoopPosition position) on cpu;
extern "C" u32 loop_cursor_query(LoopWriteKind kind, u32 cursor, u32 application_element) on cpu;
extern "C" LoopComponentWrite loop_component_write(LoopWriteKind kind, u32 cursor, u32 same_resource, u32 same_layout, u32 same_index) on cpu;
extern "C" u32 loop_deferred_input(u32 deferred, u32 view_scope) on cpu;
extern "C" u32 loop_metadata_contains(u32 begin, u32 end, u32 word) on cpu;
extern "C" u32 loop_external_proof(u32 raw, u32 view_scope) on cpu;
extern "C" u32 loop_local_input(u32 view_scope, u32 entry, u32 direct) on cpu;

extern "C" bool numeric_contains_expression(SignatureNode root, SignatureNode selected) on cpu;
extern "C" bool numeric_contains_statement(SignatureNode root, SignatureNode selected) on cpu;
extern "C" bool numeric_loop_control(StorageView storage, Slice body, u32 seek) on cpu;

enum NumericPathAction:u32 { Next=0; Stop=1; Expression=2; Target=3; Assignment=4; Declare=5; Let=6;
    Condition=7; Yes=8; No=9; Arm=10; Reenter=11; Nested=12; NestedPoint=13; ObserveLoop=14; Finish=15; };
struct NumericPathCursor { u64 index; u64 arm; u32 phase; };
struct NumericPathStep {
    NumericPathCursor cursor;
    NumericPathAction action;
    StmtRef statement;
    ExprRef value;
    Slice body;
    Optional<bootstrap.ast_storage::Text> binding;
    bootstrap.ast_storage::Text variant;
    Optional<TypeRef> annotation;
    u32 local;
};
extern "C" NumericPathStep numeric_path_step(StorageView storage, Slice block, NumericPathCursor cursor,
    Truth truth, u32 reaches, Optional<SignatureNode> point) on cpu;

enum NumericRhsAction:u32 { Unknown=0; Preserve=1; Relative=2; Shift=3; Subtract=4; Reflect=5; Value=6; Resize=7; Length=8; };
struct NumericRhsStep { u32 phase; NumericRhsAction action; ExprRef value; };
extern "C" NumericRhsStep numeric_rhs_step(SignatureNode source, u32 phase, u32 accepted, u32 length, u32 same_name) on cpu;
