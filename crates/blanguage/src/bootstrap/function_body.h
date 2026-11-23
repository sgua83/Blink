module bootstrap.function_body;
import bootstrap.ast_storage;
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
