module bootstrap.coloring;

struct ColorNode { u64 weight; u64 degree; u64 across_call; };
struct ColorPlace { u64 value; u32 kind; u8 width; u8 floating; };

// The original graph is borrowed for this call. places is the final location
// array, and order has one scratch row per node. A zero width denotes a
// separately laid out addressable declaration.
struct ColorLayout { u64 end; u32 gpr; u32 xmm; };

extern "C" ColorLayout color(address graph, u64 count, address places, address order, u32 mode, u64 base,
    cfn<ColorNode,address,u64> node,
    cfn<u64,address,u64,bool,u64> next) on cpu
    requires nonzero(node) requires nonzero(next);

// The supplied places already contain the declaration's field byte offsets.
extern "C" u64 place_private(address places, u64 count, u64 data_end, u64 bytes, u64 alignment)
    on cpu requires nonzero(alignment);

// Every definition updates its original machine place before coloring.
extern "C" void record_definition(address place, u32 literal, u64 bits) on cpu;

enum GraphSpace:u32 { Register=0; Local=1; };
enum GraphRelation:u32 { Edge=0; Affinity=1; };
enum GraphCounter:u32 { Registers=0; Outgoing=1; };
enum GraphSlotAction:u32 { Insert=0; Remove=1; };
enum GraphSetAction:u32 { Copy=0; Union=1; DrainUnion=2; Swap=3; Clear=4; };
enum GraphKind:u32 { Ordinary=0; If=1; Loop=2; Scope=3; Raise=4; Exit=5; Break=6; Continue=7; LocalLoad=8; LocalStore=9; Cast=10; Unary=11; Binary=12; Select=13; Math=14; Constant=15; Print=16; Call=17; };
struct GraphBlock { address instructions; u64 count; };
struct GraphScalars { address data; u64 count; };
struct GraphNode {
    GraphBlock first; GraphBlock second; address instruction; address arguments; u64 count;
    u64 dst; u64 a; u64 b; u64 bits; u64 outgoing;
    GraphKind kind; u32 flags; u32 operation;
};
struct GraphEvent { u64 anchor; u64 skip; u32 kind; u32 skip_present; };
struct GraphDefinition { u64 weight; u64 transfer_dst; u64 transfer_src; u64 bits; u32 transfer; u32 literal; u32 pass; };
struct GraphCore {
    cfn<u32,address,u64> addressable;
    cfn<void,address,GraphRelation,u64,u64> relation;
    cfn<u64,address,u64> weight;
    cfn<void,address,u64,u64> set_weight;
    cfn<void,address,u64> across;
    cfn<u64,address,GraphCounter> counter;
    cfn<void,address,u64> set_outgoing;
    cfn<address,address,u64> location;
    cfn<void,address,u64,address,GraphEvent> visit_set;
    cfn<void,address,GraphSlotAction,GraphSpace,u64> slot;
};
struct GraphHost {
    GraphCore core;
    cfn<GraphNode,GraphBlock,u64,GraphScalars> read;
    cfn<address,address> clone_set;
    cfn<void,address> drop_set;
    cfn<void,GraphSetAction,address,address> change;
    cfn<address,GraphBlock,u64,GraphBlock,address,address,address,address> loop_live;
    cfn<void,address,address,address,GraphDefinition> definitions;
    cfn<void,address,address,address,u64> reads;
};
// All sets, graph arrays and IR blocks are this synchronous caller's real containers.
extern "C" void graph_block(address graph,GraphBlock code,GraphScalars registers,address live,
    address break_live,address continue_live,address exit_live,u64 weight,GraphHost host) on cpu 
    requires nonzero(host.core.addressable) requires nonzero(host.core.relation) requires nonzero(host.core.weight) requires nonzero(host.core.set_weight)
    requires nonzero(host.core.across) requires nonzero(host.core.counter) requires nonzero(host.core.set_outgoing) requires nonzero(host.core.location)
    requires nonzero(host.core.visit_set) requires nonzero(host.core.slot) requires nonzero(host.read) requires nonzero(host.clone_set)
    requires nonzero(host.drop_set) requires nonzero(host.change) requires nonzero(host.loop_live) requires nonzero(host.definitions)
    requires nonzero(host.reads);
extern "C" void graph_together(address graph,address live,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot);
extern "C" void graph_inputs(address graph,address inputs,u64 count,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot);
extern "C" void graph_live_event(address graph,address live,GraphSpace space,u64 index,u64 ordinal,GraphEvent event,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot);
extern "C" void graph_definition(address graph,address live,GraphSpace space,u64 index,GraphDefinition definition,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot);
extern "C" void graph_read(address graph,address live,GraphSpace space,u64 index,u64 weight,GraphCore core) on cpu 
    requires nonzero(core.addressable) requires nonzero(core.relation) requires nonzero(core.weight) requires nonzero(core.set_weight)
    requires nonzero(core.across) requires nonzero(core.counter) requires nonzero(core.set_outgoing) requires nonzero(core.location)
    requires nonzero(core.visit_set) requires nonzero(core.slot);
