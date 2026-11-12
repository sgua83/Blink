module bootstrap.coloring;

struct ColorNode { u64 weight; u64 degree; u64 flags; };
struct ColorPlace { u64 value; u32 kind; };

// The original graph and placement storage are borrowed only for this call.
// mode: scalar, tile scalar, or SIMD packet. order has one row per graph node.
extern "C" u32 color(address context, u64 count, address order, u32 mode,
    cfn<ColorNode,address,u64> node,
    cfn<u64,address,u64,bool,u64> next,
    cfn<ColorPlace,address,u64> get,
    cfn<void,address,u64,bool,u32> put) on cpu
    requires nonzero(node) requires nonzero(next) requires nonzero(get) requires nonzero(put);
