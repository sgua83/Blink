module bootstrap.frame_layout;
import bootstrap.x64_instruction;
import bootstrap.unwind;

extern "C" u64 preserve_registers(address saved, u64 count, u64 offset,
    address operations, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    bool restore = address_bits(operations) == 0u64;
    array<u8,2> opcode = array<u8,2>(u8(0), u8(0));
    u64 index = 0u64;
    while (index < count) {
        address row = unsafe_address(address_bits(saved) + index * size_of<SavedRegister>());
        SavedRegister value = unsafe_position<SavedRegister>(row);
        bool xmm = value.bank != u16(0);
        u8 register = u8(value.register);
        Encoding input = Encoding(value.offset, register, u8(4), u8(255), u8(0), u8(2), u8(0), u16(!xmm));
        opcode[0] = xmm ? u8(0x0f) : (restore ? u8(0x8b) : u8(0x89));
        opcode[1] = restore ? u8(0x28) : u8(0x29);
        cursor += encode(input, address_of(opcode[0]), xmm ? 2u64 : 1u64, unsafe_address(cursor));
        if (!restore) {
            address record = unsafe_address(address_bits(operations) + index * size_of<Operation>());
            u16 kind = xmm ? u16(3) : u16(2);
            unsafe_position<Operation>(record) = Operation(u8(offset + cursor - start), register, kind, value.offset);
        }
        index += 1u64;
    }
    return cursor - start;
}


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

address tile_row(address rows, u64 index) on cpu {
    return unsafe_address(address_bits(rows) + index * size_of<u64>());
}

extern "C" u64 tile_frame(u64 data_end, u64 width, bool steps,
    address shared, u64 shared_count, address private_slots, u64 private_count) on cpu {
    u64 stride = width * 4u64;
    u64 base = data_end + 4u64;
    if (steps) { base += 4u64; }
    u64 end = base;
    u64 index = 0u64;
    while (index < shared_count) {
        address row = tile_row(shared, index);
        u64 position = base + unsafe_position<u64>(row) * stride;
        unsafe_position<u64>(row) = position;
        end = max(end, position + stride);
        index += 1u64;
    }
    index = 0u64;
    while (index < private_count) {
        address row = tile_row(private_slots, index);
        u64 bytes = unsafe_position<u64>(row);
        unsafe_position<u64>(row) = end;
        end += ((bytes + 3u64) / 4u64) * stride;
        index += 1u64;
    }
    return end;
}
