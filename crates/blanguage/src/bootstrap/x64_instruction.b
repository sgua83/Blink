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
