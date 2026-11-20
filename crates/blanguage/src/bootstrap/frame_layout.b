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

const u32 RANGE_SAVE_COUNT = 5;
const array<u8,RANGE_SAVE_COUNT> RANGE_SAVES =
    array<u8,RANGE_SAVE_COUNT>(u8(12), u8(13), u8(14), u8(15), u8(3));

extern "C" RangePrologue range_prologue(u32 frame, address saved, u64 count,
    address operations, address output) on cpu {
    u64 start = address_bits(output);
    u64 rows = address_bits(operations);
    u64 cursor = start;
    for (index in 0..RANGE_SAVE_COUNT) {
        u8 register = RANGE_SAVES[index];
        cursor += stack_register(register, false, unsafe_address(cursor));
        address row = unsafe_address(rows + u64(index) * size_of<Operation>());
        unsafe_position<Operation>(row) = Operation(u8(cursor - start), register, u16(0), 0);
    }
    u64 used = u64(RANGE_SAVE_COUNT);
    u64 probe = 0u64;
    if (frame != 0) {
        FrameEncoding encoded = allocate_frame(frame, unsafe_address(cursor));
        if (encoded.probe_field != 0u64) { probe = cursor - start + encoded.probe_field; }
        cursor += encoded.length;
        address row = unsafe_address(rows + used * size_of<Operation>());
        unsafe_position<Operation>(row) = Operation(u8(cursor - start), u8(0), u16(1), frame);
        used += 1u64;
    }
    address remaining = unsafe_address(rows + used * size_of<Operation>());
    cursor += preserve_registers(saved, count, cursor - start, remaining, unsafe_address(cursor));
    u8 prolog = u8(cursor - start);
    cursor += range_arguments(unsafe_address(cursor));
    return RangePrologue(cursor - start, probe, used + count, frame + 8 * RANGE_SAVE_COUNT, prolog);
}

extern "C" u64 range_epilogue(u32 frame, address saved, u64 count,
    address image, u64 end, address exits, u64 exit_count) on cpu {
    u64 start = address_bits(image) + end;
    unsafe_position<u8>(unsafe_address(start)) = u8(0x31);
    unsafe_position<u8>(unsafe_address(start + 1u64)) = u8(0xc0);
    u64 cursor = start + 2u64;
    patch_branches(image, end + 2u64, exits, exit_count);
    cursor += preserve_registers(saved, count, 0u64, address(), unsafe_address(cursor));
    if (frame != 0) { cursor += release_frame(frame, unsafe_address(cursor)); }
    for (index in 0..RANGE_SAVE_COUNT) {
        u8 register = RANGE_SAVES[RANGE_SAVE_COUNT - 1 - index];
        cursor += stack_register(register, true, unsafe_address(cursor));
    }
    unsafe_position<u8>(unsafe_address(cursor)) = u8(0xc3);
    return cursor + 1u64 - start;
}
