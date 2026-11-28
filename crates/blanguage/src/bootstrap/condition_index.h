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

