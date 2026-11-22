module bootstrap.function_body;
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
