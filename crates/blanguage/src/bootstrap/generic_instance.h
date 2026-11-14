module bootstrap.generic_instance;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.instance_domain;

// The original native Ty and semantic TypeName remain borrowed. Source syntax
// is read directly from its one sealed StorageView.
struct TypeInfo {
    address value;
    NameBytes name;
    u64 arguments;
    address values;
    u64 value_count;
    u64 parameters;
    u32 flags; // dataset=1, view=2, array=4, result=8, cfn=16
};
struct SemanticInfo {
    address value;
    NameBytes name;
    u64 arguments;
    u32 flags; // dataset=1, view=2, tile=4, count=8, contracts=16, canonical=32
    u32 never;
};
struct ActualArgument { TypeInfo ty; Optional<u32> count; };
struct CfnSlot { SemanticInfo ty; u32 access; };
struct QueryBool { u32 ready; u32 value; };
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
struct Host {
    cfn<void,address,Problem,NameBytes,address,u32,u32> fail;
    cfn<ActualArgument,address,u64> actual;
    cfn<SemanticInfo,address,u64> explicit_argument;
    cfn<bool,address,u64,u32> explicit_type;
    cfn<bool,address,u64> explicit_dataset;
    cfn<QueryBool,address,u64> family_current;
    cfn<QueryBool,address,u64,address> family_actual;
    cfn<bool,address> has_type;
    cfn<bool,address,address> same_type;
    cfn<void,address,address> keep_type;
    cfn<void,address> push_type;
    cfn<Optional<u32>,address,u64> explicit_value;
    cfn<void,address,u32> push_value;
    cfn<bool,address> finish;
    cfn<TypeInfo,address,u64> child;
    cfn<TypeInfo,address> element;
    cfn<TypeInfo,address> view;
    cfn<SemanticInfo,address> dataset_syntax;
    cfn<CfnSlot,address,u64> cfn_slot;
    // These native operations own a real World query result while a fixed B
    // continuation consumes it; they never register a mirror or retained handle.
    cfn<bool,address,SignatureNode,SemanticInfo,u64,NameBytes> type_dataset;
    cfn<bool,address,SignatureNode,SemanticInfo,u64,NameBytes,u32> type_cfn;
    cfn<bool,address,SignatureNode,SemanticInfo,NameBytes,inout<Optional<u32>>> value_dataset;
    cfn<bool,address,SignatureNode,SemanticInfo,NameBytes,inout<Optional<u32>>> value_cfn;
    cfn<bool,address,SignatureNode,TypeInfo,u64,NameBytes> record_type;
    cfn<bool,address,SignatureNode,TypeInfo,NameBytes,inout<Optional<u32>>> record_value;
    cfn<bool,address,NameBytes> record_matches;
};
struct InstanceHost {
    cfn<void,address> implementation;
    cfn<void,address> external;
    cfn<void,address> plain;
    cfn<void,address> name;
    cfn<void,address,NameBytes> type_argument;
    cfn<void,address,NameBytes,u64> value_argument;
    cfn<Optional<u32>,address,NameBytes> constant;
    cfn<void,address,u64,NameBytes> nonzero_error;
    cfn<bool,address> uses;
    cfn<void,address> construct;
    cfn<bool,address> c_abi;
    cfn<bool,address> interface;
};

extern "C" bool specialize(SignatureNode function, u64 actual_count, Optional<u64> explicit_count, Host host, address context) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches);

extern "C" bool type_mentioned(Host host, address context, SignatureNode formal, TypeInfo actual, u64 index, NameBytes name) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches);

extern "C" bool type_after_dataset(Host host, address context, SignatureNode formal, TypeInfo actual, u64 index, NameBytes name) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches);

extern "C" bool type_record(Host host, address context, SignatureNode formal, TypeInfo actual, u64 index, NameBytes name, NameBytes base) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches);

extern "C" bool infer_value(Host host, address context, SignatureNode formal, TypeInfo actual, NameBytes name, inout Optional<u32> inferred) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches);

extern "C" bool value_after_dataset(Host host, address context, SignatureNode formal, TypeInfo actual, NameBytes name, inout Optional<u32> inferred) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches);

extern "C" bool value_record(Host host, address context, SignatureNode formal, TypeInfo actual, NameBytes name, NameBytes base, inout Optional<u32> inferred) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.explicit_type) requires nonzero(host.explicit_dataset) requires nonzero(host.family_current)
    requires nonzero(host.family_actual) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.push_type) requires nonzero(host.explicit_value)
    requires nonzero(host.push_value) requires nonzero(host.finish) requires nonzero(host.child)
    requires nonzero(host.element) requires nonzero(host.view) requires nonzero(host.dataset_syntax)
    requires nonzero(host.cfn_slot) requires nonzero(host.type_dataset) requires nonzero(host.type_cfn)
    requires nonzero(host.value_dataset) requires nonzero(host.value_cfn) requires nonzero(host.record_type)
    requires nonzero(host.record_value) requires nonzero(host.record_matches);

extern "C" bool instantiate(SignatureNode function, u32 tracking, u64 type_count, u64 value_count, InstanceHost host, address context) on cpu
    requires nonzero(host.implementation) requires nonzero(host.external) requires nonzero(host.plain)
    requires nonzero(host.name) requires nonzero(host.type_argument) requires nonzero(host.value_argument)
    requires nonzero(host.constant) requires nonzero(host.nonzero_error) requires nonzero(host.uses)
    requires nonzero(host.construct) requires nonzero(host.c_abi) requires nonzero(host.interface);

extern "C" bool is_open(StorageView type_storage, Slice types, StorageView value_storage, Slice values) on cpu;
