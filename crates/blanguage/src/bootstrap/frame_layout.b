module bootstrap.frame_layout;
import bootstrap.x64_instruction;
import bootstrap.unwind;

u64 preserve_registers(SavedLayout saved, u64 offset, address operations, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    bool restore = address_bits(operations) == 0u64;
    array<u8,2> opcode = array<u8,2>(u8(0), u8(0));
    u64 index = 0u64;
    u32 bank = 0;
    while (bank < 2) {
        bool xmm = bank == 1;
        u32 used = xmm ? saved.xmm : saved.gpr;
        u64 stack = xmm ? saved.xmm_base : saved.gpr_base;
        u64 stride = xmm ? 16u64 : 8u64;
        opcode[0] = xmm ? u8(0x0f) : (restore ? u8(0x8b) : u8(0x89));
        opcode[1] = restore ? u8(0x28) : u8(0x29);
        u8 reg = u8(0);
        while (used != 0) {
            if ((used & 1) != 0) {
                Encoding input = Encoding(u32(stack), reg, u8(4), u8(255), u8(0), u8(2), u8(0), u16(!xmm));
                cursor += encode(input, address_of(opcode[0]), xmm ? 2u64 : 1u64, unsafe_address(cursor));
                if (!restore) {
                    address record = unsafe_address(address_bits(operations) + index * size_of<Operation>());
                    u16 kind = xmm ? u16(3) : u16(2);
                    unsafe_position<Operation>(record) = Operation(u8(offset + cursor - start), reg, kind, u32(stack));
                }
                stack += stride;
                index += 1u64;
            }
            used >>= 1;
            reg += u8(1);
        }
        bank += 1;
    }
    return cursor - start;
}

u64 saved_count(u32 mask) on cpu {
    u64 count = 0u64;
    u32 used = mask;
    while (used != 0) {
        used &= used - 1;
        count += 1u64;
    }
    return count;
}

extern "C" FrameSaveLayout frame_save_layout(ColorLayout scalar, ColorLayout packet) on cpu {
    u32 gpr = scalar.gpr | packet.gpr;
    u32 xmm = scalar.xmm | packet.xmm;
    u64 gprs = saved_count(gpr);
    u64 xmms = saved_count(xmm);
    u64 data_end = max(scalar.end, packet.end);
    u64 gpr_base = ((data_end + 7u64) / 8u64) * 8u64;
    u64 xmm_base = ((gpr_base + 8u64 * gprs + 15u64) / 16u64) * 16u64;
    return FrameSaveLayout(SavedLayout(gpr_base, xmm_base, gpr, xmm), gprs + xmms, xmm_base + 16u64 * xmms);
}

extern "C" u64 tile_frame(u64 data_end, u64 width, bool steps,
    address context, TileFrameHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.place) {
    u64 stride = width * 4u64;
    u64 base = data_end + 4u64;
    if (steps) { base += 4u64; }
    u64 end = base;
    TileFrameSlot slot = host.next(context, TileFrameBank::Shared);
    while (address_bits(slot.key) != 0u64) {
        u64 position = base + slot.value * stride;
        host.place(context, TileFrameBank::Shared, slot.key, u32(position));
        end = max(end, position + stride);
        slot = host.next(context, TileFrameBank::Shared);
    }
    slot = host.next(context, TileFrameBank::Private);
    while (address_bits(slot.key) != 0u64) {
        host.place(context, TileFrameBank::Private, slot.key, u32(end));
        end += ((slot.value + 3u64) / 4u64) * stride;
        slot = host.next(context, TileFrameBank::Private);
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
    u64 saved = preserve_registers(output.saved, cursor, remaining, unsafe_address(code + cursor));
    return FramePrologue(push_end, cursor, cursor + saved, probe, used + output.count);
}

extern "C" FramePrologue function_frame_prologue(u32 frame, bool scratch, u64 start, FrameOutput output) on cpu {
    u64 end = scratch ? save_stack_register(u8(15), start, 0u64, output) : start;
    return save_frame(frame, end, u64(scratch), output);
}

extern "C" RangePrologue range_prologue(u32 frame, u64 count, address context, RangeFrameHost host) on cpu
    requires nonzero(host.reserve) requires nonzero(host.publish) {
    FrameOutput buffers = host.reserve(context, 39u64 + 9u64 * count, 6u64 + count);
    u64 cursor = 0u64;
    for (index in 0..RANGE_SAVE_COUNT) {
        cursor = save_stack_register(RANGE_SAVES[index], cursor, u64(index), buffers);
    }
    FramePrologue entry = save_frame(frame, cursor, u64(RANGE_SAVE_COUNT), buffers);
    u64 end = entry.end + range_arguments(unsafe_address(address_bits(buffers.code) + entry.end));
    host.publish(context, FrameEntry(end, entry.probe_field, entry.operations, u8(entry.end)));
    return RangePrologue(end, buffers.saved, buffers.count, frame + 8 * RANGE_SAVE_COUNT);
}

struct RestoredFrame { u64 saved_end; u64 frame_end; };
RestoredFrame restore_frame(u32 frame, SavedLayout saved, u64 start) on cpu {
    u64 saved_end = start + preserve_registers(saved, 0u64, address(), unsafe_address(start));
    u64 frame_end = saved_end;
    if (frame != 0) { frame_end += release_frame(frame, unsafe_address(saved_end)); }
    return RestoredFrame(saved_end, frame_end);
}

extern "C" FunctionEpilogue function_epilogue(u32 frame, u32 scratch, SavedLayout saved,
    address image, u64 end, u64 exits) on cpu {
    patch_control_branches(image, end, exits);
    u64 bytes = address_bits(image);
    RestoredFrame restored = restore_frame(frame, saved, bytes + end);
    u64 cursor = restored.frame_end;
    if (scratch != 0) { cursor += stack_register(u8(15), true, unsafe_address(cursor)); }
    unsafe_position<u8>(unsafe_address(cursor)) = u8(0xc3);
    return FunctionEpilogue(restored.saved_end - bytes, restored.frame_end - bytes,
        cursor - bytes, cursor + 1u64 - bytes);
}

extern "C" u64 range_epilogue(u32 frame, RangePrologue entry, address image, u64 end, u64 exits) on cpu {
    u64 base = address_bits(image);
    u64 start = base + end;
    unsafe_position<u8>(unsafe_address(start)) = u8(0x31);
    unsafe_position<u8>(unsafe_address(start + 1u64)) = u8(0xc0);
    u64 cursor = start + 2u64;
    patch_control_branches(image, end + 2u64, exits);
    cursor = restore_frame(frame, entry.saved, cursor).frame_end;
    for (index in 0..RANGE_SAVE_COUNT) {
        u8 register = RANGE_SAVES[RANGE_SAVE_COUNT - 1 - index];
        cursor += stack_register(register, true, unsafe_address(cursor));
    }
    unsafe_position<u8>(unsafe_address(cursor)) = u8(0xc3);
    return cursor + 1u64 - base;
}
