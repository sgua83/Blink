module bootstrap.condition_graph;

// These descriptors borrow the original Clause query and its physical buffers.
// They are private native fields, not a stored condition or public graph format.
struct Span { address data; usize count; };
struct SourceNode { u32 kind; address left; address right; address no; u32 truth; u32 empty; };
struct Gate { usize left; usize right; u32 kind; Span parents; };
struct Graph { address nodes; usize stride; address values; };
struct Assignment { u32 present; usize node; u32 truth; };
struct Checkpoint { usize nodes; usize facts; usize negative; };
struct Alternative { u32 kind; usize node; u32 truth; Checkpoint checkpoint; };
struct Host {
 cfn<usize,address,address> index;
 cfn<usize,address> ensures memory(read) node_count;
 cfn<SourceNode,address,usize> ensures memory(read) inspect;
 cfn<void,address,usize,u32,usize,usize> define;
 cfn<void,address,usize,usize> parent_push;
 cfn<Graph,address> prepare;
 cfn<void,address,usize,u32> pending_push;
 cfn<Assignment,address> pending_pop;
 cfn<void,address> pending_clear;
 cfn<void,address,usize> trail_push;
 cfn<usize,address> trail_pop;
 cfn<Span,address> ensures memory(read) trail;
 cfn<usize,address> ensures memory(read) additions;
 cfn<void,address> undo_addition;
 cfn<void,address,usize> add_leaf;
 cfn<u32,address> contradictory;
 cfn<u32,address> add_next;
 cfn<void,address,usize> begin_facts;
 cfn<address,address> next_fact;
 cfn<u32,address,address> negate;
 cfn<usize,address> ensures memory(read) remaining;
 cfn<void,address,usize,u32,Checkpoint> push_clause;
 cfn<void,address,address,Checkpoint> push_fact;
 cfn<Alternative,address> pop_alternative;
};
extern "C" u32 overlap(address left,address right,u32 right_truth,address context,Host host) on cpu
 requires nonzero(host.index) requires nonzero(host.node_count) requires nonzero(host.inspect)
 requires nonzero(host.define) requires nonzero(host.parent_push) requires nonzero(host.prepare)
 requires nonzero(host.pending_push) requires nonzero(host.pending_pop) requires nonzero(host.pending_clear)
 requires nonzero(host.trail_push) requires nonzero(host.trail_pop) requires nonzero(host.trail)
 requires nonzero(host.additions) requires nonzero(host.undo_addition) requires nonzero(host.add_leaf)
 requires nonzero(host.contradictory) requires nonzero(host.add_next) requires nonzero(host.begin_facts)
 requires nonzero(host.next_fact) requires nonzero(host.negate) requires nonzero(host.remaining)
 requires nonzero(host.push_clause) requires nonzero(host.push_fact) requires nonzero(host.pop_alternative);

// Original required-facts query nodes; the result stays in its owning container.
struct RequiredNode { usize left; usize right; u32 kind; usize users; usize remaining; };
struct RequiredGraph { address nodes; usize stride; usize count; };
struct RequiredPending { u32 present; usize index; };
struct RequiredBuildHost {
 cfn<usize,address,address> index;
 cfn<usize,address> ensures memory(read) count;
 cfn<SourceNode,address,usize> ensures memory(read) inspect;
 cfn<void,address,usize,u32,usize,usize> define;
};
struct RequiredHost {
 cfn<void,address,usize> reset;
 cfn<u32,address,usize> ensures memory(read) status;
 cfn<void,address,usize> push;
 cfn<RequiredPending,address> pop;
 cfn<usize,address> ensures memory(read) pending_count;
 cfn<void,address,usize> impossible;
 cfn<void,address,usize> leaf;
 cfn<void,address,usize,u32,u32> union;
 cfn<void,address,usize,u32,u32> common;
 cfn<u32,address,usize> ensures memory(read) valid_additions;
 cfn<u32,address,usize,u32> release;
};
extern "C" void required_build(address root,address context,RequiredBuildHost host) on cpu
 requires nonzero(host.index) requires nonzero(host.count)
 requires nonzero(host.inspect) requires nonzero(host.define);
extern "C" void required_evaluate(RequiredGraph graph,address context,RequiredHost host) on cpu
 requires nonzero(host.reset) requires nonzero(host.status) requires nonzero(host.push)
 requires nonzero(host.pop) requires nonzero(host.pending_count) requires nonzero(host.impossible)
 requires nonzero(host.leaf) requires nonzero(host.union) requires nonzero(host.common)
 requires nonzero(host.valid_additions) requires nonzero(host.release);

enum CrossFactKind : u32 { Equal=1;Index=2;Minimum=3;Maximum=4;NonZero=5;Boolean=6;Below=7;Outside=8;Zero=9;NotEqual=10;Predicate=11; };
struct CrossFact { u32 present; CrossFactKind kind; address first; address second; address payload; address scalar; u64 value; u64 mask; u32 flag; };
struct CrossScalars { array<address,4> values; };
struct CrossHost {
 cfn<usize,address,u32> ensures memory(read) count;
 cfn<void,address,u32> begin;
 cfn<CrossFact,address> next;
 cfn<u32,address,u32,CrossFact> ensures memory(read) contains;
 cfn<u32,address,u32,CrossFact,CrossFact,u32,u32> ensures memory(read) range;
 cfn<i32,address,address> ensures memory(read) compare;
};
extern "C" u32 cross_conflict(address context,CrossScalars scalars,CrossHost host) on cpu
 requires nonzero(host.count) requires nonzero(host.begin) requires nonzero(host.next)
 requires nonzero(host.contains) requires nonzero(host.range) requires nonzero(host.compare);
