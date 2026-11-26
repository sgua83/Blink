module bootstrap.relation_order;

struct Input {
    address rows;
    address dependencies;
    usize count;
};

struct Workspace {
    address states;
    address users;
    address ready;
};

struct Output {
    address ordered;
    usize ordered_count;
    address fixed_inputs;
};


struct WalkSlice { address data; usize count; };
struct WalkVisit { usize node; u32 read; };
struct WalkEdge { usize source; usize frame; u32 read; };
struct WalkFrame { usize parent; WalkSlice writes; };
struct WalkWrite { usize target; WalkSlice fields; };

struct WalkEdges {
    cfn<WalkSlice,address,usize> edges;
    cfn<WalkEdge,address,usize> edge;
    cfn<WalkFrame,address,usize> frame;
    cfn<WalkWrite,address,usize> write;
    cfn<usize,address,usize> field;
    cfn<u32,address,WalkVisit> mark_frame;
    cfn<void,address,WalkVisit> publish;
};

struct WalkHost {
    WalkEdges edges;
    cfn<WalkVisit,address> pop;
    cfn<u32,address,WalkVisit> visit;
};

extern "C" u32 walk_relation(address graph,address work,usize root,u32 feedback,WalkHost host) on cpu
    requires nonzero(host.edges.edges) requires nonzero(host.edges.edge)
    requires nonzero(host.edges.frame) requires nonzero(host.edges.write)
    requires nonzero(host.edges.field) requires nonzero(host.edges.mark_frame)
    requires nonzero(host.edges.publish) requires nonzero(host.pop) requires nonzero(host.visit);
extern "C" void relation_dependencies(address graph,address work,usize root,WalkEdges host) on cpu
    requires nonzero(host.edges) requires nonzero(host.edge)
    requires nonzero(host.frame) requires nonzero(host.write)
    requires nonzero(host.field) requires nonzero(host.mark_frame) requires nonzero(host.publish);

extern "C" void historical_relation_frames(address graph,address work,address roots,cfn<usize,address> next,WalkEdges host) on cpu
    requires nonzero(next) requires nonzero(host.edges) requires nonzero(host.edge)
    requires nonzero(host.frame) requires nonzero(host.write) requires nonzero(host.field)
    requires nonzero(host.mark_frame) requires nonzero(host.publish);

struct OrderCount {
    usize completed;
    usize stable;
};

extern "C" OrderCount plan(Input input, Workspace work, Output output) on cpu;

struct CallRange { usize first; usize end; };
struct CallGraph { address rows; address callees; usize count; };
struct CallWork { address states; address visits; address active; };

extern "C" void recursive_call_groups(CallGraph graph, CallWork work, address groups) on cpu;

struct FeedbackEdge { usize from; u32 identity; };
struct FeedbackNode { address edges; usize count; u32 variable; u32 seeded; };
struct IdentityFeedbackHost {
    cfn<FeedbackEdge,address,usize> edge;
    cfn<FeedbackNode,address,usize> node;
    cfn<void,address,usize,usize> exchange;
};

extern "C" usize prune_identity_feedback(address graph,address edges,usize count,usize variable,IdentityFeedbackHost host) on cpu
    requires nonzero(host.edge) requires nonzero(host.node) requires nonzero(host.exchange);
