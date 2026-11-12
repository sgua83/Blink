module bootstrap.x64_instruction;

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
