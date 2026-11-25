module bootstrap.frame_layout;
import bootstrap.x64_instruction;
import bootstrap.unwind;

u64 preserve_registers(address saved, u64 count, u64 offset,
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

u64 save_stack_register(u8 register, u64 cursor, u64 index, FrameOutput output) on cpu {
    address code = unsafe_address(address_bits(output.code) + cursor);
    u64 end = cursor + stack_register(register, false, code);
    address row = unsafe_address(address_bits(output.operations) + index * size_of<Operation>());
    unsafe_position<Operation>(row) = Operation(u8(end), register, u16(0), 0);
    return end;
}

FramePrologue save_frame(u32 frame, u64 push_end, u64 pushed, FrameOutput output) on cpu {
    u64 used = pushed;
    u64 cursor = push_end;
    u64 code = address_bits(output.code);
    u64 rows = address_bits(output.operations);
    u64 probe = 0u64;
    if (frame != 0) {
        FrameEncoding encoded = allocate_frame(frame, unsafe_address(code + cursor));
        if (encoded.probe_field != 0u64) { probe = cursor + encoded.probe_field; }
        cursor += encoded.length;
        address row = unsafe_address(rows + used * size_of<Operation>());
        unsafe_position<Operation>(row) = Operation(u8(cursor), u8(0), u16(1), frame);
        used += 1u64;
    }
    address remaining = unsafe_address(rows + used * size_of<Operation>());
    u64 saved = preserve_registers(output.saved, output.count, cursor, remaining, unsafe_address(code + cursor));
    return FramePrologue(push_end, cursor, cursor + saved, probe, used + output.count);
}

extern "C" FramePrologue function_frame_prologue(u32 frame, bool scratch, u64 start, FrameOutput output) on cpu {
    u64 end = scratch ? save_stack_register(u8(15), start, 0u64, output) : start;
    return save_frame(frame, end, u64(scratch), output);
}

extern "C" RangePrologue range_prologue(u32 frame, address saved, u64 count,
    address operations, address output) on cpu {
    FrameOutput buffers = FrameOutput(output, operations, saved, count);
    u64 cursor = 0u64;
    for (index in 0..RANGE_SAVE_COUNT) {
        cursor = save_stack_register(RANGE_SAVES[index], cursor, u64(index), buffers);
    }
    FramePrologue entry = save_frame(frame, cursor, u64(RANGE_SAVE_COUNT), buffers);
    u64 args = range_arguments(unsafe_address(address_bits(output) + entry.end));
    return RangePrologue(entry.end + args, entry.probe_field, entry.operations, frame + 8 * RANGE_SAVE_COUNT, u8(entry.end));
}

struct RestoredFrame { u64 saved_end; u64 frame_end; };
RestoredFrame restore_frame(u32 frame, address saved, u64 count, u64 start) on cpu {
    u64 saved_end = start + preserve_registers(saved, count, 0u64, address(), unsafe_address(start));
    u64 frame_end = saved_end;
    if (frame != 0) { frame_end += release_frame(frame, unsafe_address(saved_end)); }
    return RestoredFrame(saved_end, frame_end);
}

extern "C" FunctionEpilogue function_epilogue(u32 frame, u32 scratch, address saved, u64 count,
    address image, u64 end, address exits, u64 exit_count) on cpu {
    patch_branches(image, end, exits, exit_count);
    u64 bytes = address_bits(image);
    RestoredFrame restored = restore_frame(frame, saved, count, bytes + end);
    u64 cursor = restored.frame_end;
    if (scratch != 0) { cursor += stack_register(u8(15), true, unsafe_address(cursor)); }
    unsafe_position<u8>(unsafe_address(cursor)) = u8(0xc3);
    return FunctionEpilogue(restored.saved_end - bytes, restored.frame_end - bytes,
        cursor - bytes, cursor + 1u64 - bytes);
}

extern "C" u64 range_epilogue(u32 frame, address saved, u64 count,
    address image, u64 end, address exits, u64 exit_count) on cpu {
    u64 start = address_bits(image) + end;
    unsafe_position<u8>(unsafe_address(start)) = u8(0x31);
    unsafe_position<u8>(unsafe_address(start + 1u64)) = u8(0xc0);
    u64 cursor = start + 2u64;
    patch_branches(image, end + 2u64, exits, exit_count);
    cursor = restore_frame(frame, saved, count, cursor).frame_end;
    for (index in 0..RANGE_SAVE_COUNT) {
        u8 register = RANGE_SAVES[RANGE_SAVE_COUNT - 1 - index];
        cursor += stack_register(register, true, unsafe_address(cursor));
    }
    unsafe_position<u8>(unsafe_address(cursor)) = u8(0xc3);
    return cursor + 1u64 - start;
}
