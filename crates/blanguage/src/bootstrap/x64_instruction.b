module bootstrap.x64_instruction;

const u8 OPCODE_REGISTER = u8(0);
const u8 DIRECT_REGISTER = u8(1);
const u8 MEMORY = u8(2);
const u8 NO_INDEX = u8(255);

extern "C" u64 encode(Encoding input, address opcode, u64 opcode_length, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    if (input.prefix != u8(0)) {
        unsafe_position<u8>(unsafe_address(cursor)) = input.prefix;
        cursor += 1u64;
    }
    bool byte_register = false;
    if (opcode_length == 1u64) {
        u8 first = unsafe_position<u8>(opcode);
        byte_register = first == u8(0x88) || first == u8(0x8a);
    }
    if (opcode_length == 2u64 && unsafe_position<u8>(opcode) == u8(0x0f)) {
        u8 second = unsafe_position<u8>(unsafe_address(address_bits(opcode) + 1u64));
        byte_register = second == u8(0xb6) || second == u8(0xbe);
    }
    u32 rex = 0x40 | (u32(input.wide) << 3) | ((u32(input.reg) >> 3) << 2) | (u32(input.base) >> 3);
    bool has_index = input.index != NO_INDEX;
    if (has_index) {
        rex |= (u32(input.index) >> 3) << 1;
    }
    if (rex != 0x40 || byte_register) {
        unsafe_position<u8>(unsafe_address(cursor)) = u8(rex);
        cursor += 1u64;
    }
    u64 index = 0u64;
    while (index < opcode_length) {
        u8 byte = unsafe_position<u8>(unsafe_address(address_bits(opcode) + index));
        unsafe_position<u8>(unsafe_address(cursor)) = byte;
        cursor += 1u64;
        index += 1u64;
    }
    if (input.form == OPCODE_REGISTER) {
        return cursor - start;
    }

    u32 reg_field = (u32(input.reg) & 7) << 3;
    u32 base_field = u32(input.base) & 7;
    // RIP-relative literal reads use 00/r/101 and a zero disp32 for later patching.
    u32 modrm = reg_field | 5;
    u32 displacement = 0;
    u32 remaining = 4;
    bool has_sib = false;
    u32 sib = 0x24;
    if (input.form == DIRECT_REGISTER) {
        modrm = 0xc0 | reg_field | base_field;
        remaining = 0;
    }
    if (input.form == MEMORY) {
        displacement = input.displacement;
        u32 mode = 2;
        if (displacement == 0 && base_field != 5) {
            mode = 0;
            remaining = 0;
        } else if (displacement <= 127) {
            mode = 1;
            remaining = 1;
        }
        has_sib = has_index || base_field == 4;
        u32 rm = base_field;
        if (has_sib) {
            rm = 4;
        }
        modrm = (mode << 6) | reg_field | rm;
        if (has_index) {
            sib = (u32(input.shift) << 6) | ((u32(input.index) & 7) << 3) | base_field;
        }
    }
    unsafe_position<u8>(unsafe_address(cursor)) = u8(modrm);
    cursor += 1u64;
    if (has_sib) {
        unsafe_position<u8>(unsafe_address(cursor)) = u8(sib);
        cursor += 1u64;
    }
    while (remaining != 0) {
        unsafe_position<u8>(unsafe_address(cursor)) = u8(displacement);
        cursor += 1u64;
        displacement >>= 8;
        remaining -= 1;
    }
    return cursor - start;
}

void write_word(u64 at, u32 word) on cpu {
    unsafe_position<u8>(unsafe_address(at)) = u8(word);
    unsafe_position<u8>(unsafe_address(at + 1u64)) = u8(word >> 8);
    unsafe_position<u8>(unsafe_address(at + 2u64)) = u8(word >> 16);
    unsafe_position<u8>(unsafe_address(at + 3u64)) = u8(word >> 24);
}

extern "C" void patch_relative(address field, u64 offset, u64 target) on cpu {
    write_word(address_bits(field), u32(target) - u32(offset + 4u64));
}

extern "C" FrameEncoding allocate_frame(u32 frame, address output) on cpu {
    u64 cursor = address_bits(output);
    u64 probe_field = 0u64;
    if (frame >= 4096) {
        // The leaf helper takes the byte count in EAX.
        unsafe_position<u8>(unsafe_address(cursor)) = u8(0xb8);
        write_word(cursor + 1u64, frame);
        unsafe_position<u8>(unsafe_address(cursor + 5u64)) = u8(0xe8);
        probe_field = 6u64;
        write_word(cursor + probe_field, 0);
        cursor += 10u64;
    }
    unsafe_position<u8>(unsafe_address(cursor)) = u8(0x48);
    unsafe_position<u8>(unsafe_address(cursor + 1u64)) = u8(0x81);
    unsafe_position<u8>(unsafe_address(cursor + 2u64)) = u8(0xec);
    write_word(cursor + 3u64, frame);
    return FrameEncoding(cursor + 7u64 - address_bits(output), probe_field);
}

// The leaf touches each page from the caller's stack pointer to the new end.
// It preserves RAX and the argument registers. Its internal branches are fixed.
const u32 STACK_PROBE_LENGTH = 52;
const array<u8,STACK_PROBE_LENGTH> STACK_PROBE = array<u8,STACK_PROBE_LENGTH>(
    u8(0x4c), u8(0x8b), u8(0xd4), u8(0x49), u8(0x83), u8(0xc2), u8(0x08), u8(0x49), u8(0x89), u8(0xc3),
    u8(0x49), u8(0x81), u8(0xfb), u8(0x00), u8(0x10), u8(0x00), u8(0x00),
    u8(0x0f), u8(0x82), u8(0x16), u8(0x00), u8(0x00), u8(0x00),
    u8(0x49), u8(0x81), u8(0xea), u8(0x00), u8(0x10), u8(0x00), u8(0x00),
    u8(0x41), u8(0x85), u8(0x02), u8(0x49), u8(0x81), u8(0xeb), u8(0x00), u8(0x10), u8(0x00), u8(0x00),
    u8(0xe9), u8(0xdd), u8(0xff), u8(0xff), u8(0xff),
    u8(0x4d), u8(0x29), u8(0xda), u8(0x41), u8(0x85), u8(0x02), u8(0xc3)
);

extern "C" u64 stack_probe_size() on cpu {
    return u64(STACK_PROBE_LENGTH);
}

extern "C" u64 append_stack_probe(address image, u64 end, u64 call_field) on cpu {
    u64 base = address_bits(image);
    patch_relative(unsafe_address(base + call_field), call_field, end);
    for (index in 0..STACK_PROBE_LENGTH) {
        unsafe_position<u8>(unsafe_address(base + end + u64(index))) = STACK_PROBE[index];
    }
    return end + u64(STACK_PROBE_LENGTH);
}

extern "C" TailRange append_literal(address image, u64 end, u64 bits, u32 width, address references, u64 count) on cpu {
    u64 base = address_bits(image);
    u64 start = ((end + 3u64) / 4u64) * 4u64;
    u64 cursor = end;
    while (cursor < start) {
        unsafe_position<u8>(unsafe_address(base + cursor)) = u8(0);
        cursor += 1u64;
    }
    write_word(base + start, u32(bits));
    if (width == 8) {
        write_word(base + start + 4u64, u32(bits >> 32));
    }
    u64 index = 0u64;
    while (index < count) {
        u64 field = unsafe_position<u64>(unsafe_address(address_bits(references) + index * 8u64));
        patch_relative(unsafe_address(base + field), field, start);
        index += 1u64;
    }
    return TailRange(start, start + u64(width));
}
