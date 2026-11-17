module bootstrap.source_bind;
import bootstrap.ast_storage;
import bootstrap.signatures;

// The original source-binding query owns this cursor and its partial native values.
// Requests end the native call; the same cursor consumes the actual child result.
struct BindingCursor {
    StorageView storage; u64 offset; Slice arguments; u64 index;
    u32 mode; u32 checked; u32 stage; Optional<u32> shape;
};
struct BindingRequest { u32 kind; u64 reference; };
struct BindingHost {
    cfn<address,address,Text> actual;
    cfn<Optional<u32>,address,Text> value;
    cfn<void,address,u64> reserve;
    cfn<bool,address,TypeRef> check;
    cfn<void,address,u32> append_value;
    cfn<void,address,u64> begin_contracts;
    cfn<void,address,PredicateRef,u64> append_predicate;
    cfn<void,address,Text,u32,Optional<u32>,u32> construct;
    cfn<void,address,address> clone_actual;
    cfn<void,address,u32,u32,u32,Optional<u32>> wrap;
};
extern "C" BindingCursor type_start(SignatureNode source) on cpu;
extern "C" BindingCursor arguments_start(StorageView storage,Slice arguments,u32 checked) on cpu;
extern "C" BindingRequest binding_step(address state,address context,BindingHost host) on cpu
    requires nonzero(host.actual) requires nonzero(host.value) requires nonzero(host.reserve)
    requires nonzero(host.check) requires nonzero(host.append_value) requires nonzero(host.begin_contracts)
    requires nonzero(host.append_predicate) requires nonzero(host.construct)
    requires nonzero(host.clone_actual) requires nonzero(host.wrap);

// parameters is present only for a cfn signature. returned remains a runtime
// output name only in initialized/cleanup conditions, as in the source contract.
extern "C" void condition(SignatureNode source, Optional<u64> parameters, u32 output, address context,
    cfn<Optional<u32>,address,Text> value,
    cfn<void,address,ConditionNode> construct) on cpu
    requires nonzero(value) requires nonzero(construct);

extern "C" void predicate(SignatureNode source, Optional<u64> parameters, address context,
    cfn<void,address,u64> reserve,
    cfn<void,address,ConditionRef,ConditionRef> append_interval,
    cfn<void,address,PredicateNode,u32> construct) on cpu
    requires nonzero(reserve) requires nonzero(append_interval) requires nonzero(construct);

extern "C" bool runtime_contract(SignatureNode source, address context,
    cfn<Optional<u32>,address,Text> value) on cpu requires nonzero(value);
