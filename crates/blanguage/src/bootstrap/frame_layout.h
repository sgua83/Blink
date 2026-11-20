module bootstrap.frame_layout;

struct SavedRegister {
    u32 offset;
    u16 register;
    u16 bank;
};

// The caller owns one output row per used register bit. GPRs precede XMMs.
extern "C" u64 saved_registers(u64 data_end, u32 gpr_mask, u32 xmm_mask, address saves) on cpu;

// Shared rows contain original word slots. Private rows contain lane byte widths.
// Replace both sets of rows with their stack byte positions.
extern "C" u64 tile_frame(u64 data_end, u64 width, bool steps,
    address shared, u64 shared_count, address private_slots, u64 private_count) on cpu;

// The caller owns nine output bytes per saved register and one unwind record
// per save. An empty operations address denotes restoration.
extern "C" u64 preserve_registers(address saved, u64 count, u64 offset,
    address operations, address output) on cpu;

// Range entry starts at image offset zero. Lend 39 + 9 * count bytes and
// 6 + count unwind rows. stack_bytes includes the five fixed pushes.
struct RangePrologue {
    u64 length;
    u64 probe_field;
    u64 operations;
    u32 stack_bytes;
    u8 prolog;
};
extern "C" RangePrologue range_prologue(u32 frame, address saved, u64 count,
    address operations, address output) on cpu;

// The caller lends 20 + 9 * count bytes at end. Exit rel32 fields point to
// restoration after the success status write.
extern "C" u64 range_epilogue(u32 frame, address saved, u64 count,
    address image, u64 end, address exits, u64 exit_count) on cpu;
