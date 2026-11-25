module bootstrap.build_graph;
import bootstrap.names;

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

// Entry keys borrow canonical paths and the planner's existing provider rows.
struct EntryKey { NameBytes path; NameBytes root; GraphRange providers; u64 unit; };
enum CaptureKind:u64 { Unique=0u64; Shared=1u64; Borrowed=2u64; };
struct CaptureSlot { u64 index; CaptureKind kind; };
extern "C" u64 capture_slots(address entries,address order,u64 entry_count,address ids,address slots,u64 unit_count) on cpu;
