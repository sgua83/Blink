module bootstrap.proof_facts;

struct FactTree { address records; u64 root; u64 count; u64 capacity; };
struct FactCursor { u64 front; u64 back; };
struct FactBound { address key; u32 exclusive; };

extern "C" void fact_tree_clear(address owner) on cpu;
extern "C" void fact_tree_copy(address original, address output) on cpu;
extern "C" u32 fact_tree_insert(address owner, address key) on cpu;
extern "C" u32 fact_tree_remove(address owner, address key) on cpu;
extern "C" u32 fact_tree_contains(address owner, address key) on cpu;
extern "C" void fact_tree_retain(address owner, address context, cfn<u32,address,address> keep) on cpu
    requires nonzero(keep);

extern "C" FactCursor fact_tree_cursor(address owner) on cpu;
extern "C" FactCursor fact_tree_range(address owner, FactBound start, FactBound end) on cpu;
extern "C" address fact_tree_next(address owner, address cursor, u32 reverse) on cpu;

extern "C" address fact_tree_merge(address left, address right, address cursors, u32 operation) on cpu;
extern "C" i32 fact_tree_compare(address left, address right) on cpu;
extern "C" u32 fact_tree_relation(address left, address right, u32 subset) on cpu;
