module bootstrap.unwind;

struct Operation {
    u8 offset;
    u8 reg;
    u16 kind;
    u32 amount;
};

// Operations are in prologue instruction order. The caller owns the output.
// Capacity is the 4-byte header plus at most 6 bytes per operation, rounded up
// to a 4-byte boundary. The result is the number of initialized bytes.
extern "C" u64 unwind_write(address operations, u64 count, u8 prolog, address output) on cpu;

// The caller provides the 12 bytes of one COFF runtime-function record.
// The result describes the fields that the COFF owner binds to code or unwind data.
struct RuntimeFunctionFields {
    u32 begin;
    u32 end;
    u32 unwind;
    u32 relocation;
};
extern "C" RuntimeFunctionFields runtime_function(u32 end, address output) on cpu;
