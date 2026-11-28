module bootstrap.call_arguments;
import bootstrap.ast_storage;
import bootstrap.signatures;
enum CallAccess:u32 {Value=0;Read=1;Write=2;InOut=3;Atomic=4;};
enum ParameterForm:u32 {Ordinary=0;Dataset=1;Tile=2;Acceleration=3;Texture=4;};
enum ArgumentKind:u32 {BorrowedData=0;Acceleration=1;Texture=2;Value=3;Dataset=4;Data=5;Scoped=6;OwnedValue=7;OwnedReference=8;Tile=9;Atomic=10;Reference=11;};
enum BindingKind:u32 {Other=0;Owned=1;Data=2;OwnedValue=3;Value=4;DataReference=5;Atomic=6;};
struct Parameter {CallAccess access;ParameterForm form;};
struct Phase {u32 phase;};
struct Cursor {u64 index;u32 phase;};
struct BuildCursor {u64 index;u32 phase;u32 retained;};
struct CheckCursor {u64 index;u64 part;u32 phase;};
struct SourceFacts {u32 present;u32 continuing;u32 dataset;};
struct ArgumentFacts {ArgumentKind kind;u32 owner;u32 scoped_dataset;u32 application_element;u64 sources;};
struct BuildFacts {Parameter parameter;ArgumentKind kind;u32 scoped_dataset;u32 present;u32 continuing;u32 declared;u32 held;};
struct HoldFacts {ArgumentKind kind;u32 owner;u32 application_element;u32 borrows;};
struct FinishFacts {ArgumentKind kind;u32 owner;BindingKind binding;};
struct CollectFacts {ArgumentKind kind;u32 owner;CallAccess access;};
struct RetainFacts {u32 present;u32 scoped;ArgumentKind kind;};
struct CheckFacts {ArgumentFacts argument;CallAccess access;u64 resources;u64 paths;u32 tasks;};
enum SourceNeed:u32 {Acceleration=0;Texture=1;EvaluateDataset=2;ApplicationReference=3;DatasetReference=4;Scoped=5;Tile=6;Atomic=7;Mutable=8;Expected=9;Read=10;Value=11;ReturnAcceleration=12;InvalidAcceleration=13;ReturnDataset=14;MissingDataset=15;ReturnApplicationReference=16;ReturnDatasetReference=17;ReturnRead=18;ReturnValue=19;MissingValue=20;Empty=21;};
enum BuildNeed:u32 {CountMismatch=0;Inspect=1;Before=2;Lower=3;Abandon=4;Append=5;OutputDeclared=6;PrepareOutput=7;Hold=8;Remember=9;Finish=10;Ready=11;Empty=12;ReadyRetained=13;};
enum HoldNeed:u32 {ReadReference=0;ClearReference=1;DataReference=2;Reference=3;Atomic=4;Value=5;OwnedValue=6;ReadOwnedReference=7;BorrowOwnedReference=8;Data=9;Existing=10;None=11;};
enum FinishNeed:u32 {Inspect=0;Remove=1;Discard=2;Reference=3;Data=4;OwnedValue=5;Value=6;Invalid=7;Complete=8;};
enum CollectNeed:u32 {Inspect=0;Scoped=1;TransferOwned=2;TransferData=3;Complete=4;};
enum RetainNeed:u32 {NextEnvironment=0;Environment=1;InspectArgument=2;Reference=3;Complete=4;};
enum CheckNeed:u32 {Inspect=0;OwnedData=1;OwnedResources=2;OwnedResource=3;Invocation=4;MutableResources=5;MutableResource=6;Tasks=7;TaskPosition=8;DataSources=9;ResourcePaths=10;FieldResource=11;ReferenceSource=12;PlaceAccess=13;PlaceIdentity=14;PlaceSource=15;HandleExclusion=16;HandleAvailable=17;Aliases=18;Complete=19;};

extern "C" Phase source_start() on cpu;
extern "C" SourceNeed source_step(inout Phase cursor,Parameter parameter,SourceFacts facts) on cpu;
extern "C" BuildCursor build_start(u64 count,u64 parameters) on cpu;
extern "C" BuildNeed build_step(inout BuildCursor cursor,u64 count,BuildFacts facts) on cpu;
extern "C" Phase hold_start() on cpu;
extern "C" HoldNeed hold_step(inout Phase cursor,CallAccess access,HoldFacts facts) on cpu;
extern "C" Cursor finish_start() on cpu;
extern "C" FinishNeed finish_step(inout Cursor cursor,u64 count,FinishFacts facts) on cpu;
extern "C" Cursor collect_start() on cpu;
extern "C" CollectNeed collect_step(inout Cursor cursor,u64 count,CollectFacts facts) on cpu;
extern "C" Cursor retain_start() on cpu;
extern "C" RetainNeed retain_step(inout Cursor cursor,u64 count,RetainFacts facts) on cpu;
extern "C" CheckCursor checks_start() on cpu;
extern "C" CheckNeed checks_step(inout CheckCursor cursor,u64 count,CheckFacts facts) on cpu;

struct ReadInputFacts {u32 present;u32 bound;u32 place;u32 temporary;u32 owns;u32 storage;u32 writable;u32 kind;u32 owner;u32 application;};
enum ReadInputNeed:u32 {Reference=0;BorrowedData=1;Root=2;IsPlace=3;Temporary=4;Type=5;Place=6;Storage=7;Writable=8;Initialize=9;Borrow=10;Aggregate=11;Application=12;OwnedReference=13;Scoped=14;Resource=15;Empty=16;};
struct ReadInputStep {Phase cursor;ReadInputNeed need;};
extern "C" Optional<Text> read_input_root(SignatureNode source) on cpu;
extern "C" ReadInputStep read_input_step(Phase cursor,ReadInputFacts facts) on cpu;

struct MutableInputFacts {u32 present;u32 place;u32 dataset;u32 owns;u32 continuing;};
enum MutableInputNeed:u32 {Reference=0;Borrowed=1;IsPlace=2;Place=3;Dataset=4;Application=5;Owns=6;Plain=7;Owned=8;Evaluate=9;Returned=10;Empty=11;};
struct MutableInputStep {Phase cursor;MutableInputNeed need;};
extern "C" MutableInputStep mutable_input_step(Phase cursor,MutableInputFacts facts) on cpu;
enum MutableValueNeed:u32 {HoldData=0;BorrowData=1;Scoped=2;BindOwner=3;OwnerPlace=4;HoldOwner=5;Reference=6;Invalid=7;};
struct MutableValueStep {Phase cursor;MutableValueNeed need;};
extern "C" MutableValueStep mutable_value_step(Phase cursor,u32 kind) on cpu;
struct OwnedInputFacts {u32 application;u32 borrowed;u32 available;CallAccess access;};
enum OwnedInputNeed:u32 {Application=0;Model=1;Borrowed=2;Prepare=3;Available=4;Unavailable=5;Load=6;Reference=7;};
struct OwnedInputStep {Phase cursor;OwnedInputNeed need;u32 reading;};
extern "C" OwnedInputStep owned_input_step(Phase cursor,OwnedInputFacts facts) on cpu;

enum ReferenceKind:u32 {Other=0;Reference=1;Local=2;Element=3;Application=4;View=5;Indexed=6;};
struct CallReferenceFacts {u32 atomic;CallAccess access;ReferenceKind kind;ReferenceKind base;u32 cross;};
enum CallReferenceNeed:u32 {Kind=0;BadAtomic=1;Base=2;ElementFacts=3;Read=4;Write=5;Cross=6;Reference=7;Local=8;Element=9;Application=10;View=11;IndexedReference=12;IndexedLocal=13;IndexedElement=14;IndexedApplication=15;IndexedView=16;BadStorage=17;DataAccess=18;Complete=19;};
struct CallReferenceStep {Phase cursor;CallReferenceNeed need;};
extern "C" CallReferenceStep call_reference_step(Phase cursor,CallReferenceFacts facts) on cpu;

enum CCallArgument:u32 {Other=0;Value=1;Reference=2;Scoped=3;Atomic=4;};
struct CCallCursor {u64 index;u32 phase;};
struct CCallFacts {u32 cpu;u32 never;u32 contract;u32 capture_names;u32 reference;CallAccess access;CCallArgument kind;u32 captured;u32 declared;};
enum CCallNeed:u32 {BadTarget=0;Inputs=1;Inspect=2;Scoped=3;Materialize=4;Value=5;CaptureName=6;CapturedValue=7;PlainValue=8;Reference=9;Address=10;WrittenReference=11;PlainReference=12;Invalid=13;Prove=14;Registers=15;Memory=16;Emit=17;Stop=18;Returned=19;WrittenInspect=20;Guards=21;Forget=22;AllGuards=23;Assign=24;Initialize=25;Finish=26;Complete=27;};
struct CCallStep {CCallCursor cursor;CCallNeed need;u32 reading;};
extern "C" CCallStep c_call_step(CCallCursor cursor,CCallFacts facts,u64 arguments,u64 written,u64 held) on cpu;

struct NativeReferenceFacts {u32 c_abi;CallAccess access;u32 expose_writes;u32 form;u32 components;u32 present;};
enum NativeReferenceNeed:u32 {CAbi=0;Layout=1;Empty=2;Whole=3;Components=4;Part=5;Words=6;Invalid=7;Base=8;Word=9;Complete=10;};
struct NativeReferenceStep {CCallCursor cursor;NativeReferenceNeed need;u32 expose_writes;};
extern "C" NativeReferenceStep native_reference_step(CCallCursor cursor,NativeReferenceFacts facts,u64 count) on cpu;
enum ContiguousNeed:u32 {Words=0;Assumed=1;True=2;Invalid=3;Physical=4;};
struct ContiguousStep {Phase cursor;ContiguousNeed need;};
struct ReferenceOffset {u32 word;u32 offset;};
extern "C" ContiguousStep contiguous_step(Phase cursor,u32 symbolic,u64 components,u32 present) on cpu;
extern "C" u32 physical_contiguous(u32 first,u32 alignment,u64 count,address context,cfn<ReferenceOffset,address> next) on cpu
 requires nonzero(alignment) requires nonzero(next);

struct FunctionValueFacts {u32 cpu;u32 present;u32 signature;u32 nonzero;u32 contracts;};
enum FunctionValueNeed:u32 {BadTarget=0;Lower=1;Empty=2;Kind=3;BadKind=4;Nonzero=5;Zero=6;Signature=7;Arguments=8;Contracts=9;ReadyPlain=10;ReadyContract=11;};
struct FunctionValueStep {Phase cursor;FunctionValueNeed need;};
extern "C" FunctionValueStep function_value_step(Phase cursor,FunctionValueFacts facts) on cpu;

struct TakeAddressFacts {u32 cpu;u64 arguments;u32 present;u32 owns;u32 views;u32 empty;u32 writable;u32 storage;};
enum TakeAddressNeed:u32 {BadTarget=0;BadCount=1;Place=2;Empty=3;Layout=4;BadLayout=5;Origin=6;Reference=7;Register=8;Storage=9;LocalOrigin=10;ReferenceIdentity=11;CopyOrigin=12;Tail=13;StorageIdentity=14;Emit=15;Complete=16;};
struct TakeAddressStep {Phase cursor;TakeAddressNeed need;CallAccess access;};
extern "C" TakeAddressStep take_address_step(Phase cursor,TakeAddressFacts facts) on cpu;
struct AddressTailHost {cfn<u32,address,u64> offset;cfn<void,address,u64,u32> append;};
enum AddressSourceKind:u32 { End=0; Position=1; Inactive=2; Other=3; };
enum AddressSourceWalk:u32 { Write=0; DerivedRegion=1; };
struct AddressSourceEntry { AddressSourceKind kind; u32 possible; };
struct AddressSourceHost {
    address context;
    cfn<AddressSourceEntry,address> next;
    cfn<u32,address> projection;
};
extern "C" u32 walk_address_sources(u32 unavailable, AddressSourceWalk walk, AddressSourceHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.projection);

struct RegionBytes { u32 word; u32 offset; u32 size; };
enum RegionSeedKind:u32 { Positions=0; Resource=1; External=2; };
struct RegionSeed { usize parts; usize positions; usize words; RegionSeedKind kind; u32 materialize; };
struct RegionPart { u32 word; u32 size; };
struct RegionSeedHost {
    address context;
    cfn<RegionPart,address,usize> ensures memory(read) part;
    cfn<void,address,usize> reserve;
    cfn<void,address,usize,RegionBytes> emit;
};
extern "C" u32 seed_memory_regions(RegionSeed seed, RegionSeedHost host) on cpu
    requires nonzero(host.part) requires nonzero(host.reserve) requires nonzero(host.emit);

struct RegionSpan { address regions; usize first; usize end; };
struct RegionHost {
    cfn<RegionBytes,address,usize> ensures memory(read) part;
    cfn<RegionBytes,address,usize> ensures memory(read) region;
    cfn<void,address,usize,RegionBytes> emit;
    cfn<void,RegionSpan> finish;
};
extern "C" u32 project_memory_regions(address origin, usize parts, u32 parent_size, RegionSpan parent,
    RegionHost host) on cpu
    requires nonzero(host.part) requires nonzero(host.region) requires nonzero(host.emit) requires nonzero(host.finish);

enum RegionWriteKind:u32 { Unrelated=0; Position=1; Resource=2; Selected=3; };
struct RegionWriteFacts { RegionWriteKind kind; u32 word; u32 target; u32 offset; u32 size; };
extern "C" u8 region_written_bytes(RegionWriteFacts facts, u32 width) on cpu;

enum WritePositionKind:u32 { None=0; Ranges=1; All=2; Composite=3; };
struct WritePositionView { usize children; WritePositionKind kind; };
struct WriteRegions { address data; usize count; u32 present; };
struct WriteProjectionHost {
    address context;
    cfn<u32,address,address> distinct_element;
    cfn<WritePositionView,address,address> position;
    cfn<address,address,usize> child;
    cfn<WriteRegions,address> regions;
    cfn<RegionWriteFacts,address,address,usize> region;
};
extern "C" u8 project_address_write_bytes(address position, u32 width, WriteProjectionHost host) on cpu
    requires nonzero(host.distinct_element) requires nonzero(host.position) requires nonzero(host.child)
    requires nonzero(host.regions) requires nonzero(host.region);

struct MemoryEffectCoverage {
    usize count;
    u32 width;
    CallAccess request;
    CallAccess external;
    u32 external_present;
    u32 outside_element;
};
struct MemoryEffectCoverageHost {
    address context;
    cfn<CallAccess,address,usize> ensures memory(read) access;
    cfn<u32,address,usize> same_element;
    cfn<u8,address,usize> project;
};
extern "C" u8 memory_effect_bytes(MemoryEffectCoverage effect, MemoryEffectCoverageHost host) on cpu
    requires nonzero(host.access) requires nonzero(host.same_element) requires nonzero(host.project);

struct WrittenValueFacts {
    usize count;
    u64 mask;
    u64 bias;
    u32 width;
    u32 integer;
    CallAccess external;
};
struct WrittenValueSource { u8 bytes; u32 proof; };
struct WrittenValueRegion { u8 bytes; u32 offset; u32 size; };
struct WrittenValueBits { u64 changed; u64 kept; u64 ordered; u32 exact; u32 partial_integer; };
struct WrittenValueHost {
    address context;
    cfn<CallAccess,address,usize> ensures memory(read) access;
    cfn<WrittenValueSource,address,usize> project;
    cfn<usize,address> regions;
    cfn<WrittenValueRegion,address,usize> region;
    cfn<u32,address,usize> retain;
    cfn<void,address,WrittenValueBits> merge;
};
extern "C" u32 written_memory_value(WrittenValueFacts facts, WrittenValueHost host) on cpu
    requires nonzero(host.access) requires nonzero(host.project) requires nonzero(host.regions)
    requires nonzero(host.region) requires nonzero(host.retain) requires nonzero(host.merge);

extern "C" u32 address_tail(address context,u64 count,u32 size,u32 first,AddressTailHost host) on cpu
 requires nonzero(host.offset) requires nonzero(host.append);
// explicit_type: 0 is not one type, 1 is a type, 2 is a root dataset.
struct ExternalPositionFacts {u32 cpu;u32 explicit_type;u64 arguments;u32 owns;u32 views;u32 empty;u32 present;u32 external;u32 identity;u32 constant;};
enum ExternalPositionNeed:u32 {BadTarget=0;BadArguments=1;Type=2;Owned=3;BadLayout=4;Argument=5;Empty=6;RequireAddress=7;Borrow=8;Requirement=9;BorrowKind=10;Identity=11;KnownBits=12;ConstantIdentity=13;DynamicIdentity=14;Reword=15;Low=16;High=17;ReadyExternal=18;ReadyBorrowed=19;};
struct ExternalPositionStep {Phase cursor;ExternalPositionNeed need;};
extern "C" ExternalPositionStep external_position_step(Phase cursor,ExternalPositionFacts facts) on cpu;
extern "C" u32 data_external_sources(u32 pending,address context,cfn<u32,address> next) on cpu requires nonzero(next);

struct DatasetWriteRegion {
    u32 word;
    u32 bytes;
};

extern "C" void dataset_write_ranges(
    address regions, usize count, address output,
    cfn<DatasetWriteRegion,address> next, cfn<void,address,DatasetWriteRegion> emit
) on cpu requires nonzero(next) requires nonzero(emit);
