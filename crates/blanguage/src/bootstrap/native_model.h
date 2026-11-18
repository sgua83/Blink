module bootstrap.native_model;
enum Kind:u32 {Unit=0;Value=1;Owned=2;Data=3;};
struct Cursor {u32 phase;u64 index;};
struct Step<T> {Cursor cursor;T need;};
enum RebindNeed:u32 {Value=0;Owned=1;OwnedPlace=2;Root=3;Length=4;DataPlace=5;Inspect=6;Aggregate=7;Variant=8;Next=9;Complete=10;};
struct RebindFacts {Kind kind;u32 variant;u32 present;u64 payloads;};
enum ValueNeed:u32 {Proof=0;Constant=1;Known=2;Uniform=3;Complete=4;};
enum TranslateNeed:u32 {Scalar=0;Root=1;Length=2;RootProof=3;LengthProof=4;Residency=5;Prefixes=6;Count=7;Elements=8;Inspect=9;Payload=10;Clear=11;Next=12;Complete=13;};
struct TranslateFacts {Kind kind;Kind model;u32 present;u32 model_present;u64 payloads;u64 model_payloads;};
struct Component {u32 word;u32 size;};
struct ProjectionHost {
 cfn<u64,address> count;
 cfn<Component,address,u64> component;
 cfn<u64,address,u64> reg;
 cfn<void,address,u64> append;
};
extern "C" Cursor start() on cpu;
extern "C" Step<RebindNeed> rebind_step(Cursor state,RebindFacts facts) on cpu;
extern "C" Step<ValueNeed> value_step(Cursor state,u32 constant) on cpu;
extern "C" Step<TranslateNeed> translate_step(Cursor state,TranslateFacts facts) on cpu;
extern "C" void project(address parent,address child,address value,u32 offset,address output,ProjectionHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.component) requires nonzero(host.reg) requires nonzero(host.append);
