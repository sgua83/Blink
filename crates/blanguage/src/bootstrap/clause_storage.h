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

extern "C" address clause_impossible() on cpu;
extern "C" address clause_facts(address facts) on cpu;
extern "C" address clause_pair(StoredClauseKind kind, u32 truth, address first, address second) on cpu;
extern "C" void clause_retain(address payload) on cpu;
extern "C" void clause_retain_identity(address payload) on cpu;
extern "C" void clause_release_identity(address payload) on cpu;
extern "C" void clause_release(address payload) on cpu;
