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
