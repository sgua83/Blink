module bootstrap.coloring;
import bootstrap.ir_dataflow;
import bootstrap.stage_fusion;
import bootstrap.scalar_fold;
import bootstrap.call_layout;

struct ColorPlace { u64 value; u32 kind; u8 width; u8 floating; };
struct ColorLayout { u64 end; u32 gpr; u32 xmm; };

// One graph owns its pairs while building and its ordered rows after sealing.
extern "C" u64 graph_count(address graph) on cpu;
extern "C" void graph_seal(address graph) on cpu;
extern "C" void graph_release(address graph) on cpu;
// Places are the actual output allocation. Packet mode also borrows index uses.
extern "C" ColorLayout color(address graph,u64 count,address places,u32 mode,Span packet_uses) on cpu;

// The cursor borrows original declaration offsets through the final stack write.
struct PrivateDeclaration { Span offsets; usize begin; u32 size; u32 alignment; };
extern "C" u64 place_private(address places, u64 data_end, usize registers, address context,
    cfn<PrivateDeclaration,address> next) on cpu requires nonzero(next);
extern "C" void record_definition(address place, u32 literal, u64 bits) on cpu;

enum GraphSpace:u32 { Register=0; Local=1; };
struct GraphEvent { u64 anchor; u64 skip; u32 kind; u32 skip_present; };
struct GraphCore {
    cfn<void,address,u64,address,GraphEvent> visit_set;
};
struct GraphHost {
    GraphCore core;
    LiveHost live;
    cfn<ScalarEncoding,Span,usize> ensures memory(read) scalar;
    cfn<CallAbi,address> ensures memory(read) call_abi;
    cfn<AbiType,address,u64> call_argument;
    cfn<u32,address,usize> register_operand;
};
// Borrow the original IR and native live sets for the synchronous graph walk.
struct GraphRequest {Span code;Span registers;address places;u64 count;address live;address empty;};
struct GraphBuilt {address state;u32 uses_r15;};
extern "C" GraphBuilt graph_create(address request,GraphHost host) on cpu
    requires nonzero(host.core.visit_set) requires nonzero(host.live.ir.inspect) requires nonzero(host.scalar)
    requires nonzero(host.call_abi) requires nonzero(host.call_argument)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set) requires nonzero(host.live.change)
    requires nonzero(host.live.insert_register) requires nonzero(host.register_operand);
extern "C" void graph_together(address graph,address live,GraphCore core) on cpu
    requires nonzero(core.visit_set);
extern "C" void graph_inputs(address graph,address inputs,u64 count) on cpu;
extern "C" void graph_live_event(address graph,address live,GraphSpace space,u64 index,u64 ordinal,GraphEvent event,GraphCore core) on cpu
    requires nonzero(core.visit_set);
extern "C" void graph_register_read(address graph,usize index,u64 weight) on cpu;
