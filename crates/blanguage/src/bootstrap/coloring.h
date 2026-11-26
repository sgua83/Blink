module bootstrap.coloring;
import bootstrap.ir_dataflow;
import bootstrap.stage_fusion;
import bootstrap.scalar_fold;
import bootstrap.call_layout;
import bootstrap.x64_instruction;
import bootstrap.frame_layout;


// The graph owns its scalar rows until the allocation is handed to emission.
extern "C" void graph_release(address graph) on cpu;
extern "C" bool prepare_packet(address graph, address context, Span code, PacketPlanHeader header, address output, PacketPlanHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.stride) requires nonzero(host.written) requires nonzero(host.scalar)
    requires nonzero(host.widths);
extern "C" void color_places_drop(Span places) on cpu;

// The cursor borrows original ranges and offsets for the graph and final stack write.
struct PrivateDeclaration { Span offsets; usize begin; usize end; u32 size; u32 alignment; };
struct ScalarAllocation { ColorPositions positions; ColorLayout layout; };
extern "C" ScalarAllocation finish_scalar(address graph,u32 mode,address context,
    cfn<PrivateDeclaration,address> next) on cpu requires nonzero(next);

enum GraphSpace:u32 { Register=0; Local=1; };
struct GraphEvent { u64 anchor; u64 skip; u32 kind; u32 skip_present; };
struct GraphCore {
    cfn<void,address,u64,address,GraphEvent> visit_set;
};
struct GraphHost {
    GraphCore core;
    LiveHost live;
    cfn<ScalarEncoding,address> ensures memory(read) scalar;
    cfn<Span,address> next_local;
    cfn<PrivateDeclaration,address> next_private;
    cfn<u32,address,usize> register_operand;
};
// Borrow the original IR and native live sets for the synchronous graph walk.
struct GraphRequest {
    Span code; Span registers; usize local_slots; address locals; address private;
    address live; address empty;
};
struct GraphBuilt {address state;u32 uses_r15;};
extern "C" GraphBuilt graph_create(address request,GraphHost host) on cpu
    requires nonzero(host.core.visit_set) requires nonzero(host.live.ir.inspect) requires nonzero(host.scalar)
    requires nonzero(host.next_local) requires nonzero(host.next_private)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set) requires nonzero(host.live.change)
    requires nonzero(host.live.insert_register) requires nonzero(host.register_operand);
extern "C" void graph_together(address graph,address live,GraphCore core) on cpu
    requires nonzero(core.visit_set);
extern "C" void graph_inputs(address graph,address inputs,u64 count) on cpu;
extern "C" void graph_live_event(address graph,address live,GraphSpace space,u64 index,u64 ordinal,GraphEvent event,GraphCore core) on cpu
    requires nonzero(core.visit_set);
extern "C" void graph_register_read(address graph,usize index,u64 weight) on cpu;
