module bootstrap.x64_instruction;
import bootstrap.scalar_fold;
import bootstrap.ir_dataflow;

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
    u64 target;
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
// The borrowed literals stay in emission order. They own their existing reference lists
// and receive target offsets. Regions holds the probe, optional initial padding and each literal.
extern "C" CodeTail write_tail(address image,u64 end,u64 probe,address literals,
    u64 literal_count,address regions) on cpu;
// The field address refers to the four-byte displacement at offset in the image.
extern "C" void patch_relative(address field, u64 offset, u64 target) on cpu;
// Known-target jump: output has five bytes and offset is its function-relative start.
extern "C" u64 jump_to(u64 offset, u64 target, address output) on cpu;
// fields is the scope's existing array of u64 function-relative rel32 positions.
extern "C" void patch_branches(address image, u64 target, address fields, u64 count) on cpu;

// condition is zero for JMP; test_bits is zero for flags, or 32/64 for EAX/RAX.
// The caller owns nine output bytes. The returned length includes the rel32.
enum ControlKind:u32 { If=0; Loop=1; Scope=2; Exit=3; Break=4; Continue=5; Raise=6; };
enum ControlJump:u32 { Always=0; Zero=1; Nonzero=2; };
enum ControlScope:u32 { Block=0; Loop=1; };
enum ControlTarget:u32 { Block=0; Loop=1; Function=2; };
struct ControlBlock { address start; u64 count; };
struct ControlInstruction { ControlBlock first; ControlBlock second; u64 register; ControlKind kind; };
struct ControlHost {
    cfn<u32,address,address,ControlBlock> emit;
    cfn<void,address,address,u64> load;
    cfn<u64,address> offset;
    cfn<u64,address,ControlJump> jump;
    cfn<void,address,u64> jump_to;
    cfn<void,address,u64> patch_here;
    cfn<void,address,ControlScope,u64> open;
    cfn<void,address,ControlScope> close;
    cfn<void,address,ControlTarget,u64> defer;
    cfn<u64,address> continuation;
};
extern "C" void emit_control(address context,address emitter,ControlInstruction instruction,ControlHost host) on cpu
    requires nonzero(host.emit) requires nonzero(host.load) requires nonzero(host.offset)
    requires nonzero(host.jump) requires nonzero(host.jump_to) requires nonzero(host.patch_here)
    requires nonzero(host.open) requires nonzero(host.close) requires nonzero(host.defer)
    requires nonzero(host.continuation);

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

enum PacketBodyKind:u32 {
    Done=0; Failed=1; Constant=2; Index=3; Uniform=4; Copy=5; Unary=6;
    Binary=7; Cast=8; Math=9; Select=10; IntegerDivide=11;
};
struct PacketBodyOp {
    array<MoveLocation,4> locations;
    ScalarEncoding source;
    ScalarEncoding target;
    PacketBodyKind kind;
    u32 literal;
    u32 flag;
    ValueUnaryOperation unary;
    BinaryOperation binary;
    PacketMathOperation math;
    address instruction;
};
struct PacketBodySpan { u64 start; u64 first; };
struct PacketBodyHost {
    cfn<PacketBodyOp,address,u64> step;
    cfn<PacketBodySpan,address> span;
    cfn<void,address,u64,PacketBodySpan> range;
    cfn<address,address,u64> reserve;
    cfn<void,address,u64> finish;
    ScalarValueHost values;
};
extern "C" void emit_packet_body(address context, u64 count, bool sparse, u32 slots, PacketBodyHost host) on cpu
    requires nonzero(host.step) requires nonzero(host.span) requires nonzero(host.range)
    requires nonzero(host.reserve) requires nonzero(host.finish)
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location) requires nonzero(host.values.scalar);

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

// The caller supplies 512 bytes for a dataset operation and 1024 for a view read.
struct PacketMemory {
    MoveLocation index;
    u32 data_offset; u32 stride_offset; u32 frame_offset; u32 offset;
    PacketAddress mode;
};
struct PacketView {
    MoveLocation index; MoveLocation base_low; MoveLocation base_high;
    MoveLocation stride_low; MoveLocation stride_high;
    u64 stride;
    u32 offset; bool dynamic_stride; PacketViewAddress mode;
};
extern "C" u64 packet_read(MoveLocation dst, PacketMemory memory, bool boolean, address output) on cpu;
extern "C" u64 packet_write(MoveLocation src, PacketMemory memory, address output) on cpu;
extern "C" u64 packet_view(MoveLocation dst, PacketView input, bool boolean, address output) on cpu;

struct CompactScatter {
    u32 source; u32 target; u32 flags; u32 prefix;
    u32 stride_offset; u32 flag_word; u32 record_bytes;
};

// The range prologue and unwind records already belong to the emitter.
// Output has space for 96 count bytes or 224 scatter bytes.
extern "C" u64 compact_count(u32 flags, u32 stride_offset, u32 flag_word, address output) on cpu;
extern "C" u64 compact_scatter(CompactScatter input, address output) on cpu;

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

// The emitter owns the prologue and unwind. The caller supplies 160, 96,
// 128 or 192 output bytes for the four respective entry points.
extern "C" u64 sort_key(SortKeyEmission input, address output) on cpu;
extern "C" u64 sort_histogram(u32 buffer_bytes, address output) on cpu;
extern "C" u64 sort_scatter(u32 buffer_bytes, address output) on cpu;
extern "C" u64 sort_gather(u32 buffer_bytes, u32 stride_offset, u32 record_bytes, address output) on cpu;


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

struct ReductionCode {
    u64 length;
    array<u64,4> literals;
};

struct TileInputBoundaryCode {
    u64 length;
    u64 skip_field;
};

struct TileInputCopy {
    u32 data_offset;
    u32 stride_offset;
    u32 source_offset;
    u32 storage_offset;
    u32 length_offset;
    u32 unit_bytes;
};

// The caller owns 16 bytes for tile_lane and 48 for tile_input_boundary.
// The boundary's skip field is a rel32 over the complete input copy.
extern "C" u64 tile_lane(u32 width, address output) on cpu;
extern "C" TileInputBoundaryCode tile_input_boundary(u32 width, u32 length_offset, address output) on cpu;
// The caller owns 80 bytes for the loop over the actual lanes in the tile.
extern "C" u64 tile_input_copy(TileInputCopy input, address output) on cpu;

enum TextureAccessKind:u32 {
    Binding=0; Levels=1; Address=2; Format=3; Size=4; Byte=5; Word=6; Rgba=7; Half=8;
};

struct TextureAccess {
    MoveLocation base;
    MoveLocation element;
    ScalarEncoding element_scalar;
    MoveLocation level;
    array<MoveLocation,3> coordinates;
    u32 in_table;
    u32 stride;
    TextureAccessKind kind;
    u32 dimensions;
};

// R12 is the binding table. R15 holds the transient binding and source address.
// The caller owns the result locations and room for the whole operation.
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

extern "C" u64 texture_access(TextureAccess input, address results, u64 count, address output) on cpu;

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
extern "C" TilePublishCode tile_publish_value(TilePublishInput input, address output) on cpu;

struct TilePrivateCopy {
    MoveLocation location;
    ScalarEncoding scalar;
    u32 storage_offset;
    u32 plane_stride;
    bool restore;
};

// The caller owns 16 bytes for the lane pointer and 32 for the phase back edge.
// bytes_back is the distance from output to the start of the current phase loop.
extern "C" u64 tile_lane_pointer(u32 base_offset, address output) on cpu;
extern "C" u64 tile_phase_advance(u32 base_offset, u32 length_offset, u64 bytes_back, address output) on cpu;
// The caller owns 64 bytes. R10 points to the current lane's word in each plane.
extern "C" u64 tile_private_copy(TilePrivateCopy input, address output) on cpu;

struct TileCollectiveInput {
    u64 identity;
    ScalarEncoding scalar;
    ReductionOperator operation;
    bool exclusive;
    u32 input_offset;
    u32 output_offset;
    u32 length_offset;
    u32 plane_stride;
    u32 input_high_delta;
};

struct TileCollectiveCode {
    u64 length;
    u64 identity_start;
    u64 loop_start;
    u64 accumulate_start;
    u64 fill_start;
    u64 fill_loop_start;
};

// The caller owns 128 output bytes. Positions are relative to the first byte.
extern "C" TileCollectiveCode tile_collective(TileCollectiveInput input, address output) on cpu;

// R12 has already been saved. The caller owns 256 output bytes and the literal pool.
extern "C" ReductionCode reduce_range(ReductionRange input, address output) on cpu;

struct SegmentedRange {
    ReductionRange values;
    u32 target;
    u32 target_word;
    u32 parameters;
    u32 head_word;
    u32 value_words;
};

// R12 and R13 have been saved. The caller owns 512 output bytes.
extern "C" u64 segmented_reduce_range(SegmentedRange input, u32 finish, address output) on cpu;

struct ScanRange {
    ReductionRange values;
    u32 target;
    u32 target_word;
    u32 parameters;
};

// R12 is saved for both passes, and R13 is saved for the summary pass.
extern "C" ReductionCode scan_range(ScanRange input, u32 add, address output) on cpu;

struct SegmentedScanRange {
    ScanRange scan;
    u32 head_word;
    u32 value_words;
};

// R12 and R13 are saved by the caller for either pass.
extern "C" u64 segmented_scan_range(SegmentedScanRange input, u32 add, address output) on cpu;

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

enum PacketPlanKind:u32 {
    Unsupported=0; Constant=1; Index=2; Uniform=3; LocalLoad=4; LocalStore=5;
    Load=6; ViewLoad=7; Store=8; Unary=9; Binary=10; Divide=11; Cast=12;
    Math=13; Select=14; If=15; Loop=16; Scope=17; Exit=18; Break=19; Continue=20;
};
enum PacketPlanOperation:u32 {
    Constant=0; Index=1; Uniform=2; LocalLoad=3; LocalStore=4; Load=5; ViewLoad=6;
    Store=7; Unary=8; Binary=9; Divide=10; Cast=11; Copy=12; Math=13; Select=14;
    If=15; Loop=16; Scope=17; Exit=18; Break=19; Continue=20;
};
enum PacketPlanVisit:u32 { Operands=0; Results=1; Stride=2; Widths=3; };
struct PacketPlanBlock { address instructions;u64 count; };
struct PacketPlanNode {
    PacketPlanBlock first;PacketPlanBlock second;
    address instruction;address payload;address other;address arguments;u64 argument_count;
    u64 dst;u64 a;u64 b;u64 c;u64 binding;u64 bits;
    u32 word;u32 stride;u32 writes;PacketPlanKind kind;u32 math;
    ScalarEncoding from;ScalarEncoding into;
};
struct PacketPlanHeader {u64 registers;u64 slots;u64 writes;u32 tile;u32 selected;};
struct PacketPlanState {address owner;address bytes;u64 count;};
struct PacketPlanFlow {PacketPlanState next;PacketPlanState breaks;PacketPlanState continues;PacketPlanState exits;};
struct PacketPlanDefinition {PacketPlanState state;address definitions;u32 uniform;u32 record;};
struct PacketPlanDecision {
    PacketPlanOperation operation;u32 uniform_count;PacketAddress dataset;PacketViewAddress view;PacketMathOperation math;
};
struct PacketPlanHost {
    cfn<PacketPlanNode,address,PacketPlanBlock,u64> read;
    cfn<u32,address,address,PacketPlanVisit,address,cfn<u32,address,u64>> visit;
    cfn<PacketPlanState,u64,address> state_new;
    cfn<void,PacketPlanState> state_drop;
    cfn<address> plan_new;
    cfn<void,address> plan_drop;
    cfn<void,address,PacketPlanNode,PacketPlanDecision,address,address> append;
    cfn<void,address,u64> address_only;
};
// All handles are actual caller-owned buffers or Step vectors, borrowed synchronously.
extern "C" address packet_plan(address context,PacketPlanBlock code,PacketPlanHeader header,PacketPlanHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.visit) requires nonzero(host.state_new) requires nonzero(host.state_drop) requires nonzero(host.plan_new) requires nonzero(host.plan_drop) requires nonzero(host.append) requires nonzero(host.address_only);
extern "C" u32 packet_plan_uniform_operand(address bytes,u64 reg) on cpu;
extern "C" u32 packet_plan_mark_operand(address bytes,u64 reg) on cpu;
extern "C" u32 packet_plan_definition(address context,u64 reg) on cpu;
extern "C" u32 packet_plan_width(address context,u64 bytes) on cpu;

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
struct ScalarValueEncoding {
    u64 length;
    ScalarLiteral first;
    ScalarLiteral second;
    u64 call_field;
    u32 symbol_length;
    array<u8,6> symbol;
};
// Borrows the original scalar value or integer division instruction and 128 writable bytes.
extern "C" ScalarValueEncoding emit_scalar_value(Node source,
    ScalarValueHost host, address output) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.location) requires nonzero(host.scalar);


enum MapRangePart:u32 {
    Enter=0; PacketBounds=1; PacketCollect=2; PacketAdvance=3; PacketBackEdge=4;
    PacketTail=5; ScalarBounds=6; ScalarSelection=7; ScalarAdvance=8; ScalarBackEdge=9; Exit=10;
};
enum MapRangeBody:u32 { Packet=0; Scalar=1; TileInput=2; TilePhases=3; };
struct MapRangeInput {
    address packet;
    usize binding;
    u32 word;
    u32 selected;
    u32 frame;
    u32 slots;
    u32 buffer_bytes;
    u32 stride_field;
    u32 tile_scope;
    u32 has_phases;
};
struct MapRangeBuffer { address image; u64 end; };
struct MapRangeEntry { u64 end; u32 stack_bytes; };
struct MapRangeObservation { u64 start; u64 end; usize binding; u32 value; MapRangePart part; };
struct MapRangeHost {
    address context;
    cfn<MapRangeEntry,address,u32> enter;
    cfn<u64,address,u32> leave;
    cfn<MapRangeBuffer,address,u64> reserve;
    cfn<void,address,u64> commit;
    cfn<u32,address,MapRangeBody,address> body;
    cfn<void,address,MapRangeObservation> observe;
};
// The emitter starts with an empty code buffer. A reserved image is lent only
// until the bytes are committed. The host keeps any diagnostic from body.
extern "C" u32 emit_map_range(MapRangeInput input, MapRangeHost host) on cpu
    requires nonzero(host.enter) requires nonzero(host.leave) requires nonzero(host.reserve)
    requires nonzero(host.commit) requires nonzero(host.body) requires nonzero(host.observe);

struct ScalarStorageEncoding { u64 length; u64 literal; u64 field; };
struct ScalarStorageHost {
    ScalarValueHost values;
    cfn<MoveLocation,address,usize> ensures memory(read) local;
    u32 buffer_bytes;
    u32 stride_field;
};
// The original scalar storage instruction borrows 68 output bytes.
extern "C" ScalarStorageEncoding emit_scalar_storage(Node instruction, ScalarStorageHost host,
    address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.local);

struct IndexedAddressHost {
    ScalarValueHost values;
    cfn<u32,address,usize,u32> ensures memory(read) tile_stream;
    u32 buffer_bytes;
    u32 stride_field;
    u32 workgroup;
};
// Borrows the original indexed source and 160 writable bytes.
extern "C" AddressEncoding emit_indexed_operand(Node source, usize offset, u32 word,
    bool memory, IndexedAddressHost host, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.tile_stream);
// Borrows the original view load and 112 writable bytes.
extern "C" u64 emit_view_load(Node instruction, ScalarValueHost host, address output) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.location) requires nonzero(host.scalar);

// An original atomic instruction borrows 312 writable bytes.
extern "C" u64 emit_atomic(Node instruction, IndexedAddressHost host, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.tile_stream);

// Reserves, writes and delivers the accepted flag of the original append.
extern "C" u64 emit_append(Node instruction, ScalarValueHost host, u32 buffer_bytes,
    u32 stride_field, address output) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.location) requires nonzero(host.scalar);
