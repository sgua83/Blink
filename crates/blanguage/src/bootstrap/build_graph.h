module bootstrap.build_graph;

struct GraphRange { u64 start; u64 count; };
struct GraphUnit { GraphRange dependencies; u64 object; };
struct GraphEdge { u64 index; u64 external; };

// After provider expansion, plan compacts direct unit edges in the caller's edge buffer.
struct GraphInput { address units; address edges; u64 count; u64 external_count; };
struct GraphOutput { address providers; address following; address waiting; address followers; };

// Providers are dense unit indices followed by the input's borrowed external object indices.
struct GraphPlan { address ids; u64 count; u64 status; u64 unit; };
extern "C" GraphPlan plan(GraphInput input,GraphOutput output) on cpu;
extern "C" void release_plan(address ids) on cpu;
