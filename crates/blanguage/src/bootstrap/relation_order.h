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
    address fixed_inputs;
};

extern "C" usize plan(Input input, Workspace work, Output output) on cpu;

struct CallRange { usize first; usize end; };
struct CallGraph { address rows; address callees; usize count; };
struct CallWork { address states; address visits; address active; };

extern "C" void recursive_call_groups(CallGraph graph, CallWork work, address groups) on cpu;
