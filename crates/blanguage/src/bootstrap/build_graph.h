module bootstrap.build_graph;
import bootstrap.names;

struct GraphRange { u64 start; u64 count; };
struct GraphRow { address dependencies; u64 count; u64 object; };
// An external edge's input index is the identity of its borrowed Object.
struct GraphEdge { u64 index; u64 external; };

struct GraphSource {
    address units;
    u64 count;
    u64 edge_count;
    cfn<GraphRow,address,u64> row;
    cfn<GraphEdge,address,u64> edge;
};
struct GraphOutput {
    address providers; address following; address waiting; address followers;
    address ready; address counts;
};

// The allocation starts with borrowed external identities. Provider IDs below
// the unit count name units, and the remaining IDs index that prefix.
struct GraphPlan { address ids; u64 count; u64 status; u64 unit; };
extern "C" GraphPlan plan(GraphSource source,GraphOutput output) on cpu
    requires nonzero(source.row) requires nonzero(source.edge);
extern "C" void release_plan(address ids) on cpu;

// Entry keys borrow canonical paths and the planner's existing provider rows.
struct EntryKey { NameBytes path; NameBytes root; GraphRange providers; u64 unit; };
struct CaptureSource {
    address context;
    u64 unit_count;
    u64 entry_count;
    address ids;
    cfn<bool,address,u64,address> key;
    cfn<void,address,u64,bool> capture;
};

// Indices start at unit_count and finish as indices into the captured inputs.
extern "C" i32 capture_sources(CaptureSource source,address indices) on cpu
    requires nonzero(source.key) requires nonzero(source.capture);
