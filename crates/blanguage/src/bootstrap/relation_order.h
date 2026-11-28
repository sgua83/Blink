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
    address schedule_flags;
    address inputs;
};


struct WalkSlice { address data; usize count; };
struct WalkVisit { usize node; u32 read; };
struct WalkEdge { usize source; usize frame; u32 read; };
struct WalkFrame { usize parent; WalkSlice writes; };
struct WalkWrite { usize target; WalkSlice fields; };

struct RelationSource {
    cfn<WalkSlice,address,usize> edges;
    cfn<WalkEdge,address,usize> edge;
    cfn<WalkFrame,address,usize> frame;
    cfn<WalkWrite,address,usize> write;
    cfn<usize,address,usize> field;
};

struct WalkEdges {
    RelationSource source;
    cfn<u32,address,WalkVisit> mark_frame;
    cfn<void,address,WalkVisit> publish;
};

struct WalkHost {
    WalkEdges edges;
    cfn<WalkVisit,address> pop;
    cfn<u32,address,WalkVisit> visit;
};

extern "C" u32 walk_relation(address graph,address work,usize root,u32 feedback,WalkHost host) on cpu
    requires nonzero(host.edges.source.edges) requires nonzero(host.edges.source.edge)
    requires nonzero(host.edges.source.frame) requires nonzero(host.edges.source.write)
    requires nonzero(host.edges.source.field) requires nonzero(host.edges.mark_frame)
    requires nonzero(host.edges.publish) requires nonzero(host.pop) requires nonzero(host.visit);
struct PlanInputsHost {
    WalkEdges edges;
    cfn<usize,address> next;
    cfn<u32,address,usize> variable;
    cfn<void,address,usize> begin;
    cfn<usize,address,usize> finish;
};

extern "C" usize prepare_plan_inputs(address graph, address work, address roots, address rows,
    u32 materialize, PlanInputsHost host) on cpu
    requires nonzero(host.edges.source.edges) requires nonzero(host.edges.source.edge)
    requires nonzero(host.edges.source.frame) requires nonzero(host.edges.source.write)
    requires nonzero(host.edges.source.field) requires nonzero(host.edges.mark_frame)
    requires nonzero(host.edges.publish) requires nonzero(host.next) requires nonzero(host.variable)
    requires nonzero(host.begin) requires nonzero(host.finish);

struct OrderCount {
    usize completed;
    usize stable;
};

extern "C" OrderCount plan(Input input, Workspace work, Output output) on cpu;

struct CallRange { usize first; usize end; };
struct CallGraph { address rows; address callees; usize count; };
struct CallWork { address states; address visits; address active; };

extern "C" void recursive_call_groups(CallGraph graph, CallWork work, address groups) on cpu;

struct IdentityFeedbackHost {
    cfn<usize,address,usize> edge;
    cfn<usize,address,usize> next;
    cfn<void,address,usize,usize> exchange;
};

extern "C" usize prune_identity_feedback(address graph,address edges,usize count,usize variable,IdentityFeedbackHost host) on cpu
    requires nonzero(host.edge) requires nonzero(host.next) requires nonzero(host.exchange);
extern "C" usize resolve_identity_binding(address context, usize root, cfn<usize,address,usize> next) on cpu
    requires nonzero(next);

struct RoundSchedule {
    address ordered;
    usize count;
    usize stable;
    address schedule_flags;
    WalkSlice inputs;
};

struct RoundHost {
    cfn<void,address,usize,u32> evaluate;
    cfn<void,address> observe;
    cfn<u32,address,usize,u32> equivalent;
    cfn<void,address,usize> forget;
};

extern "C" u32 solve_round(address context, RoundSchedule schedule, u32 initial, RoundHost host) on cpu
    requires nonzero(host.evaluate) requires nonzero(host.observe)
    requires nonzero(host.equivalent) requires nonzero(host.forget);

struct RelationSelection {
    WalkSlice nodes;
    WalkSlice frames;
    usize root;
};

struct RelationOutput {
    cfn<address,address,usize,usize,usize> node;
    cfn<void,address,address,usize,usize,usize,address> edge;
    cfn<address,address,usize,usize> frame;
    cfn<address,address,address,usize,usize> write;
    cfn<void,address,address,usize,usize> field;
};

extern "C" usize compact_index(RelationSelection selection, usize id) on cpu;

extern "C" void compact(address graph, address destination, RelationSelection selection, RelationSource source, RelationOutput output) on cpu
    requires nonzero(source.edges) requires nonzero(source.edge)
    requires nonzero(source.frame) requires nonzero(source.write) requires nonzero(source.field)
    requires nonzero(output.node) requires nonzero(output.edge)
    requires nonzero(output.frame) requires nonzero(output.write) requires nonzero(output.field);

struct ResidualOutput {
    cfn<u32,address,usize> node_facts;
    cfn<void,address,usize,u32,u32> node;
    cfn<u32,address,usize> keep;
    cfn<void,address,usize,usize,u32> write;
    cfn<void,address,usize> field;
};

extern "C" void project_return(address graph, address context, usize nodes, address frame_marks,
    usize frame_count, address schedule_flags, RelationSource source, ResidualOutput output) on cpu
    requires nonzero(source.edges) requires nonzero(source.edge) requires nonzero(source.frame)
    requires nonzero(source.write) requires nonzero(source.field)
    requires nonzero(output.node_facts) requires nonzero(output.node) requires nonzero(output.keep)
    requires nonzero(output.write) requires nonzero(output.field);

struct InitialDefinitionHost {
    cfn<u32,address,usize> initial;
    cfn<void,address,usize> begin;
    cfn<void,address,usize> edge;
};

extern "C" u32 expand_initial_definition(address context, usize count, InitialDefinitionHost host) on cpu
    requires nonzero(host.initial) requires nonzero(host.begin) requires nonzero(host.edge);

extern "C" u32 replay_writes(address graph, address context, usize frame,
    cfn<WalkFrame,address,usize> frame_at, cfn<u32,address,address,usize> write) on cpu
    requires nonzero(frame_at) requires nonzero(write);

struct CarriedScope {
    usize first;
    usize end;
    address backedge;
};

struct CarriedWriteHost {
    cfn<usize,address> next_scope;
    cfn<CarriedScope,address,usize> history;
    cfn<u32,address,usize> memory_facts;
    cfn<u32,address,address,usize,usize,address> consider;
};

extern "C" u32 visit_carried_writes(address function, address scopes, address consumer, CarriedWriteHost host) on cpu
    requires nonzero(host.next_scope) requires nonzero(host.history)
    requires nonzero(host.memory_facts) requires nonzero(host.consider);

struct HistoryRegisters {
    usize first;
    usize end;
};

struct HistoryBindingsHost {
    cfn<usize,address> next_scope;
    cfn<HistoryRegisters,address,usize> registers;
    cfn<void,address,usize,usize> insert;
};

extern "C" void write_history_bindings(address function, address output, address source, address active,
    usize prior, HistoryBindingsHost host) on cpu
    requires nonzero(host.next_scope) requires nonzero(host.registers) requires nonzero(host.insert);

struct MemoryCallFacts {
    usize memory;
    u32 deferred;
};

extern "C" usize find_memory_call(WalkSlice calls, usize memory, u32 immediate,
    cfn<MemoryCallFacts,address,usize> facts) on cpu
    requires nonzero(facts);

struct MemoryEventCursor {
    usize next_use;
    usize next_call;
    usize call_memory;
};

struct MemoryUseFacts {
    usize target;
    u32 kind;
};

struct MemoryEventHost {
    cfn<MemoryUseFacts,address,usize> usage;
    cfn<MemoryCallFacts,address,usize> call;
};

extern "C" u32 any_call_memory_write(
    WalkSlice uses, usize first, u32 related, address test,
    cfn<MemoryUseFacts,address,usize> usage, cfn<u32,address,usize> overlaps
) on cpu requires nonzero(usage) requires nonzero(overlaps);

extern "C" usize next_memory_event(WalkSlice uses, WalkSlice calls, address cursor, MemoryEventHost host) on cpu
    requires nonzero(host.usage) requires nonzero(host.call);

struct StoredUse {
    address path;
    address direct;
    address fields;
};

struct StoredLocationHost {
    cfn<StoredUse,address,usize> usage;
    cfn<MemoryCallFacts,address,usize> call;
    cfn<void,address,StoredUse,usize> project;
};

extern "C" void visit_stored_locations(WalkSlice uses, WalkSlice calls, address context, StoredLocationHost host) on cpu
    requires nonzero(host.usage) requires nonzero(host.call) requires nonzero(host.project);

extern "C" void visit_content_history(WalkSlice uses, WalkSlice calls, usize limit,
    address context, MemoryEventHost host, cfn<void,address,usize,usize> project) on cpu
    requires nonzero(host.usage) requires nonzero(host.call) requires nonzero(project);

extern "C" void collect_stored_returns(WalkSlice uses, address context,
    cfn<u32,address,usize> marked, cfn<void,address,usize> accept) on cpu
    requires nonzero(marked) requires nonzero(accept);

extern "C" usize latest_multiple_stored_return(WalkSlice uses,
    cfn<u32,address,usize> marked) on cpu requires nonzero(marked);

struct ExitEvent {
    usize at;
    usize end;
};

extern "C" void visit_exit_writes(WalkSlice events, usize memory, address context,
    cfn<void,address,usize,usize,usize> visit) on cpu requires nonzero(visit);

enum CaptureIndex : u32 { Ineligible=0; Whole=1; Concrete=2; Element=3; };

struct CaptureHost {
    cfn<CaptureIndex,address,usize> candidate;
    cfn<u32,address,usize> same_index;
    cfn<void,address,usize> accept;
};

extern "C" void collect_address_captures(usize count, u32 indexed, address context, CaptureHost host) on cpu
    requires nonzero(host.candidate) requires nonzero(host.same_index) requires nonzero(host.accept);

struct PrefixBoundary {
    usize memory;
    usize frame;
};

struct PrefixState {
    usize through;
    WalkSlice runs;
};

struct PrefixHost {
    cfn<PrefixState,address> state;
    cfn<usize,address,usize,usize> emit;
    cfn<void,address,usize,PrefixBoundary,u32,usize> publish;
};

extern "C" usize lexical_frame(address context, usize memory, PrefixHost host) on cpu
    requires nonzero(host.state) requires nonzero(host.emit) requires nonzero(host.publish);

extern "C" void label_lexical_frames(WalkSlice runs, address frames, usize count) on cpu;

struct RelationOffsets {
    usize nodes;
    usize frames;
    usize prefix;
};

extern "C" usize import_relation(address graph, address destination, usize root, usize nodes, usize frames,
    RelationOffsets offsets, address entry, RelationSource source, RelationOutput output, cfn<void,address,usize> bind_inputs) on cpu
    requires nonzero(source.edges) requires nonzero(source.edge)
    requires nonzero(source.frame) requires nonzero(source.write) requires nonzero(source.field)
    requires nonzero(output.node) requires nonzero(output.edge)
    requires nonzero(output.frame) requires nonzero(output.write) requires nonzero(output.field)
    requires nonzero(bind_inputs);

struct RelationValueOps {
    cfn<usize,address,usize> node;
    cfn<void,address,usize,usize> edge;
    cfn<usize,address,usize> frame;
    cfn<void,address,usize,usize> write;
};

extern "C" void bind_relation_values(address context, usize nodes, usize frames, RelationValueOps ops) on cpu
    requires nonzero(ops.node) requires nonzero(ops.edge)
    requires nonzero(ops.frame) requires nonzero(ops.write);

extern "C" void expand_component_ports(address context, cfn<usize,address> count,
    cfn<void,address,usize> visit) on cpu
    requires nonzero(count) requires nonzero(visit);

extern "C" void visit_relation_calls(address context, usize count, cfn<void,address,usize> visit) on cpu
    requires nonzero(visit);

struct ReadWritePaths { address written; address unwritten; };

extern "C" address fold_read_write(address remaining, ReadWritePaths paths, address context,
    cfn<u32,address> conflict, cfn<void,address,address> accept) on cpu
    requires nonzero(conflict) requires nonzero(accept);

extern "C" address select_read_path(address left, address right, u32 left_proven, cfn<u32,address> conflict,
    cfn<u32,address,u32> required) on cpu
    requires nonzero(conflict) requires nonzero(required);
