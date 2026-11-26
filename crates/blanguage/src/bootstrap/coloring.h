module bootstrap.coloring;
import bootstrap.ir_dataflow;

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
enum GraphSlotAction:u32 { Insert=0; Remove=1; };
enum GraphSetAction:u32 { Copy=0; Union=1; DrainUnion=2; Swap=3; Clear=4; };
struct GraphEvent { u64 anchor; u64 skip; u32 kind; u32 skip_present; };
struct GraphCore {
    cfn<void,handle,u64,address,GraphEvent> visit_set;
    cfn<void,handle,GraphSlotAction,GraphSpace,u64> slot;
};
struct GraphHost {
    GraphCore core;
    RegisterHost values;
    cfn<u32,Span,usize,usize> ensures memory(read) cast_bits;
    cfn<u64,address> ensures memory(read) call_outgoing;
    cfn<u64,u32> ensures memory(none) math_outgoing;
    cfn<handle,handle> clone_set;
    cfn<void,handle> drop_set;
    cfn<void,GraphSetAction,handle,handle> change;
    cfn<handle,Span,usize,Span,handle,handle,handle,handle> loop_live;
    cfn<void,address,address,handle,u64> register_reads;
};
// The IR is borrowed synchronously and the live sets are opaque to B.
struct GraphRequest {Span code;Span registers;address places;u64 count;handle live;handle empty;};
struct GraphBuilt {address state;u32 uses_r15;};
extern "C" GraphBuilt graph_create(address request,GraphHost host) on cpu
    requires nonzero(host.core.visit_set) requires nonzero(host.core.slot)
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar)
    requires nonzero(host.cast_bits) requires nonzero(host.call_outgoing) requires nonzero(host.math_outgoing)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change)
    requires nonzero(host.loop_live) requires nonzero(host.register_reads);
extern "C" void graph_together(address graph,handle live,GraphCore core) on cpu
    requires nonzero(core.visit_set);
extern "C" void graph_inputs(address graph,address inputs,u64 count) on cpu;
extern "C" void graph_live_event(address graph,handle live,GraphSpace space,u64 index,u64 ordinal,GraphEvent event,GraphCore core) on cpu
    requires nonzero(core.visit_set);
extern "C" void graph_read(address graph,handle live,GraphSpace space,u64 index,u64 weight,GraphCore core) on cpu
    requires nonzero(core.slot);
