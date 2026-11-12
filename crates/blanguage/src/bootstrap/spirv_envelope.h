module bootstrap.spirv_envelope;
import bootstrap.type_families;

struct Features {
    u32 atomic64;
    u32 float64;
    u32 integer8;
    u32 integer16;
    u32 preserve32;
    u32 preserve64;
    u32 subgroup_basic;
    u32 subgroup_vote;
    u32 subgroup_arithmetic;
    u32 subgroup_ballot;
    u32 subgroup_shuffle;
    u32 subgroup_shuffle_relative;
    u32 acceleration;
    u32 position_fetch;
    u32 texture;
    u32 descriptor_indexing;
    u32 maximal_reconvergence;
};
struct Envelope {
    u32 bound;
    u32 ext;
    u32 entry;
    u32 workgroup_width;
    Features features;
};
struct Words { address data; u64 count; };

// Fixed-word capacity excludes the interface IDs. The slices are borrowed in
// the declared interface order. A zero ID is an absent optional variable.
extern "C" u64 envelope_capacity() on cpu;
extern "C" u64 encode(Envelope input, address interfaces, u64 count, address words) on cpu;

// The compiler supplies an existing concrete scalar and already allocated IDs.
// Storage has room for 8 type/pointer words or 5 constant words respectively.
// Constant bits are the original Scalar-truncated cache key.
extern "C" u64 scalar_declaration(BuiltinScalar scalar, u32 type_id, u32 pointer_id, address words) on cpu;
extern "C" u64 constant_declaration(BuiltinScalar scalar, u32 type_id, u32 id, u64 bits, address words) on cpu;
