module bootstrap.generic_instance;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.instance_domain;

// These descriptors borrow the original Ty/TypeName and sealed source storage.
struct TypeInfo {
    address value; NameBytes name; u64 arguments; address values;
    u64 value_count; u64 parameters; u32 flags;
};
struct SemanticInfo {
    address value; NameBytes name; u64 arguments; u32 flags; u32 never;
};
struct ActualArgument { TypeInfo ty; Optional<u32> count; };
struct CfnSlot { SemanticInfo ty; u32 access; };
enum Problem:u32 {
    GenericArguments=0; TypePosition=1; TypeWrapped=2; ExplicitFamily=3;
    Resource=4; MissingType=5; ConstShape=6; RepeatCount=7; MissingValue=8;
    ValueDomain=9; ViewExpected=10; TypeParameterArguments=11; InferredFamily=12;
    TypeMismatch=13; CfnType=14; CfnArity=15; CfnTypeArgument=16; CfnAccess=17;
    CfnPosition=18; ResultType=19; ResultTypeArgument=20; ArrayType=21;
    ArrayElement=22; ArrayArity=23; RecordType=24; RecordTypeArgument=25;
    DatasetCount=26; CfnValue=27; CfnValueArity=28; ResultValue=29;
    ArrayValue=30; ArrayCount=31; RecordValue=32; RecordConstConflict=33;
};
enum MatchMode:u32 { Type=0; Value=1; };
enum Need:u32 {
    Done=0; TypeChild=1; ValueChild=2; ExplicitType=3; ExplicitDataset=4;
    FamilyCurrent=5; FamilyActual=6; ExplicitValue=7; ActualType=8;
    ChildType=9; RecordName=10; Instantiate=11;
};
// Native operations only borrow original metadata or mutate the original result
// containers. World queries return through Need after this B call has ended.
struct Host {
    cfn<void,address,Problem,NameBytes,address,u32,u32> fail;
    cfn<ActualArgument,address,u64> actual;
    cfn<SemanticInfo,address,u64> explicit_argument;
    cfn<bool,address> has_type;
    cfn<bool,address,address> same_type;
    cfn<void,address,address> keep_type;
    cfn<void,address> push_type;
    cfn<void,address,u32> push_value;
    cfn<TypeInfo,address,u64> child;
    cfn<TypeInfo,address> element;
    cfn<TypeInfo,address> view;
    cfn<SemanticInfo,address> dataset_syntax;
    cfn<CfnSlot,address,u64> cfn_slot;
    cfn<bool,address,NameBytes> record_matches;
};
enum InstanceChecks:u32 { None=0; Interface=1; CAbiAndInterface=2; };
struct InstanceHost {
    cfn<void,address> implementation;
    cfn<void,address> external;
    cfn<void,address> name;
    cfn<void,address,NameBytes> type_argument;
    cfn<void,address,NameBytes,u64> value_argument;
    cfn<Optional<u32>,address,NameBytes> constant;
    cfn<void,address,u64,NameBytes> nonzero_error;
    cfn<bool,address> uses;
};
struct SpecializeCursor {
    SignatureNode function; u32 phase; u64 actual_count;
    Optional<u64> explicit_count; u32 explicit_types; u64 first_value;
    u64 index; u64 position; NameBytes name; Optional<u32> inferred;
    u32 scalar; u32 unit; SemanticInfo semantic;
    SignatureNode next; TypeInfo next_actual;
};
struct MatchCursor {
    MatchMode mode; u32 phase; SignatureNode source; u64 function_offset;
    TypeInfo actual; NameBytes name; NameBytes base; u64 index; u64 position;
    Optional<u32> inferred; u32 scalar; u32 unit; u32 known;
    SignatureNode next; TypeInfo next_actual; SemanticInfo semantic; u32 next_known;
};
extern "C" SpecializeCursor specialize_start(SignatureNode function,u64 actual_count,Optional<u64> explicit_count) on cpu;
extern "C" Need specialize_step(address state,Host host,address context) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.has_type) requires nonzero(host.push_type) requires nonzero(host.push_value);
extern "C" MatchCursor match_start(MatchMode mode,SignatureNode source,u64 function_offset,TypeInfo actual,
    u64 index,NameBytes name,Optional<u32> inferred,u32 known) on cpu;
extern "C" Need match_step(address state,Host host,address context) on cpu
    requires nonzero(host.fail) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.child) requires nonzero(host.element)
    requires nonzero(host.view) requires nonzero(host.dataset_syntax) requires nonzero(host.cfn_slot)
    requires nonzero(host.record_matches);
extern "C" InstanceChecks bind_instance(SignatureNode function,u32 tracking,u64 type_count,u64 value_count,
    InstanceHost host,address context) on cpu
    requires nonzero(host.implementation) requires nonzero(host.external)
    requires nonzero(host.name) requires nonzero(host.type_argument) requires nonzero(host.value_argument)
    requires nonzero(host.constant) requires nonzero(host.nonzero_error) requires nonzero(host.uses);
extern "C" bool is_open(StorageView type_storage,Slice types,StorageView value_storage,Slice values) on cpu;
