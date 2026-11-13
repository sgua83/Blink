module bootstrap.source_bind;
import bootstrap.ast_storage;
import bootstrap.signatures;

// Native callbacks access the original instance and construct its actual
// semantic containers. No callback receives or rebuilds a second source tree.
extern "C" bool arguments(StorageView storage, Slice arguments, u32 checked, address context,
    cfn<Optional<u32>,address,Text> value,
    cfn<Optional<u32>,address,ShapeRef> shape,
    cfn<void,address,u64> reserve,
    cfn<bool,address,TypeRef> check,
    cfn<bool,address,TypeRef> append_type,
    cfn<void,address,u32> append_value) on cpu
    requires nonzero(value) requires nonzero(shape) requires nonzero(reserve) requires nonzero(check)
    requires nonzero(append_type) requires nonzero(append_value);

extern "C" void type_name(SignatureNode source, address context,
    cfn<address,address,Text> actual,
    cfn<Optional<u32>,address,Text> value,
    cfn<Optional<u32>,address,ShapeRef> shape,
    cfn<void,address,u64> reserve,
    cfn<bool,address,TypeRef> check,
    cfn<bool,address,TypeRef> append_type,
    cfn<void,address,u32> append_value,
    cfn<void,address,u64> begin_contracts,
    cfn<void,address,PredicateRef,u64> append_predicate,
    cfn<void,address,Text,u32,Optional<u32>,u32> construct,
    cfn<void,address,address> clone_actual,
    cfn<void,address,u32,u32,u32,Optional<u32>> wrap) on cpu
    requires nonzero(actual) requires nonzero(value) requires nonzero(shape)
    requires nonzero(reserve) requires nonzero(check) requires nonzero(append_type)
    requires nonzero(append_value) requires nonzero(begin_contracts)
    requires nonzero(append_predicate) requires nonzero(construct)
    requires nonzero(clone_actual) requires nonzero(wrap);

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
