module bootstrap.ownership_storage;
enum Kind:u32 {Unit=0;Value=1;Owned=2;Data=3;};
struct Cursor {u32 phase;u64 index;};
struct Step<T> {Cursor cursor;T need;};
struct Layout {u32 length_offset;u32 alignment;u32 size;};
enum StorageNeed:u32 {Value=0;Owned=1;Unit=2;RootProof=3;LengthProof=4;Data=5;Complete=6;};
enum RebindNeed:u32 {Value=0;Owned=1;OwnedPlace=2;Root=3;Length=4;Complete=5;};
enum UnpackNeed:u32 {Value=0;Clone=1;Inspect=2;ValueRegs=3;Owned=4;Root=5;Length=6;Count=7;Place=8;Complete=9;Next=10;};
struct UnpackFacts {u32 owns;Kind kind;u32 count;u32 place;};
enum TemplateNeed:u32 {Retain=0;Element=1;Handle=2;Length=3;FixedLength=4;LengthProof=5;DataOwned=6;DataMoved=7;InspectField=8;Field=9;Recurse=10;AggregateEmpty=11;AggregateStore=12;AggregateOwned=13;AggregateMoved=14;VariantSlots=15;VariantField=16;VariantStore=17;VariantOwned=18;VariantMoved=19;Unit=20;Value=21;Next=22;};
struct TemplateFacts {u32 dataset;u32 owns;u32 variant;u32 cleanup;u32 task;u32 available;u32 void_type;u32 child_owns;u32 count;u64 fields;u64 variants;};
enum ConstructNeed:u32 {Variant=0;Target=1;Count=2;Evaluate=3;AbandonInspect=4;AbandonRemove=5;Empty=6;Missing=7;InspectArray=8;ReturnItem=9;Copy=10;Check=11;InspectHold=12;Hold=13;Store=14;Finish=15;Take=16;Unhold=17;InspectItem=18;MoveValue=19;Storage=20;Payload=21;EmptyPayload=22;Pack=23;Next=24;};
struct ConstructFacts {u32 variant;u32 cpu;u32 application;u32 array;u32 continues;u32 present;Kind kind;u32 same_array;u32 borrows;u32 held;u32 field_owns;u64 arguments;u64 fields;u64 prepared;};
struct Component {u32 scalar;u32 word;u32 size;};
struct VariantHost {
    cfn<u64,address> count;
    cfn<Component,address,u64> component;
    cfn<void,address,u64> select;
    cfn<void,address> proof;
    cfn<u64,address,u64> selected;
    cfn<void,address,u64,u64> forget;
};
extern "C" Cursor start() on cpu;
extern "C" Layout dataset_layout(u32 root_size,u32 root_align,u32 length_size,u32 length_align) on cpu;
extern "C" Step<StorageNeed> storage_step(Cursor state,Kind kind) on cpu;
extern "C" Step<RebindNeed> rebind_step(Cursor state,Kind kind) on cpu;
extern "C" Step<UnpackNeed> unpack_step(Cursor state,UnpackFacts facts) on cpu;
extern "C" Step<TemplateNeed> template_step(Cursor state,TemplateFacts facts) on cpu;
extern "C" Step<ConstructNeed> construct_step(Cursor state,ConstructFacts facts) on cpu;
extern "C" void variant_projection(address root,address child,u32 offset,address output,VariantHost host) on cpu
    requires nonzero(host.count) requires nonzero(host.component) requires nonzero(host.select)
    requires nonzero(host.proof) requires nonzero(host.selected) requires nonzero(host.forget);
