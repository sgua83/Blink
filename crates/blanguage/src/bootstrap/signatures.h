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
