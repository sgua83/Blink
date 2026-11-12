module bootstrap.x64_instruction;
import bootstrap.scalar_fold;

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

struct TailRange {
    u64 start;
    u64 end;
};

// The destination has room for 17 bytes. The caller supplies a nonempty frame.
// probe_field is relative to the output, and zero when no call is emitted.
extern "C" FrameEncoding allocate_frame(u32 frame, address output) on cpu;
extern "C" u64 stack_probe_size() on cpu;
// The caller owns the code image through end plus stack_probe_size bytes.
extern "C" u64 append_stack_probe(address image, u64 end, u64 call_field) on cpu;
// The caller keeps width plus three spare bytes and all reference fields alive.
// The original reference array contains count image-relative u64 offsets.
extern "C" TailRange append_literal(address image, u64 end, u64 bits, u32 width, address references, u64 count) on cpu;
// The field address refers to the four-byte displacement at offset in the image.
extern "C" void patch_relative(address field, u64 offset, u64 target) on cpu;

struct DivisionPlan {
    u32 magic;
    u8 shift;
    u8 mode;
};
extern "C" DivisionPlan division_plan(u32 divisor, bool signed) on cpu requires nonzero(divisor);

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
extern "C" u64 read_scalar_move(MoveLocation dst, MoveLocation memory, u32 size, bool signed, bool boolean, address output) on cpu;
extern "C" u64 write_scalar_move(MoveLocation memory, MoveLocation src, u32 size, address output) on cpu;

struct BinaryEncoding {
    u64 length;
    u64 left_literal;
    u64 right_literal;
};

// The caller owns 64 output bytes. Literal fields are output-relative disp32
// offsets, or zero when that input does not use the original literal pool.
extern "C" BinaryEncoding scalar_compare(MoveLocation dst, MoveLocation left, MoveLocation right,
    u32 size, bool signed, bool floating, BinaryOperation operation, address output) on cpu;
extern "C" BinaryEncoding scalar_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    u32 size, bool signed, bool floating, u32 count_size, bool count_signed,
    BinaryOperation operation, address output) on cpu;
extern "C" BinaryEncoding float_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    u32 size, u8 opcode, address output) on cpu;

// These identities follow the original IR UnaryOp, including its bit-copy operation.
enum ValueUnaryOperation:u32 {
    FloatNegate=0; SignedNegate=1; UnsignedNegate=2; LogicalNot=3; BitNot=4; Bitcast=5;
};
extern "C" bool bit_copy(ScalarEncoding from, ScalarEncoding into) on cpu;
// Unary, math and selection receive 64 output bytes. Cast receives 80 bytes.
// Cast's left literal is the original input; its right literal is f32 2^63.
extern "C" MoveEncoding value_unary(MoveLocation dst, MoveLocation src, ScalarEncoding scalar,
    ValueUnaryOperation operation, address output) on cpu;
extern "C" MoveEncoding value_math(MoveLocation dst, MoveLocation src, u32 size,
    bool sqrt, address output) on cpu;
extern "C" BinaryEncoding value_cast(MoveLocation dst, MoveLocation src, ScalarEncoding from,
    ScalarEncoding into, address output) on cpu;
extern "C" BinaryEncoding value_select(MoveLocation dst, MoveLocation condition, MoveLocation left,
    MoveLocation right, u32 size, bool signed, address output) on cpu;

// kind is 1 for quotient, 2 for remainder, or 3 for both. Only kind 3 uses second.
struct DivisionOutputs { MoveLocation first; MoveLocation second; u32 kind; };
// The packet caller already has its numerator in EAX and divisor in ECX.
// Output has room for the original eleven-byte signed sequence.
extern "C" u64 divide_value_instruction(bool signed, address output) on cpu;
// Full scalar division entries borrow 128 output bytes. The 32-bit constant is
// the original nonzero divisor normalized by its Scalar, including its sign.
extern "C" u64 divide_constant32(DivisionOutputs result, MoveLocation src, u32 divisor,
    ScalarEncoding scalar, address output) on cpu requires nonzero(divisor);
extern "C" u64 divide_value(DivisionOutputs result, MoveLocation src, MoveLocation divisor,
    ScalarEncoding scalar, ScalarEncoding divisor_scalar, address output) on cpu;

enum PacketMathOperation:u32 { Sqrt=0; Abs=1; Min=2; Max=3; };
// Packet entries borrow the existing four-lane allocation; scratch XMM0..3 and
// EAX/ECX follow that allocation. Constant has 24 output bytes; load/store 16;
// from_word and multiply 64; the full numeric entries have 256 bytes.
extern "C" u64 packet_constant(u8 dst, u32 bits, address output) on cpu;
extern "C" u64 packet_load(MoveLocation src, u8 dst, address output) on cpu;
extern "C" u64 packet_store(MoveLocation dst, u8 src, address output) on cpu;
extern "C" u64 packet_copy(MoveLocation dst, MoveLocation src, address output) on cpu;
extern "C" u64 packet_from_word(bool boolean, u8 dst, address output) on cpu;
extern "C" u64 packet_multiply(bool broadcast, address output) on cpu;
extern "C" u64 packet_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    ScalarEncoding scalar, BinaryOperation operation, bool uniform_count, address output) on cpu;
extern "C" u64 packet_unary(MoveLocation dst, MoveLocation src, ValueUnaryOperation operation, address output) on cpu;
extern "C" u64 packet_cast(MoveLocation dst, MoveLocation src, ScalarEncoding from,
    ScalarEncoding into, address output) on cpu;
extern "C" u64 packet_math(MoveLocation dst, MoveLocation left, MoveLocation right,
    PacketMathOperation operation, address output) on cpu;
extern "C" u64 packet_select(MoveLocation dst, MoveLocation condition, MoveLocation left,
    MoveLocation right, address output) on cpu;

// The caller owns the output and consumes the returned native memory operand.
// B retains no address after return; the emitter owns the code and memory operand.
struct AddressEncoding { u64 length; u32 offset; u8 base; u8 index; u8 shift; };
// value_memory needs 32 output bytes; view_memory and its dynamic form need 64.
// scaled_memory, scale_index and offset_address need 16 output bytes.
extern "C" AddressEncoding value_memory(u8 base, MoveLocation index, u32 stride, u32 offset, address output) on cpu;
extern "C" AddressEncoding view_memory(u8 base, MoveLocation index, u64 stride, u32 offset, address output) on cpu;
extern "C" AddressEncoding view_memory_dynamic(u8 base, MoveLocation index, MoveLocation low, MoveLocation high, u32 offset, address output) on cpu;
extern "C" AddressEncoding scaled_memory(u8 base, u8 index, u32 stride, u32 offset, address output) on cpu;
extern "C" u64 scale_index(u8 index, u32 stride, address output) on cpu;
extern "C" u64 offset_address(u32 stride, address output) on cpu;
// Packet inputs use their original allocated XMM/stack lanes; no source is retained.
// packet_lane needs 16 bytes; either packet view entry needs 64 bytes.
extern "C" u64 packet_lane(MoveLocation src, u8 lane, u8 gpr, address output) on cpu;
extern "C" AddressEncoding packet_view_memory(u8 base, MoveLocation index, u8 lane, u64 stride, u32 offset, address output) on cpu;
extern "C" AddressEncoding packet_view_memory_dynamic(u8 base, MoveLocation index, MoveLocation low, MoveLocation high, u8 lane, u32 offset, address output) on cpu;

// Full packet payload emission borrows 512 bytes for read/write and 1024 for gather.
// Layout and eligibility remain with the original buffer, frame and IR owners.
struct PacketMemory {
    MoveLocation index;
    u32 data_offset; u32 stride_offset; u32 frame_offset; u32 offset;
    u32 index_kind; u32 contiguous;
};
struct PacketGather {
    MoveLocation index; MoveLocation base_low; MoveLocation base_high;
    MoveLocation stride_low; MoveLocation stride_high;
    u64 stride;
    u32 data_offset; u32 stride_offset; u32 offset; u32 kind; u32 uniform; u32 broadcast;
};
extern "C" u64 packet_read(MoveLocation dst, PacketMemory memory, bool boolean, address output) on cpu;
extern "C" u64 packet_write(MoveLocation src, PacketMemory memory, address output) on cpu;
extern "C" u64 packet_gather(MoveLocation dst, PacketGather input, bool boolean, address output) on cpu;

enum AtomicOperation:u32 {
    Load=0; Store=1; Add=2; Sub=3; Exchange=4; CompareExchange=5;
    And=6; Or=7; Xor=8; Min=9; Max=10;
};
enum AtomicStorage:u32 { Shared=0; Private=1; Split=2; };
struct AtomicSequence {
    MoveLocation low; MoveLocation high; MoveLocation first; MoveLocation second;
    ScalarEncoding scalar;
    AtomicOperation operation; AtomicStorage storage;
};
// The caller supplies 128 output bytes. The original value, when consumed, is in RAX.
extern "C" u64 atomic_sequence(AtomicSequence input, address output) on cpu;

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

struct AppendBranch {
    u64 length;
    u64 full;
};

// The emitter supplies 96 reservation bytes and 16 result bytes. The
// reservation returns the full-path disp32; the result returns its target.
extern "C" AppendBranch reserve_append(u32 counter_slot, u32 counter_word,
    u32 capacity_slot, u32 target_slot, address output) on cpu;
extern "C" AppendBranch append_result(address output) on cpu;

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
