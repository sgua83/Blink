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

struct BindingUse { u64 binding; u8 access; };
struct MapAccess {
    address uses;
    u64 use_count;
    address flags;
    u64 binding_count;
    address reads;
    address writes;
    address cross;
    u64 cross_count;
};
struct MapAccessCounts { u64 reads; u64 writes; u64 cross; };

// The caller owns zeroed flags and one output slot per program binding.
// Cross-element bindings are filtered in place.
extern "C" MapAccessCounts map_access(MapAccess input) on cpu;
