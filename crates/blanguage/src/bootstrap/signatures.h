module bootstrap.signatures;
import bootstrap.ast_storage;

struct SignatureNode {
    StorageView storage;
    u64 offset;
};

extern "C" i32 compare_condition(SignatureNode left, SignatureNode right) on cpu;
extern "C" i32 compare_predicate(SignatureNode left, SignatureNode right) on cpu;
extern "C" bool same_signature(SignatureNode left, SignatureNode right,
    address left_contracts, address right_contracts) on cpu;

enum RecordMember : u32 {
    Missing() = 0;
    Variant() = 1;
    Field(TypeRef) = 2;
};

extern "C" RecordMember record_member(SignatureNode record, address bytes, u64 length) on cpu;

struct CfnParameter {
    u32 access;
    Optional<TypeRef> type;
};

extern "C" CfnParameter cfn_parameter(SignatureNode formal) on cpu;

extern "C" bool type_mentions(SignatureNode type, address bytes, u64 length) on cpu;
// The caller keeps the original source nodes and resolved const values live.
extern "C" bool infer_const_count(SignatureNode type, address name, u64 length, u32 value,
    inout Optional<u32> inferred) on cpu;
extern "C" Optional<u32> infer_const_arguments(SignatureNode type, address name, u64 length,
    u64 first, address values, u64 count, inout Optional<u32> inferred) on cpu;

enum ParameterError:u32 { Valid=0; TypeName=1; EmptyConstDomain=2; ConstName=3; };
extern "C" ParameterError function_parameter_error(SignatureNode declaration) on cpu;
extern "C" ParameterError record_parameter_error(SignatureNode declaration) on cpu;
