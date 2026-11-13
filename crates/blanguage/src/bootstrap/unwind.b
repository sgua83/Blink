module bootstrap.unwind;

const u16 PUSH = u16(0);
const u16 ALLOCATE = u16(1);
const u16 SAVE_XMM = u16(3);

struct Encoding {
    u8 opcode;
    u32 extra;
    u32 value;
};

Encoding encode(Operation operation) on cpu {
    u8 reg = u8(u32(operation.reg) << 4);
    u32 scaled = operation.amount / 8;
    if (operation.kind == PUSH) {
        return Encoding(reg, 0, 0);
    }
    if (operation.kind == ALLOCATE) {
        if (operation.amount <= 128) {
            return Encoding(u8(((scaled - 1) << 4) | 2), 0, 0);
        }
        if (scaled <= 65535) {
            return Encoding(u8(1), 2, scaled);
        }
        return Encoding(u8(17), 4, operation.amount);
    }
    u8 opcode = reg | u8(4);
    if (operation.kind == SAVE_XMM) {
        opcode = reg | u8(8);
        scaled /= 2;
    }
    if (scaled <= 65535) {
        return Encoding(opcode, 2, scaled);
    }
    return Encoding(opcode + u8(1), 4, operation.amount);
}

extern "C" u64 unwind_write(address operations, u64 count, u8 prolog, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start + 4u64;
    u64 index = count;
    while (index != 0u64) {
        index -= 1u64;
        address at = unsafe_address(address_bits(operations) + index * size_of<Operation>());
        Operation operation = unsafe_position<Operation>(at);
        Encoding code = encode(operation);
        unsafe_position<u8>(unsafe_address(cursor)) = operation.offset;
        unsafe_position<u8>(unsafe_address(cursor + 1u64)) = code.opcode;
        cursor += 2u64;
        u32 value = code.value;
        for (byte in 0..code.extra) {
            unsafe_position<u8>(unsafe_address(cursor)) = u8(value);
            cursor += 1u64;
            value >>= 8;
        }
    }
    u64 bytes = cursor - start;
    unsafe_position<u8>(output) = u8(1);
    unsafe_position<u8>(unsafe_address(start + 1u64)) = prolog;
    unsafe_position<u8>(unsafe_address(start + 2u64)) = u8((bytes - 4u64) / 2u64);
    unsafe_position<u8>(unsafe_address(start + 3u64)) = u8(0);
    if ((bytes & 2u64) != 0u64) {
        unsafe_position<u8>(unsafe_address(cursor)) = u8(0);
        unsafe_position<u8>(unsafe_address(cursor + 1u64)) = u8(0);
        return bytes + 2u64;
    }
    return bytes;
}

extern "C" RuntimeFunctionFields runtime_function(u32 end, address output) on cpu {
    u64 start = address_bits(output);
    for (byte in 0..4) {
        u64 offset = u64(byte);
        unsafe_position<u8>(unsafe_address(start + offset)) = u8(0);
        unsafe_position<u8>(unsafe_address(start + 4u64 + offset)) = u8(end >> (byte * 8));
        unsafe_position<u8>(unsafe_address(start + 8u64 + offset)) = u8(0);
    }
    return RuntimeFunctionFields(0, 4, 8, 3);
}
