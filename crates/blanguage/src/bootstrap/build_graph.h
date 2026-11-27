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
struct GraphStorage {
    address providers; address following; address waiting; address followers;
    address ready; address counts; address captures;
};

// The allocation starts with borrowed external identities. Provider IDs below
// the unit count name units, and the remaining IDs index that prefix.
struct GraphPlan {
    address ids; u64 count; GraphStorage storage;
    u64 units; u64 entries; u64 status; u64 unit;
};
extern "C" GraphPlan plan(GraphSource source) on cpu
    requires nonzero(source.row) requires nonzero(source.edge);
extern "C" void release_plan(address ids,address storage) on cpu;

// The host only lends the canonical paths for the current entry.
struct CapturePath { NameBytes path; NameBytes root; };
// Borrowed only while the original unit capture callback is running.
struct ProviderNames { address rows; u64 count; address ids; };
struct CaptureSource {
    address context;
    cfn<bool,address,u64,address> key;
    cfn<NameBytes,address,u64> ensures memory(read) provider;
    cfn<void,address,u64,bool,ProviderNames> capture;
};

// Final indices live in the same plan through the last unit.
extern "C" i32 capture_sources(address owner,CaptureSource source) on cpu
    requires nonzero(source.key) requires nonzero(source.provider) requires nonzero(source.capture);

// The returned ID names the first original provider, or the maximum u64 if absent.
extern "C" u64 lookup_capture_provider(ProviderNames providers,NameBytes name) on cpu;
