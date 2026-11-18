module bootstrap.native_canonical;
enum CanonicalNeed:u32 {Seed=0;Parameters=1;Values=2;Elements=3;Sources=4;BindSources=5;Resources=6;Memory=7;Models=8;Names=9;Initialization=10;PublishNames=11;PrefixBindings=12;Translate=13;Complete=14;};
enum OutputMode:u32 {Values=0;Sources=1;Resources=2;Models=3;Translate=4;};
enum OutputNeed:u32 {Inspect=0;Value=1;Sources=2;Resources=3;Model=4;Empty=5;Translate=6;Prefix=7;Continue=8;Complete=9;};
enum SourceAccess:u32 {Value=0;Read=1;InOut=2;Other=3;};
enum ResourceKind:u32 {None=0;Application=1;Owned=2;};
enum SourceNeed:u32 {Next=0;Bound=1;Storage=2;InspectContent=3;Content=4;InspectResources=5;Resource=6;Continue=7;Complete=8;};
enum MemoryKind:u32 {None=0;Snapshot=1;Dataset=2;};
enum MemoryNeed:u32 {Next=0;Bind=1;Continue=2;Complete=3;};
enum ModelNeed:u32 {Capture=0;Rebind=1;Register=2;Identity=3;Paths=4;Root=5;BorrowedName=6;OwnedName=7;Element=8;InspectNested=9;Nested=10;RootIdentities=11;ReturnedName=12;WrittenName=13;Continue=14;Complete=15;};
enum InitialNeed:u32 {Next=0;Keep=1;Incomplete=2;Complete=3;};
enum BoundNeed:u32 {Next=0;When=1;Bind=2;Yield=3;Continue=4;Complete=5;};
enum PrefixParameterNeed:u32 {Next=0;Bound=1;Bind=2;Continue=3;Complete=4;};
enum Kind:u32 {Unit=0;Value=1;Owned=2;Data=3;};
enum PrefixNeed:u32 {Facts=0;Elements=1;Inspect=2;Payload=3;Continue=4;Complete=5;};
enum SourceModelNeed:u32 {View=0;InspectElement=1;Element=2;InspectPayload=3;Payload=4;Continue=5;Complete=6;};
struct SourceModelFacts {Kind kind;u32 view;u32 present;u64 payloads;};
struct Cursor {u32 phase;u64 index;u64 part;};
struct Step<T> {Cursor cursor;T need;};
struct OutputFacts {u32 present;u32 view;u64 count;};
struct SourceFacts {u32 present;SourceAccess access;u32 dataset;u32 tile;ResourceKind resource;u64 content;u64 resources;};
struct MemoryFacts {u32 present;MemoryKind kind;u64 width;};
struct ModelFacts {u32 present;u32 known;u32 root;u32 borrowed;u32 nested;u64 width;u64 paths;u64 depth;u64 base;u64 model_index;};
struct InitialFacts {u32 present;u32 position;u32 pending;};
struct BoundFacts {u32 present;u32 local;};
struct PrefixParameterFacts {u32 present;u32 origin;u32 reference;u32 dataset;u64 width;};
struct PrefixFacts {Kind kind;u32 present;u64 payloads;};
struct Index {u32 present;u64 value;};
struct Completion {u32 present;u64 address;u64 components;u64 tasks;};
struct Known {u32 present;u32 known;};
struct SeedHost {
 cfn<u64,address> count;cfn<u64,address,u64> input;
 cfn<void,address> prepare_returns;cfn<Index,address> next_return;cfn<Completion,address> next_completion;
 cfn<void,address,u64,u64> input_binding;cfn<void,address,u64> return_binding;
 cfn<void,address,u64,u64,u64> completion_binding;
};
struct BindingHost {cfn<Known,address> next;cfn<void,address> apply;};
extern "C" Cursor start() on cpu;
extern "C" Step<CanonicalNeed> canonical_step(Cursor state) on cpu;
extern "C" Step<OutputNeed> output_step(Cursor state,OutputMode mode,OutputFacts facts) on cpu;
extern "C" Step<SourceNeed> source_step(Cursor state,SourceFacts facts) on cpu;
extern "C" Step<MemoryNeed> memory_step(Cursor state,MemoryFacts facts) on cpu;
extern "C" Step<ModelNeed> model_step(Cursor state,ModelFacts facts) on cpu;
extern "C" Step<InitialNeed> initial_step(Cursor state,InitialFacts facts) on cpu;
extern "C" Step<BoundNeed> bound_step(Cursor state,BoundFacts facts) on cpu;
extern "C" Step<PrefixParameterNeed> prefix_parameter_step(Cursor state,PrefixParameterFacts facts) on cpu;
extern "C" Step<PrefixNeed> prefix_step(Cursor state,PrefixFacts facts) on cpu;
extern "C" void seed(address context,u32 task_source,u32 returned_view,SeedHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.input) requires nonzero(host.prepare_returns)
 requires nonzero(host.next_return) requires nonzero(host.next_completion)
 requires nonzero(host.input_binding) requires nonzero(host.return_binding) requires nonzero(host.completion_binding);
extern "C" void apply_known(address context,BindingHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.apply);
extern "C" Step<SourceModelNeed> element_sources_step(Cursor state,SourceModelFacts facts) on cpu;
