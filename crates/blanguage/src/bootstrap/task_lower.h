module bootstrap.task_lower;
struct Cursor {u32 phase;u64 index;u64 part;};
struct Step<T> {Cursor cursor;T need;};
enum Kind:u32 {Other=0;Start=1;Scope=2;Pair=3;};
enum Role:u32 {Ordinary=0;Frame=1;ReturnSource=2;Completion=3;};
struct BindHost {
 cfn<u32,address> root;
 cfn<u32,address,usize> function;
};
struct Instruction {Kind kind;address first;address second;};
struct RewriteHost {
 cfn<Instruction,address,usize> inspect;
 cfn<u32,address,address> child;
 cfn<u32,address,address,usize> start;
};
struct LookupFacts {u64 count;u32 equal;};
enum LookupNeed:u32 {Worker=0;Input=1;Start=2;Selector=3;Found=4;Missing=5;Next=6;};
struct NewFacts {u64 fields;u64 parameters;u32 frame;u32 owned;u32 reference;u32 inout;u32 has_result;};
enum NewNeed:u32 {CompletionResult=0;CompletionUnit=1;Output=2;Index=3;Layout=4;Parameter=5;Field=6;RenameWrite=7;RenameInOut=8;PositionType=9;Name=10;Entry=11;Types=12;Positions=13;Binding=14;Complete=15;Next=16;};
enum RewriteNeed:u32 {Registers=0;Address=1;Index=2;Worker=3;Trim=4;Argument=5;Symbol=6;Complete=7;};
struct EntryFacts {u64 parameters;u64 components;u64 result_components;Role role;u32 reference;u32 address;u32 same;u32 size;u32 fallible;u32 result;u32 selector;u32 field;u32 extra;};
enum EntryNeed:u32 {Body=0;Zero=1;Parameter=2;MissingResult=3;ReturnSource=4;CompletionField=5;MissingCompletion=6;CompletionReference=7;Frame=8;Field=9;MissingArgument=10;ValidateAddress=11;BadAddress=12;AddressLow=13;AddressHigh=14;Reference=15;ValidateType=16;BadType=17;LoadValue=18;Value=19;Extra=20;Unused=21;ResultBase=22;ResultReg=23;ErrorReg=24;Call=25;ErrorZero=26;ErrorCompare=27;ErrorIf=28;Store=29;Selector=30;Finish=31;Next=32;};
extern "C" Cursor start() on cpu;
extern "C" u32 bind(address context,u32 root,usize functions,BindHost host) on cpu
 requires nonzero(host.root) requires nonzero(host.function);
extern "C" u32 rewrite_block(address block,usize count,address context,RewriteHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(host.start);
extern "C" Step<LookupNeed> lookup_step(Cursor state,LookupFacts facts) on cpu;
extern "C" Step<NewNeed> new_step(Cursor state,NewFacts facts) on cpu;
extern "C" Step<RewriteNeed> rewrite_step(Cursor state) on cpu;
extern "C" Step<EntryNeed> entry_step(Cursor state,EntryFacts facts) on cpu;
