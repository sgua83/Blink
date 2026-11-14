module bootstrap.stage_fusion;

struct BindingSpan {
    address data;
    u64 count;
};
struct StageBindings {
    BindingSpan reads;
    BindingSpan writes;
    BindingSpan cross;
};

struct FusionBlock {
    address instructions;
    u64 count;
};
struct FusionNode {
    FusionBlock first;
    FusionBlock second;
    u64 next;
    bool ordered;
};
struct DomainField {
    u64 binding;
    u32 word;
    u32 present;
};
struct FusionStage {
    StageBindings bindings;
    FusionBlock body;
    u64 domain;
    DomainField prefix;
    DomainField selection;
    u32 map;
    u32 boundary_before;
};

// Both inputs borrow the original checked stage, including its sorted bindings.
// read returns the next nested block or ordered operation from the original IR.
extern "C" bool can_fuse(FusionStage left, FusionStage right,
    cfn<FusionNode,FusionBlock,u64> read) on cpu requires nonzero(read);

// output contains left_count original bindings, with space for right.count more.
// The returned index starts the sorted union in that same output buffer.
extern "C" u64 merge_bindings(address output, u64 left_count, BindingSpan right) on cpu;

// Uniformity belongs to this reduction kernel. The spans borrow its original register indices.
extern "C" bool extract_uniform(BindingSpan operands, BindingSpan results, address uniform) on cpu;

// Each original slot contains zero when unused, one when used. Replace the
// used marks with one-based dense indices in original order.
extern "C" void number_used_slots(address slots, u64 count) on cpu;

enum SelectionKind:u32 { Pure=0; LocalRead=1; LocalWrite=2; DataRead=3; Arithmetic=4; Ordered=5; };
struct SelectionNode { u64 first; u64 second; SelectionKind kind; };
struct SelectionValue { u64 slot; u64 destination; u64 yes; u64 no; };
struct SelectionSlot { u64 yes; u64 no; };
struct SelectionPlan {
    FusionBlock yes;
    FusionBlock no;
    FusionBlock following;
    u64 domain;
    address indices;
    address slots;
    address values;
};

// Both scratch buffers are owned by the caller. slots begins zeroed. values
// has space for min(yes.count, no.count, following.count) rows.
extern "C" u64 plan_selection(SelectionPlan input, address context,
    cfn<SelectionNode,address,FusionBlock,u64> read) on cpu requires nonzero(read);

struct MapAccess {
    address flags;
    u64 binding_count;
    address reads;
    address writes;
    address cross;
    u64 cross_count;
};
struct MapAccessCounts { u64 reads; u64 writes; u64 cross; };

// The caller supplies the accumulated flags and one output slot per program binding.
// Cross-element bindings are filtered in place.
extern "C" MapAccessCounts map_access(MapAccess input) on cpu;

struct TilePlacement {
    i64 begin;
    i64 end;
    u64 width;
    u64 plane;
    u64 slot;
};

// Rows are in the original lifetime order. Each scratch plane has capacity
// for the sum of its row widths. Only assigned slots have been initialized.
extern "C" void place_tile(address rows, u64 count, address narrow, address wide) on cpu;

struct StageWrite {
    u64 binding;
    u32 start;
    u32 end;
};
struct StageGuarantee {
    u64 binding;
    u64 start;
    u64 end;
    u64 retained;
};

// Writes are ordered by binding, then by start and end. B coalesces them in
// this temporary buffer and marks the original guarantee order in place.
extern "C" void retain_stage_guarantees(address writes, u64 write_count,
    address guarantees, u64 guarantee_count) on cpu;

struct LocalValue { u64 slot; u64 value; };

// Both inputs are sorted by slot. The left buffer receives their identical
// slot/value pairs in slot order and the return value is its new length.
extern "C" u64 intersect_local_values(address left, u64 left_count,
    address right, u64 right_count) on cpu;

// The original IR and PrivateSlot sets stay in their native owning containers.
// Every borrowed block and temporary clone ends with this synchronous query.
enum LiveKind:u32 { Ordinary=0; If=1; Loop=2; Scope=3; Exit=4; Break=5; Continue=6; LocalRead=7; LocalWrite=8; };
enum LiveSetAction:u32 { Copy=0; Union=1; InsertRegister=2; RemoveRegister=3; InsertLocal=4; RemoveLocal=5; Swap=6; };
enum LiveReferenceKind:u32 { Result=0; Operand=1; LocalRead=2; };
struct LiveNode {
    FusionBlock first;
    FusionBlock second;
    address instruction;
    u64 first_slot;
    u64 second_slot;
    LiveKind kind;
};
struct LiveHost {
    cfn<LiveNode,FusionBlock,u64> read_node;
    cfn<void,address,address> references;
    cfn<address,address> clone_set;
    cfn<void,address> drop_set;
    cfn<void,LiveSetAction,address,address,u64> change;
};

extern "C" void private_before(FusionBlock code, address live, address break_live,
    address continue_live, address exit_live, address empty, LiveHost host) on cpu
    requires nonzero(host.read_node) requires nonzero(host.references)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change);
extern "C" void private_loop(FusionBlock condition, u64 test, FusionBlock body,
    address after, address break_live, address continue_live, address exit_live,
    address result, address empty, LiveHost host) on cpu
    requires nonzero(host.read_node) requires nonzero(host.references)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change);
// The native IR owner enumerates its actual references without making a second
// operand list. B chooses each gen/kill action in this fixed continuation.
extern "C" void private_reference(LiveReferenceKind kind, address live, u64 slot,
    cfn<void,LiveSetAction,address,address,u64> change) on cpu requires nonzero(change);

enum DceKind:u32 { Ordinary=0; Scope=1; If=2; Loop=3; LocalWrite=4; IndexedWrite=5; Division=6; Collective=7; };
enum DceRemovalKind:u32 { Values=0; Division=1; DirectWrite=2; IndexedWrite=3; };
enum DceDivisionChoice:u32 { Quotient=1; Remainder=2; Both=3; };
struct DceNode {
    address instruction;
    FusionBlock first;
    FusionBlock second;
    address first_code;
    address second_code;
    address payload;
    u64 first_slot;
    u64 second_slot;
    u64 index;
    u64 word;
    u64 value;
    u64 quotient;
    u64 remainder;
    DceKind kind;
    u32 flags; // exempt results=1, observable effect=2, quotient=4, remainder=8
};
struct DceResults { u32 present; u32 needed; };
struct DceRemoval { DceRemovalKind kind; u64 value; u64 index; u64 length; u64 word; address division; };
struct DceHost {
    cfn<DceNode,FusionBlock,u64> read_node;
    cfn<FusionBlock,address> read_block;
    cfn<DceResults,address,address> results;
    cfn<u32,address,u32,u64> contains;
    cfn<void,LiveSetAction,address,address,u64> change;
    cfn<address,address> clone_set;
    cfn<void,address> drop_set;
    cfn<void,address,address,address,address,address,address> eliminate_block;
    cfn<void,FusionBlock,address,address,address,address> live_before;
    cfn<address,FusionBlock,u64,FusionBlock,address,address,address,address> loop_live;
    cfn<void,address,address,DceRemoval> record;
    cfn<void,address,address,DceDivisionChoice,u64,u64> prune_division;
    cfn<void,address,address> prune_collective;
};

// The IR owner enumerates actual results and moves retained instructions.
// B owns every elimination and propagation decision during that borrow.
extern "C" u32 dce_result(inout DceResults results,address needed,u64 reg,
    cfn<u32,address,u32,u64> contains) on cpu requires nonzero(contains);
extern "C" u32 dce_result_needed(address needed,u64 reg,
    cfn<u32,address,u32,u64> contains) on cpu requires nonzero(contains);
extern "C" bool eliminate_instruction(address instruction,address needed,
    address break_live,address continue_live,address exit_live,address empty,
    address context,DceHost host) on cpu
    requires nonzero(host.read_node) requires nonzero(host.read_block)
    requires nonzero(host.results) requires nonzero(host.contains) requires nonzero(host.change)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.eliminate_block)
    requires nonzero(host.live_before) requires nonzero(host.loop_live) requires nonzero(host.record)
    requires nonzero(host.prune_division) requires nonzero(host.prune_collective);
