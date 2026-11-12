module bootstrap.frame_layout;

extern "C" u64 saved_registers(u64 data_end, u32 gpr_mask, u32 xmm_mask, address saves) on cpu {
    u64 end = data_end;
    u64 index = 0u64;
    u32 bank = 0;
    while (bank < 2) {
        u32 used = gpr_mask;
        u64 stride = 8u64;
        if (bank == 1) {
            used = xmm_mask;
            stride = 16u64;
        }
        end = ((end + stride - 1u64) / stride) * stride;
        u32 reg = 0;
        while (used != 0) {
            if ((used & 1) != 0) {
                address at = unsafe_address(address_bits(saves) + index * size_of<SavedRegister>());
                unsafe_position<SavedRegister>(at) = SavedRegister(u32(end), u16(reg), u16(bank));
                end += stride;
                index += 1u64;
            }
            used >>= 1;
            reg += 1;
        }
        bank += 1;
    }
    return end;
}
