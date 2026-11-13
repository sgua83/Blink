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
struct FusionStage {
    StageBindings bindings;
    FusionBlock body;
};

// Both inputs borrow the original checked Map, including its sorted bindings.
// read returns the next nested block or ordered operation from the original IR.
extern "C" bool can_fuse(FusionStage left, FusionStage right,
    cfn<FusionNode,FusionBlock,u64> read) on cpu requires nonzero(read);

// output contains left_count original bindings, with space for right.count more.
// The returned index starts the sorted union in that same output buffer.
extern "C" u64 merge_bindings(address output, u64 left_count, BindingSpan right) on cpu;

// Uniformity belongs to this reduction kernel. The spans borrow its original register indices.
extern "C" bool extract_uniform(BindingSpan operands, BindingSpan results, address uniform) on cpu;
