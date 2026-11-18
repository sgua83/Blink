module bootstrap.function_body;
struct Cursor {u32 phase;u32 flags;u64 index;u64 part;};
enum BeginNeed:u32 {Name=0;Void=1;Never=2;Type=3;InvalidOwned=4;Shape=5;Dataset=6;Value=7;Early=8;InvalidCooperative=9;Slot=10;Initial=11;Inputs=12;Context=13;Complete=14;Next=15;};
struct BeginStep {Cursor cursor;BeginNeed need;};
struct BeginFacts {u32 void_name;u32 dataset;u32 tile;u32 view;u32 never;u32 owns;u32 value_function;u32 cpu;u32 cooperative;u32 early;u64 scalars;};
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
