module bootstrap.unwind;

struct Operation {
    u8 offset;
    u8 reg;
    u16 kind;
    u32 amount;
};

// Operations are in prologue instruction order. The caller owns the
// input and provides exactly unwind_size bytes for the finished record.
extern "C" u64 unwind_size(address operations, u64 count) on cpu;
extern "C" void unwind_write(address operations, u64 count, u8 prolog, address output) on cpu;
