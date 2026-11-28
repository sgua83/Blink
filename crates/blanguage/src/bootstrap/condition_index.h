module bootstrap.condition_index;
import bootstrap.proof_facts;

enum NodeKind:u32 { Impossible=0; Facts=1; Both=2; Either=3; };

struct NodeFields {NodeKind kind;address facts;u64 first;u64 second;};
struct ConditionNodes {address fields;usize count;};

struct IdentityTable { address entries; u64 count; u64 capacity; };
struct CanonicalGraph { OrderedTree nodes; IdentityTable identities; };
void condition_index_insert(address graph,address order,u64 key,NodeFields shape,u32 keep_pairs) on cpu;
extern "C" u64 condition_node_index(address graph,u64 key) on cpu;
extern "C" NodeFields condition_node_at(address graph,u64 index) on cpu;
extern "C" ConditionNodes condition_seal(address graph) on cpu;
extern "C" void condition_graph_drop(address graph) on cpu;

struct ContractClauseMemo {
    IdentityTable imported;
    IdentityTable emitted;
    IdentityTable negatives;
};

extern "C" void start_contract_clause_memo(address memo) on cpu;
extern "C" void drop_contract_clause_memo(address memo) on cpu;
extern "C" u64 contract_memo_imported(address memo,address clause) on cpu;
extern "C" void remember_contract_import(address memo,address clause,u64 node) on cpu;
extern "C" address contract_memo_emitted(address memo,u64 node,u32 retain) on cpu;
extern "C" void remember_contract_emission(address memo,u64 node,address owned) on cpu;
extern "C" u64 contract_memo_negative(address memo,u64 node) on cpu;
extern "C" void remember_contract_negative(address memo,u64 node,u64 negative) on cpu;

struct ContractDecisionCell { u64 low; u64 high; usize child; };
struct ContractDecision { address variable; address cells; usize count; };

extern "C" usize intern_contract_decision(
    address tree,ContractDecision value,usize variable_bytes,cfn<i32,address,address> compare_variable
) on cpu requires nonzero(compare_variable);
extern "C" ContractDecision contract_decision_at(address tree,usize node) on cpu;
extern "C" void drop_contract_decisions(address tree) on cpu;
