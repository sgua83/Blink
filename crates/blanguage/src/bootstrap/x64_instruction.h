module bootstrap.x64_instruction;
import bootstrap.scalar_fold;
import bootstrap.ir_dataflow;
import bootstrap.frame_layout;

struct Encoding {
    u32 displacement;
    u8 reg;
    u8 base;
    u8 index;
    u8 shift;
    u8 form;
    u8 prefix;
    u16 wide;
};

// The caller owns the opcode and provides opcode_length + 8 output bytes.
// The result is the number of initialized bytes.
extern "C" u64 encode(Encoding input, address opcode, u64 opcode_length, address output) on cpu;

struct FrameEncoding {
    u64 length;
    u64 probe_field;
};

struct TailLiteral {
    u64 bits;
    address references;
    u64 reference_count;
    u64 wide;
};
enum TailRegionKind:u32 { StackProbe=0; Alignment=1; Literal=2; };
struct TailRegion { u64 end; TailRegionKind kind; };
struct CodeTail { u64 end; u64 region_count; };

// The destination has room for 17 bytes. The caller supplies a nonempty frame.
// probe_field is relative to the output, and zero when no call is emitted.
extern "C" FrameEncoding allocate_frame(u32 frame, address output) on cpu;
// The caller supplies a nonempty frame and seven destination bytes.
extern "C" u64 release_frame(u32 frame, address output) on cpu;
// The caller owns two output bytes. restore selects POP instead of PUSH.
extern "C" u64 stack_register(u8 register, bool restore, address output) on cpu;

// slot is 0..3, float_size is 4 or 8 for scalar floats and zero for the GPR bank.
// to_home stores at the callee's entry, above the return address. Otherwise load from
// the caller's outgoing slot. The caller owns nine output bytes.
extern "C" u64 argument_register(u8 slot, u32 float_size, bool to_home, address output) on cpu;

enum AbiMemoryMode:u32 {
    StackLoad=0;
    StackStore=1;
    IndirectParameter=2;
    IndirectResult=3;
    FallibleResult=4;
    PackIntegerResult=5;
    UnpackIntegerResult=6;
    RangeSummary=7;
};

struct AbiValueComponent {
    MoveLocation location;
    ScalarEncoding scalar;
    ScalarEncoding field_scalar;
    u32 offset;
};

// Components borrow their locations from the original allocation. For a
// range summary the slot is the actual saved stack size; the output pointer
// is the fifth Win64 parameter. Reserve sixteen bytes plus twenty-four per
// component, or forty-eight per component for packed integer results.
extern "C" u64 abi_value(u32 slot, u64 count, AbiMemoryMode mode, address context,
    cfn<AbiValueComponent,address,u64> component, address output) on cpu requires nonzero(component);
// The caller owns sixteen bytes for computing and writing a frame address.
extern "C" u64 abi_stack_pointer(u32 offset, u32 slot, address output) on cpu;

// The caller preserves the existing bytes and supplies this tail's maximum length.
extern "C" u64 tail_capacity(u64 probe,u64 literal_count) on cpu;
// Each callback receives the literal's final offset in emission order. The returned
// reference list stays readable until write_tail returns. Regions holds the probe,
// optional initial padding and each literal.
extern "C" CodeTail write_tail(address image,u64 end,u64 probe,u64 literal_count,address regions,
    address context,cfn<TailLiteral,address,u64> next_literal) on cpu requires nonzero(next_literal);
// The field address refers to the four-byte displacement at offset in the image.
extern "C" void patch_relative(address field, u64 offset, u64 target) on cpu;
// Known-target jump: output has five bytes and offset is its function-relative start.
extern "C" u64 jump_to(u64 offset, u64 target, address output) on cpu;
// The literal and helper writers share the original array of rel32 references.
extern "C" void patch_branches(address image, u64 target, address fields, u64 count) on cpu;

// condition is zero for JMP; test_bits is zero for flags, or 32/64 for EAX/RAX.
// The caller owns nine output bytes. The returned length includes the rel32.
enum ControlKind:u32 { If=0; Loop=1; Scope=2; Exit=3; Break=4; Continue=5; Raise=6; };
enum ControlJump:u32 { Always=0; Zero=1; Nonzero=2; };
struct ControlInstruction { Span first; Span second; u64 register; ControlKind kind; };
struct ControlTargets { address function; address block; address loop_exit; u64 loop_begin; };
struct ControlHost {
    cfn<u32,address,address,Span,ControlTargets> emit;
    cfn<void,address,address,u64> load;
    cfn<NativeCodeBuffer,address,u64> buffer;
    cfn<void,address,u64> commit;
};
void patch_control_branches(address image, u64 target, u64 pending) on cpu;
// The original emitter owns code and observations. The B entry owns the exit
// head and lends it to emitted bodies. Nested targets live for their scope.
enum ScalarLeaf:u32 {
    Call=0; Print=1; Texture=2; Publish=3; Collective=4;
    TileLength=5; TileLane=6; FunctionAddress=7;
};
struct ScalarCursor { u64 start; u64 ranges; };
enum ScalarBodyStatus:u32 { Done=0; Unlowered=1; Subgroup=2; RayQuery=3; };
extern "C" u64 encode_branch(u8 condition, u32 test_bits, address output) on cpu;

struct SymbolEncoding {
    u64 length;
    u64 field;
};

// The caller owns twelve bytes. Field locates the zeroed ABS64 address in
// the emitted sequence, with or without a following call through RAX.
extern "C" SymbolEncoding symbol_target(bool invoke, address output) on cpu;
// RIP-relative indirect call or tail jump through an import address slot.
// The caller owns six bytes. Field locates the zeroed REL32 displacement.
extern "C" SymbolEncoding import_transfer(bool tail, address output) on cpu;

struct ApplicationEntry {
    u32 length;
    u32 frame;
    u32 prolog;
    u32 main_field;
    u32 exit_field;
};

// The caller owns seventeen output bytes. Offsets are relative to the entry.
extern "C" ApplicationEntry application_entry(address output) on cpu;

struct ModuleFactoryCode {
    u64 length;
    u64 image_field;
    u64 ranges_field;
};

// The x64 C aggregate return uses the caller's output in RCX. The emitter
// writes at most 64 bytes and reports the two section-address relocations.
extern "C" ModuleFactoryCode module_factory_code(u64 bytes, u64 codes, address output) on cpu;

struct FrameCreateCode {
    u64 length;
    u64 factory_field;
    u64 count_field;
    u64 target_field;
};
struct ResourceTailCode {
    u64 length;
    u64 base_field;
    u64 target_field;
};

// Each caller owns 32 bytes. All fields locate x64 REL32 relocations.
extern "C" FrameCreateCode frame_create_code(address output) on cpu;
extern "C" ResourceTailCode resource_allocate_code(u32 index, address output) on cpu;
extern "C" ResourceTailCode task_start_code(address output) on cpu;

struct ProgramUniformArgument {
    u32 tag;
    ScalarEncoding scalar;
};
struct ProgramCallLayout {
    u32 frame;
    u32 datasets;
    u32 values;
    u32 textures;
    u64 capacity;
};
struct ProgramCallCode {
    u64 length;
    u32 function_end;
    u32 prolog;
    u64 base_field;
    u64 call_field;
};

// This layout bounds the Win64 stack displacements and code capacity.
extern "C" ProgramCallLayout program_call_layout(u32 datasets, u32 values, u32 textures) on cpu;
extern "C" ProgramCallCode program_call_code(ProgramCallLayout layout, u32 program_index,
    u32 datasets, address arguments, u32 values, u32 textures, address output) on cpu;

struct CompletionLimit {
    u32 parameter;
    u32 distance;
};
struct CompletionField {
    u32 offset;
    u32 interval_count;
    u64 known_mask;
    u64 known_value;
    address limits;
    u32 limit_count;
};
struct CompletionLayout {
    u32 frame;
    u64 capacity;
};
struct CompletionPatch {
    u64 intervals;
    u64 call;
};
struct CompletionCode {
    u64 length;
    u32 function_end;
    u32 prolog;
};

extern "C" CompletionLayout completion_layout(address fields, u32 count) on cpu;
// There is one patch row per field. The interval and call fields are REL32.
extern "C" CompletionCode completion_code(CompletionLayout layout, address fields, u32 count,
    address output, address patches) on cpu;

struct LibraryRunLayout {
    u32 frame;
    u32 uniforms;
    u64 capacity;
};
struct LibraryEntryCode {
    u64 length;
    u32 function_end;
    u32 prolog;
    u32 frame;
    u64 call;
};
struct LibraryCreateCode {
    LibraryEntryCode entry;
    u64 cpu;
    u64 gpu;
    u64 factories;
    u64 count;
};

// The create entry owns a 72-byte Win64 call frame and needs 128 output bytes.
extern "C" LibraryCreateCode library_create_code(u32 has_cpu, u32 has_gpu, u32 has_factories, address output) on cpu;
extern "C" LibraryRunLayout library_run_layout(u32 datasets, u32 dataset_addresses, u32 uniforms) on cpu;
// The caller provides one address flag per dataset and layout.capacity output bytes.
extern "C" LibraryEntryCode library_run_code(LibraryRunLayout layout, address dataset_addresses,
    u32 datasets, u32 uniforms, address output) on cpu;
extern "C" SymbolEncoding library_destroy_code(address output) on cpu;

// Call through an existing outgoing stack slot. The caller owns seven bytes.
extern "C" u64 call_slot(u32 offset, address output) on cpu;

struct DivisionPlan {
    u32 magic;
    u8 shift;
    u8 mode;
};

struct MoveLocation {
    u64 value;
    u8 kind;
    u8 base;
    u8 index;
    u8 shift;
};

struct MoveEncoding {
    u64 length;
    u64 literal_field;
};

// The caller owns 24 output bytes. A nonzero literal_field is the offset
// of a disp32 to the caller's original f32 literal, including its exact bits.
extern "C" MoveEncoding copy_scalar_move(MoveLocation dst, MoveLocation src, u32 size, bool signed, address output) on cpu;
// The 84-byte output holds the original argument moves, ABS64 call and return move.
extern "C" SymbolEncoding print_call(MoveLocation dst, MoveLocation src, ScalarEncoding value,
    ScalarEncoding result, address output) on cpu;
extern "C" u64 read_scalar_move(MoveLocation dst, MoveLocation memory, u32 size, bool signed, bool boolean, address output) on cpu;
extern "C" u64 write_scalar_move(MoveLocation memory, MoveLocation src, u32 size, address output) on cpu;

// These identities follow the original IR UnaryOp, including its bit-copy operation.
enum ValueUnaryOperation:u32 {
    FloatNegate=0; SignedNegate=1; UnsignedNegate=2; LogicalNot=3; BitNot=4; Bitcast=5;
};
extern "C" bool bit_copy(ScalarEncoding from, ScalarEncoding into) on cpu;
// These discriminants are the IR MathOp identities.
enum ScalarMathOperation:u32 {
    Abs=0; Floor=1; Sin=2; Cos=3; Pow=4; Exp=5; Log=6;
    Exp2=7; Log2=8; Sqrt=9; Min=10; Max=11; Ceil=12;
};
extern "C" u64 scalar_math_outgoing(ScalarMathOperation operation) on cpu;
enum PacketMathOperation:u32 { Sqrt=0; Abs=1; Min=2; Max=3; };
// Packet entries borrow the existing four-lane allocation; scratch XMM0..3 and
// EAX/ECX follow that allocation. Constant has 24 output bytes; load/store 16;
// multiply 64; the full numeric entries have 256 bytes.
// Constants, indices and uniforms write the assigned vector register or stack slot.
extern "C" u64 packet_constant(MoveLocation dst, u32 bits, address output) on cpu;
// The caller owns 48 bytes; EBX is the dense range index, slots is the sparse group.
extern "C" u64 packet_index(MoveLocation dst, bool sparse, u32 slots, address output) on cpu;
// The caller owns 64 bytes. The uniform table is in R13.
extern "C" u64 packet_uniform(MoveLocation dst, u32 offset, bool boolean, address output) on cpu;

extern "C" u64 packet_copy(MoveLocation dst, MoveLocation src, address output) on cpu;
extern "C" u64 packet_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    ScalarEncoding scalar, BinaryOperation operation, bool uniform_count, address output) on cpu;
extern "C" u64 packet_unary(MoveLocation dst, MoveLocation src, ValueUnaryOperation operation, address output) on cpu;
extern "C" u64 packet_cast(MoveLocation dst, MoveLocation src, ScalarEncoding from,
    ScalarEncoding into, address output) on cpu;
extern "C" u64 packet_math(MoveLocation dst, MoveLocation left, MoveLocation right,
    PacketMathOperation operation, address output) on cpu;
extern "C" u64 packet_select(MoveLocation dst, MoveLocation condition, MoveLocation left,
    MoveLocation right, address output) on cpu;

struct ScalarValueHost {
    Host ir;
    address context;
    cfn<MoveLocation,address,usize> ensures memory(read) location;
    cfn<ScalarEncoding,address,usize> ensures memory(read) scalar;
};

enum PacketPlanOperation:u32 {
    Constant=0; Index=1; Uniform=2; LocalLoad=3; LocalStore=4; Load=5; ViewLoad=6;
    Store=7; Unary=8; Binary=9; Divide=10; Cast=11; Copy=12; Math=13; Select=14;
    If=15; Loop=16; Scope=17; Exit=18; Break=19; Continue=20;
};
struct PacketPlanDecision {
    PacketPlanOperation operation;u32 uniform_count;PacketAddress dataset;PacketViewAddress view;PacketMathOperation math;
};
struct PacketLayout { u32 sparse; u32 slots; u32 buffer_bytes; u32 stride_field; };
struct PacketBodyHost {
    ControlHost control;
    cfn<MoveLocation,address,usize> local;
    cfn<ScalarCursor,address> mark;
    cfn<void,address,address,ScalarCursor> publish;
    cfn<NativeCodeBuffer,address,u64> reserve;
    cfn<void,address,u64> commit;
    ScalarValueHost values;
};
extern "C" void emit_packet_body(address context, address emitter, Span code, ControlTargets targets, PacketLayout layout, PacketBodyHost host) on cpu
    requires nonzero(host.local) requires nonzero(host.mark) requires nonzero(host.publish)
    requires nonzero(host.reserve) requires nonzero(host.commit)
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location) requires nonzero(host.values.scalar)
    requires nonzero(host.control.emit) requires nonzero(host.control.load) requires nonzero(host.control.buffer)
    requires nonzero(host.control.commit);

// The caller owns the output and consumes the returned native memory operand.
// B retains no address after return; the emitter owns the code and memory operand.
struct AddressEncoding { u64 length; u32 offset; u8 base; u8 index; u8 shift; };
// value_memory needs 32 output bytes.
// scaled_memory and offset_address need 16 output bytes.
extern "C" AddressEncoding value_memory(u8 base, MoveLocation index, u32 stride, u32 offset, address output) on cpu;
extern "C" AddressEncoding scaled_memory(u8 base, u8 index, u32 stride, u32 offset, address output) on cpu;
// Place the effective address in RAX. An indexed address consumes RAX as its
// index. The caller owns eight output bytes.
extern "C" u64 effective_address(u8 target, MoveLocation memory, address output) on cpu;

// Scale an index by the stored u64 stride. Base 255 leaves the scaled index
// available as an operand; otherwise add it to the given pointer register.
// The caller owns twelve bytes for the range ABI register moves.
extern "C" u64 range_arguments(address output) on cpu;
// The caller owns sixteen bytes. The final four bytes are the exit rel32.
extern "C" u64 range_bounds(bool packet, address output) on cpu;
struct RangeEnd {
    u64 back_edge;
    u64 end;
};

// The caller reserves eight bytes after at. A zero skip_field means there
// is no predicate branch. A zero step means the body advanced the index.
extern "C" RangeEnd finish_range(address image, u64 at, u64 begin, u64 exit_field,
    u64 skip_field, u8 step) on cpu;

// Packet inputs use their original allocated XMM/stack lanes; no source is retained.
// packet_lane needs 16 bytes; either packet view entry needs 64 bytes.
extern "C" u64 packet_lane(MoveLocation src, u8 lane, u8 gpr, address output) on cpu;
extern "C" AddressEncoding packet_view_memory(u8 base, MoveLocation index, u8 lane, u64 stride, u32 offset, address output) on cpu;
extern "C" AddressEncoding packet_view_memory_dynamic(u8 base, MoveLocation index, MoveLocation low, MoveLocation high, u8 lane, u32 offset, address output) on cpu;

// The plan passes IR facts and retains the returned addressing mode through emission.
enum PacketAddress:u32 { Unsupported=0; Dense=1; Contiguous=2; Selected=3; Uniform=4; Gather=5; };
enum PacketViewAddress:u32 { Unsupported=0; Varying=1; Shared=2; Broadcast=3; };
extern "C" PacketAddress packet_dataset_address(u32 stride, u32 word, bool selected,
    bool writes_binding, bool element, bool uniform) on cpu;
extern "C" PacketViewAddress packet_view_address(u32 word, bool uniform_base,
    bool uniform_index, bool uniform_stride) on cpu;


// The caller lends the body capacity plus 16 bytes for GPR saves and
// 20 bytes for the original Windows x64 unwind stream.
struct LeafOutput { address image; address unwind; };
struct LeafRange { u64 end; u64 unwind_bytes; u32 stack_bytes; };

// A collective may append one 4-byte unit identity. The caller also lends
// seven image bytes, up to four rel32 fields and up to two tail regions.
struct CollectiveLeafOutput { LeafOutput range; address references; address regions; };
struct CollectiveLeaf {
    LeafRange range;
    u64 function_end;
    u64 literal_target;
    u64 reference_count;
    u64 region_count;
};

struct CompactScatter {
    u32 source; u32 target; u32 flags; u32 prefix;
    u32 stride_offset; u32 flag_word; u32 record_bytes;
};

// Body capacities are 96 count bytes and 224 scatter bytes.
extern "C" LeafRange compact_count(u32 flags, u32 stride_offset, u32 flag_word, LeafOutput output) on cpu;
extern "C" LeafRange compact_scatter(CompactScatter input, LeafOutput output) on cpu;

struct SortKeyEmission {
    u32 buffer_bytes;
    u32 stride_offset;
    u32 word;
    u32 bytes;
    u32 bias;
    u32 floating;
    u32 descending;
    u32 first;
};

// Body capacities are 160, 96, 128 and 192 bytes, respectively.
extern "C" LeafRange sort_key(SortKeyEmission input, LeafOutput output) on cpu;
extern "C" LeafRange sort_histogram(u32 buffer_bytes, LeafOutput output) on cpu;
extern "C" LeafRange sort_scatter(u32 buffer_bytes, LeafOutput output) on cpu;
extern "C" LeafRange sort_gather(u32 buffer_bytes, u32 stride_offset, u32 record_bytes, LeafOutput output) on cpu;


enum ReductionOperator:u32 { Sum=0; Min=1; Max=2; BitAnd=3; BitOr=4; BitXor=5; };

struct ReductionRange {
    u64 identity;
    ScalarEncoding scalar;
    u32 source;
    u32 stride_offset;
    u32 source_word;
    ReductionOperator operation;
    u32 count;
    array<u32,4> offsets;
};


// The caller owns 16 bytes for tile_lane and 48 for tile_input_boundary.
extern "C" u64 tile_lane(u32 width, address output) on cpu;

// The emitter owns the original descriptor table. B borrows the actual
// lowered instruction, assigned locations and 248 writable bytes.
struct TextureAccessHost {
    ScalarValueHost values;
    address table;
    cfn<u64,address,usize> ensures memory(read) offset;
    u32 stride;
};
extern "C" u64 emit_texture_access(View access, TextureAccessHost host, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.offset);

enum TextureColor:u32 { Rgba8=1; Srgb=2; R8=3; Rg16=4; Rgb10A2=5; Rg11B10=6; R16=7; };
enum SampleScalar:u32 { Word=0; Float=1; Bool=2; Index=3; };
enum SampleOp:u32 {
    Constant=0; Cast=1; Bitcast=2; Add=3; Divide=4; Multiply=5; LessEqual=6; Equal=7;
    ShiftRight=8; ShiftLeft=9; And=10; Or=11; Power=12; Select=13; Into=14; Set=15;
    Subtract=16; Less=17; GreaterEqual=18; NotEqual=19; Minimum=20; Maximum=21;
    Floor=22; Register=23; Format=24; AddressMode=25; Levels=26;
};
enum SampleBlock:u32 {
    Output=0; FloatArm=1; FormatTest=2; ByteArm=3; PackedArm=4;
    FormatArm=5; PreviousArm=6; FormatChoice=7; SrgbArm=8; Empty=9;
};
enum SampleBlockAction:u32 { Select=0; Append=1; Branch=2; };
enum SampleReadKind:u32 { Size=0; Float=1; Byte=2; Word=3; Half=4; };
struct SampleRegisters { array<u64,4> values; };
struct SampleRead {
    SampleReadKind kind;
    u32 dimensions;
    u64 level;
    SampleRegisters coordinate;
    SampleRegisters result;
};
struct SampleHost {
    address context;
    cfn<u64,address,SampleOp,SampleScalar,u64,u64,u64> instruction;
    cfn<u64,address,SampleRead> read_texture;
    cfn<void,address,SampleBlockAction,SampleBlock,SampleBlock,u64> block;
};
// The host only materializes the original IR nodes. The coordinate span has
// two or three entries, as determined by the typed texture instruction.
extern "C" void texture_sample_ir(address coordinate, u32 dimensions, u64 lod, address result, SampleHost host) on cpu
    requires nonzero(host.instruction) requires nonzero(host.read_texture) requires nonzero(host.block);

struct NativeCodeBuffer { address image; u64 end; };

struct TilePublishInput {
    MoveLocation location;
    ScalarEncoding scalar;
    u32 low_offset;
    u32 high_offset;
};

struct TilePublishCode {
    u64 length;
    u64 value_end;
    u64 high_start;
    u64 high_store;
};

// The caller owns 36 bytes for the lane index and shared plane base.
extern "C" u64 tile_index_base(MoveLocation index, ScalarEncoding scalar, u32 storage_offset, address output) on cpu;
// R15 holds the lane base and EAX the logical byte offset. The caller owns 40 bytes.
extern "C" u64 tile_field_address(u32 width, u32 byte_offset, address output) on cpu;
// The caller owns 48 bytes for reading the two shared words into the original value location.
extern "C" u64 tile_read_split(MoveLocation dst, MoveLocation low, MoveLocation high, address output) on cpu;
// R10 holds the lane base. Absent plane offsets are u32::MAX. The caller owns 48 bytes.
struct TilePublishField {
    TilePublishInput input;
    u32 word;
    usize value;
    address scalar_origin;
};

struct TilePublishHost {
    cfn<NativeCodeBuffer,address,u64> reserve;
    cfn<void,address,u64> commit;
    cfn<TilePublishField,address> next;
    cfn<void,address,u64,u64,TilePublishCode,TilePublishField> observe;
};

// Borrows the actual placed publication fields from the stack layout and emits in order.
extern "C" void emit_tile_publish(address context, u32 base_offset, TilePublishHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit)
    requires nonzero(host.next) requires nonzero(host.observe);

struct TileCollectiveLayout {
    ReductionOperator operation;
    bool exclusive;
    u32 length_offset;
    u32 plane_stride;
};

struct TileCollectiveField {
    u64 identity;
    ScalarEncoding scalar;
    u32 input_offset;
    u32 output_offset;
    u32 input_high_delta;
    u32 word;
    usize result;
    address scalar_origin;
};

struct TileCollectiveCode {
    u64 length;
    u64 identity_start;
    u64 loop_start;
    u64 accumulate_start;
    u64 fill_start;
    u64 fill_loop_start;
};

struct TileCollectiveHost {
    cfn<NativeCodeBuffer,address,u64> reserve;
    cfn<void,address,u64> commit;
    cfn<TileCollectiveField,address,ReductionOperator> next;
    cfn<void,address,u64,TileCollectiveLayout,TileCollectiveCode,TileCollectiveField> observe;
};

extern "C" void emit_tile_collective(address context, TileCollectiveLayout layout, TileCollectiveHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.commit)
    requires nonzero(host.next) requires nonzero(host.observe);

// Reduction body capacity is 256 bytes.
extern "C" CollectiveLeaf reduce_range(ReductionRange input, CollectiveLeafOutput output) on cpu;

struct SegmentedRange {
    ReductionRange values;
    u32 target;
    u32 target_word;
    u32 parameters;
    u32 head_word;
    u32 value_words;
};

// Each segmented reduction pass has a 512-byte body capacity.
extern "C" LeafRange segmented_reduce_range(SegmentedRange input, u32 finish, LeafOutput output) on cpu;

struct ScanRange {
    ReductionRange values;
    u32 target;
    u32 target_word;
    u32 parameters;
};

// Each scan pass has a 512-byte body capacity.
extern "C" CollectiveLeaf scan_range(ScanRange input, u32 add, CollectiveLeafOutput output) on cpu;

struct SegmentedScanRange {
    ScanRange scan;
    u32 head_word;
    u32 value_words;
};

// Each segmented scan pass has a 512-byte body capacity.
extern "C" LeafRange segmented_scan_range(SegmentedScanRange input, u32 add, LeafOutput output) on cpu;

struct PrintEncoding {
    u32 length;
    u32 unwind_length;
    u32 local;
    u32 create_locale;
    u32 format;
    u32 free_locale;
    u32 stdout_handle;
    u32 write;
};

// The caller owns 224 code bytes and 12 unwind bytes. UCRT offsets are zero
// for integer and boolean output. Other fields are relative to output.
extern "C" PrintEncoding print_helper(ScalarEncoding scalar, address output, address unwind) on cpu;

struct PacketPlanHeader {u64 registers;u64 slots;u64 writes;u32 tile;u32 selected;};
struct PacketPlanState {address bytes;u64 count;};
struct PacketPlanFlow {PacketPlanState next;PacketPlanState breaks;PacketPlanState continues;PacketPlanState exits;};
struct PacketPlanHost {
    Host ir;
    cfn<u32,address,usize> stride;
    cfn<u32,address,usize> written;
    cfn<ScalarEncoding,address,usize> scalar;
    cfn<u32,address,cfn<u32,usize>> widths;
};
struct PacketPlanProduct { Span code; Span uses; };
// Coloring consumes the uses. The code borrows the original instructions through emission.
bool packet_plan(address context, Span code, PacketPlanHeader header, address output, PacketPlanHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.stride) requires nonzero(host.written) requires nonzero(host.scalar)
    requires nonzero(host.widths);
extern "C" void packet_plan_drop(Span plan) on cpu;
extern "C" u32 packet_plan_uniform_operand(address bytes,u64 reg) on cpu;
extern "C" u32 packet_plan_mark_operand(address bytes,u64 reg) on cpu;
extern "C" u32 packet_plan_width(usize bytes) on cpu;

enum RangeEntry:u32 {
    Map=0; Reduce=1; CustomReduce=2; CustomScan=3; CustomSegmentedReduce=4;
    SegmentedReduce=5; Scan=6; Compact=7; Sort=8; ReduceTail=9; ScanPrefix=10; ScanApply=11;
    SegmentPrefix=12; SegmentApply=13; SegmentedFinish=14; ScanAdd=15; CompactScatter=16;
    SortKey=17; SortHistogram=18; SortScatter=19; SortGather=20;
};
enum RangeDestination:u32 {
    Main=0; Tail=1; Apply=2; Histogram=3; Scatter=4; Gather=5; Initializers=6;
};
enum RangeShape:u32 {
    Element=0; ReduceTail=1; CustomScan=2; SegmentedFinish=3; ScanAdd=4; CompactScatter=5; Sort=6;
};
struct RangeProduct {
    RangeShape shape;
    u32 has_values;
    usize words;
};
struct RangeSink {
    address context;
    cfn<u32,address,RangeEntry,RangeDestination,usize> emit;
};
extern "C" RangeProduct emit_ranges(RangeEntry entry, usize words_or_keys, u32 complete, RangeSink sink)
    on cpu requires nonzero(sink.emit);

struct ScalarLiteral {
    u64 bits;
    u64 field;
    u32 wide;
};
struct ScalarMathReference {
    u64 field;
    u32 length;
    array<u8,6> name;
};
struct ScalarValueEncoding {
    u64 length;
    ScalarLiteral first;
    ScalarLiteral second;
    ScalarMathReference math;
};

enum MapRangePart:u32 {
    Enter=0; PacketBounds=1; PacketCollect=2; PacketAdvance=3; PacketBackEdge=4;
    PacketTail=5; ScalarBounds=6; ScalarSelection=7; ScalarAdvance=8; ScalarBackEdge=9; Exit=10;
    TileBase=11; TilePhaseEnter=12; TilePhaseAdvance=13; TileInputBoundary=14; TilePrivatePointer=15;
};
struct MapRangeInput {
    Span code;
    address packet;
    u64 phase_count;
    usize binding;
    u32 word;
    u32 selected;
    u32 frame;
    u32 slots;
    u32 buffer_bytes;
    u32 stride_field;
    u32 tile_scope;
    u32 workgroup;
    u64 saved;
};
struct MapRangeObservation { u64 start; u64 end; usize index; u32 value; MapRangePart part; };
struct TilePhaseView { Span code; Node step; };
// An exhausted input stream has zero unit bytes; an exhausted slot stream
// carries no original scalar. The rows borrow their original placement.
struct TileInputFragment { u64 binding; u32 source_offset; u32 storage_base; u32 unit_bytes; };
struct TilePrivateSlot {
    MoveLocation location;
    ScalarEncoding scalar;
    u64 index;
    address scalar_origin;
    u32 storage_offset;
    u32 local;
};
struct MapRangeHost {
    address context;
    RangeFrameHost frame;
    cfn<NativeCodeBuffer,address,u64> reserve;
    cfn<void,address,u64> commit;
    cfn<void,address,address,ControlTargets> packet;
    cfn<u32,address,Span,ControlTargets> scalar;
    cfn<TileInputFragment,address> next_input;
    cfn<void,address,u64,u64,TileInputFragment,u32> observe_input;
    Host ir;
    cfn<TilePhaseView,address,u64> phase;
    cfn<TilePrivateSlot,address,bool> next_private;
    cfn<void,address,u64,u64,u64,bool,TilePrivateSlot> observe_private;
    cfn<void,address,address,ScalarLeaf,View> tile_step;
    cfn<void,address,MapRangeObservation> observe;
};
// The emitter starts with an empty code buffer. A reserved image is lent only
// until the bytes are committed. The original IR and phase slots remain borrowed.
extern "C" u32 emit_map_range(MapRangeInput input, MapRangeHost host) on cpu
    requires nonzero(host.frame.reserve) requires nonzero(host.frame.publish) requires nonzero(host.reserve)
    requires nonzero(host.commit) requires nonzero(host.observe)
    requires nonzero(host.packet) requires nonzero(host.scalar) requires nonzero(host.next_input) requires nonzero(host.observe_input)
    requires nonzero(host.ir.inspect) requires nonzero(host.phase)
    requires nonzero(host.next_private) requires nonzero(host.observe_private) requires nonzero(host.tile_step);

struct ScalarStorageEncoding { u64 length; u64 literal; u64 field; };
struct ScalarStorageHost {
    ScalarValueHost values;
    cfn<MoveLocation,address,usize> ensures memory(read) local;
    u32 buffer_bytes;
    u32 stride_field;
};
struct IndexedTileWord { u32 offset; u32 byte_offset; };
struct IndexedAddressHost {
    ScalarValueHost values;
    cfn<IndexedTileWord,address,usize,u32> ensures memory(read) tile_stream;
    u32 buffer_bytes;
    u32 stride_field;
    u32 workgroup;
};
// The original reference borrows its actual locations and 168 writable bytes.
extern "C" u64 emit_reference_address(Node reference, IndexedAddressHost host,
    cfn<MoveLocation,address,usize> ensures memory(read) local, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.tile_stream) requires nonzero(local);

struct ScalarBodyHost {
    ControlHost control;
    cfn<ScalarCursor,address> mark;
    cfn<void,address,address,ScalarCursor> publish;
    cfn<void,address,ScalarLeaf,View> emit;
    IndexedAddressHost addresses;
    cfn<MoveLocation,address,usize> ensures memory(read) local;
    cfn<NativeCodeBuffer,address,u64> reserve;
    cfn<void,address,u64> commit;
    cfn<void,address,u64,u64,bool> literal;
    cfn<void,address,u64,ScalarMathReference> math_symbol;
};
extern "C" u32 emit_scalar_body(address context, address emitter, Span code, ControlTargets targets, ScalarBodyHost host) on cpu
    requires nonzero(host.addresses.values.ir.inspect) requires nonzero(host.addresses.values.location)
    requires nonzero(host.addresses.values.scalar) requires nonzero(host.addresses.tile_stream)
    requires nonzero(host.local) requires nonzero(host.reserve) requires nonzero(host.commit)
    requires nonzero(host.literal) requires nonzero(host.math_symbol)
    requires nonzero(host.mark) requires nonzero(host.publish) requires nonzero(host.emit)
    requires nonzero(host.control.emit) requires nonzero(host.control.load) requires nonzero(host.control.buffer)
    requires nonzero(host.control.commit);

struct CustomRangeInput {
    Span initialize;
    Span body;
    Span finish;
    u32 frame;
    u32 break_on_head;
    u32 publish;
    u64 summary_count;
    address summary;
    u64 saved;
};

struct CustomRangeHost {
    RangeFrameHost frame;
    cfn<AbiValueComponent,address,u64> summary_component;
};

struct CustomRangeProduct { u32 stack_bytes; u32 status; };

// B owns the frame and lexical control targets for this emission. The original
// emitter lends its buffers and owns the final image and source observations.
extern "C" CustomRangeProduct emit_custom_range(address emitter, CustomRangeInput input,
    ScalarBodyHost scalar, CustomRangeHost host) on cpu
    requires nonzero(host.frame.reserve) requires nonzero(host.frame.publish)
    requires nonzero(host.summary_component)
    requires nonzero(scalar.addresses.values.ir.inspect) requires nonzero(scalar.addresses.values.location)
    requires nonzero(scalar.addresses.values.scalar) requires nonzero(scalar.addresses.tile_stream)
    requires nonzero(scalar.local) requires nonzero(scalar.reserve) requires nonzero(scalar.commit)
    requires nonzero(scalar.literal) requires nonzero(scalar.math_symbol)
    requires nonzero(scalar.mark) requires nonzero(scalar.publish) requires nonzero(scalar.emit)
    requires nonzero(scalar.control.emit) requires nonzero(scalar.control.load) requires nonzero(scalar.control.buffer)
    requires nonzero(scalar.control.commit);
