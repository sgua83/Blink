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
struct EntrySpec {
 usize parameters;
 usize fields;
 usize results;
 u32 result_bytes;
 u32 has_result;
 u32 fallible;
 u32 selector;
};
struct EntryParameter { Role role; u32 reference; u32 size; usize components; };
struct EntryField { address ty; u32 offset; usize buffer; };
struct EntryAddress { usize low; usize high; };
struct EntryCall { usize result; usize error; };
enum EntryStatus:u32 { Complete=0; MissingResult=1; MissingCompletion=2; MissingArgument=3; BadAddress=4; BadType=5; Unused=6; };
struct EntryHost {
 cfn<EntryParameter,address,usize> parameter;
 cfn<EntryField,address,usize> field;
 cfn<u32,address,usize,address,u32> valid;
 cfn<EntryAddress,address,EntryField> pointer;
 cfn<void,address,usize,Role,EntryAddress,u32> reference;
 cfn<usize,address,usize,usize,EntryField> load_value;
 cfn<void,address,usize,usize,usize> value;
 cfn<EntryCall,address,u32> call;
 cfn<void,address,usize> raise_error;
 cfn<void,address,usize,usize> store;
 cfn<void,address> selector;
};
extern "C" Cursor start() on cpu;
extern "C" u32 bind(address context,u32 root,usize functions,BindHost host) on cpu
 requires nonzero(host.root) requires nonzero(host.function);
extern "C" u32 rewrite_block(address block,usize count,address context,RewriteHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(host.start);
extern "C" Step<LookupNeed> lookup_step(Cursor state,LookupFacts facts) on cpu;
extern "C" Step<NewNeed> new_step(Cursor state,NewFacts facts) on cpu;
extern "C" Step<RewriteNeed> rewrite_step(Cursor state) on cpu;
extern "C" EntryStatus emit_entry(address context,EntrySpec spec,EntryHost host) on cpu
 requires nonzero(host.parameter) requires nonzero(host.field) requires nonzero(host.valid)
 requires nonzero(host.pointer) requires nonzero(host.reference) requires nonzero(host.load_value)
 requires nonzero(host.value) requires nonzero(host.call) requires nonzero(host.raise_error)
 requires nonzero(host.store) requires nonzero(host.selector);
