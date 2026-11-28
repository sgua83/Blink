module bootstrap.call_arguments;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.ir_dataflow;
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

enum OriginSourceKind:u32 { End=0; Position=1; Inactive=2; External=3; Other=4; };
struct OriginSource { OriginSourceKind kind; u32 layout_matches; };
enum OriginSourceAction:u32 { Direct=0; Project=1; Inactive=2; External=3; Retain=4; Pending=5; };
struct OriginNormalizationHost {
    address context;
    cfn<OriginSource,address> next;
    cfn<void,address,OriginSourceAction> emit;
};
extern "C" void normalize_address_origin(u32 derived, u32 pending, OriginNormalizationHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.emit);

struct OriginWord { u32 word; u32 initialized; };
enum OriginProjectionResult:u32 { Missing=0; ReadOnly=1; Writable=2; };
struct OriginProjectionHost {
    address context;
    cfn<OriginWord,address,usize> ensures memory(read) selected;
    cfn<u32,address,usize> ensures memory(read) parent_offset;
    cfn<OriginWord,address,usize> ensures memory(read) parent;
    cfn<void,address,usize,u32,u32> emit;
};
extern "C" OriginProjectionResult project_address_origin(usize selected, usize parent,
    u32 selected_writable, u32 parent_writable, OriginProjectionHost host) on cpu
    requires nonzero(host.selected) requires nonzero(host.parent_offset)
    requires nonzero(host.parent) requires nonzero(host.emit);

enum AddressPositionRoot:u32 { Other=0; Derived=1; Dataset=2; };
enum AddressPositionValue:u32 { Stored=0; Initialized=1; };
struct AddressPositionHost {
    address context;
    cfn<u32,address,usize> ensures memory(read) offset;
    cfn<u32,address,usize> ensures memory(read) initialized;
    cfn<void,address,usize,AddressPositionValue> emit;
    cfn<AddressSourceEntry,address> next;
    cfn<u32,address,usize> resolve;
    cfn<u32,address> same;
    cfn<void,address> accept;
    cfn<u32,address> ensures memory(read) pending;
};
extern "C" u32 address_position_at(u32 word, usize parts, u32 stored, AddressPositionRoot root,
    AddressPositionHost host) on cpu requires nonzero(host.offset) requires nonzero(host.initialized)
    requires nonzero(host.emit) requires nonzero(host.next) requires nonzero(host.resolve)
    requires nonzero(host.same) requires nonzero(host.accept) requires nonzero(host.pending);

enum BorrowedPositionMatch:u32 { Unavailable=0; Equal=1; Different=2; };
struct BorrowedPositionHost {
    address context;
    cfn<AddressSourceEntry,address> next;
    cfn<u32,address,usize> remember;
    cfn<BorrowedPositionMatch,address,usize> compare;
    cfn<void,address,usize> forget;
    cfn<void,address,usize,u32> publish;
};
extern "C" void borrowed_address_positions(usize fields, BorrowedPositionHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.remember) requires nonzero(host.compare)
    requires nonzero(host.forget) requires nonzero(host.publish);

enum AddressLayoutKind:u32 { End=0; Trusted=1; Parent=2; Relation=3; Unknown=4; };
struct AddressLayoutEntry { AddressLayoutKind kind; u32 fits; };
struct AddressLayoutHost {
    address context;
    cfn<AddressLayoutEntry,address> next;
    cfn<u32,address> validate;
};
extern "C" u32 address_layout_valid(AddressLayoutHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.validate);

enum AddressOriginRoot:u32 { External=0; Derived=1; Dataset=2; Stored=3; Parameter=4; Resource=5; };
enum AddressOriginQuery:u32 { Storage=0; Symbolic=1; };
enum AddressOriginSource:u32 { End=0; Position=1; Symbolic=2; Relation=3; Other=4; };
struct AddressOriginHost {
    address context;
    cfn<AddressOriginSource,address> next;
    cfn<u32,address> follow;
    cfn<u32,address> symbolic_relation;
    cfn<u32,address> possible;
};
extern "C" u32 query_address_origin(AddressOriginRoot root, AddressOriginQuery query, AddressOriginHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.follow)
    requires nonzero(host.symbolic_relation) requires nonzero(host.possible);

enum AddressOriginBindingMode:u32 { Substitute=0; Outputs=1; Call=2; };
struct AddressOriginBindingHost {
    address context;
    cfn<u32,address> parameter;
    cfn<void,address> unresolved_parameter;
    cfn<void,address> bind_borrow;
    cfn<void,address> external;
    cfn<void,address> clear_positions;
    cfn<u32,address> index;
    cfn<void,address> unresolved_index;
};
extern "C" void bind_address_origin(AddressOriginRoot root, AddressOriginBindingMode mode, AddressOriginBindingHost host) on cpu
    requires nonzero(host.parameter) requires nonzero(host.unresolved_parameter)
    requires nonzero(host.bind_borrow) requires nonzero(host.external)
    requires nonzero(host.clear_positions) requires nonzero(host.index) requires nonzero(host.unresolved_index);

enum AddressIdentityKind:u32 { Constant=0; Values=1; Storage=2; };
enum AddressIdentityResult:u32 { Original=0; Bound=1; Values=2; };
struct AddressIdentitySources { u32 present; usize count; };
struct AddressIdentityHost {
    address context;
    cfn<u32,address,usize> address_at;
    cfn<AddressIdentitySources,address,usize> sources;
    cfn<void,address,usize> append_source;
    cfn<void,address,usize> append_original;
    cfn<void,address> bind_storage;
};
extern "C" AddressIdentityResult bind_address_identity(AddressIdentityKind kind, usize count,
    AddressOriginBindingMode mode, AddressIdentityHost host) on cpu
    requires nonzero(host.address_at) requires nonzero(host.sources)
    requires nonzero(host.append_source) requires nonzero(host.append_original) requires nonzero(host.bind_storage);

struct AddressStorageHost {
    address context;
    cfn<AddressSourceKind,address> next;
    cfn<u32,address> local;
    cfn<void,address,u32> emit;
};
extern "C" void bind_external_address_storage(AddressStorageHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.local) requires nonzero(host.emit);

struct AddressJoinEntry { u32 equal; u32 storage; };
enum AddressJoinAction:u32 { Keep=0; Unknown=1; Conditional=2; Union=3; };
struct AddressJoinHost {
    address context;
    cfn<void,address,usize> reserve;
    cfn<AddressJoinEntry,address,usize> inspect;
    cfn<void,address,usize,AddressJoinAction> emit;
};
extern "C" void join_address_identities(usize width, usize left, usize right, u32 conditional, AddressJoinHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.inspect) requires nonzero(host.emit);

struct BorrowJoinHost {
    address context;
    cfn<u32,address,usize> equal;
    cfn<void,address> branch_paths;
    cfn<void,address,usize> restrict_left;
    cfn<void,address,usize> merge_right;
    cfn<void,address,usize> union_right;
};
extern "C" void join_borrow_components(usize width, u32 conditional, BorrowJoinHost host) on cpu
    requires nonzero(host.equal) requires nonzero(host.branch_paths) requires nonzero(host.restrict_left)
    requires nonzero(host.merge_right) requires nonzero(host.union_right);

enum BorrowCallPart:u32 { Inputs=0; Calls=1; Reads=2; Sources=3; };
struct BorrowCallCounts { usize inputs; usize calls; usize reads; usize sources; };
struct BorrowCallHost {
    address context;
    cfn<void,address,BorrowCallPart> collect;
    cfn<void,address> project;
    cfn<void,address,BorrowCallPart> restore;
    cfn<void,address> map_source;
};
extern "C" void map_call_borrows(BorrowCallCounts counts, BorrowCallHost host) on cpu
    requires nonzero(host.collect) requires nonzero(host.project)
    requires nonzero(host.restore) requires nonzero(host.map_source);

enum TaskBorrowProjection:u32 { Returned=0; ReturnedWithCompletion=1; Writes=2; Places=3; Initialized=4; };
enum TaskBorrowSource:u32 { Task=0; TaskOutput=1; Position=2; Storage=3; Inactive=4; Unresolved=5; Other=6; };
enum TaskBorrowAction:u32 {
    TakeReturned=0; TakeInitialized=1; ProjectCaptures=2; ProjectReturnedOwners=3;
    ProjectWrites=4; ProjectRequested=5; Keep=6; Defer=7; Omit=8;
};
struct TaskBorrowEdge {
    address source;
    address condition;
    TaskBorrowSource kind;
    TaskBorrowProjection held;
    u32 writable;
};
struct TaskBorrowHost {
    address context;
    cfn<TaskBorrowEdge,address> next;
    cfn<void,address,TaskBorrowEdge,TaskBorrowAction> emit;
    cfn<void,address> emit_pending;
};
extern "C" void project_task_borrows(usize count, TaskBorrowProjection projection, u32 pending, TaskBorrowHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.emit) requires nonzero(host.emit_pending);

enum BorrowFieldKind:u32 { Field=0; Element=1; };
enum BorrowFieldAction:u32 { Field=0; Element=1; ExpandElement=2; };
struct BorrowFieldSpan { address data; usize count; };
struct BorrowFieldHost {
    address context;
    cfn<u32,address> next;
    cfn<address,address,usize> reserve;
    cfn<void,address,BorrowFieldAction,usize> emit;
    cfn<void,address> emit_pending;
};
extern "C" void project_borrow_fields(BorrowFieldSpan fields, usize count, BorrowFieldKind kind, u32 pending, BorrowFieldHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.reserve) requires nonzero(host.emit) requires nonzero(host.emit_pending);

enum CallSourceKind:u32 {
    Data=0; Parameter=1; Position=2; Relation=3; Contents=4; Element=5;
    Projection=6; Task=7; TaskOutput=8; Keep=9; Missing=10;
};
enum CallSourceTaskPart:u32 { Captures=0; Initialized=1; Output=2; Completion=3; Guard=4; };
struct CallSourceShape { CallSourceKind kind; usize indices; usize outputs; usize completions; usize guards; };
struct CallSourceHost {
    address context;
    cfn<u32,address> lookup;
    cfn<u32,address> borrow;
    cfn<u32,address> identity;
    cfn<void,address,usize> index;
    cfn<void,address,CallSourceTaskPart,usize> task;
    cfn<void,address,CallSourceKind> finish;
};
extern "C" void bind_call_borrow_source(CallSourceShape source, CallSourceHost host) on cpu
    requires nonzero(host.lookup) requires nonzero(host.borrow) requires nonzero(host.identity)
    requires nonzero(host.index) requires nonzero(host.task) requires nonzero(host.finish);

enum OutputSourcePart:u32 { Root=0; Index=1; TaskResult=2; };
struct OutputSourceHost {
    address context;
    cfn<address,address,OutputSourcePart,usize> identity;
    cfn<void,address,OutputSourcePart,usize,address> rename;
    cfn<void,address,CallSourceKind> child;
    cfn<void,address,CallSourceTaskPart,usize> task;
};
extern "C" void bind_output_borrow_source(CallSourceShape source,address bindings,OutputSourceHost host) on cpu
    requires nonzero(host.identity) requires nonzero(host.rename) requires nonzero(host.child) requires nonzero(host.task);

struct BorrowEdgeGroup { address tree; usize stride; usize condition; };

enum BorrowEdgeMode : u32 { Union=0; Canonical=1; Normalized=2; };
struct BorrowEdgeClauses {
    cfn<address,address,address> both;
    cfn<address,address,address> either;
    cfn<address,address> canonical;
    cfn<address,address,address> normalized;
};
struct BorrowEdgeMerge {
    BorrowEdgeClauses clauses;
    cfn<i32,address,address> compare;
    cfn<void,address,address> key;
    address normal;
};

extern "C" u32 merge_borrow_edge(BorrowEdgeGroup group,address key,address when,BorrowEdgeMode mode,BorrowEdgeMerge host) on cpu
    requires nonzero(group.stride)
    requires nonzero(host.compare) requires nonzero(host.key)
    requires nonzero(host.clauses.either) requires nonzero(host.clauses.canonical)
    requires nonzero(host.clauses.normalized);
extern "C" void union_borrow_edges(BorrowEdgeGroup group,BorrowEdgeGroup source,address path,BorrowEdgeMerge host) on cpu
    requires nonzero(group.stride)
    requires nonzero(host.compare) requires nonzero(host.key) requires nonzero(host.clauses.both)
    requires nonzero(host.clauses.either) requires nonzero(host.clauses.canonical)
    requires nonzero(host.clauses.normalized);
extern "C" void restrict_borrow_edges(BorrowEdgeGroup group,address path,cfn<address,address,address> both) on cpu
    requires nonzero(both);

struct BorrowOutputShape { BorrowEdgeGroup inputs; BorrowEdgeGroup calls; BorrowEdgeGroup reads; BorrowEdgeGroup sources; };
struct BorrowOutputHost {
    address context;
    cfn<void,address,BorrowCallPart,address> collect;
    cfn<void,address> project;
    cfn<void,address,BorrowCallPart,address> restore;
    cfn<void,address,BorrowCallPart,address> rename_source;
};
extern "C" void bind_borrow_output(BorrowOutputShape shape,u32 rename_sources,BorrowOutputHost host) on cpu
    requires nonzero(host.collect) requires nonzero(host.project)
    requires nonzero(host.restore) requires nonzero(host.rename_source);

struct AddressRequirements { u32 writable; u32 initialized; u32 atomic; };
enum AddressViolation:u32 { None=0; Inactive=1; Layout=2; Atomic=3; ReadOnly=4; Uninitialized=5; };
struct AddressRequirementHost {
    address context;
    cfn<AddressSourceKind,address> next;
    cfn<u32,address> valid_layout;
    cfn<u32,address> blink_storage;
    cfn<u32,address> writable;
    cfn<u32,address> initialized;
    cfn<u32,address> possible;
    cfn<u32,address> applies;
};
extern "C" AddressViolation validate_address_requirements(AddressRequirements required, AddressRequirementHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.valid_layout) requires nonzero(host.blink_storage)
    requires nonzero(host.writable) requires nonzero(host.initialized) requires nonzero(host.possible)
    requires nonzero(host.applies);

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

enum EffectSourceKind:u32 { End=0; Position=1; External=2; Other=3; };
struct EffectSourceView { EffectSourceKind kind; u32 writable; };
struct EffectSourceHost {
    address context;
    cfn<EffectSourceView,address> next;
    cfn<u32,address> possible;
    cfn<u32,address> external;
    cfn<void,address,CallAccess,u32> record;
};
extern "C" u32 collect_memory_effect_sources(CallAccess request, EffectSourceHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.possible) requires nonzero(host.external)
    requires nonzero(host.record);

enum MemoryIrRowKind:u32 { Local=0; Data=1; };
enum MemoryIrOutput:u32 { Call=0; Pointer=1; };
struct MemoryIrRow { MemoryIrRowKind kind; CallAccess access; usize base; usize length; };
struct OptionalMemoryAccess { CallAccess access; u32 present; };
extern "C" OptionalMemoryAccess merge_external_memory_access(OptionalMemoryAccess left, OptionalMemoryAccess right) on cpu;

struct IrMemorySourceHost {
    address context;
    cfn<EffectSourceView,address> next;
    cfn<u32,address> external;
    cfn<void,address,address,CallAccess> origin;
};
extern "C" OptionalMemoryAccess collect_ir_memory_sources(address known, OptionalMemoryAccess external, CallAccess request,
    IrMemorySourceHost host) on cpu requires nonzero(host.next) requires nonzero(host.external) requires nonzero(host.origin);
extern "C" OptionalMemoryAccess collect_closed_ir_memory(address known, usize count, OptionalMemoryAccess external,
    OptionalMemoryAccess incoming, address context, cfn<void,address,address,usize> origin) on cpu requires nonzero(origin);
extern "C" void append_ir_memory_aliases(address known, address aliases, usize count, CallAccess access,
    cfn<MemoryIrRow,address,usize,CallAccess> row) on cpu requires nonzero(row);

enum MemoryAliasRoot:u32 { Other=0; Local=1; Data=2; Borrow=3; };
struct MemoryAliasOrigin { MemoryAliasRoot kind; usize base; usize length; address borrow; };
struct MemoryAliasCollector {
    address keys;
    address parent;
    address context;
    cfn<u32,address,MemoryIrRow> needed;
};
struct MemoryAliasOriginHost {
    address context;
    cfn<u32,address> possible;
    cfn<void,address,MemoryAliasCollector> borrow;
};
struct MemoryAliasBorrowHost {
    address context;
    cfn<EffectSourceKind,address> next;
    cfn<void,address,MemoryAliasCollector> position;
};
extern "C" void collect_memory_origin_aliases(MemoryAliasOrigin origin, MemoryAliasCollector collector, MemoryAliasOriginHost host) on cpu
    requires nonzero(collector.needed) requires nonzero(host.possible) requires nonzero(host.borrow);
extern "C" void collect_memory_borrow_aliases(MemoryAliasCollector collector, MemoryAliasBorrowHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.position);
extern "C" void project_memory_aliases(MemoryAliasOrigin origin, MemoryAliasOriginHost host, address context,
    cfn<u32,address,MemoryIrRow> needed, address output, cfn<void,address,MemoryIrRow> emit) on cpu
    requires nonzero(host.possible) requires nonzero(host.borrow) requires nonzero(needed) requires nonzero(emit);

struct MemoryIrLocal { usize end; usize length; };
struct MemoryIrRows { address known; usize known_count; usize locals; CallAccess external; u32 external_present; };
struct MemoryIrRowsHost {
    address context;
    cfn<MemoryIrLocal,address> local;
    cfn<void,address,MemoryIrRow> emit;
};
extern "C" u32 contains_ir_memory_record(address known, usize count, MemoryIrRow sought) on cpu;
extern "C" void materialize_ir_memory(MemoryIrRows rows, MemoryIrOutput format, address output, MemoryIrRowsHost host) on cpu
    requires nonzero(host.local) requires nonzero(host.emit);

enum MemoryIrOwner:u32 { Instruction=0; Argument=1; };
struct MemoryIrHost {
    bootstrap.ir_dataflow::Host ir;
    cfn<bootstrap.ir_dataflow::Span,address,address> call;
    cfn<void,address,address,MemoryIrOwner,usize> pointer;
    cfn<bootstrap.ir_dataflow::Span,address,u32> child;
};
extern "C" void publish_ir_memory(bootstrap.ir_dataflow::Span code, address context, MemoryIrHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.call) requires nonzero(host.pointer) requires nonzero(host.child);

enum ResourceIdentityKind:u32 { Unknown=0; Input=1; Data=2; Element=3; Projection=4; };
struct ResourceIdentityView { ResourceIdentityKind kind; address key; address parent; };
struct ResourceIdentityHost {
    cfn<ResourceIdentityView,address> ensures memory(read) inspect;
    cfn<u32,ResourceIdentityKind,address,address> ensures memory(read) equal;
};
extern "C" u32 same_position_resource(address left, address right, ResourceIdentityHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.equal);

struct HeldPosition { CallAccess access; u32 whole_dataset; };
struct RegionAccess { u8 bytes; u32 offset; u32 size; };
struct AccessConflictHost {
    address context;
    cfn<EffectSourceKind,address> next;
    cfn<u32,address> possible;
    cfn<usize,address> held_count;
    cfn<HeldPosition,address,usize> held;
    cfn<u32,address,usize> descendant;
    cfn<usize,address> region_count;
    cfn<RegionAccess,address,usize> region;
};
extern "C" u32 memory_access_conflicts(CallAccess access, AccessConflictHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.possible) requires nonzero(host.held_count)
    requires nonzero(host.held) requires nonzero(host.descendant) requires nonzero(host.region_count)
    requires nonzero(host.region);

extern "C" void push_memory_effect_record(address records, usize width, address record) on cpu
    requires nonzero(width);
extern "C" void append_memory_effect_records(address records, address source, usize width) on cpu
    requires nonzero(width);
extern "C" void drop_memory_effect_records(address records, cfn<void,address,usize> release) on cpu
    requires nonzero(release);

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
