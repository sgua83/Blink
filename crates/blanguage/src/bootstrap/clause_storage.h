module bootstrap.clause_storage;
import bootstrap.proof_facts;

enum StoredClauseKind : u32 {
    Impossible=0;
    Facts=1;
    Both=2;
    Either=3;
    Branch=4;
    Condition=5;
};

struct StoredClause { StoredClauseKind kind; u32 truth; };

struct SourceNode { u32 kind; address left; address right; address no; u32 truth; u32 empty; address condition; };
SourceNode clause_source(address clause) on cpu;

extern "C" address clause_impossible() on cpu;
extern "C" address clause_facts(address facts) on cpu;
extern "C" address clause_pair(StoredClauseKind kind, u32 truth, address first, address second) on cpu;
extern "C" void clause_retain(address payload) on cpu;
extern "C" void clause_retain_identity(address payload) on cpu;
extern "C" void clause_release_identity(address payload) on cpu;
extern "C" void clause_release(address payload) on cpu;

extern "C" address checked_clause_facts(address facts,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict);
extern "C" address combine_clause_both(address left,address right,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict);
extern "C" address combine_clause_either(address left,address right,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict);
extern "C" address combine_clause_branch(address parent,address condition,u32 truth) on cpu;

extern "C" void clause_walk_drop(address owner) on cpu;
extern "C" void clause_fact_walk_start(address owner,address context,cfn<address,address> next) on cpu
    requires nonzero(next);
extern "C" address clause_fact_walk_next(address owner) on cpu;
extern "C" u32 clause_input_identities(address owner,address context,u32 nested_domains,cfn<u32,address,address> accept) on cpu
    requires nonzero(accept);
