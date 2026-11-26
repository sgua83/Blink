module bootstrap.spirv_envelope;
import bootstrap.ir_dataflow;
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
    u32 prepare;
    Features features;
};
struct Words { address data; u64 count; };

// Fixed-word capacity excludes the interface IDs. The slices are borrowed in
// the declared interface order. A zero ID is an absent optional variable.
extern "C" u64 envelope_capacity() on cpu;
extern "C" u64 encode(Envelope input, address interfaces, u64 count, address words) on cpu;

// The compiler supplies an existing concrete scalar and already allocated IDs.
// Storage has room for 8 type/pointer words or 5 constant words respectively.
// The caller supplies space for the void type and scalar declarations,
// and the original scalar-code indexed type and pointer slots.
struct ScalarDeclarations { u64 words; u32 void_id; u32 next; };
extern "C" ScalarDeclarations shader_scalars(u32 next, u32 float64, u32 integer8, u32 integer16,
    address definitions, address types, address pointers) on cpu;

struct ShaderConstant { BuiltinScalar scalar; u32 type_id; u32 next; u64 bits; };
struct ConstantWords { u64 words; u32 id; u32 next; };
struct ShaderConstants { address context; cfn<u32,address,BuiltinScalar,u64,u32> intern; };
extern "C" ConstantWords shader_constant(ShaderConstant input, ShaderConstants constants,
    address definitions, cfn<address,address> reserve) on cpu requires nonzero(constants.intern) requires nonzero(reserve);

enum TypeShape:u32 { Void=0; Vector=1; Matrix=2; SampledImage=3; Array=4; Function=5; Acceleration=6; RayQuery=7; Image=8; Pointer=9; };

// The existing shader supplies result IDs and type operands. Output has room for 9 words.
extern "C" u64 type_declaration(TypeShape shape, u32 id, u32 first, u32 second, address words) on cpu;
// A zero length ID selects the runtime array for the page table.
struct ShaderResourceInput {
    usize buffers;
    usize slots;
    usize indirect;
    usize textures;
    usize accelerations;
    u32 next;
    u32 float_type;
    u32 word_type;
    u32 wide_type;
    u32 byte_type;
    u32 short_type;
    u32 uniform_bytes;
    u32 publishes_summary;
    u32 single_group;
    u32 argument_pass;
    u32 grid;
};
struct ResourceStreams {
    address definitions;
    address decorations;
    address pages;
    address textures;
    address accelerations;
};
struct TextureDeclaration { u32 dimensions; u32 arrayed; u32 length; };
struct ShaderResourceHost {
    address textures;
    cfn<TextureDeclaration,address,usize> texture;
    ShaderConstants words;
};
struct SampledType { u32 sampled; u32 pointer; };
struct ShaderResourceIds {
    u64 definitions;
    u64 decorations;
    u32 next;
    u32 vec3;
    u32 vec2;
    u32 vec4;
    u32 acceleration;
    u32 ray_pointer;
    u32 index_type;
    u32 global;
    u32 num_groups;
    u32 words32;
    u32 words64;
    u32 bytes;
    u32 shorts;
    u32 table_pointer;
    u32 push;
    u32 push_pointer;
    SampledType sampled2;
    SampledType sampled3;
};
// Borrow the original texture list and constant pool. Initialize every interface slot
// and write declarations into the emitter's definition and decoration buffers.
extern "C" ShaderResourceIds shader_resources(ShaderResourceInput input, ShaderResourceHost host, ResourceStreams streams) on cpu
    requires nonzero(host.texture) requires nonzero(host.words.intern);

enum StorageClass:u32 {
    UniformConstant=0; Input=1; Workgroup=4; Function=7; PushConstant=9; StorageBuffer=12; PhysicalStorageBuffer=5349;
};
enum VariableAnnotation:u32 { Descriptor=0; PageTable=1; BuiltIn=2; };

struct FunctionArray { usize start; usize end; usize fields; address shape; u32 bytes; };
struct FunctionLocals { usize start; usize fields; address shape; };
struct FunctionStorageInput {
    usize rays; u32 next; u32 ray_pointer; u32 word_type; u32 byte_type; u32 short_type;
};
struct FunctionStorageWords { u64 definitions; u64 body; u32 next; };
struct FunctionStorageHost {
    address context;
    cfn<FunctionArray,address> array;
    cfn<FunctionLocals,address> scalar_group;
    cfn<u32,address,usize> width;
    cfn<u32,address,address,usize> pointer;
    cfn<void,address,FunctionArray,u32,u32> retain_array;
};
// The locals, definitions and body are the original emitter buffers.
extern "C" FunctionStorageWords function_storage(FunctionStorageInput input, FunctionStorageHost host,
    ShaderConstants constants, address locals, address definitions, address body) on cpu
    requires nonzero(host.array) requires nonzero(host.scalar_group) requires nonzero(host.width)
    requires nonzero(host.pointer) requires nonzero(host.retain_array) requires nonzero(constants.intern);
// Output has room for four definition words and eleven decoration words.

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
};
struct ScalarWords { u64 body; u64 decorations; };
struct ScalarStreams { address body; address decorations; };
struct ScalarHost {
    address context;
    cfn<u32,address,BuiltinScalar,u64> constant;
    cfn<u32,address,BuiltinScalar> type_id;
    cfn<u32,address,u32> ids;
    cfn<ScalarStreams,address,usize,usize> reserve;
};
struct CastEmission {
    BuiltinScalar from;
    BuiltinScalar to;
    u32 result_type;
    u32 result;
    u32 value;
};


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

struct ScalarDivision {
    BuiltinScalar scalar;
    u32 numerator;
    u32 denominator;
    u32 quotient;
    u32 remainder;
    u64 immediate;
};

// Operation identities follow the original IR UnaryOp and MathOp.
enum UnaryOperation:u32 {
    FloatNegate=0; SignedNegate=1; UnsignedNegate=2; LogicalNot=3; BitNot=4; Bitcast=5;
};
enum MathOperation:u32 {
    Abs=0; Floor=1; Sin=2; Cos=3; Pow=4; Exp=5; Log=6; Exp2=7; Log2=8; Sqrt=9; Min=10; Max=11; Ceil=12;
};
struct UnaryEmission {
    UnaryOperation operation;
    BuiltinScalar scalar;
    u32 result_type;
    u32 result;
    u32 value;
};
struct MathEmission {
    MathOperation operation;
    BuiltinScalar scalar;
    u32 result_type;
    u32 result;
    u32 left;
    u32 right;
    u32 extended_set;
};

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
struct PageInputs {
    u32 descriptor_pointer;
    u32 page_pointer;
    u32 descriptor;
    u32 low;
    u32 high;
    u32 stride;
    u32 index;
    u32 word;
    u32 has_word;
    u32 extra;
    u32 page_bytes;
};
extern "C" PageAddress prepare_page(PageInputs input, u32 result_pointer, ScalarHost host) on cpu
    requires nonzero(host.constant) requires nonzero(host.type_id) requires nonzero(host.ids);
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
struct PayloadResult { u32 type_id; u32 id; u32 compare_zero; };
struct PhysicalAccess {
    PageInputs page;
    BuiltinScalar scalar;
    u32 value;
    u32 value_type;
};
struct MemoryHost {
    ScalarHost scalars;
    cfn<u32,address,BuiltinScalar> pointer_type;
    cfn<void,address,u64> finish_body;
};
// An absent result ID returns a raw payload to the existing shared-word consumer.
extern "C" u32 physical_scalar_read(PhysicalAccess input, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant) requires nonzero(host.scalars.type_id)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.pointer_type) requires nonzero(host.finish_body);
extern "C" void physical_scalar_write(PhysicalAccess input, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant) requires nonzero(host.scalars.type_id)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.pointer_type) requires nonzero(host.finish_body);
extern "C" u32 encode_raw_bits(BuiltinScalar scalar, u32 value, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant) requires nonzero(host.scalars.type_id)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.finish_body);
struct UniformRead {
    u32 next;
    u32 push_pointer;
    u32 push_block;
    u32 uint_type;
    u32 raw_type;
    PayloadResult result;
    u32 zero;
    u32 low_word;
    u32 high_word;
    u32 byte_shift;
    u32 high_shift;
};
// The emitter supplies the declared IDs and room for both word loads,
// the original payload join and the final scalar definition.
struct PackedField { u32 value; u32 offset; u32 raw_type; u32 bytes; };
struct PackingConstant { u32 id; u32 next; };
struct PackingHost {
    address context;
    cfn<u32,address,address> field;
    cfn<PackingConstant,address,u32,u32> constant;
};
// Borrow the component stream, constant pool and both output buffers for this record.
// Record slots start at zero; the body can hold up to 33 words per component.
extern "C" PayloadWrite pack_record(PackingHost host, u32 word_type, u32 zero, u32 next, address packed, address body) on cpu
    requires nonzero(host.field) requires nonzero(host.constant);
// The original paged record contains count consecutive u32 words. The caller
// provides count pointer slots; step is the constant ID for a four-byte advance.
extern "C" PayloadWrite summary_addresses(PageAddress input, u32 step, address pointers, u32 count, address body) on cpu;
// Operands are word IDs in Words mode and pointer IDs for storage accesses.
// Shifts contains the original constant ID for each nonzero word position.
// The emitter supplies count operand slots and 20*count (read) or 14*count
// (write) body words. Only a Words write uses the output-ID buffer.
// Summary cells are consecutive u32 result IDs or borrowed u32 value IDs.
extern "C" u64 read_summary_words(address body, u32 type_id, u32 first, address pointers, u32 count) on cpu;
extern "C" u64 write_summary_words(address body, address pointers, address values, u32 count) on cpu;
struct PayloadShape { PayloadAccess access; BuiltinScalar scalar; BuiltinScalar unit; u32 count; };
extern "C" u32 emit_payload_load(PayloadShape shape, address operands, u32 result, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant) requires nonzero(host.scalars.type_id)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve) requires nonzero(host.finish_body);
extern "C" void emit_payload_store(PayloadShape shape, u32 value, address pointers, address output, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant) requires nonzero(host.scalars.type_id)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve) requires nonzero(host.finish_body);

extern "C" u64 plain_load(address body, u64 at, u32 type_id, u32 result, u32 pointer) on cpu;
extern "C" u64 plain_store(address body, u64 at, u32 pointer, u32 value) on cpu;
extern "C" u64 aligned_load(address body, u64 at, u32 type_id, u32 result, u32 pointer, u32 alignment) on cpu;
extern "C" u64 read_shader_index(u32 vector_type, u32 uint_type, u32 input, u32 global, u32 x, address body) on cpu;
extern "C" u64 unpack_summary_word(u32 type_id, u32 result, u32 word, u32 shift, address body) on cpu;

struct ControlWords { u64 words; u32 label; u32 reaches_next; };
// The emitter supplies allocated labels and the actual exit status of each
// child block. A zero result label means the fragment opened no new block.
// The return edge slice contains label/returned u32 pairs.
extern "C" ControlWords begin_shader_body(u32 result_type, u32 function, u32 signature, u32 entry, address body) on cpu;
extern "C" ControlWords end_shader_body(address body) on cpu;
extern "C" ControlWords open_selection(u32 merge, u32 condition, u32 yes, u32 no, address body) on cpu;
extern "C" ControlWords open_index_selection(u32 bool_type, u32 test, u32 index, u32 bound,
    bool less, u32 run, u32 end, address body) on cpu;
struct TileCollectiveInput {
    u64 results;
    u32 width;
    u32 first_word;
    u32 lane;
    u32 sum;
    u32 exclusive;
};
struct TileCollectiveHost {
    cfn<u32,address,u32> constant;
    cfn<u32,address,u64,u64,u32> tree;
    cfn<u32,address,u64,u64> seed;
    cfn<void,address,u64,u32,u32,u32> pad;
    cfn<void,address> prepare;
    cfn<void,address,u32,u32,u32> layer;
    cfn<void,address,u32,u32> root;
    cfn<void,address,u64,u64,u32> read;
    cfn<void,address,u32> barrier;
};
extern "C" void emit_tile_collective(address context, TileCollectiveInput input, TileCollectiveHost host) on cpu
    requires nonzero(host.constant) requires nonzero(host.tree) requires nonzero(host.seed)
    requires nonzero(host.pad) requires nonzero(host.prepare) requires nonzero(host.layer)
    requires nonzero(host.root) requires nonzero(host.read) requires nonzero(host.barrier);

extern "C" ControlWords open_tile_padding(u32 uint_type, u32 bool_type, u32 lane, u32 width,
    u32 capacity, u32 index, u32 test, u32 run, u32 end, address body) on cpu;
extern "C" ControlWords next_control_block(u32 target, u32 label, u32 reaches, address body) on cpu;
extern "C" u64 phi_two(address body, u64 at, u32 type, u32 result,
                        u32 first_value, u32 first_block, u32 second_value, u32 second_block) on cpu;
// Borrowed original blocks. The host owns instruction storage and word buffers.
enum BlockFlow:u32 { Error=0; Stop=1; Next=2; };
struct ControlTargets { u32 end; u32 next; };
enum ControlConstant:u32 { Selector=0; Flag=1; };
struct ReturnTargets { u32 function_merge; u32 loop_merge; address loop_edges; u32 label; };
struct StructuredHost {
    address context;
    cfn<u32,address,u32> labels;
    cfn<address,address,usize> reserve_words;
    cfn<void,address,ControlWords> finish_words;
    cfn<BlockFlow,address,Span,ControlTargets> block;
    cfn<u32,address,usize> value;
    cfn<u32,address,Span,u32> begin_return;
    cfn<void,address,u32> normal_exit;
    cfn<void,address> finish_return;
    cfn<u32,address,ControlConstant,u32> constant;
    cfn<void,address,u32> open_function;
    cfn<void,address> close_function;
    cfn<ReturnTargets,address> exit_targets;
    cfn<void,address,u32> record_return;
};
enum BlockRejection:u32 { Print=0; CpuCall=1; CpuTexture=2; CpuAddress=3; CpuFunction=4; FloatMath=5; AtomicAddress=6; };
struct BlockValues {
    address registers;
    ScalarHost scalars;
    cfn<BuiltinScalar,address,usize> scalar;
    cfn<BinaryOperation,address> binary;
    cfn<void,address,ScalarWords> finish;
    u32 index;
    u32 push_pointer;
    u32 push_block;
    u32 extended_set;
};
struct BlockPaging {
    PageInputs base;
    address descriptors;
    cfn<u32,address,BuiltinScalar> pointer_type;
    cfn<void,address,u64> finish_body;
    cfn<u32,address> current_label;
};
struct BlockSubgroup { u32 lane; u32 size; u32 id; u32 ballot_type; };
struct PrivateField { u32 variable; u32 pointer_type; BuiltinScalar unit; u32 offset; };
struct BlockLocals { address slots; cfn<PrivateField,address,usize> array; };
enum TileKeyKind : u32 { Input = 0; Published = 1; };
struct TileKey { usize owner; u32 word; TileKeyKind kind; };
struct TileCell { BuiltinScalar unit; u32 slot; u32 byte; u32 variable; u32 pointer_type; };
struct TileInputLayout {
    PageInputs page;
    address descriptors;
    u32 count;
    u32 active;
    u32 width;
    u32 label;
};
struct TileInputRange { u32 lane; u32 length; u32 label; };
struct TileInputRow { usize binding; u32 word; usize fragments; TileCell cell; };
struct TileInputFragment { u32 byte; BuiltinScalar unit; };
struct TileInputSource {
    address cursor;
    cfn<TileInputRow,address,address> row;
    cfn<TileInputFragment,address> fragment;
};
// Borrow the placed input cells and the fragments belonging to the current cell.
extern "C" TileInputRange emit_tile_input(TileInputLayout input, TileInputSource source, MemoryHost memory) on cpu
    requires nonzero(source.row) requires nonzero(source.fragment) requires nonzero(memory.pointer_type)
    requires nonzero(memory.scalars.constant) requires nonzero(memory.scalars.type_id)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body);

struct TileStorage {
    cfn<TileCell,address,TileKey> cell;
    cfn<TileKey,address,usize,u32> word;
    u32 width;
    u32 lane;
    u32 length;
};
extern "C" u32 tile_storage_read(TileStorage storage, TileKey key, u32 index, u32 offset,
    BuiltinScalar scalar, MemoryHost memory) on cpu
    requires nonzero(storage.cell)
    requires nonzero(memory.scalars.constant) requires nonzero(memory.scalars.type_id)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve) requires nonzero(memory.finish_body);

struct TextureSampling {
    address bindings;
    cfn<TextureDeclaration,address,usize> declaration;
    address variables;
    SampledType sampled2;
    SampledType sampled3;
    u32 vec2;
    u32 vec3;
    u32 vec4;
};

struct RayDeclaration { u32 id; u32 positions; u32 object_to_world; u32 world_to_object; };
struct RayBindings {
    address accelerations;
    u32 acceleration_type;
    u32 vec2;
    u32 vec3;
    address types;
    cfn<RayDeclaration,address,usize> ensures memory(read) query;
    cfn<u32,address,TypeShape,u32,u32> declare_type;
};

struct BlockHost {
    Host ir;
    BlockValues values;
    BlockPaging paging;
    BlockLocals locals;
    TileStorage tile;
    BlockSubgroup subgroup;
    TextureSampling textures;
    RayBindings rays;
    cfn<BlockFlow,address,BlockRejection> reject;
    cfn<void,address,address> collective;
};
extern "C" BlockFlow emit_instruction_block(Span code, ControlTargets targets,
    StructuredHost host, BlockHost emit) on cpu
    requires nonzero(emit.ir.inspect) requires nonzero(emit.reject)
    requires nonzero(emit.textures.declaration) requires nonzero(emit.rays.query) requires nonzero(emit.rays.declare_type)
    requires nonzero(emit.collective)
    requires nonzero(emit.tile.cell) requires nonzero(emit.tile.word)
    requires nonzero(emit.paging.pointer_type) requires nonzero(emit.paging.finish_body)
    requires nonzero(emit.paging.current_label)
    requires nonzero(emit.values.scalar) requires nonzero(emit.values.binary) requires nonzero(emit.values.finish)
    requires nonzero(emit.values.scalars.constant) requires nonzero(emit.values.scalars.type_id)
    requires nonzero(emit.values.scalars.ids) requires nonzero(emit.values.scalars.reserve)
    requires nonzero(emit.locals.array)
    requires nonzero(host.labels) requires nonzero(host.reserve_words) requires nonzero(host.finish_words)
    requires nonzero(host.block) requires nonzero(host.value) requires nonzero(host.begin_return)
    requires nonzero(host.normal_exit) requires nonzero(host.finish_return)
    requires nonzero(host.constant) requires nonzero(host.open_function) requires nonzero(host.close_function)
    requires nonzero(host.exit_targets) requires nonzero(host.record_return);

struct TilePhaseView { Span code; Node step; u32 present; };
struct TilePhaseValue { usize register; u32 value; u32 value_type; u32 module_constant; };
struct TilePhaseHost {
    address cursor;
    cfn<TilePhaseView,address> next_phase;
    cfn<TilePhaseValue,address,address> next_value;
};
extern "C" void emit_tile_phases(u32 active, StructuredHost control, TilePhaseHost host, BlockHost emit) on cpu
    requires nonzero(control.labels) requires nonzero(control.reserve_words)
    requires nonzero(control.finish_words) requires nonzero(control.block)
    requires nonzero(host.next_phase) requires nonzero(host.next_value)
    requires nonzero(emit.ir.inspect) requires nonzero(emit.collective) requires nonzero(emit.tile.cell)
    requires nonzero(emit.paging.current_label) requires nonzero(emit.paging.finish_body) requires nonzero(emit.values.scalar)
    requires nonzero(emit.values.scalars.constant) requires nonzero(emit.values.scalars.type_id)
    requires nonzero(emit.values.scalars.ids) requires nonzero(emit.values.scalars.reserve);

extern "C" void lower_return_loop(address edges, u32 count, u32 bool_type, StructuredHost host) on cpu
    requires nonzero(host.labels) requires nonzero(host.constant) requires nonzero(host.exit_targets)
    requires nonzero(host.record_return) requires nonzero(host.reserve_words) requires nonzero(host.finish_words);



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

struct TreeComponent {
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
// group_remaining writes 10 words and two IDs, with the count at first + 1.
extern "C" void group_remaining(address body, u32 uint_type, u32 index, u32 lane, u32 count, u32 first) on cpu;
struct CustomTree {
    u32 lane;
    u32 shared;
    u32 pointer;
    u32 remaining;
    u32 value_words;
    u32 tag_words;
    u32 down;
};

struct CustomMerge {
    u32 value_words;
    u32 tag_words;
    u32 label;
};

struct CustomCombine {
    address context;
    // The checked emitter returns its final block label, or zero with its diagnostic.
    cfn<u32,address,address,address,address,u32,u32> emit;
};

// Each workspace span has value_words + tag_words entries. Spans are offsets,
// neighbor pointers, left, right and the reusable result.
extern "C" u32 emit_custom_tree(CustomTree tree, address own, address workspace,
    MemoryHost memory, CustomCombine combine) on cpu
    requires nonzero(memory.scalars.constant) requires nonzero(memory.scalars.type_id)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) requires nonzero(combine.emit);
extern "C" u32 emit_custom_merge(CustomMerge input, address left, address right, address result,
    MemoryHost memory, CustomCombine combine) on cpu
    requires nonzero(memory.scalars.constant) requires nonzero(memory.scalars.type_id)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.finish_body) requires nonzero(combine.emit);

struct TreeLayer {
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
struct TreeWords { u64 body; u64 decorations; u32 next; u32 label; };

// Selector is the scan lane mask or the tile's doubled stride.
// The host provides 42 + 176*count body words and 3*count decoration words.
extern "C" TreeWords tree_layer(TreeLayer input, address components, u32 count,
    address body, address decorations) on cpu;

struct CollectiveField {
    u32 source_offset;
    u32 target_offset;
    u32 unit_type;
    u32 unit_bytes;
    u32 pointer_type;
    u32 step;
    u32 shift;
    u32 slot;
};

struct ReductionWriteback {
    PageAddress target;
    u32 bool_type;
    u32 push_pointer;
    u32 push;
    u32 width;
    u32 length_word;
    u32 bias;
};

struct CollectiveInput {
    array<u32,2> identity;
    u32 source;
    u32 lane;
    u32 active;
    u32 count;
    u32 before;
    u32 scope;
    u32 semantics;
};

struct ReductionGroup {
    ReductionWriteback output;
    CollectiveInput lanes;
};

struct ScanGroup {
    PageAddress target;
    CollectiveInput lanes;
    u32 summary;
    u32 width;
    u32 last;
    u32 bool_type;
    u32 normalize;
};

// Workspace has two pointer words and two input words per component.
extern "C" TreeWords scan_group(ScanGroup input, address levels, address components,
    address fields, u32 count, address workspace, address body, address decorations) on cpu;
extern "C" TreeWords reduction_group(ReductionGroup input, address strides, address components,
    address fields, u32 count, address workspace, address body, address decorations) on cpu;
extern "C" StageWords reduction_writeback(ReductionWriteback input, u32 lane, u32 root,
    address fields, u32 count, address pointers, address body) on cpu;

struct ScanPrefix {
    PageAddress target;
    u32 prefix_descriptor;
    u32 head_descriptor;
    u32 head_word;
    u32 width;
    u32 bool_type;
    CollectiveOperation operation;
    u32 label;
};

struct PhysicalField {
    BuiltinScalar scalar;
    u32 raw_type;
    u32 value_type;
    u32 unit_type;
    u32 unit_bytes;
    u32 pointer_type;
    u32 offset;
    u32 step;
    u32 shift;
};

// The caller provides 233 + 144*count body words and 3*count decoration words.
// A segment head descriptor uses a further 148 body words for its load and branch.
extern "C" TreeWords add_scan_prefix(ScanPrefix input, address fields, u32 count,
    address body, address decorations) on cpu;

struct CustomInput {
    PageInputs source;
    u32 summary_descriptor;
    u32 active;
    u32 lane;
    u32 shared;
    u32 pointer;
    u32 value_words;
    u32 tag_words;
    u32 head_word;
    u32 tail;
};

struct CustomInputHost {
    address context;
    // The checked emitter returns its final block label, or zero with its diagnostic.
    cfn<u32,address,address,u32> identity;
    cfn<Span,address> fields;
    cfn<u32,address,address,u32,u32,u32> project;
};

// Values holds max(source fields, value_words) + tag_words entries.
extern "C" u32 emit_custom_input(CustomInput input, address own, address identity,
    address values, MemoryHost memory, CustomInputHost host) on cpu
    requires nonzero(memory.scalars.constant) requires nonzero(memory.scalars.type_id)
    requires nonzero(memory.scalars.ids) requires nonzero(memory.scalars.reserve)
    requires nonzero(memory.pointer_type) requires nonzero(memory.finish_body)
    requires nonzero(host.identity) requires nonzero(host.fields) requires nonzero(host.project);

struct CollectiveBranch {
    u64 words;
    u32 next;
    u32 run;
    u32 end;
};

struct CustomSegmentPrefix {
    PageAddress parent;
    u32 bool_type;
    u32 active;
    u32 step;
};

struct CustomSegmentOutput {
    PageAddress source;
    u32 bool_type;
    u32 length;
    u32 from;
    u32 end;
    u32 target;
    u32 target_word;
    u32 count_target;
    u32 count_word;
};

// The workspace receives exclusive and parent summaries, words result IDs each.
extern "C" CollectiveBranch open_custom_segment_finish(CustomSegmentPrefix input, address own,
    address identity, u32 words, address workspace, address body) on cpu;
extern "C" StageWords close_custom_segment_finish(CustomSegmentOutput input, address fields,
    u32 field_count, address total, u32 segments, address body) on cpu;

enum SummaryRootMode : u32 { Publish = 0; PublishReset = 1; EmptyCountReset = 2; };

struct CustomSummaryRoot {
    PageAddress target;
    SummaryRootMode mode;
    u32 bool_type;
    u32 zero;
    u32 lane;
    u32 active;
    u32 length;
    u32 width;
    u32 last;
    u32 shared;
    u32 pointer;
    u32 step;
    u32 scope;
    u32 semantics;
};

struct SummaryRootWords {
    u64 words;
    u32 next;
    u32 label;
    u32 group;
};

// Workspace spans hold root offsets/pointers, destination pointers and loaded values.
extern "C" SummaryRootWords custom_summary_root(CustomSummaryRoot input, address identity,
    u32 words, address workspace, address body) on cpu;

struct CustomScanWrite {
    PageAddress target;
    u32 active;
    u32 bool_type;
    u32 head;
    u32 step;
    u32 tail;
};

// Two workspace spans hold loaded summary values and recursive output pointers.
extern "C" StageWords custom_scan_write(CustomScanWrite input, address own, address identity,
    u32 words, address fields, u32 field_count, address workspace, address body) on cpu;

struct CustomPrefix {
    PageAddress target;
    u32 prefix_descriptor;
    u32 head_descriptor;
    u32 head_word;
    u32 width;
    u32 bool_type;
    u32 active;
    u32 step;
    u32 packed;
    u32 words;
    u32 field_count;
};

struct CustomPrefixOpen {
    u64 words;
    u32 next;
    u32 label;
    u32 end;
    u32 head_end;
};

// Workspace holds prefix words, target words, and the target pointers used by the write.
extern "C" CustomPrefixOpen open_custom_prefix(read CustomPrefix input, address fields,
    address workspace, address body) on cpu;
extern "C" StageWords close_custom_prefix(read CustomPrefix input, CustomPrefixOpen opened,
    address fields, address locations, address values, u32 next, address body) on cpu;

struct SegmentField {
    BuiltinScalar scalar;
    u32 word;
    u32 raw_type;
    u32 value_type;
    u32 shift;
};

struct SegmentMerge {
    CollectiveOperation operation;
    u32 uint_type;
    u32 bool_type;
    u32 zero;
    u32 value_words;
    u32 counted;
};

struct ScanLevel { u32 mask; u32 stride; };
struct SegmentAdd {
    SegmentMerge merge;
    PageAddress target;
    u32 prefix_descriptor;
    u32 width;
    u32 step;
    u32 active;
    u32 run;
    u32 end;
};

// The workspace holds five summary spans: two pointer lists, two input lists,
// and the merged result. The existing run/end IDs delimit the active lanes.
extern "C" TreeWords segment_add(SegmentAdd input, address fields, u32 count,
    address workspace, address body, address decorations) on cpu;

struct SegmentStorageField {
    u32 word;
    u32 offset;
    u32 unit_type;
    u32 unit_bytes;
    u32 pointer_type;
    u32 step;
};

enum SegmentPass : u32 { Main = 0; Tail = 1; Finish = 2; };

struct SegmentGroup {
    SegmentMerge merge;
    PageAddress source;
    SegmentPass pass;
    u32 target;
    u32 summary;
    u32 head_offset;
    u32 head_word;
    u32 marker_word;
    u32 count_target;
    u32 count_word;
    u32 active;
    u32 count;
    u32 lane;
    u32 shared;
    u32 pointer;
    u32 width;
    u32 last;
    u32 scope;
    u32 semantics;
    u32 step;
    u32 before;
};

// Six tree spans followed by the input summary and its workgroup pointers.
extern "C" TreeWords segment_group(SegmentGroup input, address levels, address fields,
    address source_fields, address target_fields, u32 field_count, address identity,
    address workspace, address body, address decorations) on cpu;

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

struct DispatchLayout {
    u32 table_pointer;
    u32 page_pointer;
    u32 word_pointer;
    u32 descriptor;
    u32 width;
    u32 width_shift;
    u32 record_bytes;
};

struct DispatchScalars {
    u32 uint_type;
    u32 bool_type;
    u32 zero;
    u32 one;
};

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
extern "C" u32 encode_index_scatter(ScatterInput input, u32 target_descriptor,
    DispatchLayout layout, u32 max_x, u32 max_y, u32 first_byte, usize scatter_words, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant) requires nonzero(host.scalars.type_id)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.finish_body);

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
extern "C" u64 workgroup_barrier(address body, u64 at, u32 scope, u32 semantics) on cpu;

struct SortKeyInput {
    PageInputs target;
    PageInputs source;
    BuiltinScalar scalar;
    u32 descending;
    u32 first;
    u32 active;
};

extern "C" u32 sort_initialize(SortKeyInput input, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant) requires nonzero(host.scalars.type_id)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.pointer_type) requires nonzero(host.finish_body);

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

struct DispatchPush { u32 uint_type; u32 pointer_type; u32 block; u32 zero; };
struct DispatchArgumentInput {
    DispatchLayout layout;
    PageInputs prefix;
    DispatchPush push;
    u32 invocation;
    u32 operation;
    u32 selected;
    u32 fixed;
    u32 has_fixed;
    u32 scan_count;
    u32 scan_width;
    u32 count_word;
    u32 histogram_byte;
};
struct DispatchArgumentResult { u32 label; u32 collective; };
extern "C" DispatchArgumentResult emit_dispatch_arguments(DispatchArgumentInput input, MemoryHost host) on cpu
    requires nonzero(host.scalars.constant) requires nonzero(host.scalars.type_id)
    requires nonzero(host.scalars.ids) requires nonzero(host.scalars.reserve)
    requires nonzero(host.pointer_type) requires nonzero(host.finish_body);
extern "C" void declare_dispatch_continuation(u32 type_id, u32 result, u32 spec_id, u32 bytes,
    address definitions, address decorations) on cpu;
extern "C" PayloadRead dispatch_count_index(DispatchPush push, u32 word, u32 field, u32 first, address body) on cpu;
extern "C" PrefixIndexWords encode_linear_index(DispatchPush push, u32 wide_type, u32 bool_type,
    u32 local, u32 one, u32 first, address body) on cpu;
extern "C" PrefixIndexWords encode_prefix_index(PrefixIndexEmission input, address body) on cpu;

// Requests are sent in the native emitter's order, stopping at its first error.
enum EntryPass:u32 {
    Main=0; ReductionTail=1; ScanTail=2; ScanAdd=3; ScanRootAdd=4;
    SegmentedSingleGroup=5; CompactScatter=6; SelectionArguments=7;
    SelectionScan=8; SelectionScatter=9; SortHistogram=10; SortScatter=11;
    SortKey=12; SortScan=13; SortGather=14; DispatchArguments=15;
};
struct ShaderPass {
    EntryPass kind; u32 numeric; usize parameter; u32 word; u32 arguments;
};
extern "C" ShaderPass shader_pass(EntryPass kind, usize parameter, u32 word) on cpu;

enum EntryOutput:u32 {
    Main=0; Tail=1; ScanAdd=2; Scatter=3; SegmentedFinish=4; RootAdd=5;
    Dispatch=6; Selector=7; Additional=8;
};
enum EntryKernel:u32 {
    None=0; PrepareMain=1; PrepareTail=2; PrepareAdd=3; Temporary=4;
    Main=5; Tail=6; Add=7;
};
struct StageEntryInput {
    u32 operation; u32 selected; u32 prefix; u32 root_add;
    usize keys; usize prefix_binding; u32 prefix_word;
    u32 root_scan; u32 tail_scan; u32 single_group;
};
struct StageEntryRequest {
    ShaderPass pass; EntryOutput output; EntryKernel kernel; u32 flags;
};
struct StageEntrySink { address context; cfn<u32,address,StageEntryRequest> emit; };
extern "C" void emit_stage_entries(StageEntryInput input, StageEntrySink sink) on cpu requires nonzero(sink.emit);

struct WorkgroupLayout { u32 scan; u32 local_index; u32 ranks; u32 words; };
struct ShaderBodyInput {
    u32 operation;
    EntryPass pass;
    u32 selection;
    u32 scan_local;
    u32 segmented_scan;
    u32 invariants;
    u32 tile_scope;
    u32 prefix_index;
};
enum BodyStep:u32 {
    DispatchArguments=0; SelectionArguments=1; PrefixIndex=2; LinearIndex=3; Invariants=4;
    Local=5; Open=6; Merge=7; SelectionScatter=8; SortKey=9; SortGather=10; SortRank=11;
    Scan=12; ScanAdd=13; SegmentedScan=14; SegmentedAdd=15; SegmentedRoot=16;
    CustomSegmented=17; CustomScan=18; CustomScanAdd=19; CustomReduce=20; Reduce=21;
    SegmentedReduce=22; SegmentedReduceAdd=23; CompactScatter=24; TileInput=25; TilePhases=26;
    SelectedIndex=27; Map=28;
};
enum BodyScan:u32 { Original=0; Selection=1; Compact=2; Sort=3; };
enum BodyStepResult:u32 { Error=0; No=1; Yes=2; };
enum ShaderBodyEnd:u32 { Plain=0; Prepared=1; BadSortPass=2; BadSegmentedPass=3; };
struct ShaderBodySink {
    address context;
    cfn<BodyStepResult,address,BodyStep,BodyScan,u32> emit;
};
extern "C" ShaderBodyEnd emit_shader_body(ShaderBodyInput input, ShaderBodySink sink) on cpu
    requires nonzero(sink.emit);
extern "C" WorkgroupLayout workgroup_layout(
    u32 operation, EntryPass pass, u32 selected, u32 summary_words, u32 scan_width
) on cpu;

struct WorkgroupDeclaration {
    WorkgroupLayout layout;
    u32 next;
    u32 word_type;
    u32 wide_type;
    u32 subgroup_mask;
    u32 ballot;
    u32 width;
    u64 narrow_cells;
    u64 wide_cells;
    u64 scratch_words;
};
struct SharedArray { u32 variable; u32 pointer; };
struct WorkgroupVariables {
    u64 definitions;
    u64 decorations;
    u64 shared_bytes;
    u32 next;
    u32 local;
    u32 rank_id;
    u32 rank_lane;
    u32 ballot;
    u32 lane;
    u32 size;
    u32 id;
    u32 tile_words;
    SharedArray reduction;
    SharedArray tile;
    SharedArray tile_wide;
};
// Initialize the result and append words when the tile arrays fit SPIR-V lengths.
extern "C" u32 workgroup_declarations(WorkgroupDeclaration input, ShaderConstants constants, address definitions,
    address decorations, address output) on cpu requires nonzero(constants.intern);

// The pass class is Main, Add, Root, Arguments, or the remaining identity-bearing pass.
enum KernelPreparation:u32 { Original=0; ProjectIdentity=1; Identity=2; Combine=3; };
extern "C" KernelPreparation custom_kernel_preparation(EntryPass pass, u32 segmented) on cpu;
