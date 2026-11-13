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
