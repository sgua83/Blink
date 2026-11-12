module bootstrap.coloring;

struct ColorNode { u64 weight; u64 degree; u64 across_call; };
struct ColorPlace { u64 value; u32 kind; u8 width; u8 floating; };

// The original graph is borrowed for this call. places is the final location
// array, and order has one scratch row per node. A zero width denotes a
// separately laid out addressable declaration.
extern "C" u64 color(address graph, u64 count, address places, address order, u32 mode, u64 base,
    cfn<ColorNode,address,u64> node,
    cfn<u64,address,u64,bool,u64> next) on cpu
    requires nonzero(node) requires nonzero(next);
