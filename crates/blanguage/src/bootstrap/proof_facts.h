module bootstrap.proof_facts;

struct OrderedTree { address records; u64 root; u64 count; u64 capacity; };
struct OrderedNode { u64 left; u64 right; u64 parent; u64 height; };
address ordered_tree_prepare(address owner,u64 stride) on cpu requires nonzero(stride);
void ordered_tree_copy(address original,address output,u64 stride) on cpu;
void ordered_tree_publish(address owner,u64 stride,u64 parent,i32 order) on cpu;
u64 ordered_tree_first(OrderedTree tree,u64 stride) on cpu;
u64 ordered_tree_next(OrderedTree tree,u64 stride,u64 current) on cpu;
struct FactCursor { u64 front; u64 back; };
struct FactBound { address key; u32 exclusive; };

struct OrderedPlace { address entry; u64 parent; i32 order; };
extern "C" OrderedPlace ordered_tree_seek(address owner,u64 stride,address key,cfn<i32,address,address> compare) on cpu
    requires nonzero(compare);
OrderedPlace ordered_tree_seek_next(address owner,u64 stride,address key,
    inout u64 position,cfn<i32,address,address> compare) on cpu
    requires nonzero(stride) requires nonzero(compare);
extern "C" address ordered_tree_insert_slot(address owner,u64 stride,u64 parent,i32 order) on cpu
    requires nonzero(stride);
extern "C" void ordered_tree_erase_slot(address owner,u64 stride,address entry) on cpu
    requires nonzero(stride);
extern "C" void ordered_tree_retain_owned(address owner,u64 stride,address context,cfn<u32,address,address> retain) on cpu
    requires nonzero(retain);
extern "C" void ordered_tree_clone_owned(address original,address output,u64 stride,cfn<void,address,address> clone) on cpu
    requires nonzero(clone);
extern "C" FactCursor ordered_tree_cursor(address owner,u64 stride) on cpu;
extern "C" address ordered_tree_step(address owner,u64 stride,address cursor,u32 reverse) on cpu;

extern "C" void fact_tree_clear(address owner) on cpu;
extern "C" void fact_tree_copy(address original, address output) on cpu;
address fact_tree_prepare_key(address owner) on cpu;
extern "C" u32 fact_tree_insert(address owner, address key) on cpu;
extern "C" u32 fact_tree_remove(address owner, address key) on cpu;
extern "C" u32 fact_tree_contains(address owner, address key) on cpu;
u32 fact_tree_contains_view(address owner,address key) on cpu;
bool fact_tree_range_contains_view(address owner,FactBound start,FactBound end) on cpu;
extern "C" void fact_tree_retain(address owner, address context, cfn<u32,address,address> keep) on cpu
    requires nonzero(keep);

extern "C" FactCursor fact_tree_cursor(address owner) on cpu;
extern "C" FactCursor fact_tree_range(address owner, FactBound start, FactBound end) on cpu;
extern "C" address fact_tree_next(address owner, address cursor, u32 reverse) on cpu;

extern "C" void alias_facts_build(address owner,address facts,address roots,cfn<address,address,address> lookup) on cpu
    requires nonzero(lookup);
extern "C" u32 alias_facts_insert(address owner,address fact,address roots,cfn<address,address,address> lookup) on cpu
    requires nonzero(lookup);
extern "C" void alias_facts_remove(address owner,address fact,address roots,cfn<address,address,address> lookup) on cpu
    requires nonzero(lookup);
FactCursor conflict_fact_cursor(address owner,u32 aliases) on cpu;
address conflict_fact_next(address owner,address cursor,u32 aliases) on cpu;
address conflict_fact_select(address owner,address key,u32 preceding,u32 aliases) on cpu;
bool conflict_fact_contains(address owner,address key,u32 aliases) on cpu;

extern "C" address fact_tree_merge(address left, address right, address cursors, u32 operation) on cpu;
extern "C" void fact_tree_collect_merge(address left,address right,address cursors,u32 operation,address output) on cpu;
extern "C" i32 fact_tree_compare(address left, address right) on cpu;
extern "C" u32 fact_tree_relation(address left, address right, u32 subset) on cpu;

extern "C" void domain_tree_copy(address original,address output) on cpu;
extern "C" u32 domain_tree_insert(address owner,address key) on cpu;
extern "C" u32 domain_tree_contains(address owner,address key) on cpu;
extern "C" u32 domain_tree_contains_range(address owner,address start,address end) on cpu;
extern "C" void domain_tree_append(address owner,address incoming) on cpu;
extern "C" void domain_tree_retain(address owner,address context,cfn<u32,address,address> keep) on cpu
    requires nonzero(keep);
extern "C" FactCursor domain_tree_cursor(address owner) on cpu;
extern "C" address domain_tree_next(address owner,address cursor,u32 reverse) on cpu;
extern "C" address domain_tree_merge(address left,address right,address left_cursor,address right_cursor,u32 operation) on cpu;
extern "C" i32 domain_tree_compare(address left,address right) on cpu;
