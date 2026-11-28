module bootstrap.function_body;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.operator_types;

enum CheckedDispatch:u32 { Constant=0; Native=1; Body=2; };
extern "C" CheckedDispatch checked_dispatch(SignatureNode declaration) on cpu;

enum CheckedProgramNeed:u32 { Program=0; Prefixes=1; Builder=2; };
struct CheckedProgramStep { u32 phase; CheckedProgramNeed need; };
extern "C" CheckedProgramStep checked_program_step(u32 phase) on cpu;
extern "C" void checked_prefixes(SignatureNode declaration,address context,
    cfn<u32,address,PrefixPredicate,Span> bind) on cpu requires nonzero(bind);

enum CheckedBodyNeed:u32 {
    Context=0; Views=1; Parameters=2; Result=3; Layout=4; ReturnInputs=5; Body=6; Finish=7;
};
struct CheckedBodyStep { u32 phase; CheckedBodyNeed need; };
struct CheckedBodyFacts { u32 application; u32 never; address result_name; u64 result_length; };
extern "C" CheckedBodyStep checked_body_step(u32 phase,CheckedBodyFacts facts) on cpu;

enum CheckedFinishNeed:u32 {
    NormalizeReturn=0; InspectWritten=1; NormalizeWritten=2; Outputs=3; Canonical=4; Publish=5;
};
struct CheckedFinishStep { u32 phase; CheckedFinishNeed need; usize index; };
extern "C" CheckedFinishStep checked_finish_step(u32 phase,usize index,u32 returned,
    usize written,u32 present) on cpu;

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
struct OwnerBinding {u32 may_own;u32 reference;};
struct OwnerScanHost {
 address context;
 cfn<OwnerName,address> next_binding;
 cfn<u32,address> original;
 cfn<OwnerBinding,address> describe;
};
enum OwnerSource:u32 {End=0;Different=1;Overlap=2;};
struct OwnerEvidenceHost {
 address context;
 cfn<u32,address> append_resource;
 cfn<u32,address> restrict;
 cfn<OwnerSource,address> next_source;
};
struct OwnerNamesHost {
 address context;
 cfn<OwnerName,address,u64> name;
 cfn<void,address,u64,u64> exchange;
 cfn<void,address,u64> take;
};
extern "C" void next_owner_candidate(OwnerScanHost host) on cpu
 requires nonzero(host.next_binding) requires nonzero(host.original) requires nonzero(host.describe);
extern "C" u32 retain_owner(OwnerEvidenceHost host) on cpu
 requires nonzero(host.append_resource) requires nonzero(host.restrict) requires nonzero(host.next_source);
extern "C" void move_owners(u64 count,OwnerNamesHost host) on cpu
 requires nonzero(host.name) requires nonzero(host.exchange) requires nonzero(host.take);

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
struct ValueFacts {u32 collision;u32 mutable;u32 readonly;u32 uniform;u32 control;u64 slots;u64 regs;};
enum ValueNeed:u32 {View=0;Collision=1;Mutability=2;ReadOnly=3;MaterializeReadOnly=4;BindReadOnly=5;Immutable=6;Slots=7;Store=8;Finish=9;};
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

struct ApplicationCaptureHost {
    address context;
    cfn<u64,address,Text> add_binding;
    cfn<void,address,Text> add_reference;
    cfn<void,address,u64> restore;
};
extern "C" void application_captures(SignatureNode statement,ApplicationCaptureHost host) on cpu
    requires nonzero(host.add_binding) requires nonzero(host.add_reference) requires nonzero(host.restore);

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

struct MatchFacts {u32 owns;u32 root;u32 borrowed;u32 continuing;u32 present;u32 kind;};
enum MatchNeed:u32 {Type=0;Root=1;RootKind=2;Parent=3;Borrowed=4;Read=5;Evaluate=6;Missing=7;InvalidType=8;Patterns=9;Arms=10;Complete=11;};
struct MatchStep {LocalCursor cursor;MatchNeed need;};
extern "C" MatchStep match_step(LocalCursor cursor,MatchFacts facts) on cpu;
enum MatchBranch:u32 {Last=0;Value=1;Owned=2;};
extern "C" MatchBranch match_branch(u64 index,u64 count,u32 kind) on cpu;
extern "C" u32 match_binding(u32 present,address data,u64 length) on cpu;
struct MatchArmFacts {u32 owned;u32 named;u32 continuing;u32 present;};
enum MatchArmNeed:u32 {Unpack=0;Payload=1;Name=2;BindValue=3;BindReturned=4;Body=5;Lookup=6;DropBinding=7;Remove=8;DropValue=9;Complete=10;};
struct MatchArmStep {LocalCursor cursor;MatchArmNeed need;};
extern "C" MatchArmStep match_arm_step(LocalCursor cursor,MatchArmFacts facts) on cpu;
struct PatternLabel {u32 qualified;u64 owner;u64 start;};
extern "C" PatternLabel pattern_label(SeekName spelling) on cpu;
extern "C" u64 pattern_find(SeekName name,u64 count,address context,cfn<SeekName,address,u64> variant) on cpu requires nonzero(variant);
struct PatternFacts {u32 eligible;u32 owner;u32 found;u32 duplicate;u32 unit;u32 binding;u32 named;u32 present;u64 arms;u64 variants;};
enum PatternNeed:u32 {InvalidType=0;Arm=1;Owner=2;Find=3;Unknown=4;Select=5;Duplicate=6;Inspect=7;MissingBinding=8;UnitBinding=9;Coverage=10;Incomplete=11;Complete=12;};
struct PatternStep {ValueCursor cursor;PatternNeed need;};
extern "C" PatternStep pattern_step(ValueCursor cursor,PatternFacts facts) on cpu;

struct TryFacts {u64 arguments;u32 continuing;u32 present;u32 kind;u32 result;u32 expected;};
enum TryNeed:u32 {Arity=0;Evaluate=1;Inspect=2;BadSource=3;Expected=4;BadFunction=5;ErrorType=6;ValueCondition=7;OwnedCondition=8;Branches=9;ValueSuccess=10;OwnedSuccess=11;Complete=12;};
struct TryStep {LocalCursor cursor;TryNeed need;};
extern "C" TryStep try_step(LocalCursor cursor,TryFacts facts) on cpu;
struct TryErrorFacts {u32 kind;u32 owns;u32 item;};
enum TryErrorNeed:u32 {Value=0;Owned=1;Expected=2;Storage=3;MoveValue=4;Build=5;ValueReturn=6;OwnedReturn=7;};
struct TryErrorStep {LocalCursor cursor;TryErrorNeed need;};
extern "C" TryErrorStep try_error_step(LocalCursor cursor,TryErrorFacts facts) on cpu;

struct ReadCursor {u32 phase;u32 path;};
struct ReadFacts {u32 source;u32 binding;u32 application;u32 owns;u32 dataset;u32 present;u32 kind;u32 available;u32 variants;u32 place;u32 continuing;};
enum ReadNeed:u32 {Inspect=0;Name=1;NameStorage=2;NameType=3;Application=4;ElementType=5;Place=6;Model=7;Projection=8;Projected=9;BadOwned=10;Available=11;BadAvailable=12;Prepare=13;Variants=14;ProjectionPlace=15;Initialize=16;Load=17;HoldOwned=18;Borrow=19;HoldData=20;Return=21;Member=22;Index=23;Evaluate=24;BadRead=25;Complete=26;};
struct ReadStep {ReadCursor cursor;ReadNeed need;};
extern "C" ReadStep read_step(ReadCursor cursor,ReadFacts facts) on cpu;
struct ReadMemberFacts {SeekName field;u32 present;u32 kind;u32 found;u32 empty;u32 owner;};
enum ReadMemberNeed:u32 {Parent=0;Find=1;MissingField=2;Unpack=3;Value=4;Length=5;BadField=6;Item=7;Retained=8;Release=9;Plain=10;Return=11;Complete=12;};
struct ReadMemberStep {LocalCursor cursor;ReadMemberNeed need;};
extern "C" ReadMemberStep read_member_step(LocalCursor cursor,ReadMemberFacts facts) on cpu;
extern "C" u64 read_field(SeekName name,u64 count,address context,cfn<SeekName,address,u64> field) on cpu requires nonzero(field);
struct ReadIndexCursor {u32 phase;u32 model;u64 index;};
struct ReadIndexFacts {u32 present;u32 kind;u32 indexed;u32 owns;u32 array;u32 owner;u32 place;u32 valid;u64 count;u64 minimum;u64 maximum;u64 components;};
enum ReadIndexNeed:u32 {Reference=0;Parent=1;Kind=2;ArrayValue=3;ViewPlace=4;Data=5;DataIndex=6;DataType=7;Plain=8;Storage=9;Materialize=10;BorrowElement=11;Return=12;BadIndex=13;ArrayType=14;BadArray=15;Owner=16;Hold=17;Expression=18;Type=19;Bounds=20;BadBounds=21;Range=22;Unpack=23;First=24;Join=25;SourcePlace=26;MaterializeArray=27;ArrayPlace=28;Component=29;Rebind=30;ModelKind=31;BorrowModel=32;Complete=33;SetPlace=34;};
struct ReadIndexStep {ReadIndexCursor cursor;ReadIndexNeed need;u64 position;};
extern "C" ReadIndexStep read_index_step(ReadIndexCursor cursor,ReadIndexFacts facts) on cpu;

struct VariantBuildFacts {u32 owned;u32 cpu;u32 application;u32 unit;u64 arguments;u32 continuing;u32 present;};
enum VariantBuildNeed:u32 {InvalidTarget=0;Payload=1;VoidArguments=2;PayloadArguments=3;Unit=4;Evaluate=5;Missing=6;Check=7;Storage=8;Build=9;Complete=10;};
struct VariantBuildStep {LocalCursor cursor;VariantBuildNeed need;};
extern "C" VariantBuildStep variant_build_step(LocalCursor cursor,VariantBuildFacts facts) on cpu;

struct ReturnExprFacts {u32 binding;u32 owns;u32 temporary;u32 present;u32 task;};
enum ReturnExprNeed:u32 {Target=0;Member=1;IndexType=2;Temporary=3;ExpressionType=4;OwnedIndex=5;Place=6;Take=7;BindType=8;Storage=9;BadTask=10;OwnedRecord=11;Record=12;Binding=13;Dataset=14;Owned=15;Selection=16;Value=17;Complete=18;};
struct ReturnExprStep {LocalCursor cursor;ReturnExprNeed need;};
extern "C" ReturnExprStep return_expr_step(LocalCursor cursor,SignatureNode source,ReturnExprFacts facts) on cpu;
struct MemberFacts {SeekName name;u32 owns;u32 present;u32 variants;u32 temporary;u32 place;u32 resource;u32 binding;u32 continuing;u32 kind;u32 found;u32 field_owns;u32 item_kind;u32 empty;u32 array;u32 view;};
enum MemberNeed:u32 {Type=0;Place=1;Take=2;Projection=3;Projected=4;Reference=5;ReferenceLength=6;BadField=7;Variants=8;BadVariant=9;Temporary=10;IsPlace=11;Resource=12;Binding=13;TextureShape=14;Evaluate=15;Find=16;MissingField=17;Unpack=18;FieldType=19;ItemKind=20;MoveData=21;MoveOwner=22;Retained=23;DropOwner=24;HoldOwner=25;Length=26;DropData=27;Value=28;Missing=29;Return=30;Complete=31;TextureLength=32;BadTexture=33;TileLength=34;TileLane=35;BadTile=36;DatasetLength=37;BasePlace=38;OrdinaryPlace=39;PlaceKind=40;ArrayLength=41;ReadView=42;BadView=43;ReadField=44;OrdinaryValue=45;OrdinaryMemberValue=46;};
struct MemberStep {LocalCursor cursor;MemberNeed need;};
extern "C" MemberStep member_step(LocalCursor cursor,MemberFacts facts) on cpu;
struct OwnedIndexFacts {u32 defined;u32 present;u32 continuing;u32 kind;u32 array;u32 indexed;u32 owns;u32 empty;};
enum OwnedIndexNeed:u32 {ExistingType=0;Place=1;TakePlace=2;Evaluate=3;Inspect=4;BadArray=5;HoldData=6;DataIndex=7;DataType=8;TakeData=9;ReadData=10;Storage=11;Release=12;Return=13;HoldOwner=14;Index=15;ArrayPlace=16;TakeElement=17;ArrayValue=18;ViewPlace=19;ReadView=20;Complete=21;};
struct OwnedIndexStep {LocalCursor cursor;OwnedIndexNeed need;};
extern "C" OwnedIndexStep owned_index_step(LocalCursor cursor,OwnedIndexFacts facts) on cpu;

// Borrow selectors from the sealed source store. Actual typed products remain
// with the one World future; a step never retains a pointer into that future.
enum SourceValueNeed:u32 {BadFunction=0;BadCooperate=1;StaticU32=2;Bool=3;Number=4;Name=5;Specialized=6;Member=7;Index=8;Select=9;Target=10;ConstructType=11;Construct=12;Returned=13;FinishReturned=14;Negative=15;Operand=16;Unary=17;Left=18;Right=19;Binary=20;Condition=21;RequireCondition=22;ShortCircuit=23;Empty=24;};
struct SourceValueStep {LocalCursor cursor;SourceValueNeed need;Text spelling;u64 first;u64 second;u64 third;Slice arguments;u32 bits;u32 target;};
extern "C" SourceValueStep source_value_step(LocalCursor cursor,SignatureNode source,u32 present) on cpu;
enum ReturnedValueNeed:u32 {Empty=0;Ordinary=1;Value=2;Bool=3;Complete=4;};
struct ReturnedValueStep {LocalCursor cursor;ReturnedValueNeed need;};
extern "C" ReturnedValueStep returned_value_step(LocalCursor cursor,u32 continuing,u32 boolean) on cpu;
enum ShortCircuitOrder:u32 {RightYes=0;RightNo=1;};
struct ShortCircuitPlan {ShortCircuitOrder order;u32 skipped;};
extern "C" ShortCircuitPlan short_circuit_plan(SeekName operation) on cpu;


enum SourcePlaceNeed:u32 {Name=0;Bind=1;External=2;Parent=3;Project=4;Index=5;Bad=6;Empty=7;};
struct SourcePlaceStep {LocalCursor cursor;SourcePlaceNeed need;Text spelling;u64 first;u64 second;Slice types;Slice arguments;};
extern "C" SourcePlaceStep source_place_step(LocalCursor cursor,SignatureNode source,u32 present) on cpu;
enum MemberPositionNeed:u32 {Project=0;BadView=1;};
extern "C" MemberPositionNeed member_position(u32 view) on cpu;
struct NamePositionFacts {u32 kind;u32 present;u32 arrays;};
enum NamePositionNeed:u32 {Atomic=0;Owned=1;ViewElement=2;InspectOwned=3;ExistingOwned=4;MaterializeOwned=5;ArrayType=6;MaterializeArray=7;Bad=8;};
struct NamePositionStep {LocalCursor cursor;NamePositionNeed need;};
extern "C" NamePositionStep name_position_step(LocalCursor cursor,NamePositionFacts facts) on cpu;
struct IndexPositionFacts {u32 present;u32 bounded;u32 current;u32 same_domain;u32 same_shape;u32 access;u32 whole;u32 appending;};
enum IndexPositionNeed:u32 {Application=0;ApplicationIndex=1;TileBinding=2;TileIndex=3;Dataset=4;View=5;Index=6;Type=7;Bounds=8;BadBounds=9;Append=10;BadAppend=11;Domain=12;Element=13;Empty=14;};
struct IndexPositionStep {LocalCursor cursor;IndexPositionNeed need;Text name;u32 writable;};
extern "C" IndexPositionStep index_position_step(LocalCursor cursor,SignatureNode source,IndexPositionFacts facts) on cpu;
struct PositionAppend {u32 present;u64 output;u64 count;u32 word;};
extern "C" u32 position_append(u64 binding,address context,cfn<PositionAppend,address> next) on cpu requires nonzero(next);
struct ViewPositionFacts {u32 placed;u32 present;u32 dataset;u32 indexed;u32 continuing;u32 kind;u32 array;};
enum ViewPositionNeed:u32 {Parent=0;DatasetType=1;IndexedType=2;Reference=3;ApplicationIndex=4;Read=5;Returned=6;Kind=7;OwnedArray=8;HoldDataset=9;HoldOwned=10;Ordinary=11;ArrayType=12;Materialize=13;ArrayPlace=14;View=15;Empty=16;};
struct ViewPositionStep {LocalCursor cursor;ViewPositionNeed need;};
extern "C" ViewPositionStep view_position_step(LocalCursor cursor,ViewPositionFacts facts) on cpu;


enum LexicalNeed:u32 {Enter=0;Statement=1;After=2;Finish=3;Release=4;Retain=5;Complete=6;Next=7;};
struct LexicalStep {Cursor cursor;LexicalNeed need;};
extern "C" LexicalStep lexical_block_step(Cursor cursor,u64 count,u32 continuing) on cpu;
struct SourceStatementFacts {u32 present;u32 entry;};
enum SourceStatementNeed:u32 {Bind=0;Declare=1;Let=2;Assign=3;Expression=4;Condition=5;RequireBool=6;Conditional=7;InitialLoop=8;While=9;For=10;LoopControl=11;Match=12;Return=13;Parallel=14;Entry=15;BadPhase=16;Seek=17;Complete=18;};
struct SourceStatementStep {LocalCursor cursor;SourceStatementNeed need;Text name;Text operation;u64 first;u64 second;Slice body;Slice other;u32 present;};
extern "C" SourceStatementStep source_statement_step(LocalCursor cursor,SignatureNode source,SourceStatementFacts facts) on cpu;


struct ReadPlaceFacts {u32 owns;u32 kind;u32 access;u32 reference;u32 memory;u32 known;u32 input;u32 current;u32 field_address;u32 indexed_base;u64 components;u64 words;u64 fields;};
enum ReadPlaceNeed:u32 {Access=0;Owns=1;Kind=2;OwnedError=3;AtomicError=4;AtomicTileError=5;AtomicDataError=6;IndexedAtomicError=7;ReferenceMemory=8;Address=9;RememberRead=10;Memory=11;Initialize=12;Zero=13;LoadReference=14;LoadView=15;LoadTile=16;LoadLocal=17;LoadData=18;Proof=19;Snapshot=20;BorrowFields=21;BorrowField=22;SetBorrow=23;ExternalProof=24;DatasetEffect=25;ApplicationRead=26;ApplicationProof=27;TileInput=28;DataAccess=29;DataCross=30;FinishApplication=31;Finish=32;ShareBorrow=33;IndexedRead=34;IndexedAccess=35;LoadIndexedLocal=36;LoadIndexed=37;};
struct ReadPlaceStep {Cursor cursor;ReadPlaceNeed need;u32 flags;};
extern "C" ReadPlaceStep read_place_step(Cursor cursor,ReadPlaceFacts facts) on cpu;
struct NameValueFacts {u32 present;u32 kind;u32 constant;};
enum NameValueNeed:u32 {Scope=0;ScopeValue=1;Binding=2;Value=3;OwnedPlace=4;NamedPlace=5;Read=6;Resolve=7;FunctionKind=8;ModuleConstant=9;Unknown=10;};
struct NameValueStep {LocalCursor cursor;NameValueNeed need;};
extern "C" NameValueStep name_value_step(LocalCursor cursor,NameValueFacts facts) on cpu;
struct SourcePlaceQueryHost {address context;cfn<u32,address,Text> ensures memory(read) binding;};
extern "C" u32 source_is_place(SignatureNode source,SourcePlaceQueryHost host) on cpu requires nonzero(host.binding);

struct WritePlaceFacts {u32 views;u32 persistent;u32 kind;u32 access;u32 writable;u32 memory;u32 pending;u32 resolved;u32 field_address;u32 unique;u32 iterator;u32 loops_empty;u32 indexed_base;u64 words;u64 values;u64 fields;};
enum WritePlaceNeed:u32 {Access=0;ViewType=1;ViewPayloadError=2;Owned=3;ViewAvailable=4;Same=5;Kind=6;NumericProof=7;ReferenceMemory=8;Pending=9;Resolved=10;Snapshot=11;AddressWrite=12;BorrowFields=13;BorrowField=14;AddField=15;Memory=16;AtomicError=17;AtomicTileError=18;AtomicDataError=19;IndexedAtomicError=20;ReadOnlyError=21;ViewError=22;TileError=23;WriteError=24;Unique=25;DuplicateError=26;Writable=27;AssignProof=28;Initialize=29;Zero=30;DatasetEffect=31;CaptureProof=32;Alias=33;ApplicationWrite=34;AssignApplicationProof=35;CompleteWrites=36;DataAccess=37;StoreReference=38;StoreLocal=39;StoreData=40;Complete=41;IndexedWritable=42;IndexedAccess=43;IndexedWrite=44;StoreIndexedLocal=45;StoreIndexed=46;};
struct WritePlaceStep {Cursor cursor;WritePlaceNeed need;};
extern "C" WritePlaceStep write_place_step(Cursor cursor,WritePlaceFacts facts) on cpu;

struct UnsafeReadFacts {u32 present;u32 access;};
enum UnsafeReadNeed:u32 {BadArguments=0;Application=1;ApplicationCpu=2;Dataset=3;BadAccess=4;Value=5;Index=6;IndexType=7;ApplicationRead=8;DatasetRead=9;ViewRead=10;Empty=11;};
struct UnsafeReadStep {LocalCursor cursor;UnsafeReadNeed need;};
extern "C" UnsafeReadStep unsafe_read_step(LocalCursor cursor,StorageView source,Slice arguments,UnsafeReadFacts facts) on cpu;
struct UnsafeViewFacts {u32 view;u32 present;};
enum UnsafeViewNeed:u32 {BadView=0;Hold=1;Index=2;Release=3;Result=4;IndexType=5;Read=6;Empty=7;};
struct UnsafeViewStep {LocalCursor cursor;UnsafeViewNeed need;};
extern "C" UnsafeViewStep unsafe_view_step(LocalCursor cursor,UnsafeViewFacts facts) on cpu;

enum MemberValueNeed:u32 {ArrayLength=0;ViewLength=1;Project=2;};
extern "C" MemberValueNeed member_value_need(u32 array,u32 view,address bytes,usize length) on cpu;

struct RayQueryFacts {u64 count;u32 any;u32 present;};
enum RayQueryNeed:u32 {BadArguments=0;Name=1;MissingName=2;Binding=3;BadSource=4;Origin=5;Minimum=6;Direction=7;Maximum=8;Empty=9;OriginType=10;MinimumType=11;DirectionType=12;MaximumType=13;Cull=14;CullType=15;AnyType=16;ClosestType=17;Registers=18;AnyResult=19;ClosestResult=20;Emit=21;Value=22;};
struct RayQueryStep {LocalCursor cursor;RayQueryNeed need;};
extern "C" RayQueryStep ray_query_step(LocalCursor cursor,RayQueryFacts facts) on cpu;

struct TextureArgumentFacts {u32 present;u32 array;u32 bounded;};
enum TextureArgumentNeed:u32 {Binding=0;Resource=1;Base=2;Shape=3;BadShape=4;Index=5;IndexType=6;Bounds=7;BadBounds=8;Select=9;Empty=10;BadArguments=11;BadSource=12;};
struct TextureArgumentStep {Cursor cursor;TextureArgumentNeed need;Text name;};
extern "C" TextureArgumentStep texture_argument_step(Cursor cursor,SignatureNode source,TextureArgumentFacts facts) on cpu;
struct TextureSampleFacts {u64 count;u32 present;u32 array;u32 texture_uniform;u32 coordinate_uniform;u32 lod_uniform;};
enum TextureSampleNeed:u32 {BadArguments=0;Texture=1;Shape=2;BadArray=3;Coordinate=4;Lod=5;Binding=6;CoordinateType=7;LodType=8;Proof=9;Registers=10;Emit=11;Value=12;Empty=13;};
struct TextureSampleStep {LocalCursor cursor;TextureSampleNeed need;u32 uniform;};
extern "C" TextureSampleStep texture_sample_step(LocalCursor cursor,TextureSampleFacts facts) on cpu;

struct AppendFacts {u32 present;u32 access;u32 views;u32 target_domain;u32 count_domain;u64 target;u64 count;u64 target_components;u64 value_components;u64 registers;};
enum AppendNeed:u32 {BadArguments=0;Binding=1;BadOutput=2;Count=3;TargetAccess=4;CountAccess=5;Initialized=6;Shape=7;CountType=8;BadContract=9;Value=10;Empty=11;ValueType=12;BadViews=13;Domains=14;BadDomain=15;Pairs=16;BadPair=17;Pair=18;Read=19;Writes=20;CountRegion=21;MemoryField=22;Cross=23;Register=24;ValueField=25;Emit=26;Complete=27;};
struct AppendStep {Cursor cursor;AppendNeed need;Text name;};
extern "C" AppendStep append_step(Cursor cursor,StorageView source,Slice arguments,AppendFacts facts) on cpu;
extern "C" u32 append_pair_conflict(u64 target,u64 count,u32 word,address context,cfn<PositionAppend,address> next) on cpu requires nonzero(next);

struct ClearPrefixFacts {u32 present;u32 target_access;u32 count_access;u32 single;u64 target;u64 count;u32 word;};
enum ClearPrefixNeed:u32 {BadArguments=0;Binding=1;BadOutput=2;Count=3;Shape=4;BadContract=5;Written=6;Relation=7;Origin=8;RegisterU32=9;Constant=10;Store=11;WriteCount=12;WriteRegion=13;StageMap=14;Output=15;};
struct ClearPrefixStep {LocalCursor cursor;ClearPrefixNeed need;Text name;usize reg;u32 end;u64 bits;u32 boundary;};
extern "C" ClearPrefixStep clear_prefix_step(LocalCursor cursor,StorageView source,Slice arguments,ClearPrefixFacts facts) on cpu;

struct OrdinalFacts {u32 binding;u32 place;u32 minimum;u32 maximum;};
enum OrdinalNeed:u32 {BadArguments=0;BadName=1;Binding=2;BadCursor=3;Application=4;Dataset=5;Tile=6;View=7;};
struct OrdinalStep {LocalCursor cursor;OrdinalNeed need;Text name;u32 minimum;u32 maximum;};
extern "C" OrdinalStep ordinal_step(LocalCursor cursor,StorageView source,Slice arguments,OrdinalFacts facts) on cpu;

struct RecordConstructFacts {u64 arguments;u64 held;u64 fields;u64 elements;u32 owns;u32 variants;u32 enumeration;u32 matrix;u32 c_function;u32 numeric;u32 array;u32 empty;u32 present;u32 borrows;u32 same;u32 uniform;u32 constant;};
enum RecordConstructNeed:u32 {Argument=0;Hold=1;Push=2;ReleaseBegin=3;Release=4;Owned=5;Variant=6;BadEnum=7;EnumValue=8;EnumType=9;EnumReturn=10;Matrix=11;Numeric=12;SameArray=13;Single=14;ArrayType=15;BadArray=16;EmptyArray=17;ArrayBegin=18;ArrayElement=19;ArrayValue=20;Record=21;Empty=22;};
struct RecordConstructStep {Cursor cursor;RecordConstructNeed need;u32 expected;};
extern "C" RecordConstructStep record_construct_step(Cursor cursor,RecordConstructFacts facts) on cpu;
struct RecordPackFacts {u64 fields;u64 values;u32 uniform;u32 constant;};
enum RecordPackNeed:u32 {BadCount=0;Field=1;Append=2;Value=3;};
struct RecordPackStep {Cursor cursor;RecordPackNeed need;};
extern "C" RecordPackStep record_pack_step(Cursor cursor,RecordPackFacts facts) on cpu;

enum DatasetProjectionUse:u32 {Reference=0;Field=1;Presence=2;};
enum DatasetProjectionKind:u32 {Missing=0;Found=1;BadField=2;BadRoot=3;};
struct DatasetProjection {DatasetProjectionKind kind;u32 offset;u64 binding;u64 fields;Span span;};
struct DatasetField {u32 present;u32 offset;u64 fields;SeekName name;};
struct DatasetProjectionHost {address context;cfn<DatasetProjection,address,Text,u32> root;cfn<DatasetField,address,u64> ensures memory(read) declared;cfn<DatasetField,address,Text> field;cfn<void,address,u64,u32> select;};
extern "C" u32 dataset_layout(u64 count,address context,cfn<u32,address,u64> part) on cpu requires nonzero(part);
extern "C" DatasetProjection dataset_projection(SignatureNode source,DatasetProjectionUse usage,DatasetProjectionHost host) on cpu requires nonzero(host.root) requires nonzero(host.declared) requires nonzero(host.field) requires nonzero(host.select);
extern "C" u32 dataset_element_words(u32 base,u64 count,u32 access,u32 requested,address context,cfn<u32,address,u64> ensures memory(read) component,cfn<void,address,u32> append) on cpu requires nonzero(component) requires nonzero(append);

struct StageRootFacts {u32 present;u32 constant;};
enum StageRootNeed:u32 {Binding=0;Bound=1;Scope=2;ScopeValue=3;Resolve=4;FunctionKind=5;Constant=6;Unknown=7;StaticU32=8;Number=9;Expression=10;};
struct StageRootStep {LocalCursor cursor;StageRootNeed need;Text spelling;u32 bits;};
extern "C" StageRootStep stage_root_step(LocalCursor cursor,SignatureNode source,StageRootFacts facts) on cpu;

struct StageControlCursor {u32 phase;u32 index;u32 last;};
struct StageControlFacts {u32 present;u32 literal;u32 same_type;u32 bits;u32 returned;};
enum StageControlNeed:u32 {Parallel=0;Expression=1;Invalid=2;Name=3;BadLetName=4;Annotation=5;LetValue=6;Bind=7;Condition=8;BadCondition=9;BadBoolean=10;ConditionValue=11;IfBody=12;IfOrigin=13;First=14;FirstValue=15;Last=16;LastValue=17;BadCounterName=18;BadBound=19;BeginFor=20;Iteration=21;IterationOrigin=22;ForOrigin=23;Boundary=24;Return=25;Complete=26;};
struct StageControlStep {StageControlCursor cursor;StageControlNeed need;Text name;u64 expression;Slice body;};
extern "C" StageControlStep stage_control_step(StageControlCursor state,SignatureNode source,StageControlFacts facts) on cpu;

struct EnvironmentCursor {u64 index;u64 part;u32 phase;};
struct RootEnvironmentFacts {u64 roots;u64 components;u32 kind;};
enum RootEnvironmentNeed:u32 {Entry=0;Texture=1;Acceleration=2;Data=3;Literal=4;BeginUniform=5;UniformComponent=6;BindUniform=7;Complete=8;};
struct RootEnvironmentStep {EnvironmentCursor cursor;RootEnvironmentNeed need;};
extern "C" RootEnvironmentStep root_environment_step(EnvironmentCursor state,RootEnvironmentFacts facts) on cpu;
struct ParallelPrepareFacts {u32 present;u32 access;u32 initialized;u32 cpu;u32 known_domain;u32 domain_count;u32 known_selection;u32 selection_count;};
enum ParallelPrepareNeed:u32 {Root=0;BadRoot=1;Prefix=2;Selection=3;Initialized=4;BadSelection=5;Counts=6;BadCapacity=7;Builder=8;Frame=9;Context=10;Roots=11;Index=12;Refine=13;Name=14;BadName=15;Initialize=16;Complete=17;};
struct ParallelPrepareStep {LocalCursor cursor;ParallelPrepareNeed need;Text spelling;u64 expression;};
extern "C" ParallelPrepareStep parallel_prepare_step(LocalCursor state,SignatureNode source,ParallelPrepareFacts facts) on cpu;
struct ParallelInitializeFacts {u64 views;u64 components;u64 binding;u64 domain;u32 access;u32 present;u32 domain_access;};
enum ParallelInitializeNeed:u32 {Element=0;View=1;Component=2;Initialized=3;Mark=4;BindElement=5;Reset=6;AliasView=7;SameLength=8;Alias=9;Complete=10;};
struct ParallelInitializeStep {EnvironmentCursor cursor;ParallelInitializeNeed need;u32 flags;};
extern "C" ParallelInitializeStep parallel_initialize_step(EnvironmentCursor state,ParallelInitializeFacts facts) on cpu;

struct ParallelOutputFacts {u64 items;u64 components;u64 regions;u64 binding;u64 expected_index;u64 source_binding;u64 source_index;u64 append_output;u64 append_count;u32 access;u32 prefix;u32 selection;u32 present;u32 same_type;u32 offset;u32 bytes;u32 start;u32 end;u32 stored;};
enum ParallelOutputNeed:u32 {Old=0;Atomic=1;Component=2;Region=3;Join=4;Forget=5;Assign=6;Appends=7;Append=8;Elements=9;Element=10;Type=11;Proof=12;Complete=13;};
struct ParallelOutputStep {EnvironmentCursor cursor;ParallelOutputNeed need;};
extern "C" ParallelOutputStep parallel_output_step(EnvironmentCursor state,ParallelOutputFacts facts) on cpu;
struct ParallelEffectsFacts {u64 writes;u64 regions;u64 guarantees;u32 output;};
enum ParallelEffectsNeed:u32 {Binding=0;Output=1;Written=2;Elements=3;Grant=4;Complete=5;};
struct ParallelEffectsStep {EnvironmentCursor cursor;ParallelEffectsNeed need;};
extern "C" ParallelEffectsStep parallel_effects_step(EnvironmentCursor state,ParallelEffectsFacts facts) on cpu;
struct WrittenRegion {u32 start;u32 end;};
extern "C" u32 parallel_written_overlap(u32 offset,u32 bytes,u64 count,address context,cfn<WrittenRegion,address> next) on cpu requires nonzero(next);

struct StageKnownOutput {u64 binding;u64 domain;u64 words;u32 has_domain;};
extern "C" u32 stage_initialized(u32 known,u32 count,u64 binding,u32 word,u64 outputs,address context,cfn<StageKnownOutput,address,u64> output,cfn<u32,address,u64,u64> field,cfn<u32,address,u64,u64> length) on cpu requires nonzero(output) requires nonzero(field) requires nonzero(length);
struct PromoteStageFacts {u64 roots;u64 components;u32 access;u32 initialized;};
enum PromoteStageNeed:u32 {Root=0;Component=1;Promote=2;Complete=3;};
struct PromoteStageStep {EnvironmentCursor cursor;PromoteStageNeed need;};
extern "C" PromoteStageStep promote_stage_step(EnvironmentCursor state,PromoteStageFacts facts) on cpu;
struct StageWriteInitFacts {u64 writes;u64 appends;u64 components;u64 binding;u64 append_output;u64 append_count;u64 selected;u32 access;u32 prefix;u32 selection;u32 local;u32 known;};
enum StageWriteInitNeed:u32 {Binding=0;Append=1;Component=2;Known=3;Word=4;Emit=5;Complete=6;};
struct StageWriteInitStep {EnvironmentCursor cursor;StageWriteInitNeed need;};
extern "C" StageWriteInitStep stage_write_init_step(EnvironmentCursor state,StageWriteInitFacts facts) on cpu;
struct CollectiveInitFacts {u64 stages;u64 components;u64 target;u64 source;u64 count;u32 kind;u32 prefix;u32 target_word;u32 count_word;u32 offset;};
enum CollectiveInitNeed:u32 {Stage=0;Descriptor=1;Component=2;Word=3;Emit=4;Complete=5;};
struct CollectiveInitStep {EnvironmentCursor cursor;CollectiveInitNeed need;u32 flags;u64 binding;u64 domain;u32 word;};
extern "C" CollectiveInitStep collective_init_step(EnvironmentCursor state,CollectiveInitFacts facts) on cpu;

struct ParallelBuildFacts {u32 prefix;u32 selection;u32 tiles;u32 frame;u32 used;u32 scope;u32 has_native;u32 native_frame;u32 uniform_bytes;};
enum ParallelBuildNeed:u32 {Prefix=0;SetPrefix=1;TileInputs=2;Body=3;Access=4;Appends=5;Initialize=6;Tiles=7;BadSparseTile=8;Selection=9;Frame=10;FrameUse=11;BindFrame=12;Scope=13;Seal=14;BindScope=15;CommitFrame=16;Facts=17;Publish=18;Complete=19;};
struct ParallelBuildStep {LocalCursor cursor;ParallelBuildNeed need;u32 offset;};
extern "C" ParallelBuildStep parallel_build_step(LocalCursor state,ParallelBuildFacts facts) on cpu;

enum ParallelAccessIssue:u32 {Clear=0;AppendRead=1;CrossWrite=2;};
struct ParallelAccessResult {ParallelAccessIssue issue;usize binding;};
struct ParallelAccessBinding {usize binding;u32 present;};
struct ParallelAccessAppend {usize output;usize counter;u32 present;};
struct ParallelAccessRange {u32 start;u32 end;u32 cross;u32 present;};
struct ParallelAccessHost {
 cfn<ParallelAccessAppend,address> next_append;
 cfn<u32,address,usize> has_reads;
 cfn<ParallelAccessBinding,address> next_binding;
 cfn<ParallelAccessRange,address> next_region_read;
 cfn<ParallelAccessRange,address> next_region_write;
};
extern "C" ParallelAccessResult parallel_access(address context,ParallelAccessHost host) on cpu
 requires nonzero(host.next_append) requires nonzero(host.has_reads) requires nonzero(host.next_binding)
 requires nonzero(host.next_region_read) requires nonzero(host.next_region_write);
enum ParallelRelationsNeed:u32 {Relation=0;Prove=1;Complete=2;};
struct ParallelRelationsStep {EnvironmentCursor cursor;ParallelRelationsNeed need;};
extern "C" ParallelRelationsStep parallel_relations_step(EnvironmentCursor state,u64 relations) on cpu;

struct StageSequenceFacts {u32 present;u32 returned;};
enum StageSequenceNeed:u32 {Next=0;Begin=1;Dispatch=2;Outputs=3;Complete=4;};
struct StageSequenceStep {LocalCursor cursor;StageSequenceNeed need;};
extern "C" StageSequenceStep stage_sequence_step(LocalCursor state,StageSequenceFacts facts) on cpu;
struct StageExpressionCursor {u32 phase;u32 custom;u32 flags;};
struct StageExpressionFacts {u32 missing;u32 unique;};
enum StageExpressionNeed:u32 {Resolve=0;BeginSegment=1;Clear=2;Sort=3;Compact=4;Reduce=5;SegmentedReduce=6;Scan=7;SegmentedScan=8;CustomReduce=9;CustomScan=10;CustomSegmentedScan=11;CustomSegmentedReduce=12;CallPrepare=13;CallExecute=14;ResetBoundary=15;ResetFirst=16;Invalid=17;Unknown=18;Complete=19;};
struct StageExpressionStep {StageExpressionCursor cursor;StageExpressionNeed need;Text name;Slice arguments;Slice types;CollectiveOperator operation;};
extern "C" StageExpressionStep stage_expression_step(StageExpressionCursor state,SignatureNode source,StageExpressionFacts facts) on cpu;

enum ValueKernelError:u32 {Valid=0;Missing=1;Reference=2;};
struct ValueKernelSource {ValueKernelError error;u32 explicit;u64 reference;Text name;Slice captures;Slice types;};
struct ValueKernelFacts {u32 unique;u32 valid;u64 parameters;u64 captures;u32 explicit;u32 uniform;u32 views;u32 reads;u32 writes;};
enum ValueKernelNeed:u32 {Resolve=0;Unknown=1;Signature=2;InvalidSignature=3;Begin=4;Parameter=5;BeginCaptures=6;Capture=7;InvalidCapture=8;PushCapture=9;EndCaptures=10;Explicit=11;Specialize=12;Origin=13;Invoke=14;Ordinary=15;Value=16;Same=17;Publish=18;};
struct ValueKernelStep {ValueCursor cursor;ValueKernelNeed need;u64 index;};
extern "C" ValueKernelSource value_kernel_source(SignatureNode source) on cpu;
extern "C" u32 value_kernel_signature(SignatureNode declaration,u32 invocation,u64 parameters,u64 captures) on cpu;
extern "C" ValueKernelStep value_kernel_step(ValueCursor state,ValueKernelFacts facts) on cpu;

struct CollectiveFieldSyntax {u32 member;u32 named;Text name;};
struct CollectiveProjectionHost {cfn<u32,address,u64> root;cfn<u32,address,Text> field;};
enum CollectivePrefixKind:u32 {Plain=0;Arity=1;Prefix=2;WrongName=3;};
struct CollectivePrefixInput {CollectivePrefixKind kind;u64 root;u64 count;Text name;};
extern "C" CollectiveFieldSyntax collective_field_syntax(SignatureNode source) on cpu;
extern "C" u32 collective_project(SignatureNode source,address context,CollectiveProjectionHost host) on cpu requires nonzero(host.root) requires nonzero(host.field);
extern "C" CollectivePrefixInput collective_prefix_input(SignatureNode source) on cpu;
extern "C" u32 collective_read_access(u32 access,u64 components,address context,cfn<u32,address,u64> initialized) on cpu requires nonzero(initialized);


enum ValueKind:u32 {Reduce=0;Scan=1;SegmentedScan=2;SegmentedReduce=3;};
enum NumericNeed:u32 {Source=0;Target=1;Head=2;Count=3;Types=4;Access=5;TargetShape=6;CountShape=7;Capacity=8;Fields=9;CountWrite=10;GrantInput=11;GrantOutput=12;Reads=13;Publish=14;Error=15;};
enum NumericError:u32 {ReduceArity=0;ScanArity=1;SegmentedArity=2;ReduceType=3;ScanType=4;SegmentedType=5;HeadAccess=6;ReduceAccess=7;ScanAccess=8;SegmentedAccess=9;ReduceShape=10;CountShape=11;ScanCapacity=12;SegmentedCapacity=13;};
enum ValueOperation:u32 {Reduce=0;Scan=1;SegmentedReduce=2;};
struct NumericFacts {
 u64 arguments;u64 source;u64 target;u64 count;u64 prefix_count;u64 heads;u64 head_count;
 u32 has_prefix;u32 has_head_prefix;u32 prefix_word;u32 head_count_word;
 u32 source_access;u32 target_access;u32 head_access;u32 count_access;
 u32 numeric;u32 integer;u32 same;u32 source_overlap;u32 head_overlap;
 u32 source_known;u32 source_size;u32 target_known;u32 target_size;u32 count_known;u32 count_size;
};
struct NumericStep {LocalCursor cursor;NumericNeed need;NumericError error;ValueOperation operation;u64 argument;};
struct CollectiveComponent {u32 offset;u32 size;};
struct CollectiveReads {u64 first;u64 second;u64 third;u64 count;};
extern "C" NumericStep numeric_step(ValueKind kind,CollectiveOperator operation,LocalCursor cursor,NumericFacts facts) on cpu;
extern "C" void collective_write_fields(address type,address context,u32 base,u64 count,cfn<CollectiveComponent,address,u64> component,cfn<void,address,u32,u32> write) on cpu requires nonzero(component) requires nonzero(write);
extern "C" void collective_write_count(address context,u32 word,cfn<void,address,u32,u32> write) on cpu requires nonzero(write);
extern "C" CollectiveReads collective_reads(u64 source,u32 second_present,u64 second,u32 third_present,u64 third) on cpu;

enum RecordKind:u32 {Compact=0;Sort=1;};
enum RecordNeed:u32 {Source=0;Predicate=1;Target=2;Count=3;Views=4;FlagInitialized=5;Types=6;CountShape=7;PredicateShape=8;Capacity=9;Key=10;RootRead=11;SourceFacts=12;Elements=13;JoinElements=14;Guarantees=15;Fields=16;CountWrite=17;GrantInput=18;GrantOutput=19;Reads=20;Publish=21;Error=22;KeyComponents=23;SameLength=24;};
enum RecordError:u32 {CompactArity=0;SortArity=1;Descending=2;CompactViews=3;SortViews=4;CompactAccess=5;KeyAccess=6;KeyType=7;SortAccess=8;CompactType=9;SortType=10;CountShape=11;PredicateShape=12;CompactCapacity=13;SortCapacity=14;};
struct RecordCursor {u64 index;u32 phase;};
struct RecordFacts {
 u64 source;u64 predicate;u64 target;u64 count;u64 prefix_count;u64 key_binding;
 u32 has_prefix;u32 source_access;u32 predicate_access;u32 target_access;u32 count_access;
 u32 views;u32 initialized;u32 same_type;u32 count_known;u32 count_size;
 u32 source_known;u32 source_size;u32 predicate_known;u32 predicate_size;u32 target_known;u32 target_size;
 u32 key_access;u32 key_same_prefix;u32 key_numeric;u32 root_read;u32 source_proof;u32 same_length;
};
struct RecordStep {RecordCursor cursor;RecordNeed need;RecordError error;u64 argument;u64 expression;u32 order;u32 set_source;};
struct RecordComponent {u32 scalar;u32 offset;u32 size;};
extern "C" RecordStep record_step(RecordKind kind,RecordCursor cursor,StorageView source,Slice arguments,RecordFacts facts) on cpu;
extern "C" void record_sort_components(address type,address keys,u32 word,u32 order,u64 count,cfn<RecordComponent,address,u64> component,cfn<void,address,u32,u32,u32> push) on cpu requires nonzero(component) requires nonzero(push);

enum CustomNeed:u32 {Source=0;Head=1;Target=2;Types=3;Access=4;Count=5;CountShape=6;TargetShape=7;Capacity=8;Project=9;Identity=10;Combine=11;Fields=12;CountWrite=13;GrantInput=14;GrantOutput=15;Reads=16;Publish=17;Error=18;};
enum CustomError:u32 {Arity=0;Head=1;Type=2;Access=3;CountAccess=4;CountShape=5;Capacity=6;TargetShape=7;};
struct CustomFacts {
 u64 arguments;u64 source;u64 target;u64 heads;u64 count;u64 prefix_count;u64 head_count;
 u32 has_prefix;u32 has_head_prefix;u32 prefix_word;u32 head_count_word;
 u32 source_access;u32 target_access;u32 head_access;u32 count_access;
 u32 same;u32 source_views;u32 target_views;u32 source_overlap;u32 head_overlap;
 u32 source_known;u32 source_size;u32 target_known;u32 target_size;u32 count_known;u32 count_size;
};
struct CustomStep {LocalCursor cursor;CustomNeed need;CustomError error;ValueOperation operation;u64 argument;};
extern "C" CustomStep custom_collection_step(ValueKind kind,u32 map,LocalCursor cursor,CustomFacts facts) on cpu;

struct TileCursor {u64 index;u32 phase;};
struct TileInputFacts {u32 name_exists;u32 access;u32 views;u32 nonempty;u32 source_known;u32 source_size;u32 domain_known;u32 domain_size;u32 found;};
enum TileInputNeed:u32 {Name=0;Width=1;Field=2;Capacity=3;ElementDomain=4;Find=5;Push=6;Bind=7;Complete=8;BadSyntax=9;Duplicate=10;BadType=11;BadCapacity=12;};
struct TileInputStep {TileCursor cursor;TileInputNeed need;Text name;u64 field;u64 width;};
struct TileCandidate {u64 binding;u32 word;u32 input;};
extern "C" TileInputStep tile_input_step(TileCursor cursor,StorageView source,Slice body,TileInputFacts facts) on cpu;
extern "C" u64 tile_find_input(u64 count,u64 binding,u32 word,address context,cfn<TileCandidate,address,u64> candidate,cfn<u32,address,u64> same_type) on cpu requires nonzero(candidate) requires nonzero(same_type);
struct TileRange {u32 present;u32 fixed;u32 width;};
struct TileWidthFacts {TileRange first;u32 constant;u32 scalar;u32 width;};
enum TileWidthNeed:u32 {Read=0;Complete=1;ConstantError=2;WidthError=3;};
struct TileWidthStep {LocalCursor cursor;TileWidthNeed need;};
extern "C" TileWidthStep tile_width_step(LocalCursor cursor,TileWidthFacts facts) on cpu;
struct TilePublishFacts {u64 arguments;u32 annotated;u32 root;u32 cooperative;u32 has_tile;u32 present;u32 views;u32 nonempty;u32 atomic;};
enum TilePublishNeed:u32 {FixedWidth=0;InheritedWidth=1;Value=2;Publish=3;Bind=4;Complete=5;BadRoot=6;BadFixed=7;MissingWidth=8;BadValue=9;};
struct TilePublishStep {LocalCursor cursor;TilePublishNeed need;u32 access;};
extern "C" TilePublishStep tile_publish_step(LocalCursor cursor,TilePublishFacts facts) on cpu;
struct TileCollectiveFacts {u64 arguments;u32 root;u32 atomic;u32 input;u32 numeric;u32 integer;u32 sum;u32 proof;};
enum TileCollectiveNeed:u32 {View=0;Read=1;Type=2;CommonProof=3;SumProof=4;Publish=5;BadRoot=6;BadAtomic=7;BadType=8;};
struct TileCollectiveStep {LocalCursor cursor;TileCollectiveNeed need;};
extern "C" TileCollectiveStep tile_collective_step(CollectiveOperator operation,LocalCursor cursor,TileCollectiveFacts facts) on cpu;
struct TileProjectionHost {cfn<u32,address,u64,Text> root;cfn<u32,address,u64,Text> member;cfn<u32,address,u64> invalid;};
extern "C" u32 tile_project(SignatureNode source,address context,TileProjectionHost host) on cpu requires nonzero(host.root) requires nonzero(host.member) requires nonzero(host.invalid);
extern "C" void tile_words(u64 count,address context,cfn<u32,address,u64> get,cfn<void,address,u32> push) on cpu requires nonzero(get) requires nonzero(push);
struct TilePair {u32 word;u64 reg;};
extern "C" void tile_publication(u64 count,address context,cfn<TilePair,address,u64> get,cfn<void,address,TilePair> push) on cpu requires nonzero(get) requires nonzero(push);
struct TileComponent {u32 scalar;u32 word;};
struct TileResultHost {cfn<TileComponent,address,u64> get;cfn<u64,address,u32> allocate;cfn<void,address,TilePair> push;};
extern "C" void tile_results(u64 count,address context,TileResultHost host) on cpu requires nonzero(host.get) requires nonzero(host.allocate) requires nonzero(host.push);
extern "C" u32 tile_root(u32 function,u32 cooperative,u64 root,u64 depth) on cpu;
extern "C" u32 tile_barrier_allowed(u64 arguments,u32 root,u64 tiles) on cpu;
struct TileLowerFacts {u32 present;u32 actual;u32 fixed;};
enum TileLowerNeed:u32 {Next=0;Publish=1;Complete=2;Invalid=3;};
struct TileLowerStep {LocalCursor cursor;TileLowerNeed need;};
extern "C" TileLowerStep tile_lower_step(LocalCursor cursor,TileLowerFacts facts) on cpu;
extern "C" u32 tile_width_bound(TileRange range) on cpu;
struct TileLanePlan {u32 maximum;u32 mark;u32 width;};
extern "C" TileLanePlan tile_lane_plan(TileRange range) on cpu;
extern "C" u32 tile_index_minimum(u32 width,u32 count) on cpu requires nonzero(width);
struct TilePlaceFacts {u32 present;u32 bounded;u32 fixed;u32 prefix;u32 known;};
enum TilePlaceNeed:u32 {Type=0;Range=1;Count=2;Minimum=3;Bound=4;Publish=5;Complete=6;Invalid=7;};
struct TilePlaceStep {LocalCursor cursor;TilePlaceNeed need;};
extern "C" TilePlaceStep tile_place_step(LocalCursor cursor,TilePlaceFacts facts) on cpu;

// Original call/return fields borrowed only for this function's completion.
struct RecursiveModelCall {SeekName function;usize memory;address arguments;};
struct RecursiveNativeCall {u32 defined;SeekName function;u32 memory_present;usize memory;address regs;usize count;};
struct RecursiveValue {address node;usize count;};
struct RecursiveForward {usize component;address arguments;};
struct RecursiveDomain {address original;u32 kind;address origin;u32 origin_kind;};
struct RecursiveHost {
 cfn<RecursiveModelCall,address,usize> model;
 cfn<void,address,usize,address> remember_argument;
 cfn<void,address> calls;
 cfn<address,address,usize> arguments;
 cfn<void,address,usize,usize,address> remember_forward;
 cfn<RecursiveValue,address,usize> exit;
 cfn<usize,address,usize> reg;
 cfn<RecursiveForward,address,usize> forwarded;
 cfn<void,address,address,usize> domains;
 cfn<u32,address,usize,address> contains_domain;
 cfn<address,address,address,usize> origin;
 cfn<u32,address,address> same_origin;
 cfn<void,address,usize,address> inherit;
 cfn<void,address> apply;
 cfn<address,address,usize> inherited;
 cfn<void,address,usize,address> mark_domains;
};
extern "C" void recursive_ranges(address context,SeekName function,usize models,usize exits,RecursiveHost host) on cpu
 requires nonzero(host.model) requires nonzero(host.remember_argument) requires nonzero(host.calls)
 requires nonzero(host.exit) requires nonzero(host.reg) requires nonzero(host.forwarded)
 requires nonzero(host.domains) requires nonzero(host.apply);
extern "C" void recursive_call(address context,SeekName function,RecursiveNativeCall call,RecursiveHost host) on cpu
 requires nonzero(host.arguments) requires nonzero(host.remember_forward);
extern "C" void recursive_domain(address context,usize component,RecursiveDomain domain,usize exits,RecursiveHost host) on cpu
 requires nonzero(host.exit) requires nonzero(host.reg) requires nonzero(host.forwarded)
 requires nonzero(host.contains_domain) requires nonzero(host.origin) requires nonzero(host.same_origin) requires nonzero(host.inherit);
extern "C" void recursive_apply(address context,RecursiveValue value,RecursiveHost host) on cpu
 requires nonzero(host.reg) requires nonzero(host.inherited) requires nonzero(host.mark_domains);

struct LoopResource {address children;usize count;u32 variant;u32 owned;};
struct LoopPayload {u32 present;LoopResource resource;};
struct LoopProgressBinding {u32 present;usize components;LoopResource entry;LoopResource current;};
struct LoopFactPair {u32 entry;u32 current;};
struct LoopProgressHost {
    cfn<LoopProgressBinding,address> next;
    cfn<LoopFactPair,address,usize> boolean_at;
    cfn<LoopPayload,address,address,usize> payload;
};
extern "C" u32 initial_loop_progress(address context, LoopProgressHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.boolean_at) requires nonzero(host.payload);

struct LoopTerminalHost {
    cfn<u32,address,usize> bounded;
    cfn<void,address,usize,u32> project;
};
extern "C" void loop_terminal_paths(address context,usize edges,LoopTerminalHost host) on cpu
    requires nonzero(host.bounded) requires nonzero(host.project);

enum LoopElementEdge:u32 { Repeated=0; Broken=1; };
enum LoopElementExit:u32 { Skip=0; Partial=1; Natural=2; };
struct LoopElementHost {
    cfn<void,address> seed_partial;
    cfn<void,address,LoopElementEdge> join_partial;
    cfn<void,address> finish_partial;
    cfn<u32,address,usize> add_natural;
    cfn<void,address,u32> finish_natural;
};
extern "C" LoopElementExit loop_elements(address context,usize repeated,usize broken,u32 complete,u32 direct,LoopElementHost host) on cpu
    requires nonzero(host.seed_partial) requires nonzero(host.join_partial) requires nonzero(host.finish_partial)
    requires nonzero(host.add_natural) requires nonzero(host.finish_natural);

struct NaturalDatasetHost {
    cfn<void,address,usize> terminal;
    cfn<void,address,usize> repeated;
    cfn<void,address> publish_terminal;
    cfn<void,address,u32> publish_repeated;
};
extern "C" void loop_natural_dataset(address context,usize edges,u32 written,u32 terminal,NaturalDatasetHost host) on cpu
    requires nonzero(host.terminal) requires nonzero(host.repeated)
    requires nonzero(host.publish_terminal) requires nonzero(host.publish_repeated);
struct NaturalScalarHost {
    cfn<u32,address,usize> header_identity;
    cfn<u32,address,usize> header_write;
    cfn<u32,address,usize,usize> repeated;
    cfn<void,address,usize> publish;
};
extern "C" void loop_natural_scalar(address context,usize components,usize edges,NaturalScalarHost host) on cpu
    requires nonzero(host.header_identity) requires nonzero(host.header_write)
    requires nonzero(host.repeated) requires nonzero(host.publish);

enum LoopExitGuard:u32 { Body=0; Terminal=1; };
struct LoopExitHost {
    cfn<u32,address> terminal_reachable;
    cfn<usize,address,LoopExitGuard,u32> guards;
    cfn<void,address> initialize;
    cfn<void,address,u32> adopt;
    cfn<void,address,usize> merge;
    cfn<void,address,u32> publish;
};
extern "C" u32 loop_exit_join(address context,usize exits,u32 natural,u32 terminal,LoopExitHost host) on cpu
    requires nonzero(host.terminal_reachable) requires nonzero(host.guards) requires nonzero(host.initialize)
    requires nonzero(host.adopt) requires nonzero(host.merge) requires nonzero(host.publish);

struct LoopEntryDatasetHost {
    cfn<void,address,usize> independent;
    cfn<void,address,usize,u32> root;
    cfn<void,address> elements;
};
extern "C" void loop_entry_dataset(address context,usize index,u32 independent,u32 zero_valid,LoopEntryDatasetHost host) on cpu
    requires nonzero(host.independent) requires nonzero(host.root) requires nonzero(host.elements);

enum LoopBackedgeKind:u32 { End=0; Other=1; Aggregate=2; Dataset=3; };
enum LoopBackedgeOwnership:u32 { Owned=0; Moved=1; Other=2; };
struct LoopBackedgeFacts { LoopBackedgeKind kind; LoopBackedgeOwnership entry; u32 same; };
enum LoopBackedgeNeed:u32 { Done=0; Compare=1; AggregateOwnership=2; DatasetOwnership=3; };
extern "C" LoopBackedgeNeed loop_backedge_next(address context,cfn<LoopBackedgeFacts,address> next) on cpu
    requires nonzero(next);

struct LoopSourceHost {
    cfn<void,address,usize> seed;
    cfn<void,address,usize,usize> add_edge;
    cfn<void,address,usize> classify;
    cfn<void,address> seal;
    cfn<void,address,usize> publish;
    cfn<void,address,usize,usize> write_value;
    cfn<void,address,usize> write_resources;
};
extern "C" void close_loop_sources(address context,usize inputs,usize edges,usize exits,LoopSourceHost host) on cpu
    requires nonzero(host.seed) requires nonzero(host.add_edge) requires nonzero(host.classify)
    requires nonzero(host.seal) requires nonzero(host.publish)
    requires nonzero(host.write_value) requires nonzero(host.write_resources);
