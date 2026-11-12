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

// RAX holds the u64 source. Output has room for 49 bytes.
extern "C" u64 unsigned_to_float(u8 target, u8 prefix, address output) on cpu;
// XMM0 holds the source and XMM1 holds 2^63. Output has room for 34 bytes.
extern "C" u64 float_to_unsigned(u8 target, u8 prefix, address output) on cpu;

struct DivisionPlan {
    u32 magic;
    u8 shift;
    u8 mode;
};
extern "C" DivisionPlan division_plan(u32 divisor) on cpu requires nonzero(divisor);

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
extern "C" MoveEncoding cast_boolean(MoveLocation dst, MoveLocation src,
    u32 size, bool signed, bool floating, address output) on cpu;
