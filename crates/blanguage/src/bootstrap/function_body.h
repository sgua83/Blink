module bootstrap.function_body;
import bootstrap.ast_storage;
import bootstrap.signatures;
struct Cursor {u32 phase;u32 flags;u64 index;u64 part;};
enum BeginNeed:u32 {Void=0;Never=1;Type=2;InvalidOwned=3;Shape=4;Dataset=5;Value=6;Early=7;InvalidCooperative=8;Slot=9;Initial=10;Inputs=11;Context=12;Complete=13;Next=14;};
struct BeginStep {Cursor cursor;BeginNeed need;};
struct BeginFacts {u32 void_name;u32 dataset;u32 tile;u32 view;u32 never;u32 owns;u32 value_function;u32 cpu;u32 cooperative;u32 early;u32 layout_ready;u64 scalars;};
enum SequenceNeed:u32 {StartNext=0;StartStop=1;Statement=2;After=3;Complete=4;Next=5;};
struct SequenceStep {Cursor cursor;SequenceNeed need;};
enum FinishNeed:u32 {
 InvalidNever=0;MissingEmpty=1;MissingReturn=2;ImplicitReturn=3;Context=4;Code=5;OtherExits=6;PopExit=7;Scope=8;Append=9;Initial=10;First=11;NoExit=12;FirstModel=13;Slot=14;Registers=15;EmptyRegisters=16;Written=17;BindFirst=18;NextExit=19;BindOther=20;MergePath=21;JoinResult=22;MergeBorrowed=23;RebindWritten=24;Snapshot=25;OutputStorage=26;NextModel=27;Dataset=28;Parameter=29;Place=30;Guards=31;DirectGuards=32;Ownership=33;Guard=34;Initialization=35;Value=36;Owned=37;InvalidModel=38;Insert=39;RebindResult=40;Publish=41;Complete=42;Next=43;
};
struct FinishStep {Cursor cursor;FinishNeed need;};
struct FinishFacts {
 u32 expected;u32 continuing;u32 empty;u32 scoped;u32 last_exit;u32 other_exits;u32 present;u32 result;u32 other_result;u32 return_consumer;u32 model;u32 declared;u32 owns;u32 top;u32 cpu;u32 active;
 u64 slots;u64 scalars;u64 guards;
};
struct CodeSpan {address data;u64 count;};
struct ExitNode {u32 kind;CodeSpan first;CodeSpan second;};
struct ExitHost {cfn<ExitNode,address,u64> ensures memory(read) inspect;};
struct WrittenItem {u32 present;u32 kind;u64 fields;};
struct WrittenHost {cfn<WrittenItem,address> next;cfn<void,address,u64> allocate;cfn<void,address> finish;};
struct ModelsHost {cfn<u32,address> next;cfn<void,address> visit;};
extern "C" Cursor start() on cpu;
extern "C" BeginStep begin_step(Cursor state,BeginFacts facts) on cpu;
extern "C" SequenceStep sequence_step(Cursor state,u64 count,u32 possible,u32 continuing) on cpu;
extern "C" FinishStep finish_step(Cursor state,FinishFacts facts) on cpu;
extern "C" u32 owns_exit(CodeSpan code,ExitHost host) on cpu requires nonzero(host.inspect);
extern "C" u32 allocate_written(address context,WrittenHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.allocate) requires nonzero(host.finish);
extern "C" void models(address context,ModelsHost host) on cpu requires nonzero(host.next) requires nonzero(host.visit);
extern "C" void snapshot(u64 count,address context,cfn<void,address,u64> append) on cpu requires nonzero(append);
extern "C" u64 parameter(u64 count,address context,cfn<u32,address,u64> equal) on cpu requires nonzero(equal);


// A branch body is advanced once by its existing Rust future owner. Only the
// actual completed flow facts are supplied before the join decision.
struct BranchCursor { u32 phase; };
struct BranchFacts { u32 truth; u32 outer_control; u32 condition_uniform; u32 yes_live; u32 no_live; };
enum BranchNeed:u32 { KnownYes=0; KnownNo=1; DynamicYes=2; DynamicNo=3; OnlyYes=4; OnlyNo=5; Both=6; Neither=7; Complete=8; };
struct BranchStep { BranchCursor cursor; BranchNeed need; u32 uniform; };
extern "C" BranchStep branch_step(BranchCursor cursor,BranchFacts facts) on cpu;

struct SelectionResultInput {u32 truth;u32 uniform;};
struct SelectionStorage {u32 yes;u32 no;u64 yes_regs;u64 no_regs;u32 yes_uniform;u32 no_uniform;};
struct SelectionLayout {u64 slots;u64 components;};
enum SelectionAction:u32 {Known=0;Store=1;Emit=2;Load=3;BeginPaths=4;Aliases=5;Path=6;CommonProof=7;CaptureProof=8;Join=9;Take=10;Finish=11;};
struct SelectionHost {
 address context;cfn<SelectionStorage,address> storage;cfn<SelectionLayout,address> layout;
 cfn<void,address,SelectionAction,u32,u64> action;
};
extern "C" void selection_result(SelectionResultInput input,SelectionHost host) on cpu
 requires nonzero(host.storage) requires nonzero(host.layout) requires nonzero(host.action);

struct SelectionMergeInput {u32 truth;u32 yes_live;u32 no_live;u32 returns;u32 uniform;};
struct SelectionWriteFacts {u32 present;u32 occupied;u32 stored;u32 value;};
enum SelectionMergeAction:u32 {
 RememberYes=0;RememberNo=1;Initialize=2;Split=3;BeginRegions=4;Region=5;InsertWrite=6;JoinWrite=7;ClearWrite=8;EndRegions=9;
 Environment=10;YesEffects=11;MergeEffects=12;YesPath=13;NoPath=14;EitherPath=15;ImpossiblePath=16;SelectExits=17;
 YesFlow=18;NoFlow=19;UnionFlow=20;RestoreInit=21;NoEnvironment=22;JoinEnvironment=23;Control=24;PrepareOwners=25;BeginOwners=26;HoldOwner=27;EndOwners=28;
};
struct SelectionMergeHost {
 address context;cfn<u32,address> next_region;cfn<SelectionWriteFacts,address> next_write;
 cfn<u32,address,u32> owner_possible;cfn<u32,address> next_owner;cfn<void,address,SelectionMergeAction,u32> action;
};
extern "C" void selection_merge(SelectionMergeInput input,SelectionMergeHost host) on cpu
 requires nonzero(host.next_region) requires nonzero(host.next_write) requires nonzero(host.owner_possible) requires nonzero(host.next_owner) requires nonzero(host.action);

struct SelectionCursor {u32 phase;};
struct SelectionFacts {u32 continuing;u32 present;u32 other;u32 outer;u32 condition;};
enum SelectionNeed:u32 {Prepare=0;Modified=1;EnterYes=2;Yes=3;Missing=4;EnterNo=5;No=6;Same=7;Finish=8;};
struct SelectionStep {SelectionCursor cursor;SelectionNeed need;u32 uniform;};
extern "C" SelectionStep selection_step(SelectionCursor cursor,SelectionFacts facts) on cpu;

struct OwnerName {address data;u64 length;};
struct OwnerCursor {u32 phase;};
struct OwnerFacts {OwnerName name;u32 present;u32 kind;u32 answer;};
enum OwnerNeed:u32 {NextBinding=0;Original=1;Kind=2;MayOwn=3;Reference=4;Borrow=5;NextResource=6;UnionResource=7;Restrict=8;BorrowPending=9;ReferencesPending=10;StartSources=11;NextSource=12;Overlap=13;Keep=14;Complete=15;};
struct OwnerStep {OwnerCursor cursor;OwnerNeed need;};
struct OwnerNamesHost {address context;cfn<OwnerName,address,u64> name;cfn<void,address,u64,u64> exchange;cfn<void,address> begin;cfn<void,address> take;};
extern "C" OwnerStep owner_step(OwnerCursor cursor,OwnerFacts facts) on cpu;
extern "C" void move_owners(u64 count,OwnerNamesHost host) on cpu
 requires nonzero(host.name) requires nonzero(host.exchange) requires nonzero(host.begin) requires nonzero(host.take);

struct WhileCursor {u32 phase;};
struct WhileFacts {u32 initial;u32 present;u32 truth;u32 possible;u32 continuing;u32 outer;u32 condition;u32 flow;u32 next;};
enum WhileNeed:u32 {Enter=0;Writes=1;Condition=2;Empty=3;Inspect=4;Refine=5;Bounds=6;NextBound=7;Bound=8;Retain=9;Impossible=10;SaveBody=11;Natural=12;RestoreBody=13;EnterBody=14;Body=15;CheckBackedge=16;Split=17;Expand=18;Leave=19;Emit=20;};
struct WhileStep {WhileCursor cursor;WhileNeed need;u32 uniform;u32 flow;};
extern "C" WhileStep while_step(WhileCursor cursor,WhileFacts facts) on cpu;

struct ForCursor {u32 phase;u32 initial;};
struct ForFacts {u32 initial;u32 minimum;u32 maximum;u32 same;u32 seeks;u32 domain;u32 present;u32 outer;u32 begin_uniform;u32 limit_uniform;u32 flow;u32 next;u32 nonempty;};
enum ForNeed:u32 {EmptyBegin=0;EmptyNext=1;EmptyInitialize=2;EmptyDone=3;Seeks=4;ClearInitialization=5;Enter=6;Cursor=7;Test=8;BodyCursor=9;RefineStart=10;Application=11;ApplicationInput=12;ApplicationBinding=13;DatasetBinding=14;TileBinding=15;ViewBinding=16;NextComponent=17;Component=18;FinishOwned=19;FinishView=20;IndexBinding=21;Install=22;Writes=23;Capture=24;Natural=25;Retain=26;EnterBody=27;Body=28;Advance=29;CheckBackedge=30;EndBody=31;IterationElements=32;ExitInitialization=33;TestEntry=34;DropNatural=35;Expand=36;Leave=37;Emit=38;};
struct ForStep {ForCursor cursor;ForNeed need;u32 uniform;u32 flow;};
extern "C" ForStep for_step(ForCursor cursor,ForFacts facts) on cpu;

struct ForInputCursor {u32 phase;u32 nonempty;};
struct ForInputFacts {u32 collision;u32 range;u32 present;u32 name;u32 kind;u32 access;u32 view;u32 begin_min;u32 begin_max;u32 limit_min;u32 limit_max;u32 continuing;};
enum ForInputNeed:u32 {Collision=0;RangeBegin=1;RangeEnd=2;BeginType=3;EndType=4;ApplicationReference=5;SourceKind=6;NameBinding=7;DatasetReference=8;Zero=9;Classify=10;Application=11;Dataset=12;Tile=13;ViewExpression=14;InvalidAccess=15;InvalidSource=16;View=17;Bounds=18;Borrowed=19;Initial=20;Slot=21;Initialization=22;BindApplication=23;Loop=24;CompleteWrites=25;Complete=26;};
struct ForInputStep {ForInputCursor cursor;ForInputNeed need;};
extern "C" ForInputStep for_input_step(ForInputCursor cursor,ForInputFacts facts) on cpu;
struct ForWrittenHost {address context;cfn<u32,address,u64> storage;cfn<u64,address> locals;cfn<u32,address,u64> local;};
extern "C" u32 for_written(u32 unknown,u64 count,ForWrittenHost host) on cpu
 requires nonzero(host.storage) requires nonzero(host.locals) requires nonzero(host.local);

struct LocalCursor {u32 phase;};
struct LetFacts {u32 collision;u32 annotation;u32 kind;u32 publish;u32 tile;u32 continuing;u32 present;};
enum LetNeed:u32 {Collision=0;Inspect=1;Publish=2;Expected=3;Collective=4;Cooperate=5;Tile=6;Evaluate=7;Missing=8;CheckType=9;Bind=10;Complete=11;};
struct LetStep {LocalCursor cursor;LetNeed need;u32 atomic;};
extern "C" u32 local_publish(address bytes,u64 length) on cpu;
extern "C" LetStep let_step(LocalCursor cursor,LetFacts facts) on cpu;
struct DeclareFacts {u32 collision;u32 dataset;u32 owns;};
enum DeclareNeed:u32 {Collision=0;Dataset=1;Type=2;Owned=3;Plain=4;};
struct DeclareStep {LocalCursor cursor;DeclareNeed need;};
extern "C" DeclareStep declare_step(LocalCursor cursor,DeclareFacts facts) on cpu;
struct ValueCursor {u32 phase;u64 index;};
struct ValueFacts {u32 collision;u32 mutable;u32 arrays;u32 uniform;u32 control;u64 slots;u64 regs;};
enum ValueNeed:u32 {View=0;Collision=1;Mutability=2;Arrays=3;Array=4;ArrayFinish=5;Immutable=6;Slots=7;Store=8;Finish=9;};
struct ValueStep {ValueCursor cursor;ValueNeed need;u32 uniform;u64 index;};
extern "C" ValueStep value_step(ValueCursor cursor,ValueFacts facts) on cpu;
struct OwnedFacts {u32 collision;u32 rebound;u32 variants;};
enum OwnedNeed:u32 {Collision=0;Rebound=1;Variants=2;Bind=3;};
struct OwnedStep {LocalCursor cursor;OwnedNeed need;u32 mutable;};
extern "C" OwnedStep owned_step(LocalCursor cursor,OwnedFacts facts) on cpu;
struct ReturnedFacts {u32 kind;u32 collision;u32 rebound;};
enum ReturnedNeed:u32 {Value=0;Owned=1;Unit=2;Collision=3;Rebound=4;Dataset=5;Name=6;};
struct ReturnedStep {LocalCursor cursor;ReturnedNeed need;u32 mutable;};
extern "C" ReturnedStep returned_step(LocalCursor cursor,ReturnedFacts facts) on cpu;
extern "C" u32 binding_rebound(address nodes,address source,address stored,Slice body,address name,u64 length) on cpu;

struct AssignmentCursor {u32 phase;u32 compound;};
struct OperatorSpelling {address data;u64 length;};
struct AssignmentFacts {OperatorSpelling operation;u32 owns;u32 kind;u32 dataset;u32 present;u32 name;u32 borrowed;};
enum AssignmentNeed:u32 {Type=0;Projection=1;Inspect=2;BaseType=3;ElementType=4;Reference=5;Project=6;Name=7;NamedData=8;Application=9;Place=10;PlaceType=11;Owned=12;BorrowKind=13;Hold=14;Right=15;Release=16;Read=17;Combine=18;Write=19;Reverse=20;Element=21;Complete=22;};
struct AssignmentStep {AssignmentCursor cursor;AssignmentNeed need;};
extern "C" AssignmentStep assignment_step(AssignmentCursor cursor,AssignmentFacts facts) on cpu;
extern "C" void reverse_projection(u64 count,address context,cfn<void,address,u64,u64> exchange) on cpu requires nonzero(exchange);
struct ExpressionFacts {u32 kind;u32 atomic;u32 barrier;u32 present;};
enum ExpressionNeed:u32 {Inspect=0;AtomicQuery=1;BarrierQuery=2;Atomic=3;Barrier=4;Cooperate=5;Evaluate=6;Drop=7;Complete=8;};
struct ExpressionStep {LocalCursor cursor;ExpressionNeed need;};
extern "C" u32 statement_barrier(address bytes,u64 length) on cpu;
extern "C" ExpressionStep expression_step(LocalCursor cursor,ExpressionFacts facts) on cpu;

struct JumpFacts {u32 local;u32 breaking;u32 iterator;};
enum JumpNeed:u32 {Outside=0;Release=1;Break=2;Iterator=3;Advance=4;Backedge=5;Continue=6;};
struct JumpStep {LocalCursor cursor;JumpNeed need;};
extern "C" JumpStep jump_step(LocalCursor cursor,JumpFacts facts) on cpu;
struct SeekName {address data;u64 length;};
struct SeekFacts {SeekName requested;SeekName current;u32 iterator;u32 present;u32 outside;};
enum SeekNeed:u32 {Iterator=0;Missing=1;WrongName=2;Position=3;Type=4;Release=5;Compare=6;Break=7;Store=8;Backedge=9;Continue=10;Complete=11;};
struct SeekStep {LocalCursor cursor;SeekNeed need;};
extern "C" SeekStep seek_step(LocalCursor cursor,SeekFacts facts) on cpu;

struct MatchFacts {u32 owns;u32 root;u32 borrowed;u32 continuing;u32 present;u32 kind;};
enum MatchNeed:u32 {Type=0;Root=1;RootKind=2;Parent=3;Borrowed=4;Read=5;Evaluate=6;Missing=7;InvalidType=8;Patterns=9;Arms=10;Complete=11;};
struct MatchStep {LocalCursor cursor;MatchNeed need;};
extern "C" MatchStep match_step(LocalCursor cursor,MatchFacts facts) on cpu;
enum MatchBranch:u32 {Last=0;Value=1;Owned=2;};
extern "C" MatchBranch match_branch(u64 index,u64 count,u32 kind) on cpu;
extern "C" u32 match_binding(u32 present,address data,u64 length) on cpu;
struct MatchArmFacts {u32 owned;u32 named;u32 continuing;u32 present;};
enum MatchArmNeed:u32 {Unpack=0;Payload=1;Name=2;BindValue=3;BindReturned=4;Body=5;Lookup=6;DropBinding=7;Remove=8;DropValue=9;Complete=10;};
struct MatchArmStep {LocalCursor cursor;MatchArmNeed need;};
extern "C" MatchArmStep match_arm_step(LocalCursor cursor,MatchArmFacts facts) on cpu;
struct PatternLabel {u32 qualified;u64 owner;u64 start;};
extern "C" PatternLabel pattern_label(SeekName spelling) on cpu;
extern "C" u64 pattern_find(SeekName name,u64 count,address context,cfn<SeekName,address,u64> variant) on cpu requires nonzero(variant);
struct PatternFacts {u32 eligible;u32 owner;u32 found;u32 duplicate;u32 unit;u32 binding;u32 named;u32 present;u64 arms;u64 variants;};
enum PatternNeed:u32 {InvalidType=0;Arm=1;Owner=2;Find=3;Unknown=4;Select=5;Duplicate=6;Inspect=7;MissingBinding=8;UnitBinding=9;Coverage=10;Incomplete=11;Complete=12;};
struct PatternStep {ValueCursor cursor;PatternNeed need;};
extern "C" PatternStep pattern_step(ValueCursor cursor,PatternFacts facts) on cpu;

struct TryFacts {u64 arguments;u32 continuing;u32 present;u32 kind;u32 result;u32 expected;};
enum TryNeed:u32 {Arity=0;Evaluate=1;Inspect=2;BadSource=3;Expected=4;BadFunction=5;ErrorType=6;ValueCondition=7;OwnedCondition=8;Branches=9;ValueSuccess=10;OwnedSuccess=11;Complete=12;};
struct TryStep {LocalCursor cursor;TryNeed need;};
extern "C" TryStep try_step(LocalCursor cursor,TryFacts facts) on cpu;
struct TryErrorFacts {u32 kind;u32 owns;u32 item;};
enum TryErrorNeed:u32 {Value=0;Owned=1;Expected=2;Storage=3;MoveValue=4;Build=5;ValueReturn=6;OwnedReturn=7;};
struct TryErrorStep {LocalCursor cursor;TryErrorNeed need;};
extern "C" TryErrorStep try_error_step(LocalCursor cursor,TryErrorFacts facts) on cpu;

struct ReadCursor {u32 phase;u32 path;};
struct ReadFacts {u32 source;u32 binding;u32 application;u32 owns;u32 dataset;u32 present;u32 kind;u32 available;u32 variants;u32 place;u32 continuing;};
enum ReadNeed:u32 {Inspect=0;Name=1;NameStorage=2;NameType=3;Application=4;ElementType=5;Place=6;Model=7;Projection=8;Projected=9;BadOwned=10;Available=11;BadAvailable=12;Prepare=13;Variants=14;ProjectionPlace=15;Initialize=16;Load=17;HoldOwned=18;Borrow=19;HoldData=20;Return=21;Member=22;Index=23;Evaluate=24;BadRead=25;Complete=26;};
struct ReadStep {ReadCursor cursor;ReadNeed need;};
extern "C" ReadStep read_step(ReadCursor cursor,ReadFacts facts) on cpu;
struct ReadMemberFacts {SeekName field;u32 present;u32 kind;u32 found;u32 empty;u32 owner;};
enum ReadMemberNeed:u32 {Parent=0;Find=1;MissingField=2;Unpack=3;Value=4;Length=5;BadField=6;Item=7;Retained=8;Release=9;Plain=10;Return=11;Complete=12;};
struct ReadMemberStep {LocalCursor cursor;ReadMemberNeed need;};
extern "C" ReadMemberStep read_member_step(LocalCursor cursor,ReadMemberFacts facts) on cpu;
extern "C" u64 read_field(SeekName name,u64 count,address context,cfn<SeekName,address,u64> field) on cpu requires nonzero(field);
struct ReadIndexCursor {u32 phase;u32 model;u64 index;};
struct ReadIndexFacts {u32 present;u32 kind;u32 indexed;u32 owns;u32 array;u32 owner;u32 place;u32 valid;u64 count;u64 minimum;u64 maximum;u64 components;};
enum ReadIndexNeed:u32 {Reference=0;Parent=1;Kind=2;ArrayValue=3;ViewPlace=4;Data=5;DataIndex=6;DataType=7;Plain=8;Storage=9;Materialize=10;BorrowElement=11;Return=12;BadIndex=13;ArrayType=14;BadArray=15;Owner=16;Hold=17;Expression=18;Type=19;Bounds=20;BadBounds=21;Range=22;Unpack=23;First=24;Join=25;SourcePlace=26;MaterializeArray=27;ArrayPlace=28;Component=29;Rebind=30;ModelKind=31;BorrowModel=32;Complete=33;SetPlace=34;};
struct ReadIndexStep {ReadIndexCursor cursor;ReadIndexNeed need;u64 position;};
extern "C" ReadIndexStep read_index_step(ReadIndexCursor cursor,ReadIndexFacts facts) on cpu;

struct VariantBuildFacts {u32 owned;u32 cpu;u32 application;u32 unit;u64 arguments;u32 continuing;u32 present;};
enum VariantBuildNeed:u32 {InvalidTarget=0;Payload=1;VoidArguments=2;PayloadArguments=3;Unit=4;Evaluate=5;Missing=6;Check=7;Storage=8;Build=9;Complete=10;};
struct VariantBuildStep {LocalCursor cursor;VariantBuildNeed need;};
extern "C" VariantBuildStep variant_build_step(LocalCursor cursor,VariantBuildFacts facts) on cpu;

struct ReturnExprFacts {u32 binding;u32 owns;u32 temporary;u32 present;u32 task;};
enum ReturnExprNeed:u32 {Target=0;Member=1;IndexType=2;Temporary=3;ExpressionType=4;OwnedIndex=5;Place=6;Take=7;BindType=8;Storage=9;BadTask=10;OwnedRecord=11;Record=12;Binding=13;Dataset=14;Owned=15;Selection=16;Value=17;Complete=18;};
struct ReturnExprStep {LocalCursor cursor;ReturnExprNeed need;};
extern "C" ReturnExprStep return_expr_step(LocalCursor cursor,SignatureNode source,ReturnExprFacts facts) on cpu;
struct MemberFacts {SeekName name;u32 owns;u32 present;u32 variants;u32 temporary;u32 place;u32 resource;u32 named;u32 continuing;u32 kind;u32 found;u32 field_owns;u32 item_kind;u32 empty;};
enum MemberNeed:u32 {Type=0;Place=1;Take=2;Projection=3;Projected=4;Reference=5;ReferenceLength=6;BadField=7;Variants=8;BadVariant=9;Temporary=10;IsPlace=11;Resource=12;Named=13;Member=14;Evaluate=15;Find=16;MissingField=17;Unpack=18;FieldType=19;ItemKind=20;MoveData=21;MoveOwner=22;Retained=23;DropOwner=24;HoldOwner=25;Length=26;DropData=27;Value=28;Missing=29;Return=30;Complete=31;};
struct MemberStep {LocalCursor cursor;MemberNeed need;};
extern "C" MemberStep member_step(LocalCursor cursor,MemberFacts facts) on cpu;
struct OwnedIndexFacts {u32 defined;u32 present;u32 continuing;u32 kind;u32 array;u32 indexed;u32 owns;u32 empty;};
enum OwnedIndexNeed:u32 {ExistingType=0;Place=1;TakePlace=2;Evaluate=3;Inspect=4;BadArray=5;HoldData=6;DataIndex=7;DataType=8;TakeData=9;ReadData=10;Storage=11;Release=12;Return=13;HoldOwner=14;Index=15;ArrayPlace=16;TakeElement=17;ArrayValue=18;ViewPlace=19;ReadView=20;Complete=21;};
struct OwnedIndexStep {LocalCursor cursor;OwnedIndexNeed need;};
extern "C" OwnedIndexStep owned_index_step(LocalCursor cursor,OwnedIndexFacts facts) on cpu;

// Borrow selectors from the sealed source store. Actual typed products remain
// with the one World future; a step never retains a pointer into that future.
enum SourceValueNeed:u32 {BadFunction=0;BadCooperate=1;StaticU32=2;Bool=3;Number=4;Name=5;Specialized=6;Member=7;Index=8;Select=9;Target=10;ConstructType=11;Construct=12;Returned=13;FinishReturned=14;Negative=15;Operand=16;Unary=17;Left=18;Right=19;Binary=20;Condition=21;RequireCondition=22;ShortCircuit=23;Empty=24;};
struct SourceValueStep {LocalCursor cursor;SourceValueNeed need;Text spelling;u64 first;u64 second;u64 third;Slice arguments;u32 bits;u32 target;};
extern "C" SourceValueStep source_value_step(LocalCursor cursor,SignatureNode source,u32 present) on cpu;
enum ReturnedValueNeed:u32 {Empty=0;Ordinary=1;Value=2;Bool=3;Complete=4;};
struct ReturnedValueStep {LocalCursor cursor;ReturnedValueNeed need;};
extern "C" ReturnedValueStep returned_value_step(LocalCursor cursor,u32 continuing,u32 boolean) on cpu;
enum ShortCircuitOrder:u32 {RightYes=0;RightNo=1;};
struct ShortCircuitPlan {ShortCircuitOrder order;u32 skipped;};
extern "C" ShortCircuitPlan short_circuit_plan(SeekName operation) on cpu;


enum SourcePlaceNeed:u32 {Name=0;Bind=1;External=2;Parent=3;Project=4;Index=5;Bad=6;Empty=7;};
struct SourcePlaceStep {LocalCursor cursor;SourcePlaceNeed need;Text spelling;u64 first;u64 second;Slice types;Slice arguments;};
extern "C" SourcePlaceStep source_place_step(LocalCursor cursor,SignatureNode source,u32 present) on cpu;
enum MemberPositionNeed:u32 {Project=0;BadView=1;};
extern "C" MemberPositionNeed member_position(u32 view) on cpu;
struct NamePositionFacts {u32 kind;u32 present;u32 arrays;};
enum NamePositionNeed:u32 {Atomic=0;Owned=1;ViewElement=2;InspectOwned=3;ExistingOwned=4;MaterializeOwned=5;ArrayType=6;MaterializeArray=7;Bad=8;};
struct NamePositionStep {LocalCursor cursor;NamePositionNeed need;};
extern "C" NamePositionStep name_position_step(LocalCursor cursor,NamePositionFacts facts) on cpu;
struct IndexPositionFacts {u32 present;u32 bounded;u32 current;u32 same_domain;u32 same_shape;u32 access;u32 whole;u32 appending;};
enum IndexPositionNeed:u32 {Application=0;ApplicationIndex=1;TileBinding=2;TileIndex=3;Dataset=4;View=5;Index=6;Type=7;Bounds=8;BadBounds=9;Append=10;BadAppend=11;Domain=12;Element=13;Empty=14;};
struct IndexPositionStep {LocalCursor cursor;IndexPositionNeed need;Text name;u32 writable;};
extern "C" IndexPositionStep index_position_step(LocalCursor cursor,SignatureNode source,IndexPositionFacts facts) on cpu;
struct PositionAppend {u32 present;u64 output;u64 count;};
extern "C" u32 position_append(u64 binding,address context,cfn<PositionAppend,address> next) on cpu requires nonzero(next);
struct ViewPositionFacts {u32 placed;u32 present;u32 dataset;u32 indexed;u32 continuing;u32 kind;u32 array;};
enum ViewPositionNeed:u32 {Parent=0;DatasetType=1;IndexedType=2;Reference=3;ApplicationIndex=4;Read=5;Returned=6;Kind=7;OwnedArray=8;HoldDataset=9;HoldOwned=10;Ordinary=11;ArrayType=12;Materialize=13;ArrayPlace=14;View=15;Empty=16;};
struct ViewPositionStep {LocalCursor cursor;ViewPositionNeed need;};
extern "C" ViewPositionStep view_position_step(LocalCursor cursor,ViewPositionFacts facts) on cpu;


enum LexicalNeed:u32 {Enter=0;Statement=1;After=2;Finish=3;Release=4;Retain=5;Complete=6;Next=7;};
struct LexicalStep {Cursor cursor;LexicalNeed need;};
extern "C" LexicalStep lexical_block_step(Cursor cursor,u64 count,u32 continuing) on cpu;
struct SourceStatementFacts {u32 present;u32 entry;};
enum SourceStatementNeed:u32 {Bind=0;Declare=1;Let=2;Assign=3;Expression=4;Condition=5;RequireBool=6;Conditional=7;InitialLoop=8;While=9;For=10;LoopControl=11;Match=12;Return=13;Parallel=14;Entry=15;BadPhase=16;Seek=17;Complete=18;};
struct SourceStatementStep {LocalCursor cursor;SourceStatementNeed need;Text name;Text operation;u64 first;u64 second;Slice body;Slice other;u32 present;};
extern "C" SourceStatementStep source_statement_step(LocalCursor cursor,SignatureNode source,SourceStatementFacts facts) on cpu;


struct ReadPlaceFacts {u32 owns;u32 kind;u32 access;u32 reference;u32 memory;u32 known;u32 input;u32 current;u32 field_address;u64 components;u64 words;u64 fields;};
enum ReadPlaceNeed:u32 {Access=0;Owns=1;Kind=2;OwnedError=3;AtomicError=4;AtomicTileError=5;AtomicDataError=6;Array=7;ReferenceMemory=8;Address=9;RememberRead=10;Memory=11;Initialize=12;Zero=13;LoadReference=14;LoadView=15;LoadTile=16;LoadLocal=17;LoadData=18;Proof=19;Snapshot=20;BorrowFields=21;BorrowField=22;SetBorrow=23;ExternalProof=24;DatasetEffect=25;ApplicationRead=26;ApplicationProof=27;TileInput=28;DataAccess=29;DataCross=30;FinishApplication=31;Finish=32;ShareBorrow=33;};
struct ReadPlaceStep {Cursor cursor;ReadPlaceNeed need;u32 flags;};
extern "C" ReadPlaceStep read_place_step(Cursor cursor,ReadPlaceFacts facts) on cpu;
struct NameValueFacts {u32 present;u32 kind;u32 constant;};
enum NameValueNeed:u32 {Scope=0;ScopeValue=1;Binding=2;Value=3;OwnedPlace=4;NamedPlace=5;Read=6;Resolve=7;FunctionKind=8;ModuleConstant=9;Unknown=10;};
struct NameValueStep {LocalCursor cursor;NameValueNeed need;};
extern "C" NameValueStep name_value_step(LocalCursor cursor,NameValueFacts facts) on cpu;
struct SourcePlaceQueryHost {address context;cfn<u32,address,Text> ensures memory(read) binding;};
extern "C" u32 source_is_place(SignatureNode source,SourcePlaceQueryHost host) on cpu requires nonzero(host.binding);

struct WritePlaceFacts {u32 views;u32 persistent;u32 kind;u32 access;u32 writable;u32 memory;u32 pending;u32 resolved;u32 field_address;u32 unique;u32 iterator;u32 loops_empty;u64 words;u64 values;u64 fields;};
enum WritePlaceNeed:u32 {Access=0;ViewType=1;ViewPayloadError=2;Owned=3;ViewAvailable=4;Same=5;Kind=6;NumericProof=7;ReferenceMemory=8;Pending=9;Resolved=10;Snapshot=11;AddressWrite=12;BorrowFields=13;BorrowField=14;AddField=15;Memory=16;AtomicError=17;AtomicTileError=18;AtomicDataError=19;Array=20;ReadOnlyError=21;ViewError=22;TileError=23;WriteError=24;Unique=25;DuplicateError=26;Writable=27;AssignProof=28;Initialize=29;Zero=30;DatasetEffect=31;CaptureProof=32;Alias=33;ApplicationWrite=34;AssignApplicationProof=35;CompleteWrites=36;DataAccess=37;StoreReference=38;StoreLocal=39;StoreData=40;Complete=41;};
struct WritePlaceStep {Cursor cursor;WritePlaceNeed need;};
extern "C" WritePlaceStep write_place_step(Cursor cursor,WritePlaceFacts facts) on cpu;
