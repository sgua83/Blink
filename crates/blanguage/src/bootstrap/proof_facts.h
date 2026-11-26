module bootstrap.proof_facts;

struct FactTree { address records; u64 root; u64 count; u64 capacity; };
struct FactCursor { u64 front; u64 back; };
struct FactBound { address key; u32 exclusive; };

extern "C" void fact_tree_clear(address owner) on cpu;
extern "C" void fact_tree_copy(address original, address output, u64 width) on cpu;
extern "C" u32 fact_tree_insert(address owner, address key, u64 width, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare);
extern "C" u32 fact_tree_remove(address owner, address key, u64 width, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare);
extern "C" u32 fact_tree_contains(address owner, address key, u64 width, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare);
extern "C" void fact_tree_retain(address owner, u64 width, address context, cfn<u32,address,address> keep) on cpu
    requires nonzero(keep);

extern "C" FactCursor fact_tree_cursor(address owner, u64 width) on cpu;
extern "C" FactCursor fact_tree_range(address owner, u64 width, FactBound start, FactBound end,
    cfn<i32,address,address> compare) on cpu requires nonzero(compare);
extern "C" address fact_tree_next(address owner, u64 width, address cursor, u32 reverse) on cpu;

extern "C" address fact_tree_merge(address left, address right, address cursors, u64 width, u32 operation,
    cfn<i32,address,address> compare) on cpu requires nonzero(compare);
extern "C" i32 fact_tree_compare(address left, address right, u64 width, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare);
extern "C" u32 fact_tree_relation(address left, address right, u64 width, u32 subset,
    cfn<i32,address,address> compare) on cpu requires nonzero(compare);
