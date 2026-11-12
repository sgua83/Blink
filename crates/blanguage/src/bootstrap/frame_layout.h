module bootstrap.frame_layout;

struct SavedRegister {
    u32 offset;
    u16 register;
    u16 bank;
};

// The caller owns one output row per used register bit. GPRs precede XMMs.
extern "C" u64 saved_registers(u64 data_end, u32 gpr_mask, u32 xmm_mask, address saves) on cpu;
