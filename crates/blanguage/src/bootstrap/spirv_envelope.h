module bootstrap.spirv_envelope;
import bootstrap.type_families;
import bootstrap.scalar_fold;

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

enum TypeShape:u32 { Void=0; Vector=1; Matrix=2; SampledImage=3; Array=4; Function=5; Acceleration=6; RayQuery=7; Image=8; Pointer=9; };
struct ResourceBlock { u32 element; u32 length; u32 array; u32 block; u32 pointer; };

// The existing shader supplies result IDs and type operands. Output has room for 9 words.
extern "C" u64 type_declaration(TypeShape shape, u32 id, u32 first, u32 second, address words) on cpu;
// A zero length ID selects the runtime array for the page table.
// Definitions have room for 11 words, and the 12 decoration words are always written.
extern "C" u64 resource_block(ResourceBlock input, address definitions, address decorations) on cpu;

enum StorageClass:u32 {
    UniformConstant=0; Input=1; Workgroup=4; Function=7; PushConstant=9; StorageBuffer=12; PhysicalStorageBuffer=5349;
};
enum VariableAnnotation:u32 { Descriptor=0; PageTable=1; BuiltIn=2; };

// The caller owns four definition words for a variable without an initializer.
extern "C" void variable_declaration(StorageClass storage, u32 pointer, u32 id, address words) on cpu;
// Output has room for four definition words and eleven decoration words.
// Operand is the assigned descriptor binding or the built-in enumerant.
extern "C" u64 annotated_variable(VariableAnnotation annotation, u32 pointer, u32 id, u32 operand, address definitions, address decorations) on cpu;

enum FlagDecoration:u32 { NoContraction=42; NonUniform=5300; };
// Appends one flag to the borrowed decoration words and returns the new end.
extern "C" u64 flag_decoration(address decorations, u64 at, FlagDecoration decoration, u32 id) on cpu;

struct BinaryEmission {
    BinaryOperation operation;
    BuiltinScalar scalar;
    u32 result_type;
    u32 result;
    u32 left;
    u32 right;
    u32 mask;
    u32 masked;
};
struct BinaryWords { u64 body; u64 decorations; };
struct CastEmission {
    BuiltinScalar from;
    BuiltinScalar to;
    u32 result_type;
    u32 result;
    u32 value;
    u32 zero;
    u32 one;
};

// The original emitter owns the IDs and buffers. A masked ID is present for shifts.
// Storage has room for ten body words and three decoration words.
extern "C" BinaryWords scalar_binary(BinaryEmission input, address body, address decorations) on cpu;
// Zero and one are the original constant IDs for a conversion involving bool.
// Storage has room for six body words.
extern "C" u64 scalar_cast(CastEmission input, address body) on cpu;
extern "C" u64 select_words(address body, u64 at, u32 result_type, u32 result,
                            u32 condition, u32 yes, u32 no) on cpu;

enum CollectiveOperation:u32 { Sum=0; Min=1; Max=2; BitAnd=3; BitOr=4; BitXor=5; };
struct CollectivePair {
    CollectiveOperation operation;
    BuiltinScalar scalar;
    u32 raw_type;
    u32 value_type;
    u32 bool_type;
    u32 left;
    u32 right;
    u32 next;
};
struct CollectiveWords { u64 body; u64 decorations; u32 next; u32 value; };

// The inputs and result carry the original unsigned payload bits. The emitter
// supplies the next ID and room for nineteen body and three decoration words.
extern "C" CollectiveWords collective_pair(CollectivePair input, address body, address decorations) on cpu;

struct DivisionEmission {
    u32 integer_type;
    u32 magnitude_type;
    u32 numerator;
    u32 denominator;
    u32 quotient;
    u32 remainder;
    u32 sign_shift;
    u32 next;
};
struct DivisionWords { u32 body; u32 next; };

// A zero result ID denotes the unused quotient or remainder. A zero sign-shift
// ID denotes unsigned inputs. The caller supplies the original next ID and
// holds ten (unsigned) or eighty-one (signed) output words.
extern "C" DivisionWords integer_division(DivisionEmission input, address body) on cpu;

// Operation identities follow the original IR UnaryOp and MathOp.
enum UnaryOperation:u32 {
    FloatNegate=0; SignedNegate=1; UnsignedNegate=2; LogicalNot=3; BitNot=4; Bitcast=5;
};
enum MathOperation:u32 {
    Abs=0; Floor=1; Sin=2; Cos=3; Pow=4; Exp=5; Log=6; Exp2=7; Log2=8; Sqrt=9; Min=10; Max=11;
};
struct FloatSignOperands { u32 raw_type; u32 mask; u32 temporary; };
struct UnaryEmission {
    UnaryOperation operation;
    u32 result_type;
    u32 result;
    u32 value;
    u32 zero;
    FloatSignOperands sign;
};
struct MathEmission {
    MathOperation operation;
    u32 result_type;
    u32 result;
    u32 left;
    u32 right;
    u32 extended_set;
    FloatSignOperands sign;
};
// The emitter supplies consecutive temporary IDs for the sign operation and
// thirteen output words. A zero right operand ID denotes a unary math operation.
extern "C" u64 scalar_unary(UnaryEmission input, address body) on cpu;
extern "C" u64 scalar_math(MathEmission input, address body) on cpu;

struct PageAddress {
    u32 u32_type;
    u32 u64_type;
    u32 descriptor_pointer;
    u32 page_pointer;
    u32 result_pointer;
    u32 descriptor;
    u32 low;
    u32 high;
    u32 stride;
    u32 index;
    u32 word;
    u32 extra;
    u32 zero;
    u32 one;
    u32 high_shift;
    u32 page_shift;
    u32 page_bytes;
    u32 next;
};
struct PageAddressResult { u64 words; u32 next; u32 pointer; u32 second; };
// A descriptor ID selects the dataset header. Otherwise the input contains the
// view page table and stride IDs. The extra byte offset is an optional ID.
// A nonzero step ID adds a second pointer from the same physical address.
// The emitter owns the next ID and up to 118 output words.
extern "C" PageAddressResult page_address(PageAddress input, u32 step, address body) on cpu;

struct RecordCopy {
    PageAddress source;
    u32 target_descriptor;
    u32 unit_type;
    u32 unit_bytes;
    u32 count;
    u32 step;
};

enum PayloadAccess:u32 { Words=0; Local=1; Physical=2; };
struct PayloadEmission {
    PayloadAccess access;
    u32 raw_type;
    u32 unit_type;
    u32 unit_bytes;
    u32 count;
    u32 next;
};
struct PayloadRead { u64 words; u32 next; u32 value; };
struct PayloadWrite { u64 words; u32 next; };
// Operands are word IDs in Words mode and pointer IDs for storage accesses.
// Shifts contains the original constant ID for each nonzero word position.
// The emitter supplies count operand slots and 20*count (read) or 14*count
// (write) body words. Only a Words write uses the output-ID buffer.
extern "C" PayloadRead payload_read(PayloadEmission input, address operands, address shifts, address body) on cpu;
extern "C" PayloadWrite payload_write(PayloadEmission input, u32 value, address pointers, address shifts, address output, address body) on cpu;

struct ControlWords { u64 words; u32 label; u32 reaches_next; };
struct LoopLabels { u32 header; u32 check; u32 run; u32 next; u32 end; };
struct LoopReturn {
    u32 bool_type;
    u32 result;
    u32 target;
    u32 merge;
    u32 not_returned;
    u32 returned;
    u32 edge_count;
};
// The emitter supplies allocated labels and the actual exit status of each
// child block. A zero result label means the fragment opened no new block.
// The return edge slice contains label/returned u32 pairs.
extern "C" ControlWords open_selection(u32 merge, u32 condition, u32 yes, u32 no, address body) on cpu;
extern "C" ControlWords next_control_block(u32 target, u32 label, u32 reaches, address body) on cpu;
extern "C" ControlWords close_selection(u32 merge, u32 yes_reaches, u32 no_reaches, address body) on cpu;
extern "C" ControlWords open_loop(LoopLabels labels, address body) on cpu;
extern "C" ControlWords test_loop(LoopLabels labels, u32 test, address body) on cpu;
extern "C" ControlWords close_loop(LoopLabels labels, u32 reaches, address body) on cpu;
extern "C" ControlWords open_function_scope(u32 merge, u32 selector, u32 begin, address body) on cpu;
extern "C" ControlWords leave_control(u32 target, address body) on cpu;
extern "C" ControlWords dispatch_loop_return(LoopReturn input, address edges, address body) on cpu;

// Operation identities follow the original IR SubgroupOp.
enum SubgroupOperation:u32 {
    Lane=0; Size=1; Id=2; Elect=3; Any=4; All=5; Count=6; PrefixCount=7;
    BroadcastFirst=8; Shuffle=9; ShuffleXor=10; ShuffleUp=11; ShuffleDown=12;
    Sum=13; ExclusiveSum=14; Min=15; Max=16; BitAnd=17; BitOr=18; BitXor=19;
    ExclusiveBitAnd=20; ExclusiveBitOr=21; ExclusiveBitXor=22;
};
struct SubgroupEmission {
    SubgroupOperation operation;
    BuiltinScalar scalar;
    u32 result_type;
    u32 result;
    u32 scope_or_pointer;
    u32 value;
    u32 argument;
    u32 ballot_type;
    u32 next;
};
struct SubgroupWords { u64 words; u32 next; };
// The input carries the resolved built-in pointer or Subgroup scope constant.
// The emitter supplies up to eleven words. Ballot count consumes one temporary ID.
extern "C" SubgroupWords encode_subgroup(SubgroupEmission input, address body) on cpu;

struct TextureSampleEmission {
    u32 variable;
    u32 element;
    u32 index_type;
    u32 sampled_type;
    u32 sampled_pointer;
    u32 coordinate_type;
    u32 vector_type;
    u32 component_type;
    u32 x;
    u32 y;
    u32 z;
    u32 lod;
    u32 next;
};
struct TextureWords { u64 body; u64 decorations; u32 next; };
// Element and z are zero for a single binding and a two-component coordinate
// respectively. The emitter lends four result IDs and room for 46 body and
// nine decoration words.
extern "C" TextureWords encode_texture_sample(TextureSampleEmission input, address results, address body, address decorations) on cpu;

enum AtomicOperation:u32 {
    Load=0; Store=1; Add=2; Sub=3; Exchange=4; CompareExchange=5;
    Min=6; Max=7; And=8; Or=9; Xor=10;
};
struct AtomicEmission {
    AtomicOperation operation;
    BuiltinScalar scalar;
    u32 result_type;
    u32 result;
    u32 pointer;
    u32 scope;
    u32 semantics;
    u32 value;
    u32 compare;
    u32 failure;
};
// The emitter supplies resolved SPIR-V IDs and room for nine words.
// A zero result is the resultless 32-bit store.
extern "C" u64 encode_atomic(AtomicEmission input, address body) on cpu;

struct AppendEmission {
    u32 uint_type;
    u32 bool_type;
    u32 ballot_type;
    u32 device_scope;
    u32 relaxed;
    u32 subgroup_scope;
    u32 active;
    u32 counter;
    u32 push_pointer_type;
    u32 push;
    u32 capacity_word;
    u32 outer;
    u32 accepted;
    u32 next;
};

struct AppendWords {
    u64 words;
    u32 next;
    u32 ticket;
    u32 write_label;
    u32 finish;
};

extern "C" AppendWords encode_append_reservation(AppendEmission input, address body) on cpu;

struct SharedAddress {
    u32 index_type;
    u32 pointer_type;
    u32 variable;
    u32 node;
    u32 scale;
    u32 count;
    u32 next;
};
struct SharedAddressWords { u64 words; u32 next; };

// The input slots carry constant offset IDs, with zero for the first plane.
// The same slots receive the resulting pointers. A zero scale denotes one word.
extern "C" SharedAddressWords shared_addresses(SharedAddress input, address offsets, address body) on cpu;

struct ScanTreeComponent {
    u32 shared;
    u32 pointer_type;
    u32 low;
    u32 high;
    u32 scale;
    u32 shift;
    u32 count;
    CollectiveOperation operation;
    BuiltinScalar scalar;
    u32 raw_type;
    u32 value_type;
};
struct ScanTreeLayer {
    u32 uint_type;
    u32 bool_type;
    u32 lane;
    u32 stride;
    u32 selector;
    u32 tile_capacity;
    u32 remaining;
    u32 one;
    u32 scope;
    u32 semantics;
    u32 down;
    u32 next;
};
struct ScanTreeWords { u64 body; u64 decorations; u32 next; u32 label; };

// Selector is the ordinary lane mask or the tile's doubled stride.
// The host provides 42 + 176*count body words and 3*count decoration words.
extern "C" ScanTreeWords scan_tree_layer(ScanTreeLayer input, address components, u32 count,
    address body, address decorations) on cpu;

struct PrivateAddress {
    u32 index_type;
    u32 pointer_type;
    u32 variable;
    u32 index;
    u32 offset;
    u32 width;
    u32 count;
    u32 next;
};
struct PrivateAddressWords { u64 words; u32 next; };
// A zero index means parts already holds fixed element IDs. Otherwise parts
// holds the relative element IDs after the first one. Zero offset or width
// omits the corresponding arithmetic. The caller owns count pointer slots
// and 10*count+5 body words.
extern "C" PrivateAddressWords private_address(PrivateAddress input, address parts, address pointers, address body) on cpu;

struct RayEmission {
    u32 acceleration_type;
    u32 acceleration;
    u32 query;
    u32 u32_type;
    u32 f32_type;
    u32 bool_type;
    u32 vec2_type;
    u32 vec3_type;
    array<u32,3> origin;
    array<u32,3> direction;
    u32 minimum;
    u32 maximum;
    u32 flags;
    u32 mask;
    u32 committed;
    u32 zero;
    u32 float_zero;
    u32 position_type;
    u32 object_to_world;
    u32 world_to_object;
    u32 closest;
    u32 next;
};
struct RayWords { u64 words; u32 next; u32 label; };
// Results contains the original RayResult IDs (one for Any, 39 for Closest).
// Optional aggregate type IDs are zero when the field group is unused.
// The body has room for 61 words for Any or 587 for full Closest.
extern "C" RayWords encode_ray(RayEmission input, address results, address body) on cpu;

struct TileAddress {
    u32 index_type;
    u32 pointer_type;
    u32 shared;
    u32 index;
    u32 offset;
    u32 bias;
    u32 cell_bytes;
    u32 width;
    u32 base;
    u32 next;
};
struct TileAddressWords { u64 words; u32 next; u32 pointer; u32 byte; };
// The emitter resolves the selected plane and the actual constant IDs.
// Zero offset selects a fixed cell; zero bias or base omits an addition.
// The body has room for thirty words.
extern "C" TileAddressWords tile_address(TileAddress input, address body) on cpu;

struct TileExtract {
    u32 index_type;
    u32 cell_type;
    u32 pointer;
    u32 byte;
    u32 cell_bytes;
    u32 bits_per_byte;
    u32 shift;
    u32 next;
};
// A dynamic byte ID computes the shift. Otherwise the caller supplies a
// fixed shift ID, or zero for the low bits. The body has room for 19 words.
extern "C" PayloadRead tile_extract(TileExtract input, address body) on cpu;

struct DispatchScalars {
    u32 uint_type;
    u32 bool_type;
    u32 zero;
    u32 one;
};

struct DispatchValue {
    u64 words;
    u32 next;
    u32 value;
};

struct DispatchGrid {
    DispatchScalars scalars;
    u32 wide_type;
    u32 table_pointer;
    u32 page_pointer;
    u32 word_pointer;
    u32 descriptor;
    u32 count;
    u32 width;
    u32 max_x;
    u32 max_y;
    u32 levels;
    u32 collective;
    u32 bins;
    u32 next;
};
 
extern "C" DispatchValue encode_dispatch_grid(DispatchGrid input, address offsets, address body) on cpu;

struct ScatterInput {
    PageAddress flag;
    u32 count;
    u32 active;
    u32 bool_type;
    u32 prefix_descriptor;
};

// Record copy is absent for a zero-byte record. Both calls write directly to
// the shader body and return its next ID and final active-merge label.
extern "C" StageWords encode_record_scatter(ScatterInput input, address record_copy,
    u32 count_descriptor, u32 count_word, address body) on cpu;
extern "C" StageWords encode_index_scatter(ScatterInput input, u32 target_descriptor,
    DispatchGrid grid, address offsets, address body) on cpu;

struct SortRankEmission {
    PageAddress source;
    PageAddress totals;
    PageAddress target;
    DispatchScalars scalars;
    u32 histogram;
    u32 count;
    u32 index;
    u32 label;
    u32 wave_input;
    u32 lane_input;
    u32 ballot_type;
    u32 shared;
    u32 shared_pointer;
    u32 shift;
    u32 width;
    u32 wave_width;
    u32 bins;
    u32 mask;
    array<u32,4> bit_masks;
    u32 subgroup_scope;
    u32 workgroup_scope;
    u32 shared_semantics;
    u32 pair_step;
    u32 next;
};
struct StageWords { u64 words; u32 next; u32 label; };
extern "C" StageWords encode_sort_rank(SortRankEmission input, address body) on cpu;

struct SortKeyWrite {
    u32 type_id;
    u32 key;
    u32 sign;
    u32 zero;
    u32 shift;
    u32 bias;
    u32 inverted;
    u32 key_pointer;
    u32 index_pointer;
    u32 original;
    u32 next;
    u32 label;
};

extern "C" StageWords write_sort_key(SortKeyWrite input, address body) on cpu;

extern "C" StageWords encode_sort_gather(PageAddress indices, address layout, u32 active, address body) on cpu;

struct PrefixIndexEmission {
    u32 uint_type;
    u32 wide_type;
    u32 bool_type;
    u32 vector_type;
    u32 global;
    u32 global_x;
    u32 num_groups;
    u32 count_pointer;
    u32 width;
    u32 wide_width;
    u32 zero;
    u32 one;
    u32 collective;
    u32 next;
};

struct PrefixIndexWords {
    u64 words;
    u32 next;
    u32 index;
    u32 count;
    u32 active;
    u32 label;
    u32 groups_x;
    u32 groups_y;
};

extern "C" PrefixIndexWords encode_prefix_index(PrefixIndexEmission input, address body) on cpu;
