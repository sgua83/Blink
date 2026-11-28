module bootstrap.ownership_position;
import bootstrap.fact_key;
enum PresenceTag:u32 { None=0; All=1; Input=2; Selected=3; Not=4; Both=5; Either=6; };
struct PositionIndex { u32 kind; u32 value; address identity; u64 reg; };
struct PositionView {
    usize payload_offset;
    usize stride;
    usize index_payload_offset;
    usize index_register_offset;
    usize index_width;
    usize selection_payload_offset;
    usize selection_width;
};
extern "C" PositionIndex read_index(address value, PositionView view) on cpu;
extern "C" void write_index(address output, PositionIndex value, PositionView view) on cpu;
extern "C" i32 compare_position_index(PositionIndex a, PositionIndex b) on cpu;
struct PositionNode { u32 kind; u64 owner; u64 member; u64 word; PositionIndex index; address condition; u64 count; };
extern "C" PositionNode inspect_position(address value, PositionView view) on cpu;
extern "C" address position_child(address value, u64 at, PositionView view) on cpu;

struct PositionItem { address node; u32 present; };
struct LoopFieldState { address known; address path; address effects; };
// Every address is borrowed for this synchronous model operation or belongs to
// the one actual output container. No node directory or second tree is stored.
struct Host {
    PositionView positions;
    cfn<u32,address,address> equal;
    cfn<u32,address,address> presence_equal;
    cfn<u32,address,PositionIndex> presence_uses_index;
    cfn<u32,address,address> contains;
    cfn<u32,address,u64,u64> contains_parameter;
    cfn<u32,address,u64,PositionIndex,u32> contains_element;
    cfn<u32,address,u64,u64,PositionIndex,u32> contains_input;
    cfn<u32,LoopFieldState,PositionNode> field_coverage;
    cfn<address,address,address> iterate;
    cfn<PositionItem,address> next;
    cfn<void,address> drop_iterator;
    cfn<address> fresh_vector;
    cfn<void,address,address> push_vector;
    cfn<u64,address> vector_count;
    cfn<address,address,u64> vector_at;
    cfn<address,PositionNode,address,address,address> make_position;
    cfn<address,address> clone_position;
    cfn<address> fresh_set;
    cfn<void,address,address> insert_set;
};

extern "C" u32 select(address when,address a,address b,cfn<u32,address,address> equal) on cpu
    requires nonzero(equal);
extern "C" u32 both(address a,address b,cfn<u32,address,address> equal) on cpu
    requires nonzero(equal);
extern "C" address index(address values,PositionIndex selected,u64 width,Host host) on cpu
    requires nonzero(host.fresh_vector)
    requires nonzero(host.push_vector)
    requires nonzero(host.vector_count)
    requires nonzero(host.vector_at)
    requires nonzero(host.make_position)
    requires nonzero(host.clone_position);
extern "C" address word(address value,PositionIndex selected,u32 field,Host host) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.presence_equal)
    requires nonzero(host.fresh_vector)
    requires nonzero(host.push_vector)
    requires nonzero(host.make_position)
    requires nonzero(host.clone_position);
struct SharedPositionSpan { address positions; usize count; };
enum SharedNodeKind:u32 { Position=0; Presence=1; };
extern "C" address allocate_shared_positions(usize bytes) on cpu;
extern "C" void retain_shared_positions(address positions) on cpu;
extern "C" u32 take_unique_shared_position(address value, address output, usize width) on cpu;
extern "C" void release_shared_positions(SharedPositionSpan positions, address layout,
    cfn<void,address,SharedNodeKind> drop_payload) on cpu requires nonzero(drop_payload);
extern "C" void release_shared_presence(address value, address layout,
    cfn<void,address,SharedNodeKind> drop_payload) on cpu requires nonzero(drop_payload);

struct PositionFrames { address data; usize capacity; };
address position_frame(address data, usize index, usize width) on cpu;
address reserve_position_frame(inout PositionFrames frames, usize depth, usize width) on cpu;
void release_position_frames(PositionFrames frames) on cpu;

struct PresenceView {
    usize payload_offset;
    usize first_index_offset;
    usize ownership_payload_offset;
    address positions;
    usize width;
};
extern "C" address publish_presence(address value, address layout) on cpu;

struct Selection { u32 operation; PositionIndex a; PositionIndex b; };
struct PresenceBranches { address first; address second; };
struct SavedIdentities { IdentityKey a; IdentityKey b; };
extern "C" SavedIdentities selection_storage() on cpu;
extern "C" Selection save_selection(Selection value, address a, address b) on cpu;
extern "C" void write_comparison(address output, Selection value, address layout,
    cfn<void,address,SharedNodeKind> drop_payload) on cpu requires nonzero(drop_payload);
extern "C" void write_truth(address output, u32 value, address layout,
    cfn<void,address,SharedNodeKind> drop_payload) on cpu requires nonzero(drop_payload);
extern "C" void move_presence(address output, address source, address layout,
    cfn<void,address,SharedNodeKind> drop_payload) on cpu requires nonzero(drop_payload);
extern "C" void pair_presence(address left, address right, u32 any, address layout) on cpu;
extern "C" void wrap_not_presence(address value, address layout) on cpu;
extern "C" void conditional_ownership(address output, address value, address layout,
    cfn<void,address,SharedNodeKind> drop_payload) on cpu requires nonzero(drop_payload);
extern "C" void write_ownership_constant(address output, u32 kind, address layout,
    cfn<void,address,SharedNodeKind> drop_payload) on cpu requires nonzero(drop_payload);

struct DatasetPositionCleanup {
    cfn<void,address> position;
    cfn<void,address> vector;
};
extern "C" address dataset_position(address value, PositionIndex selected, Host host,
    DatasetPositionCleanup cleanup) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.fresh_vector) requires nonzero(host.push_vector)
    requires nonzero(host.make_position) requires nonzero(host.clone_position)
    requires nonzero(cleanup.position) requires nonzero(cleanup.vector);
enum DatasetLeafAction:u32 { Unchanged=0; Rejected=1; Rewrite=2; Materialized=3; };
struct DatasetMappedLeaf { PositionNode shape; address position; DatasetLeafAction action; };
struct DatasetOutputMapping {
    address context;
    u64 scope;
    cfn<DatasetMappedLeaf,address,PositionNode> field;
};
extern "C" address dataset_output_position(address value, DatasetOutputMapping output, Host host,
    DatasetPositionCleanup cleanup) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.fresh_vector) requires nonzero(host.push_vector)
    requires nonzero(host.make_position) requires nonzero(host.clone_position)
    requires nonzero(cleanup.position) requires nonzero(cleanup.vector)
    requires nonzero(output.field);

extern "C" u32 ready(address value,address known,Host host) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.field_coverage)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator);
enum InitializationReadMode:u32 { Complete=0; Borrow=1; };
struct InitializationReadHost {
    cfn<u32,address> input;
    cfn<void,address,address> emit;
};
struct InitializationReadSpan { address positions; usize count; usize stride; };
extern "C" u32 initialization_read(InitializationReadSpan positions, address known, address output,
    InitializationReadMode mode, Host host, InitializationReadHost read) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains) requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element) requires nonzero(host.contains_input)
    requires nonzero(host.field_coverage) requires nonzero(host.iterate)
    requires nonzero(host.next) requires nonzero(host.drop_iterator)
    requires nonzero(read.input) requires nonzero(read.emit);

extern "C" u32 word_ready(address value,PositionIndex selected,u32 field,address known,Host host) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.field_coverage)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator);
extern "C" u32 prefix_ready(address value,u32 length,u32 field,address known,Host host) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.field_coverage)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator);
extern "C" u32 uses_scope(address value,u64 owner,Host host) on cpu
    ;
struct InitializationHost {
    cfn<address,address,usize> positions;
    cfn<void,address,usize,address> retain_conditional;
    cfn<void,address,usize,address> append_conditional;
    cfn<void,address> drop_positions;
};
extern "C" address merge_initialization(address exits, usize count, usize condition_count,
    Host host, InitializationHost initialization) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.field_coverage)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.fresh_set)
    requires nonzero(host.insert_set)
    requires nonzero(initialization.positions)
    requires nonzero(initialization.retain_conditional)
    requires nonzero(initialization.append_conditional)
    requires nonzero(initialization.drop_positions);

struct LoopFieldRow { address position; address words; usize count; };
struct LoopFieldHost {
    cfn<LoopFieldRow,address,usize> ensures memory(read) row;
    cfn<LoopFieldState,address,usize> ensures memory(read) edge;
    cfn<void,address,address> publish;
};
extern "C" void loop_field_initialization(address context, address output, usize rows, usize states,
    PositionIndex selected, Host host, LoopFieldHost exit) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.field_coverage)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.fresh_vector)
    requires nonzero(host.push_vector)
    requires nonzero(host.make_position)
    requires nonzero(host.clone_position)
    requires nonzero(exit.row)
    requires nonzero(exit.edge)
    requires nonzero(exit.publish);

struct ArrayRange { u32 begin; u32 limit; u32 nonempty; };
struct ArrayState { address known; u32 ready; };
struct ArrayExitHost {
    cfn<PositionItem,address> next;
    cfn<ArrayState,address,usize,address> ensures memory(read) state;
};
extern "C" void array_initialization(address context, address output, usize count, ArrayRange range,
    PositionIndex selected, Host host, ArrayExitHost exits) on cpu
    requires nonzero(host.equal)
    requires nonzero(host.presence_equal)
    requires nonzero(host.presence_uses_index)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.field_coverage)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(exits.next)
    requires nonzero(exits.state)
    requires nonzero(host.insert_set);


struct InitializationFilter { u32 kind; u32 indexed; u64 owner; u64 member; u64 word; };
struct InitializationEffectsHost {
    cfn<void,address,PositionNode,u64> enter;
    cfn<void,address> leave;
    cfn<u32,address,address,PositionIndex> effect;
};

extern "C" u32 visit_initialization_effects(address initialized, address context,
    InitializationFilter filter, Host host, InitializationEffectsHost sink) on cpu
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(sink.enter)
    requires nonzero(sink.leave)
    requires nonzero(sink.effect);
extern "C" u32 visit_initialization_position(address position, address context,
    InitializationFilter filter, Host host, InitializationEffectsHost sink) on cpu
    requires nonzero(sink.enter)
    requires nonzero(sink.leave)
    requires nonzero(sink.effect);



struct ResolutionBase {
    u64 choices;
    u64 results;
};
struct ResolutionPair {
    address left;
    address right;
};
struct ResolutionHost {
    PositionView positions;
    cfn<u32,address,address> equal;
    cfn<u32,address,address> truth;
    cfn<void,address,address,u32> assume;
    cfn<void,address> leave;
    cfn<ResolutionBase,address> mark;
    cfn<u32,address,PositionIndex,u64> query_choice;
    cfn<u32,address,u64> assume_choice;
    cfn<void,address,u64> release_choices;
    cfn<void,address,address> emit;
    cfn<ResolutionPair,address> pair;
    cfn<void,address,address,u32> finish_pair;
    cfn<address,address,u64> result;
    cfn<void,address,PositionIndex,u64,u32> finish_indexed;
};
extern "C" void resolve_position(address value,address context,ResolutionHost resolver) on cpu
    requires nonzero(resolver.equal)
    requires nonzero(resolver.truth)
    requires nonzero(resolver.assume)
    requires nonzero(resolver.leave)
    requires nonzero(resolver.mark)
    requires nonzero(resolver.query_choice)
    requires nonzero(resolver.assume_choice)
    requires nonzero(resolver.release_choices)
    requires nonzero(resolver.emit)
    requires nonzero(resolver.pair)
    requires nonzero(resolver.finish_pair)
    requires nonzero(resolver.result)
    requires nonzero(resolver.finish_indexed);
