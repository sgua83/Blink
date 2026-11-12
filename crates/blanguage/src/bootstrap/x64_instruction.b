module bootstrap.x64_instruction;

const u8 OPCODE_REGISTER = u8(0);
const u8 DIRECT_REGISTER = u8(1);
const u8 MEMORY = u8(2);
const u8 NO_INDEX = u8(255);

u64 instruction_prefix(Encoding input, bool byte_register, u64 at) on cpu {
    u64 cursor = at;
    if (input.prefix != u8(0)) {
        unsafe_position<u8>(unsafe_address(cursor)) = input.prefix;
        cursor += 1u64;
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
    return cursor;
}

u64 instruction_operand(Encoding input, u64 at) on cpu {
    u64 cursor = at;
    if (input.form == OPCODE_REGISTER) {
        return cursor;
    }
    bool has_index = input.index != NO_INDEX;

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
    return cursor;
}

bool uses_byte_register(u8 first, u8 second) on cpu {
    return first == u8(0x88) || first == u8(0x8a) ||
        (first == u8(0x0f) && (second == u8(0xb6) || second == u8(0xbe)));
}

extern "C" u64 encode(Encoding input, address opcode, u64 opcode_length, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    bool byte_register = false;
    if (opcode_length == 1u64) {
        byte_register = uses_byte_register(unsafe_position<u8>(opcode), u8(0));
    }
    if (opcode_length == 2u64) {
        u8 first = unsafe_position<u8>(opcode);
        u8 second = unsafe_position<u8>(unsafe_address(address_bits(opcode) + 1u64));
        if (first == u8(0x0f)) { byte_register = uses_byte_register(first, second); }
    }
    cursor = instruction_prefix(input, byte_register, cursor);
    u64 index = 0u64;
    while (index < opcode_length) {
        u8 byte = unsafe_position<u8>(unsafe_address(address_bits(opcode) + index));
        unsafe_position<u8>(unsafe_address(cursor)) = byte;
        cursor += 1u64;
        index += 1u64;
    }
    return instruction_operand(input, cursor) - start;
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

u64 register_instruction(u64 at, u16 opcode, u8 prefix, bool wide, u8 reg, u8 operand) on cpu {
    return encode_operand_instruction(at, opcode, prefix, wide, reg, move_gpr(operand));
}

void put_byte(u64 at, u8 value) on cpu {
    unsafe_position<u8>(unsafe_address(at)) = value;
}

u64 condition_field(u64 at, u8 condition) on cpu {
    put_byte(at, u8(0x0f));
    put_byte(at + 1u64, condition);
    return at + 2u64;
}

u64 jump_field(u64 at) on cpu {
    put_byte(at, u8(0xe9));
    return at + 1u64;
}

const u32 ODD_HALF_LENGTH = 12;
const array<u8,ODD_HALF_LENGTH> ODD_HALF = array<u8,ODD_HALF_LENGTH>(
    u8(0x48), u8(0x89), u8(0xc1), // mov rcx, rax
    u8(0x83), u8(0xe1), u8(1),    // and ecx, 1
    u8(0x48), u8(0xd1), u8(0xe8), // shr rax, 1
    u8(0x48), u8(0x09), u8(0xc8)  // or rax, rcx
);

u64 unsigned_to_float(u8 target, u8 prefix, address output) on cpu {
    u64 start = address_bits(output);
    put_byte(start, u8(0x48));
    put_byte(start + 1u64, u8(0x85));
    put_byte(start + 2u64, u8(0xc0)); // test rax, rax
    u64 low = condition_field(start + 3u64, u8(0x89)); // jns
    u64 cursor = low + 4u64;
    // Preserve the sticky bit before conversion so doubling rounds correctly.
    for (index in 0..ODD_HALF_LENGTH) {
        put_byte(cursor + u64(index), ODD_HALF[index]);
    }
    cursor += u64(ODD_HALF_LENGTH);
    cursor = register_instruction(cursor, u16(0x570f), u8(0), false, target, target);
    cursor = register_instruction(cursor, u16(0x2a0f), prefix, true, target, u8(0));
    cursor = register_instruction(cursor, u16(0x580f), prefix, false, target, target);
    u64 done = jump_field(cursor);
    cursor = done + 4u64;
    patch_relative(unsafe_address(low), low, cursor);
    cursor = register_instruction(cursor, u16(0x570f), u8(0), false, target, target);
    cursor = register_instruction(cursor, u16(0x2a0f), prefix, true, target, u8(0));
    patch_relative(unsafe_address(done), done, cursor);
    return cursor - start;
}

u64 float_to_unsigned(u8 target, u8 prefix, address output) on cpu {
    u64 start = address_bits(output);
    u8 compare_prefix = u8(0);
    if (prefix == u8(0xf2)) {
        compare_prefix = u8(0x66);
    }
    u64 cursor = register_instruction(start, u16(0x2e0f), compare_prefix, false, u8(0), u8(1));
    u64 low = condition_field(cursor, u8(0x82)); // jb
    cursor = low + 4u64;
    // Subtract 2^63 for the signed conversion and restore the high bit.
    cursor = register_instruction(cursor, u16(0x5c0f), prefix, false, u8(0), u8(1));
    cursor = register_instruction(cursor, u16(0x2c0f), prefix, true, target, u8(0));
    cursor = register_instruction(cursor, u16(0xba0f), u8(0), true, u8(5), target);
    put_byte(cursor, u8(63));
    u64 done = jump_field(cursor + 1u64);
    cursor = done + 4u64;
    patch_relative(unsafe_address(low), low, cursor);
    cursor = register_instruction(cursor, u16(0x2c0f), prefix, true, target, u8(0));
    patch_relative(unsafe_address(done), done, cursor);
    return cursor - start;
}

const u8 DIVISION_SHIFT = u8(0);
const u8 DIVISION_MULTIPLY = u8(1);
const u8 DIVISION_MULTIPLY_ADD = u8(2);

extern "C" DivisionPlan division_plan(u32 divisor, bool signed) on cpu requires nonzero(divisor) {
    u32 magnitude = divisor;
    if (signed && i32(divisor) < i32(0)) { magnitude = u32(-i64(i32(divisor))); }
    u32 high = magnitude;
    u32 shift = 0;
    if (high >= 0x10000) { high >>= 16; shift += 16; }
    if (high >= 0x100) { high >>= 8; shift += 8; }
    if (high >= 0x10) { high >>= 4; shift += 4; }
    if (high >= 4) { high >>= 2; shift += 2; }
    if (high >= 2) { shift += 1; }
    if ((magnitude & (magnitude - 1)) == 0) {
        return DivisionPlan(0, u8(shift), DIVISION_SHIFT);
    }

    u64 d = u64(magnitude);
    u64 power = 1u64 << u64(shift);
    u64 numerator = 0x100000000u64 * power;
    u64 quotient = numerator / d;
    u64 remainder = numerator % d;
    // A non-power-of-two magnitude cannot divide the power-of-two numerator.
    if (d - remainder < power) {
        return DivisionPlan(u32(quotient + 1u64), u8(shift), DIVISION_MULTIPLY);
    }

    // Compute ceil(2 * numerator / d) from the quotient and remainder.
    // The multiplier takes the low 32 bits after dropping the implicit high bit.
    u64 ceil_twice = 2u64 * quotient + 1u64;
    if (2u64 * remainder > d) {
        ceil_twice += 1u64;
    }
    return DivisionPlan(u32(ceil_twice), u8(shift), DIVISION_MULTIPLY_ADD);
}

const u8 MOVE_GPR = u8(0);
const u8 MOVE_XMM = u8(1);
const u8 MOVE_MEMORY = u8(2);
const u8 MOVE_IMMEDIATE = u8(3);
const u8 RIP_RELATIVE = u8(3);

MoveLocation move_gpr(u8 register) on cpu {
    return MoveLocation(0u64, MOVE_GPR, register, NO_INDEX, u8(0));
}

u8 move_temporary(MoveLocation memory) on cpu {
    if (memory.base != u8(0) && memory.index != u8(0)) { return u8(0); }
    if (memory.base != u8(1) && memory.index != u8(1)) { return u8(1); }
    return u8(2);
}

u64 encode_operand_instruction(u64 at, u16 opcode, u8 prefix, bool wide, u8 reg, MoveLocation operand) on cpu {
    u8 form = DIRECT_REGISTER;
    if (operand.kind == MOVE_MEMORY) { form = MEMORY; }
    if (operand.kind == MOVE_IMMEDIATE) { form = RIP_RELATIVE; }
    Encoding input = Encoding(u32(operand.value), reg, operand.base, operand.index, operand.shift, form, prefix, u16(wide));
    u8 first = u8(opcode);
    u8 second = u8(opcode >> 8);
    u64 cursor = instruction_prefix(input, uses_byte_register(first, second), at);
    put_byte(cursor, first);
    cursor += 1u64;
    if (second != u8(0)) {
        put_byte(cursor, second);
        cursor += 1u64;
    }
    return instruction_operand(input, cursor);
}

u64 move_immediate(u64 at, u8 register, u64 value, bool wide) on cpu {
    bool full = wide && value > 0xffffffffu64;
    Encoding input = Encoding(0, u8(0), register, NO_INDEX, u8(0), OPCODE_REGISTER, u8(0), u16(full));
    u64 cursor = instruction_prefix(input, false, at);
    put_byte(cursor, u8(0xb8) | (register & u8(7)));
    write_word(cursor + 1u64, u32(value));
    cursor += 5u64;
    if (full) {
        write_word(cursor, u32(value >> 32));
        cursor += 4u64;
    }
    return cursor;
}

u64 load_extended(u64 at, u8 dst, MoveLocation src, u32 size, bool signed) on cpu {
    u16 opcode = u16(0xb60f);
    if (signed) { opcode = u16(0xbe0f); }
    if (size == 2) { opcode += u16(0x100); }
    return encode_operand_instruction(at, opcode, u8(0), false, dst, src);
}

u64 load_bits(u64 at, u8 dst, MoveLocation src, bool wide) on cpu {
    return encode_operand_instruction(at, u16(0x8b), u8(0), wide, dst, src);
}

u64 load_float(u64 at, u8 dst, MoveLocation src, bool wide) on cpu {
    u16 opcode = u16(0x100f);
    if (wide) { opcode = u16(0x7e0f); }
    return encode_operand_instruction(at, opcode, u8(0xf3), false, dst, src);
}

u64 store_bits(u64 at, MoveLocation dst, u8 src, u32 size) on cpu {
    u16 opcode = u16(0x89);
    u8 prefix = u8(0);
    if (size == 1) { opcode = u16(0x88); }
    if (size == 2) { prefix = u8(0x66); }
    return encode_operand_instruction(at, opcode, prefix, size == 8, src, dst);
}

u64 store_float(u64 at, MoveLocation dst, u8 src, bool wide) on cpu {
    if (wide) { return encode_operand_instruction(at, u16(0xd60f), u8(0x66), false, src, dst); }
    return encode_operand_instruction(at, u16(0x110f), u8(0xf3), false, src, dst);
}

u64 store_immediate(u64 at, MoveLocation dst, u64 value, u32 size) on cpu {
    u16 opcode = u16(0xc7);
    u8 prefix = u8(0);
    if (size == 1) { opcode = u16(0xc6); }
    if (size == 2) { prefix = u8(0x66); }
    u64 cursor = encode_operand_instruction(at, opcode, prefix, false, u8(0), dst);
    for (byte in 0..size) {
        put_byte(cursor + u64(byte), u8(value >> u64(byte * 8)));
    }
    return cursor + u64(size);
}

u64 copy_small(u64 at, MoveLocation dst, MoveLocation src, u32 size, bool signed) on cpu {
    u8 target = u8(0);
    if (dst.kind == MOVE_GPR) { target = dst.base; }
    u64 cursor = at;
    if (src.kind == MOVE_IMMEDIATE) {
        u32 sign = 0x80;
        if (size == 2) { sign = 0x8000; }
        u32 bits = u32(src.value) & (2 * sign - 1);
        if (signed) { bits = (bits ^ sign) - sign; }
        cursor = move_immediate(cursor, target, u64(bits), false);
    } else {
        cursor = load_extended(cursor, target, src, size, signed);
    }
    if (dst.kind == MOVE_MEMORY) { cursor = store_bits(cursor, dst, target, size); }
    return cursor;
}

u64 copy_bits(u64 at, MoveLocation dst, MoveLocation src, u32 size) on cpu {
    bool wide = size == 8;
    if (src.kind == MOVE_IMMEDIATE) {
        if (dst.kind == MOVE_GPR) { return move_immediate(at, dst.base, src.value, wide); }
        if (dst.kind == MOVE_XMM) {
            if (!wide) { return load_float(at, dst.base, src, false); }
            u64 cursor = move_immediate(at, u8(0), src.value, true);
            return encode_operand_instruction(cursor, u16(0x6e0f), u8(0x66), true, dst.base, move_gpr(u8(0)));
        }
        if (!wide) { return store_immediate(at, dst, src.value, 4); }
        u64 cursor = move_immediate(at, u8(0), src.value, true);
        return store_bits(cursor, dst, u8(0), size);
    }
    if (dst.kind == MOVE_GPR) {
        if (src.kind == MOVE_XMM) {
            return encode_operand_instruction(at, u16(0x7e0f), u8(0x66), wide, src.base, dst);
        }
        return load_bits(at, dst.base, src, wide);
    }
    if (dst.kind == MOVE_XMM) {
        if (src.kind == MOVE_GPR) {
            return encode_operand_instruction(at, u16(0x6e0f), u8(0x66), wide, dst.base, src);
        }
        if (src.kind == MOVE_XMM) {
            return encode_operand_instruction(at, u16(0x280f), u8(0), false, dst.base, src);
        }
        return load_float(at, dst.base, src, wide);
    }
    if (src.kind == MOVE_XMM) { return store_float(at, dst, src.base, wide); }
    if (src.kind == MOVE_GPR) { return store_bits(at, dst, src.base, size); }
    u64 cursor = load_bits(at, u8(0), src, wide);
    return store_bits(cursor, dst, u8(0), size);
}

extern "C" MoveEncoding copy_scalar_move(MoveLocation dst, MoveLocation src, u32 size, bool signed, address output) on cpu {
    u64 start = address_bits(output);
    if (dst.kind == MOVE_IMMEDIATE) { return MoveEncoding(0u64, 0u64); }
    if (size < 4) { return MoveEncoding(copy_small(start, dst, src, size, signed) - start, 0u64); }
    if (dst.kind == src.kind && dst.value == src.value && dst.base == src.base) {
        return MoveEncoding(0u64, 0u64);
    }
    u64 length = copy_bits(start, dst, src, size) - start;
    u64 literal = 0u64;
    if (dst.kind == MOVE_XMM && src.kind == MOVE_IMMEDIATE && size == 4) { literal = length - 4u64; }
    return MoveEncoding(length, literal);
}

extern "C" u64 read_scalar_move(MoveLocation dst, MoveLocation memory, u32 size, bool signed, bool boolean, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    if (size < 4) {
        u8 target = dst.base;
        if (dst.kind != MOVE_GPR) { target = move_temporary(memory); }
        cursor = load_extended(cursor, target, memory, size, signed);
        if (dst.kind == MOVE_MEMORY) { cursor = store_bits(cursor, dst, target, size); }
    } else if (boolean) {
        u8 temporary = move_temporary(memory);
        cursor = encode_operand_instruction(cursor, u16(0x83), u8(0), false, u8(7), memory);
        put_byte(cursor, u8(0));
        put_byte(cursor + 1u64, u8(0x0f));
        put_byte(cursor + 2u64, u8(0x95));
        put_byte(cursor + 3u64, u8(0xc0) | temporary);
        MoveLocation value = move_gpr(temporary);
        cursor = load_extended(cursor + 4u64, temporary, value, 1, false);
        if (dst.kind != MOVE_IMMEDIATE && (dst.kind != MOVE_GPR || dst.base != temporary)) {
            cursor = copy_bits(cursor, dst, value, 4);
        }
    } else if (dst.kind == MOVE_GPR) {
        cursor = load_bits(cursor, dst.base, memory, size == 8);
    } else if (dst.kind == MOVE_XMM) {
        cursor = load_float(cursor, dst.base, memory, size == 8);
    } else {
        u8 temporary = move_temporary(memory);
        cursor = load_bits(cursor, temporary, memory, size == 8);
        if (dst.kind == MOVE_MEMORY) { cursor = store_bits(cursor, dst, temporary, size); }
    }
    return cursor - start;
}

extern "C" u64 write_scalar_move(MoveLocation memory, MoveLocation src, u32 size, address output) on cpu {
    u64 start = address_bits(output);
    if (src.kind == MOVE_IMMEDIATE && size != 8) {
        return store_immediate(start, memory, src.value, size) - start;
    }
    if (src.kind == MOVE_XMM && size >= 4) {
        return store_float(start, memory, src.base, size == 8) - start;
    }
    u8 register = src.base;
    u64 cursor = start;
    if (src.kind != MOVE_GPR) {
        register = move_temporary(memory);
        if (src.kind == MOVE_IMMEDIATE) {
            cursor = move_immediate(cursor, register, src.value, true);
        } else if (src.kind == MOVE_XMM) {
            cursor = encode_operand_instruction(cursor, u16(0x7e0f), u8(0x66), false, src.base, move_gpr(register));
        } else if (size < 4) {
            cursor = load_extended(cursor, register, src, size, false);
        } else {
            cursor = load_bits(cursor, register, src, size == 8);
        }
    }
    return store_bits(cursor, memory, register, size) - start;
}

u64 comparison_result(u64 at, MoveLocation dst, BinaryOperation operation, bool signed, bool floating) on cpu {
    u8 condition = u8(0x94);
    if (operation == BinaryOperation::NotEqual) { condition = u8(0x95); }
    if (operation == BinaryOperation::Less) { condition = u8(0x92); }
    if (operation == BinaryOperation::Greater) { condition = u8(0x97); }
    if (operation == BinaryOperation::LessEqual) { condition = u8(0x96); }
    if (operation == BinaryOperation::GreaterEqual) { condition = u8(0x93); }
    if (signed && !floating) {
        if (operation == BinaryOperation::Less) { condition = u8(0x9c); }
        if (operation == BinaryOperation::Greater) { condition = u8(0x9f); }
        if (operation == BinaryOperation::LessEqual) { condition = u8(0x9e); }
        if (operation == BinaryOperation::GreaterEqual) { condition = u8(0x9d); }
    }
    put_byte(at, u8(0x0f));
    put_byte(at + 1u64, condition);
    put_byte(at + 2u64, u8(0xc0));
    u64 cursor = at + 3u64;
    if (floating) {
        u8 parity = u8(0x9b);
        u8 combine = u8(0x20);
        if (operation == BinaryOperation::NotEqual) {
            parity = u8(0x9a);
            combine = u8(0x08);
        }
        put_byte(cursor, u8(0x0f));
        put_byte(cursor + 1u64, parity);
        put_byte(cursor + 2u64, u8(0xc2));
        put_byte(cursor + 3u64, combine);
        put_byte(cursor + 4u64, u8(0xd0));
        cursor += 5u64;
    }
    u8 target = u8(0);
    if (dst.kind == MOVE_GPR) { target = dst.base; }
    cursor = load_extended(cursor, target, move_gpr(u8(0)), 1, false);
    MoveEncoding result = copy_scalar_move(dst, move_gpr(target), 4, false, unsafe_address(cursor));
    return cursor + result.length;
}

u64 integer_immediate(u64 at, u8 extension, u8 left, u64 bits, bool wide) on cpu {
    i64 value = i64(bits);
    if (!wide) { value = i64(i32(u32(bits))); }
    u16 opcode = u16(0x81);
    if (value >= -i64(128) && value <= i64(127)) { opcode = u16(0x83); }
    u64 cursor = register_instruction(at, opcode, u8(0), wide, extension, left);
    if (opcode == u16(0x83)) {
        put_byte(cursor, u8(bits));
        return cursor + 1u64;
    }
    write_word(cursor, u32(bits));
    return cursor + 4u64;
}

extern "C" BinaryEncoding scalar_compare(MoveLocation dst, MoveLocation left, MoveLocation right,
    u32 size, bool signed, bool floating, BinaryOperation operation, address output) on cpu {
    u64 start = address_bits(output);
    u8 target = u8(0);
    MoveLocation loaded = move_gpr(target);
    if (floating) {
        if (left.kind == MOVE_XMM) { target = left.base; }
        loaded = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
    } else if (left.kind == MOVE_GPR) {
        target = left.base;
        loaded = move_gpr(target);
    }
    MoveEncoding load = copy_scalar_move(loaded, left, size, signed, output);
    u64 cursor = start + load.length;
    u64 right_literal = 0u64;
    bool wide = size == 8;
    if (floating) {
        u8 prefix = u8(0);
        if (wide) { prefix = u8(0x66); }
        cursor = encode_operand_instruction(cursor, u16(0x2e0f), prefix,
            wide && right.kind == MOVE_IMMEDIATE, target, right);
        if (right.kind == MOVE_IMMEDIATE) { right_literal = cursor - start - 4u64; }
    } else if (size < 4) {
        MoveEncoding source = copy_scalar_move(move_gpr(u8(1)), right, size, signed, unsafe_address(cursor));
        cursor = register_instruction(cursor + source.length, u16(0x3b), u8(0), false, target, u8(1));
    } else if (right.kind == MOVE_IMMEDIATE && (!wide || i64(right.value) == i64(i32(u32(right.value))))) {
        cursor = integer_immediate(cursor, u8(7), target, right.value, wide);
    } else {
        MoveLocation source = right;
        if (right.kind == MOVE_IMMEDIATE) {
            cursor = move_immediate(cursor, u8(1), right.value, right.value > 0xffffffffu64);
            source = move_gpr(u8(1));
        }
        cursor = encode_operand_instruction(cursor, u16(0x3b), u8(0), wide, target, source);
    }
    cursor = comparison_result(cursor, dst, operation, signed, floating);
    return BinaryEncoding(cursor - start, load.literal_field, right_literal);
}

MoveEncoding cast_boolean(MoveLocation dst, MoveLocation src,
    u32 size, bool signed, bool floating, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    u64 literal = 0u64;
    if (floating) {
        u8 left = u8(0);
        if (src.kind == MOVE_XMM) { left = src.base; }
        MoveLocation target = MoveLocation(0u64, MOVE_XMM, left, NO_INDEX, u8(0));
        MoveEncoding load = copy_scalar_move(target, src, size, signed, output);
        cursor += load.length;
        literal = load.literal_field;
        cursor = register_instruction(cursor, u16(0x570f), u8(0), false, u8(1), u8(1));
        u8 prefix = u8(0);
        if (size == 8) { prefix = u8(0x66); }
        cursor = register_instruction(cursor, u16(0x2e0f), prefix, false, left, u8(1));
    } else {
        MoveLocation source = src;
        if (size < 4) {
            MoveEncoding load = copy_scalar_move(move_gpr(u8(0)), src, size, signed, output);
            cursor += load.length;
            source = move_gpr(u8(0));
        } else if (src.kind == MOVE_IMMEDIATE) {
            cursor = move_immediate(cursor, u8(1), src.value, src.value > 0xffffffffu64);
            source = move_gpr(u8(1));
        }
        cursor = encode_operand_instruction(cursor, u16(0x83), u8(0), size == 8, u8(7), source);
        put_byte(cursor, u8(0));
        cursor += 1u64;
    }
    cursor = comparison_result(cursor, dst, BinaryOperation::NotEqual, signed, floating);
    return MoveEncoding(cursor - start, literal);
}

bool same_move_location(MoveLocation left, MoveLocation right) on cpu {
    return left.kind == right.kind && left.value == right.value && left.base == right.base;
}

u8 binary_target(MoveLocation dst, MoveLocation left, MoveLocation right, u8 kind) on cpu {
    if (dst.kind == kind && (!same_move_location(dst, right) || same_move_location(left, right))) {
        return dst.base;
    }
    return u8(0);
}

extern "C" BinaryEncoding float_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    u32 size, u8 opcode, address output) on cpu {
    u64 start = address_bits(output);
    u8 target = binary_target(dst, left, right, MOVE_XMM);
    MoveLocation loaded = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
    MoveEncoding load = copy_scalar_move(loaded, left, size, false, output);
    bool wide = size == 8;
    u8 prefix = u8(0xf3);
    if (wide) { prefix = u8(0xf2); }
    u64 cursor = encode_operand_instruction(start + load.length, u16(0x0f) | (u16(opcode) << 8),
        prefix, wide && right.kind == MOVE_IMMEDIATE, target, right);
    u64 right_literal = 0u64;
    if (right.kind == MOVE_IMMEDIATE) { right_literal = cursor - start - 4u64; }
    MoveEncoding store = copy_scalar_move(dst, loaded, size, false, unsafe_address(cursor));
    return BinaryEncoding(cursor + store.length - start, load.literal_field, right_literal);
}

extern "C" BinaryEncoding scalar_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    u32 size, bool signed, bool floating, u32 count_size, bool count_signed,
    BinaryOperation operation, address output) on cpu {
    if (floating) {
        u8 opcode = u8(0x58);
        if (operation == BinaryOperation::Subtract) { opcode = u8(0x5c); }
        if (operation == BinaryOperation::Multiply) { opcode = u8(0x59); }
        if (operation == BinaryOperation::Divide) { opcode = u8(0x5e); }
        return float_binary(dst, left, right, size, opcode, output);
    }
    u64 start = address_bits(output);
    u64 cursor = start;
    bool wide = size == 8;
    u8 target = u8(0);
    if (operation == BinaryOperation::ShiftLeft || operation == BinaryOperation::ShiftRight || operation == BinaryOperation::ShiftRightSigned) {
        if (right.kind != MOVE_IMMEDIATE) {
            MoveEncoding count = copy_scalar_move(move_gpr(u8(1)), right, count_size, count_signed, output);
            cursor += count.length;
            if (size < 4) { cursor = integer_immediate(cursor, u8(4), u8(1), u64(size * 8 - 1), false); }
        }
        target = u8(0);
        if (dst.kind == MOVE_GPR) { target = dst.base; }
        MoveEncoding load = copy_scalar_move(move_gpr(target), left, size, signed, unsafe_address(cursor));
        cursor += load.length;
        u8 extension = u8(4);
        if (operation == BinaryOperation::ShiftRight) { extension = u8(5); }
        if (operation == BinaryOperation::ShiftRightSigned) { extension = u8(7); }
        u16 opcode = u16(0xd3);
        if (right.kind == MOVE_IMMEDIATE) { opcode = u16(0xc1); }
        cursor = register_instruction(cursor, opcode, u8(0), wide, extension, target);
        if (right.kind == MOVE_IMMEDIATE) {
            put_byte(cursor, u8(right.value & u64(size * 8 - 1)));
            cursor += 1u64;
        }
    } else {
        target = binary_target(dst, left, right, MOVE_GPR);
        MoveEncoding load = copy_scalar_move(move_gpr(target), left, size, signed, output);
        cursor += load.length;
        u16 opcode = u16(0x03);
        u8 extension = u8(0);
        if (operation == BinaryOperation::Subtract) { opcode = u16(0x2b); extension = u8(5); }
        if (operation == BinaryOperation::Multiply) { opcode = u16(0xaf0f); }
        if (operation == BinaryOperation::And) { opcode = u16(0x23); extension = u8(4); }
        if (operation == BinaryOperation::Or) { opcode = u16(0x0b); extension = u8(1); }
        if (operation == BinaryOperation::Xor) { opcode = u16(0x33); extension = u8(6); }
        if (size < 4) {
            MoveEncoding source = copy_scalar_move(move_gpr(u8(1)), right, size, signed, unsafe_address(cursor));
            cursor = register_instruction(cursor + source.length, opcode, u8(0), false, target, u8(1));
        } else if (right.kind == MOVE_IMMEDIATE) {
            if (wide && i64(right.value) != i64(i32(u32(right.value)))) {
                cursor = move_immediate(cursor, u8(1), right.value, true);
                cursor = register_instruction(cursor, opcode, u8(0), true, target, u8(1));
            } else if (operation == BinaryOperation::Multiply) {
                cursor = register_instruction(cursor, u16(0x69), u8(0), wide, target, target);
                write_word(cursor, u32(right.value));
                cursor += 4u64;
            } else {
                cursor = integer_immediate(cursor, extension, target, right.value, wide);
            }
        } else {
            cursor = encode_operand_instruction(cursor, opcode, u8(0), wide, target, right);
        }
    }
    MoveEncoding store = copy_scalar_move(dst, move_gpr(target), size, signed, unsafe_address(cursor));
    return BinaryEncoding(cursor + store.length - start, 0u64, 0u64);
}

extern "C" bool bit_copy(ScalarEncoding from, ScalarEncoding into) on cpu {
    if (from.kind == into.kind && from.width == into.width) { return true; }
    bool source_integer = from.kind == ScalarKind::Unsigned || from.kind == ScalarKind::Signed;
    bool target_integer = into.kind == ScalarKind::Unsigned || into.kind == ScalarKind::Signed;
    return target_integer && ((source_integer && from.width == into.width) ||
        (from.kind == ScalarKind::Boolean && into.width == 32));
}

MoveEncoding float_sign(MoveLocation dst, MoveLocation src, u32 size, bool negate, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation bits = move_gpr(u8(0));
    MoveEncoding load = copy_scalar_move(bits, src, size, false, output);
    u8 extension = u8(6);
    if (negate) { extension = u8(7); }
    u64 cursor = register_instruction(start + load.length, u16(0xba0f), u8(0), size == 8, extension, u8(0));
    put_byte(cursor, u8(size * 8 - 1));
    MoveEncoding store = copy_scalar_move(dst, bits, size, false, unsafe_address(cursor + 1u64));
    return MoveEncoding(cursor + 1u64 + store.length - start, 0u64);
}

extern "C" MoveEncoding value_unary(MoveLocation dst, MoveLocation src, ScalarEncoding scalar,
    ValueUnaryOperation operation, address output) on cpu {
    u32 size = scalar.width / 8;
    bool signed = scalar.kind == ScalarKind::Signed;
    if (operation == ValueUnaryOperation::Bitcast) { return copy_scalar_move(dst, src, size, signed, output); }
    if (operation == ValueUnaryOperation::FloatNegate) { return float_sign(dst, src, size, true, output); }
    MoveLocation target = move_gpr(u8(0));
    if (dst.kind == MOVE_GPR || (dst.kind == MOVE_MEMORY && same_move_location(dst, src) && size >= 4)) { target = dst; }
    u64 start = address_bits(output);
    MoveEncoding load = copy_scalar_move(target, src, size, signed, output);
    u16 opcode = u16(0xf7);
    u8 extension = u8(3);
    if (operation == ValueUnaryOperation::BitNot) { extension = u8(2); }
    if (operation == ValueUnaryOperation::LogicalNot) { opcode = u16(0x83); extension = u8(6); }
    u64 cursor = encode_operand_instruction(start + load.length, opcode, u8(0), size == 8, extension, target);
    if (operation == ValueUnaryOperation::LogicalNot) { put_byte(cursor, u8(1)); cursor += 1u64; }
    MoveEncoding store = copy_scalar_move(dst, target, size, signed, unsafe_address(cursor));
    return MoveEncoding(cursor + store.length - start, 0u64);
}

extern "C" MoveEncoding value_math(MoveLocation dst, MoveLocation src, u32 size, bool sqrt, address output) on cpu {
    if (!sqrt) { return float_sign(dst, src, size, false, output); }
    u64 start = address_bits(output);
    u8 target = u8(0);
    if (dst.kind == MOVE_XMM) { target = dst.base; }
    u8 prefix = u8(0xf3);
    if (size == 8) { prefix = u8(0xf2); }
    u64 cursor = encode_operand_instruction(start, u16(0x510f), prefix,
        size == 8 && src.kind == MOVE_IMMEDIATE, target, src);
    u64 literal = 0u64;
    if (src.kind == MOVE_IMMEDIATE) { literal = cursor - start - 4u64; }
    MoveLocation value = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
    MoveEncoding store = copy_scalar_move(dst, value, size, false, unsafe_address(cursor));
    return MoveEncoding(cursor + store.length - start, literal);
}

extern "C" BinaryEncoding value_cast(MoveLocation dst, MoveLocation src, ScalarEncoding from,
    ScalarEncoding into, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = start;
    u32 source_size = from.width / 8;
    u32 target_size = into.width / 8;
    bool signed = from.kind == ScalarKind::Signed;
    bool target_signed = into.kind == ScalarKind::Signed;
    bool source_float = from.kind == ScalarKind::Float;
    bool target_float = into.kind == ScalarKind::Float;
    u64 source_literal = 0u64;
    u64 constant_literal = 0u64;
    if (from.kind == ScalarKind::Unsigned && source_size == 8 && target_float) {
        MoveEncoding load = copy_scalar_move(move_gpr(u8(0)), src, 8, false, output);
        cursor += load.length;
        u8 target = u8(0);
        if (dst.kind == MOVE_XMM) { target = dst.base; }
        u8 prefix = u8(0xf3);
        if (target_size == 8) { prefix = u8(0xf2); }
        cursor += unsigned_to_float(target, prefix, unsafe_address(cursor));
        MoveLocation value = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
        MoveEncoding store = copy_scalar_move(dst, value, target_size, false, unsafe_address(cursor));
        cursor += store.length;
    } else if (source_float && into.kind == ScalarKind::Unsigned && target_size == 8) {
        MoveLocation input = MoveLocation(0u64, MOVE_XMM, u8(0), NO_INDEX, u8(0));
        MoveEncoding load = copy_scalar_move(input, src, source_size, false, output);
        cursor += load.length;
        source_literal = load.literal_field;
        u64 bits = 0x5f000000u64;
        if (source_size == 8) { bits = 0x43e0000000000000u64; }
        MoveLocation constant = MoveLocation(bits, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
        MoveLocation high = MoveLocation(0u64, MOVE_XMM, u8(1), NO_INDEX, u8(0));
        MoveEncoding bound = copy_scalar_move(high, constant, source_size, false, unsafe_address(cursor));
        if (bound.literal_field != 0u64) { constant_literal = cursor - start + bound.literal_field; }
        cursor += bound.length;
        u8 target = u8(0);
        if (dst.kind == MOVE_GPR) { target = dst.base; }
        u8 prefix = u8(0xf3);
        if (source_size == 8) { prefix = u8(0xf2); }
        cursor += float_to_unsigned(target, prefix, unsafe_address(cursor));
        MoveEncoding store = copy_scalar_move(dst, move_gpr(target), 8, false, unsafe_address(cursor));
        cursor += store.length;
    } else if (source_float && target_float && source_size != target_size) {
        u8 target = u8(0);
        if (dst.kind == MOVE_XMM) { target = dst.base; }
        u8 prefix = u8(0xf3);
        if (source_size == 8) { prefix = u8(0xf2); }
        cursor = encode_operand_instruction(cursor, u16(0x5a0f), prefix,
            source_size == 8 && src.kind == MOVE_IMMEDIATE, target, src);
        if (src.kind == MOVE_IMMEDIATE) { source_literal = cursor - start - 4u64; }
        MoveLocation value = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
        MoveEncoding store = copy_scalar_move(dst, value, target_size, false, unsafe_address(cursor));
        cursor += store.length;
    } else if (bit_copy(from, into)) {
        MoveEncoding move = copy_scalar_move(dst, src, target_size, target_signed, output);
        return BinaryEncoding(move.length, move.literal_field, 0u64);
    } else if (into.kind == ScalarKind::Boolean) {
        MoveEncoding value = cast_boolean(dst, src, source_size, signed, source_float, output);
        return BinaryEncoding(value.length, value.literal_field, 0u64);
    } else if (!source_float && !target_float) {
        u8 target = u8(0);
        if (dst.kind == MOVE_GPR) { target = dst.base; }
        MoveLocation value = move_gpr(target);
        u32 load_size = source_size;
        bool load_signed = signed;
        if (target_size <= 4 && source_size >= 4) { load_size = target_size; load_signed = target_signed; }
        MoveEncoding load = copy_scalar_move(value, src, load_size, load_signed, output);
        cursor += load.length;
        if (target_size <= 4) {
            if (target_size == 4 && source_size >= 4 && same_move_location(src, value)) {
                cursor = register_instruction(cursor, u16(0x8b), u8(0), false, target, target);
            }
        } else if (signed && source_size < 8) {
            cursor = register_instruction(cursor, u16(0x63), u8(0), true, target, target);
        }
        MoveEncoding store = copy_scalar_move(dst, value, target_size, target_signed, unsafe_address(cursor));
        cursor += store.length;
    } else if (source_float) {
        u8 target = u8(0);
        if (dst.kind == MOVE_GPR) { target = dst.base; }
        u8 prefix = u8(0xf3);
        if (source_size == 8) { prefix = u8(0xf2); }
        bool wide = (into.kind == ScalarKind::Unsigned && target_size == 4) || target_size == 8;
        if (src.kind == MOVE_IMMEDIATE) { wide = source_size == 8; }
        cursor = encode_operand_instruction(cursor, u16(0x2c0f), prefix, wide, target, src);
        if (src.kind == MOVE_IMMEDIATE) { source_literal = cursor - start - 4u64; }
        if (into.kind == ScalarKind::Unsigned && target_size == 4) {
            cursor = register_instruction(cursor, u16(0x8b), u8(0), false, target, target);
        }
        MoveEncoding store = copy_scalar_move(dst, move_gpr(target), target_size, target_signed, unsafe_address(cursor));
        cursor += store.length;
    } else {
        u8 target = u8(0);
        if (dst.kind == MOVE_XMM) { target = dst.base; }
        bool unsigned32 = from.kind == ScalarKind::Unsigned && source_size == 4;
        MoveLocation input = src;
        if (source_size < 4 || (unsigned32 && src.kind == MOVE_MEMORY)) {
            input = move_gpr(u8(0));
            MoveEncoding load = copy_scalar_move(input, src, source_size, signed, output);
            cursor += load.length;
        } else if (src.kind == MOVE_IMMEDIATE) {
            cursor = move_immediate(cursor, u8(1), src.value, src.value > 0xffffffffu64);
            input = move_gpr(u8(1));
        }
        cursor = register_instruction(cursor, u16(0x570f), u8(0), false, target, target);
        u8 prefix = u8(0xf3);
        if (target_size == 8) { prefix = u8(0xf2); }
        cursor = encode_operand_instruction(cursor, u16(0x2a0f), prefix, unsigned32 || source_size == 8, target, input);
        MoveLocation value = MoveLocation(0u64, MOVE_XMM, target, NO_INDEX, u8(0));
        MoveEncoding store = copy_scalar_move(dst, value, target_size, false, unsafe_address(cursor));
        cursor += store.length;
    }
    return BinaryEncoding(cursor - start, source_literal, constant_literal);
}

extern "C" BinaryEncoding value_select(MoveLocation dst, MoveLocation condition, MoveLocation left,
    MoveLocation right, u32 size, bool signed, address output) on cpu {
    if (condition.kind == MOVE_IMMEDIATE) {
        if (condition.value != 0u64) {
            MoveEncoding move = copy_scalar_move(dst, left, size, signed, output);
            return BinaryEncoding(move.length, move.literal_field, 0u64);
        }
        MoveEncoding move = copy_scalar_move(dst, right, size, signed, output);
        return BinaryEncoding(move.length, 0u64, move.literal_field);
    }
    u64 start = address_bits(output);
    u64 cursor = encode_operand_instruction(start, u16(0x83), u8(0), false, u8(7), condition);
    put_byte(cursor, u8(0));
    cursor += 1u64;
    u8 target = u8(0);
    if (dst.kind == MOVE_GPR) { target = dst.base; }
    MoveLocation initial = right;
    MoveLocation source = left;
    u16 opcode = u16(0x450f);
    if (same_move_location(dst, left)) { initial = left; source = right; opcode = u16(0x440f); }
    MoveEncoding load = copy_scalar_move(move_gpr(target), initial, size, signed, unsafe_address(cursor));
    cursor += load.length;
    if (size < 4 || source.kind == MOVE_XMM || source.kind == MOVE_IMMEDIATE) {
        MoveEncoding selected = copy_scalar_move(move_gpr(u8(1)), source, size, signed, unsafe_address(cursor));
        cursor += selected.length;
        source = move_gpr(u8(1));
    }
    cursor = encode_operand_instruction(cursor, opcode, u8(0), size == 8, target, source);
    MoveEncoding store = copy_scalar_move(dst, move_gpr(target), size, signed, unsafe_address(cursor));
    return BinaryEncoding(cursor + store.length - start, 0u64, 0u64);
}

u64 division_shift(u64 at, u8 target, u8 kind, bool wide, u8 count) on cpu {
    u64 cursor = register_instruction(at, u16(0xc1), u8(0), wide, kind, target);
    put_byte(cursor, count);
    return cursor + 1u64;
}

u64 integer_magnitude(u64 at, u8 value, u8 mask, bool wide) on cpu {
    u64 cursor = register_instruction(at, u16(0x89), u8(0), wide, value, mask);
    u8 count = u8(31);
    if (wide) { count = u8(63); }
    cursor = division_shift(cursor, mask, u8(7), wide, count);
    cursor = register_instruction(cursor, u16(0x31), u8(0), wide, mask, value);
    return register_instruction(cursor, u16(0x29), u8(0), wide, mask, value);
}

u64 divide_unsigned_instruction(u64 at, bool wide) on cpu {
    u64 cursor = register_instruction(at, u16(0x31), u8(0), false, u8(2), u8(2));
    return register_instruction(cursor, u16(0xf7), u8(0), wide, u8(6), u8(1));
}

extern "C" u64 divide_value_instruction(bool signed, address output) on cpu {
    u64 start = address_bits(output);
    if (!signed) { return divide_unsigned_instruction(start, false) - start; }
    u64 cursor = register_instruction(start, u16(0x63), u8(0), true, u8(0), u8(0));
    cursor = register_instruction(cursor, u16(0x63), u8(0), true, u8(1), u8(1));
    put_byte(cursor, u8(0x48));
    put_byte(cursor + 1u64, u8(0x99));
    return register_instruction(cursor + 2u64, u16(0xf7), u8(0), true, u8(7), u8(1)) - start;
}

u64 load_magnitude32(u64 at, MoveLocation src, ScalarEncoding scalar) on cpu {
    bool signed = scalar.kind == ScalarKind::Signed;
    MoveEncoding load = copy_scalar_move(move_gpr(u8(0)), src, scalar.width / 8, signed, unsafe_address(at));
    u64 cursor = at + load.length;
    if (signed) { cursor = integer_magnitude(cursor, u8(0), u8(1), false); }
    return cursor;
}

u64 store_division(u64 at, MoveLocation dst, u8 register, ScalarEncoding scalar) on cpu {
    MoveEncoding store = copy_scalar_move(dst, move_gpr(register), scalar.width / 8,
        scalar.kind == ScalarKind::Signed, unsafe_address(at));
    return at + store.length;
}

MoveLocation division_remainder(DivisionOutputs result) on cpu {
    if (result.kind == 3) { return result.second; }
    return result.first;
}

extern "C" u64 divide_constant32(DivisionOutputs result, MoveLocation src, u32 divisor,
    ScalarEncoding scalar, address output) on cpu requires nonzero(divisor) {
    u64 start = address_bits(output);
    bool signed = scalar.kind == ScalarKind::Signed;
    DivisionPlan plan = division_plan(divisor, signed);
    u64 cursor = load_magnitude32(start, src, scalar);
    if (plan.mode == DIVISION_SHIFT) {
        if (plan.shift != u8(0)) { cursor = division_shift(cursor, u8(0), u8(5), false, plan.shift); }
    } else {
        cursor = move_immediate(cursor, u8(1), u64(plan.magic), false);
        cursor = register_instruction(cursor, u16(0xf7), u8(0), false, u8(4), u8(1));
        if (plan.mode == DIVISION_MULTIPLY_ADD) {
            cursor = load_magnitude32(cursor, src, scalar);
            cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(2), u8(0));
            cursor = register_instruction(cursor, u16(0xd1), u8(0), false, u8(5), u8(0));
            cursor = register_instruction(cursor, u16(0x01), u8(0), false, u8(2), u8(0));
        } else {
            cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(2), u8(0));
        }
        cursor = division_shift(cursor, u8(0), u8(5), false, plan.shift);
    }
    if (signed) {
        MoveEncoding sign = copy_scalar_move(move_gpr(u8(2)), src, scalar.width / 8, true, unsafe_address(cursor));
        cursor = division_shift(cursor + sign.length, u8(2), u8(7), false, u8(31));
        if (i32(divisor) < i32(0)) { cursor = register_instruction(cursor, u16(0xf7), u8(0), false, u8(2), u8(2)); }
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(2), u8(0));
        cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(2), u8(0));
    }
    if (result.kind != 1) {
        if (result.kind == 3) { cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(0), u8(1)); }
        cursor = register_instruction(cursor, u16(0x69), u8(0), false, u8(0), u8(0));
        write_word(cursor, divisor);
        cursor += 4u64;
        MoveEncoding source = copy_scalar_move(move_gpr(u8(2)), src, scalar.width / 8, signed, unsafe_address(cursor));
        cursor = register_instruction(cursor + source.length, u16(0x29), u8(0), false, u8(0), u8(2));
        if (result.kind == 3) { cursor = store_division(cursor, result.first, u8(1), scalar); }
        cursor = store_division(cursor, division_remainder(result), u8(2), scalar);
    } else {
        cursor = store_division(cursor, result.first, u8(0), scalar);
    }
    return cursor - start;
}

extern "C" u64 divide_value(DivisionOutputs result, MoveLocation src, MoveLocation divisor,
    ScalarEncoding scalar, ScalarEncoding divisor_scalar, address output) on cpu {
    u64 start = address_bits(output);
    u32 size = scalar.width / 8;
    bool signed = scalar.kind == ScalarKind::Signed;
    u32 divisor_size = divisor_scalar.width / 8;
    bool divisor_signed = divisor_scalar.kind == ScalarKind::Signed;
    if (size == 8) { divisor_size = size; divisor_signed = signed; }
    MoveEncoding right = copy_scalar_move(move_gpr(u8(1)), divisor, divisor_size, divisor_signed, output);
    u64 cursor = start + right.length;
    MoveEncoding left = copy_scalar_move(move_gpr(u8(0)), src, size, signed, unsafe_address(cursor));
    cursor += left.length;
    if (size == 8) {
        if (signed) {
            cursor = integer_magnitude(cursor, u8(0), u8(2), true);
            cursor = integer_magnitude(cursor, u8(1), u8(2), true);
        }
        cursor = divide_unsigned_instruction(cursor, true);
        if (signed) {
            if (result.kind != 2) {
                MoveEncoding sign = copy_scalar_move(move_gpr(u8(1)), src, size, true, unsafe_address(cursor));
                cursor += sign.length;
                if (divisor.kind == MOVE_IMMEDIATE) {
                    if (i64(divisor.value) < i64(0)) { cursor = register_instruction(cursor, u16(0xf7), u8(0), true, u8(2), u8(1)); }
                } else {
                    cursor = encode_operand_instruction(cursor, u16(0x33), u8(0), true, u8(1), divisor);
                }
                cursor = division_shift(cursor, u8(1), u8(7), true, u8(63));
                cursor = register_instruction(cursor, u16(0x31), u8(0), true, u8(1), u8(0));
                cursor = register_instruction(cursor, u16(0x29), u8(0), true, u8(1), u8(0));
            }
            if (result.kind != 1) {
                MoveEncoding sign = copy_scalar_move(move_gpr(u8(1)), src, size, true, unsafe_address(cursor));
                cursor = division_shift(cursor + sign.length, u8(1), u8(7), true, u8(63));
                cursor = register_instruction(cursor, u16(0x31), u8(0), true, u8(1), u8(2));
                cursor = register_instruction(cursor, u16(0x29), u8(0), true, u8(1), u8(2));
            }
        }
    } else {
        cursor += divide_value_instruction(signed, unsafe_address(cursor));
    }
    if (result.kind != 2) { cursor = store_division(cursor, result.first, u8(0), scalar); }
    if (result.kind != 1) { cursor = store_division(cursor, division_remainder(result), u8(2), scalar); }
    return cursor - start;
}

MoveLocation packet_register(u8 reg) on cpu {
    return MoveLocation(0u64, MOVE_XMM, reg, NO_INDEX, u8(0));
}

u64 packet_instruction(u64 at, u8 prefix, u8 opcode, u8 dst, u8 src) on cpu {
    return register_instruction(at, u16(0x0f) | (u16(opcode) << 8), prefix, false, dst, src);
}

u64 packet_shuffle_at(u64 at, u8 dst, u8 src, u8 pattern) on cpu {
    u64 cursor = packet_instruction(at, u8(0x66), u8(0x70), dst, src);
    put_byte(cursor, pattern);
    return cursor + 1u64;
}

u64 packet_constant_at(u64 at, u8 dst, u32 bits) on cpu {
    if (bits == 0) { return packet_instruction(at, u8(0x66), u8(0xef), dst, dst); }
    if (bits == 0xffffffff) { return packet_instruction(at, u8(0x66), u8(0x76), dst, dst); }
    u64 cursor = move_immediate(at, u8(0), u64(bits), false);
    cursor = packet_instruction(cursor, u8(0x66), u8(0x6e), dst, u8(0));
    return packet_shuffle_at(cursor, dst, dst, u8(0));
}

u64 packet_load_at(u64 at, MoveLocation src, u8 dst) on cpu {
    if (src.kind == MOVE_XMM && src.base == dst) { return at; }
    return encode_operand_instruction(at, u16(0x280f), u8(0), false, dst, src);
}

u64 packet_store_at(u64 at, MoveLocation dst, u8 src) on cpu {
    if (dst.kind == MOVE_XMM) {
        if (dst.base == src) { return at; }
        return packet_instruction(at, u8(0), u8(0x28), dst.base, src);
    }
    return encode_operand_instruction(at, u16(0x290f), u8(0), false, src, dst);
}

u8 packet_destination(MoveLocation dst) on cpu {
    if (dst.kind == MOVE_XMM) { return dst.base; }
    return u8(0);
}

u64 packet_bool_at(u64 at, u8 dst) on cpu {
    u64 cursor = packet_constant_at(at, u8(3), 1);
    return packet_instruction(cursor, u8(0x66), u8(0xdb), dst, u8(3));
}

extern "C" u64 packet_constant(u8 dst, u32 bits, address output) on cpu {
    u64 start = address_bits(output);
    return packet_constant_at(start, dst, bits) - start;
}

extern "C" u64 packet_load(MoveLocation src, u8 dst, address output) on cpu {
    u64 start = address_bits(output);
    return packet_load_at(start, src, dst) - start;
}

extern "C" u64 packet_store(MoveLocation dst, u8 src, address output) on cpu {
    u64 start = address_bits(output);
    return packet_store_at(start, dst, src) - start;
}

extern "C" u64 packet_copy(MoveLocation dst, MoveLocation src, address output) on cpu {
    u64 start = address_bits(output);
    if (same_move_location(dst, src)) { return 0u64; }
    u8 temporary = u8(0);
    if (dst.kind == MOVE_XMM) { temporary = dst.base; }
    if (src.kind == MOVE_XMM) { temporary = src.base; }
    u64 cursor = packet_load_at(start, src, temporary);
    return packet_store_at(cursor, dst, temporary) - start;
}

extern "C" u64 packet_from_word(bool boolean, u8 dst, address output) on cpu {
    u64 start = address_bits(output);
    if (!boolean) { return 0u64; }
    u64 cursor = packet_constant_at(start, u8(1), 0);
    cursor = packet_instruction(cursor, u8(0x66), u8(0x76), dst, u8(1));
    cursor = packet_constant_at(cursor, u8(1), 1);
    return packet_instruction(cursor, u8(0x66), u8(0xfe), dst, u8(1)) - start;
}

u64 packet_multiply_at(u64 at, bool broadcast) on cpu {
    u64 cursor = packet_instruction(at, u8(0), u8(0x28), u8(2), u8(0));
    if (!broadcast) { cursor = packet_instruction(cursor, u8(0), u8(0x28), u8(3), u8(1)); }
    cursor = packet_instruction(cursor, u8(0x66), u8(0x73), u8(3), u8(2));
    put_byte(cursor, u8(4)); cursor += 1u64;
    if (!broadcast) {
        cursor = packet_instruction(cursor, u8(0x66), u8(0x73), u8(3), u8(3));
        put_byte(cursor, u8(4)); cursor += 1u64;
    }
    cursor = packet_instruction(cursor, u8(0x66), u8(0xf4), u8(0), u8(1));
    u8 right = u8(3);
    if (broadcast) { right = u8(1); }
    cursor = packet_instruction(cursor, u8(0x66), u8(0xf4), u8(2), right);
    cursor = packet_shuffle_at(cursor, u8(0), u8(0), u8(0x88));
    cursor = packet_shuffle_at(cursor, u8(2), u8(2), u8(0x88));
    return packet_instruction(cursor, u8(0x66), u8(0x62), u8(0), u8(2));
}

extern "C" u64 packet_multiply(bool broadcast, address output) on cpu {
    u64 start = address_bits(output);
    return packet_multiply_at(start, broadcast) - start;
}

u64 packet_lane_at(u64 at, MoveLocation src, u8 lane, u8 gpr) on cpu {
    if (src.kind == MOVE_XMM) {
        u8 source = src.base;
        u64 cursor = at;
        if (lane != u8(0)) {
            cursor = packet_shuffle_at(cursor, u8(3), source, lane * u8(0x55));
            source = u8(3);
        }
        return packet_instruction(cursor, u8(0x66), u8(0x7e), source, gpr);
    }
    MoveLocation field = src;
    field.value += u64(lane) * 4u64;
    return encode_operand_instruction(at, u16(0x8b), u8(0), false, gpr, field);
}

u64 packet_compare_at(u64 at, MoveLocation dst, MoveLocation left, MoveLocation right,
    ScalarEncoding scalar, BinaryOperation operation) on cpu {
    bool floating = scalar.kind == ScalarKind::Float;
    bool swap = floating && (operation == BinaryOperation::Greater || operation == BinaryOperation::GreaterEqual)
        || !floating && (operation == BinaryOperation::Less || operation == BinaryOperation::GreaterEqual);
    MoveLocation a = left; MoveLocation b = right;
    if (swap) { a = right; b = left; }
    bool equality = operation == BinaryOperation::Equal || operation == BinaryOperation::NotEqual;
    bool bias = !equality && (scalar.kind == ScalarKind::Unsigned || scalar.kind == ScalarKind::Boolean);
    u8 target = binary_target(dst, a, b, MOVE_XMM);
    if (bias && dst.kind == MOVE_XMM) { target = dst.base; }
    u64 cursor = at;
    if (bias) { cursor = packet_load_at(cursor, b, u8(1)); }
    cursor = packet_load_at(cursor, a, target);
    if (floating) {
        cursor = encode_operand_instruction(cursor, u16(0xc20f), u8(0), false, target, b);
        u8 predicate = u8(0);
        if (operation == BinaryOperation::NotEqual) { predicate = u8(4); }
        if (operation == BinaryOperation::Less || operation == BinaryOperation::Greater) { predicate = u8(1); }
        if (operation == BinaryOperation::LessEqual || operation == BinaryOperation::GreaterEqual) { predicate = u8(2); }
        put_byte(cursor, predicate); cursor += 1u64;
    } else {
        MoveLocation source = b;
        if (bias) {
            cursor = packet_constant_at(cursor, u8(2), 0x80000000);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(2));
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), u8(1), u8(2));
            source = packet_register(u8(1));
        }
        u8 opcode = u8(0x66);
        if (equality) { opcode = u8(0x76); }
        cursor = encode_operand_instruction(cursor, u16(0x0f) | (u16(opcode) << 8), u8(0x66), false, target, source);
        if (operation == BinaryOperation::NotEqual || operation == BinaryOperation::LessEqual || operation == BinaryOperation::GreaterEqual) {
            cursor = packet_constant_at(cursor, u8(2), 0xffffffff);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(2));
        }
    }
    cursor = packet_bool_at(cursor, target);
    return packet_store_at(cursor, dst, target);
}

extern "C" u64 packet_binary(MoveLocation dst, MoveLocation left, MoveLocation right,
    ScalarEncoding scalar, BinaryOperation operation, bool uniform_count, address output) on cpu {
    u64 start = address_bits(output);
    if (u32(operation) >= u32(BinaryOperation::Equal)) {
        return packet_compare_at(start, dst, left, right, scalar, operation) - start;
    }
    u64 cursor = start;
    bool shift = operation == BinaryOperation::ShiftLeft || operation == BinaryOperation::ShiftRight || operation == BinaryOperation::ShiftRightSigned;
    if (shift) {
        if (uniform_count) {
            u8 target = packet_destination(dst);
            cursor = packet_load_at(cursor, right, u8(1));
            cursor = packet_instruction(cursor, u8(0x66), u8(0x7e), u8(1), u8(0));
            put_byte(cursor, u8(0x83)); put_byte(cursor + 1u64, u8(0xe0)); put_byte(cursor + 2u64, u8(0x1f)); cursor += 3u64;
            cursor = packet_instruction(cursor, u8(0x66), u8(0x6e), u8(1), u8(0));
            cursor = packet_load_at(cursor, left, target);
            u8 opcode = u8(0xd2);
            if (operation == BinaryOperation::ShiftLeft) { opcode = u8(0xf2); }
            if (operation == BinaryOperation::ShiftRightSigned) { opcode = u8(0xe2); }
            cursor = packet_instruction(cursor, u8(0x66), opcode, target, u8(1));
            return packet_store_at(cursor, dst, target) - start;
        }
        cursor = packet_constant_at(cursor, u8(0), 0);
        u8 kind = u8(5);
        if (operation == BinaryOperation::ShiftLeft) { kind = u8(4); }
        if (operation == BinaryOperation::ShiftRightSigned) { kind = u8(7); }
        for (lane in 0..4) {
            cursor = packet_lane_at(cursor, right, u8(lane), u8(1));
            cursor = packet_lane_at(cursor, left, u8(lane), u8(0));
            put_byte(cursor, u8(0xd3)); put_byte(cursor + 1u64, u8(0xc0) | (kind << u8(3))); cursor += 2u64;
            cursor = packet_instruction(cursor, u8(0x66), u8(0x6e), u8(1), u8(0));
            if (lane != 0) {
                cursor = packet_instruction(cursor, u8(0x66), u8(0x73), u8(7), u8(1));
                put_byte(cursor, u8(lane * 4)); cursor += 1u64;
            }
            cursor = packet_instruction(cursor, u8(0x66), u8(0xeb), u8(0), u8(1));
        }
        return packet_store_at(cursor, dst, u8(0)) - start;
    }
    bool floating = scalar.kind == ScalarKind::Float;
    if (operation == BinaryOperation::Multiply && !floating) {
        cursor = packet_load_at(cursor, left, u8(0));
        cursor = packet_load_at(cursor, right, u8(1));
        cursor = packet_multiply_at(cursor, false);
        return packet_store_at(cursor, dst, u8(0)) - start;
    }
    u8 target = binary_target(dst, left, right, MOVE_XMM);
    cursor = packet_load_at(cursor, left, target);
    u8 prefix = u8(0x66); u8 opcode = u8(0xfe);
    if (operation == BinaryOperation::And) { opcode = u8(0xdb); }
    else if (operation == BinaryOperation::Or) { opcode = u8(0xeb); }
    else if (operation == BinaryOperation::Xor) { opcode = u8(0xef); }
    else if (floating) {
        prefix = u8(0); opcode = u8(0x58);
        if (operation == BinaryOperation::Subtract) { opcode = u8(0x5c); }
        if (operation == BinaryOperation::Multiply) { opcode = u8(0x59); }
        if (operation == BinaryOperation::Divide) { opcode = u8(0x5e); }
    } else if (operation == BinaryOperation::Subtract) { opcode = u8(0xfa); }
    cursor = encode_operand_instruction(cursor, u16(0x0f) | (u16(opcode) << 8), prefix, false, target, right);
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_unary(MoveLocation dst, MoveLocation src, ValueUnaryOperation operation, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u8 target = packet_destination(dst);
    if (operation == ValueUnaryOperation::SignedNegate || operation == ValueUnaryOperation::UnsignedNegate) {
        cursor = packet_load_at(cursor, src, u8(1));
        cursor = packet_constant_at(cursor, target, 0);
        cursor = packet_instruction(cursor, u8(0x66), u8(0xfa), target, u8(1));
    } else {
        cursor = packet_load_at(cursor, src, target);
        if (operation != ValueUnaryOperation::Bitcast) {
            u32 bits = 0xffffffff;
            if (operation == ValueUnaryOperation::FloatNegate) { bits = 0x80000000; }
            if (operation == ValueUnaryOperation::LogicalNot) { bits = 1; }
            cursor = packet_constant_at(cursor, u8(1), bits);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(1));
        }
    }
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_cast(MoveLocation dst, MoveLocation src, ScalarEncoding from,
    ScalarEncoding into, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u8 target = packet_destination(dst);
    bool floating = from.kind == ScalarKind::Float;
    if (into.kind == ScalarKind::Float && (from.kind == ScalarKind::Signed || from.kind == ScalarKind::Boolean)
        || floating && into.kind == ScalarKind::Signed) {
        u8 prefix = u8(0);
        if (floating) { prefix = u8(0xf3); }
        cursor = encode_operand_instruction(cursor, u16(0x5b0f), prefix, false, target, src);
    } else {
        cursor = packet_load_at(cursor, src, target);
        if (into.kind == ScalarKind::Boolean) {
            cursor = packet_constant_at(cursor, u8(1), 0);
            if (floating) {
                cursor = packet_instruction(cursor, u8(0), u8(0xc2), target, u8(1));
                put_byte(cursor, u8(4)); cursor += 1u64;
            } else {
                cursor = packet_instruction(cursor, u8(0x66), u8(0x76), target, u8(1));
                cursor = packet_constant_at(cursor, u8(1), 0xffffffff);
                cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(1));
            }
            cursor = packet_bool_at(cursor, target);
        } else if (into.kind == ScalarKind::Float) {
            cursor = packet_instruction(cursor, u8(0), u8(0x28), u8(1), target);
            cursor = packet_constant_at(cursor, u8(2), 0xffff);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), target, u8(2));
            cursor = packet_instruction(cursor, u8(0x66), u8(0x72), u8(2), u8(1));
            put_byte(cursor, u8(16)); cursor += 1u64;
            cursor = packet_instruction(cursor, u8(0), u8(0x5b), target, target);
            cursor = packet_instruction(cursor, u8(0), u8(0x5b), u8(1), u8(1));
            cursor = packet_constant_at(cursor, u8(2), 0x47800000);
            cursor = packet_instruction(cursor, u8(0), u8(0x59), u8(1), u8(2));
            cursor = packet_instruction(cursor, u8(0), u8(0x58), target, u8(1));
        } else {
            cursor = packet_constant_at(cursor, u8(1), 0x4f000000);
            cursor = packet_instruction(cursor, u8(0), u8(0x28), u8(3), u8(1));
            cursor = packet_instruction(cursor, u8(0), u8(0xc2), u8(1), target);
            put_byte(cursor, u8(2)); cursor += 1u64;
            cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), u8(3), u8(1));
            cursor = packet_instruction(cursor, u8(0), u8(0x5c), target, u8(3));
            cursor = packet_instruction(cursor, u8(0xf3), u8(0x5b), target, target);
            cursor = packet_constant_at(cursor, u8(2), 0x80000000);
            cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), u8(1), u8(2));
            cursor = packet_instruction(cursor, u8(0x66), u8(0xef), target, u8(1));
        }
    }
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_math(MoveLocation dst, MoveLocation left, MoveLocation right,
    PacketMathOperation operation, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u8 target = packet_destination(dst);
    if (operation == PacketMathOperation::Sqrt) {
        cursor = encode_operand_instruction(cursor, u16(0x510f), u8(0), false, target, left);
    } else if (operation == PacketMathOperation::Abs) {
        cursor = packet_load_at(cursor, left, target);
        cursor = packet_constant_at(cursor, u8(1), 0x7fffffff);
        cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), target, u8(1));
    } else {
        target = binary_target(dst, left, right, MOVE_XMM);
        cursor = packet_load_at(cursor, left, target);
        u16 opcode = u16(0x5d0f);
        if (operation == PacketMathOperation::Max) { opcode = u16(0x5f0f); }
        cursor = encode_operand_instruction(cursor, opcode, u8(0), false, target, right);
    }
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_select(MoveLocation dst, MoveLocation condition, MoveLocation left,
    MoveLocation right, address output) on cpu {
    u64 start = address_bits(output); u8 target = packet_destination(dst);
    u64 cursor = packet_load_at(start, right, u8(1));
    MoveLocation source = left;
    if (left.kind == MOVE_XMM && left.base == target) {
        cursor = packet_load_at(cursor, left, u8(2)); source = packet_register(u8(2));
    }
    cursor = packet_load_at(cursor, condition, target);
    cursor = packet_constant_at(cursor, u8(3), 0);
    cursor = packet_instruction(cursor, u8(0x66), u8(0x76), target, u8(3));
    cursor = packet_instruction(cursor, u8(0x66), u8(0xdb), u8(1), target);
    cursor = encode_operand_instruction(cursor, u16(0xdf0f), u8(0x66), false, target, source);
    cursor = packet_instruction(cursor, u8(0x66), u8(0xeb), target, u8(1));
    return packet_store_at(cursor, dst, target) - start;
}

// A scalar input index is zero extended before the existing address arithmetic.
u8 address_shift(u32 stride) on cpu {
    u8 shift = u8(0);
    while (shift < u8(3) && (stride & (1 << u32(shift))) == 0) { shift += u8(1); }
    return shift;
}

u64 address_scale_at(u64 at, u8 index, u32 stride) on cpu {
    if (stride == 1) { return at; }
    u32 remaining = stride;
    u8 shift = u8(0);
    while (remaining > 1 && (remaining & 1) == 0) { remaining >>= 1; shift += u8(1); }
    u64 cursor = at;
    if (remaining == 1) {
        cursor = encode_operand_instruction(cursor, u16(0xc1), u8(0), true, u8(4), move_gpr(index));
        put_byte(cursor, shift);
        return cursor + 1u64;
    }
    cursor = encode_operand_instruction(cursor, u16(0x69), u8(0), true, index, move_gpr(index));
    write_word(cursor, stride);
    return cursor + 4u64;
}

extern "C" AddressEncoding scaled_memory(u8 base, u8 index, u32 stride, u32 offset, address output) on cpu {
    u64 start = address_bits(output);
    u8 shift = address_shift(stride);
    u64 cursor = address_scale_at(start, index, stride >> u32(shift));
    return AddressEncoding(cursor - start, offset, base, index, shift);
}

extern "C" AddressEncoding value_memory(u8 base, MoveLocation index, u32 stride, u32 offset, address output) on cpu {
    u8 shift = address_shift(stride);
    if ((stride >> u32(shift)) == 1 && index.kind == MOVE_GPR) {
        return AddressEncoding(0u64, offset, base, index.base, shift);
    }
    u64 start = address_bits(output);
    MoveEncoding loaded = copy_scalar_move(move_gpr(u8(0)), index, 4, false, output);
    u64 cursor = address_scale_at(start + loaded.length, u8(0), stride >> u32(shift));
    return AddressEncoding(cursor - start, offset, base, u8(0), shift);
}

// The allocation chooses scalar or packet input; both use the same stride arithmetic.
u64 address_word_at(u64 at, MoveLocation src, bool packet, u8 lane, u8 gpr) on cpu {
    if (packet) { return packet_lane_at(at, src, lane, gpr); }
    return at + copy_scalar_move(move_gpr(gpr), src, 4, false, unsafe_address(at)).length;
}

AddressEncoding view_memory_at(u8 base, MoveLocation index, u64 stride, u32 offset,
    bool packet, u8 lane, address output) on cpu {
    u64 start = address_bits(output);
    if (stride <= 0xffffffffu64) {
        if (!packet) { return value_memory(base, index, u32(stride), offset, output); }
        u64 cursor = packet_lane_at(start, index, lane, u8(0));
        AddressEncoding result = scaled_memory(base, u8(0), u32(stride), offset, unsafe_address(cursor));
        result.length += cursor - start;
        return result;
    }
    u64 cursor = start + copy_scalar_move(move_gpr(u8(1)), MoveLocation(stride, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0)), 8, false, output).length;
    cursor = address_word_at(cursor, index, packet, lane, u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(1));
    return AddressEncoding(cursor - start, offset, base, u8(0), u8(0));
}

AddressEncoding view_memory_dynamic_at(u8 base, MoveLocation index, MoveLocation low,
    MoveLocation high, u32 offset, bool packet, u8 lane, address output) on cpu {
    u64 start = address_bits(output);
    u8 high_register = u8(0);
    if (packet) { high_register = u8(1); }
    u64 cursor = address_word_at(start, high, packet, lane, high_register);
    cursor = encode_operand_instruction(cursor, u16(0xc1), u8(0), true, u8(4), move_gpr(high_register));
    put_byte(cursor, u8(32)); cursor += 1u64;
    if (!packet) { cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(0), u8(1)); }
    cursor = address_word_at(cursor, low, packet, lane, u8(0));
    cursor = register_instruction(cursor, u16(0x09), u8(0), true, u8(0), u8(1));
    cursor = address_word_at(cursor, index, packet, lane, u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(1));
    return AddressEncoding(cursor - start, offset, base, u8(0), u8(0));
}

extern "C" AddressEncoding view_memory(u8 base, MoveLocation index, u64 stride, u32 offset, address output) on cpu {
    return view_memory_at(base, index, stride, offset, false, u8(0), output);
}

extern "C" AddressEncoding view_memory_dynamic(u8 base, MoveLocation index, MoveLocation low, MoveLocation high, u32 offset, address output) on cpu {
    return view_memory_dynamic_at(base, index, low, high, offset, false, u8(0), output);
}

extern "C" u64 packet_lane(MoveLocation src, u8 lane, u8 gpr, address output) on cpu {
    u64 start = address_bits(output);
    return packet_lane_at(start, src, lane, gpr) - start;
}

extern "C" AddressEncoding packet_view_memory(u8 base, MoveLocation index, u8 lane, u64 stride, u32 offset, address output) on cpu {
    return view_memory_at(base, index, stride, offset, true, lane, output);
}

extern "C" AddressEncoding packet_view_memory_dynamic(u8 base, MoveLocation index, MoveLocation low, MoveLocation high, u8 lane, u32 offset, address output) on cpu {
    return view_memory_dynamic_at(base, index, low, high, offset, true, lane, output);
}

extern "C" u64 scale_index(u8 index, u32 stride, address output) on cpu {
    u64 start = address_bits(output);
    return address_scale_at(start, index, stride) - start;
}

extern "C" u64 offset_address(u32 stride, address output) on cpu {
    u64 start = address_bits(output);
    u8 shift = address_shift(stride);
    if ((stride >> u32(shift)) == 1 && stride <= 8) {
        MoveLocation memory = MoveLocation(0u64, MOVE_MEMORY, u8(15), u8(0), shift);
        return encode_operand_instruction(start, u16(0x8d), u8(0), true, u8(15), memory) - start;
    }
    u64 cursor = address_scale_at(start, u8(0), stride);
    put_byte(cursor, u8(0x49)); put_byte(cursor + 1u64, u8(0x01)); put_byte(cursor + 2u64, u8(0xc7));
    return cursor + 3u64 - start;
}

// Buffer field offsets and stack indices come from the original native layout.
u64 packet_table_base(u64 at, u32 offset) on cpu {
    put_byte(at, u8(0x4d)); put_byte(at + 1u64, u8(0x8b));
    put_byte(at + 2u64, u8(0xbc)); put_byte(at + 3u64, u8(0x24));
    write_word(at + 4u64, offset);
    return at + 8u64;
}

MoveLocation packet_buffer_operand(u32 offset) on cpu {
    return MoveLocation(u64(offset), MOVE_MEMORY, u8(15), u8(0), u8(0));
}

u64 packet_buffer_scale(u64 at, u32 stride_offset) on cpu {
    MoveLocation stride = MoveLocation(u64(stride_offset), MOVE_MEMORY, u8(12), NO_INDEX, u8(0));
    return encode_operand_instruction(at, u16(0xaf0f), u8(0), true, u8(0), stride);
}

u64 packet_memory_at(u64 at, PacketMemory memory, u8 lane) on cpu {
    u64 cursor = at;
    if (memory.index_kind == 1) {
        MoveLocation index = MoveLocation(u64(memory.frame_offset + u32(lane) * 4), MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(0), index);
    } else if (memory.index_kind == 2) {
        cursor = packet_lane_at(cursor, memory.index, u8(0), u8(0));
    } else {
        put_byte(cursor, u8(0x89)); put_byte(cursor + 1u64, u8(0xd8)); cursor += 2u64;
        if (lane != u8(0)) {
            cursor = register_instruction(cursor, u16(0x83), u8(0), true, u8(0), u8(0));
            put_byte(cursor, lane); cursor += 1u64;
        }
    }
    cursor = packet_table_base(cursor, memory.data_offset);
    return packet_buffer_scale(cursor, memory.stride_offset);
}

u64 packet_contiguous_field(u64 at, u32 stride_offset) on cpu {
    MoveLocation stride = MoveLocation(u64(stride_offset), MOVE_MEMORY, u8(12), NO_INDEX, u8(0));
    u64 cursor = encode_operand_instruction(at, u16(0x83), u8(0), true, u8(7), stride);
    put_byte(cursor, u8(4));
    return condition_field(cursor + 1u64, u8(0x85));
}

u64 packet_combine_at(u64 at, u8 target, bool broadcast) on cpu {
    if (broadcast) { return packet_shuffle_at(at, target, target, u8(0)); }
    u64 cursor = packet_instruction(at, u8(0x66), u8(0x62), target, u8(1));
    cursor = packet_instruction(cursor, u8(0x66), u8(0x62), u8(2), u8(3));
    return packet_instruction(cursor, u8(0x66), u8(0x6c), target, u8(2));
}

extern "C" u64 packet_read(MoveLocation dst, PacketMemory memory, bool boolean, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u64 done = 0u64;
    u8 target = packet_destination(dst);
    MoveLocation operand = packet_buffer_operand(memory.offset);
    if (memory.contiguous != 0) {
        u64 strided = packet_contiguous_field(cursor, memory.stride_offset);
        cursor = packet_memory_at(strided + 4u64, memory, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x100f), u8(0), false, target, operand);
        done = jump_field(cursor); cursor = done + 4u64;
        patch_relative(unsafe_address(strided), strided, cursor);
    }
    bool broadcast = memory.index_kind == 2;
    u8 count = u8(4); if (broadcast) { count = u8(1); }
    for (ordinal in 0..u32(count)) {
        u8 lane = u8(ordinal);
        cursor = packet_memory_at(cursor, memory, lane);
        u8 to = lane; if (lane == u8(0)) { to = target; }
        cursor = encode_operand_instruction(cursor, u16(0x6e0f), u8(0x66), false, to, operand);
    }
    cursor = packet_combine_at(cursor, target, broadcast);
    if (done != 0u64) { patch_relative(unsafe_address(done), done, cursor); }
    cursor += packet_from_word(boolean, target, unsafe_address(cursor));
    return packet_store_at(cursor, dst, target) - start;
}

extern "C" u64 packet_write(MoveLocation src, PacketMemory memory, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start; u64 done = 0u64;
    u8 source = u8(1);
    if (src.kind == MOVE_XMM) { source = src.base; }
    else { cursor = packet_load_at(cursor, src, source); }
    MoveLocation operand = packet_buffer_operand(memory.offset);
    if (memory.contiguous != 0) {
        u64 strided = packet_contiguous_field(cursor, memory.stride_offset);
        cursor = packet_memory_at(strided + 4u64, memory, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x110f), u8(0), false, source, operand);
        done = jump_field(cursor); cursor = done + 4u64;
        patch_relative(unsafe_address(strided), strided, cursor);
    }
    for (ordinal in 0..4) {
        u8 lane = u8(ordinal);
        cursor = packet_memory_at(cursor, memory, lane);
        u8 from = source;
        if (lane != u8(0)) {
            cursor = packet_shuffle_at(cursor, u8(0), source, lane * u8(0x55)); from = u8(0);
        }
        cursor = encode_operand_instruction(cursor, u16(0x7e0f), u8(0x66), false, from, operand);
    }
    if (done != 0u64) { patch_relative(unsafe_address(done), done, cursor); }
    return cursor - start;
}

u64 packet_gather_base(u64 at, PacketGather input, u8 lane) on cpu {
    if (input.kind == 0) { return packet_table_base(at, input.data_offset); }
    u64 cursor = packet_lane_at(at, input.base_high, lane, u8(15));
    put_byte(cursor, u8(0x49)); put_byte(cursor + 1u64, u8(0xc1));
    put_byte(cursor + 2u64, u8(0xe7)); put_byte(cursor + 3u64, u8(32)); cursor += 4u64;
    cursor = packet_lane_at(cursor, input.base_low, lane, u8(0));
    put_byte(cursor, u8(0x49)); put_byte(cursor + 1u64, u8(0x09)); put_byte(cursor + 2u64, u8(0xc7));
    return cursor + 3u64;
}

extern "C" u64 packet_gather(MoveLocation dst, PacketGather input, bool boolean, address output) on cpu {
    u64 start = address_bits(output); u64 cursor = start;
    bool shared = input.kind == 0 || input.uniform != 0;
    if (shared) { cursor = packet_gather_base(cursor, input, u8(0)); }
    u8 count = u8(4); if (input.broadcast != 0) { count = u8(1); }
    for (ordinal in 0..u32(count)) {
        u8 lane = u8(ordinal);
        if (!shared) { cursor = packet_gather_base(cursor, input, lane); }
        MoveLocation operand = packet_buffer_operand(input.offset);
        if (input.kind == 0) {
            cursor = packet_lane_at(cursor, input.index, lane, u8(0));
            cursor = packet_buffer_scale(cursor, input.stride_offset);
        } else {
            AddressEncoding memory;
            if (input.kind == 1) {
                memory = view_memory_at(u8(15), input.index, input.stride, input.offset, true, lane, unsafe_address(cursor));
            } else {
                memory = view_memory_dynamic_at(u8(15), input.index, input.stride_low, input.stride_high, input.offset, true, lane, unsafe_address(cursor));
            }
            cursor += memory.length;
            operand = MoveLocation(u64(memory.offset), MOVE_MEMORY, memory.base, memory.index, memory.shift);
        }
        // Extract all original lanes before the allocated destination is overwritten.
        cursor = encode_operand_instruction(cursor, u16(0x6e0f), u8(0x66), false, lane, operand);
    }
    cursor = packet_combine_at(cursor, u8(0), input.broadcast != 0);
    cursor += packet_from_word(boolean, u8(0), unsafe_address(cursor));
    return packet_store_at(cursor, dst, u8(0)) - start;
}

u64 atomic_input(u64 at, MoveLocation source, u32 size) on cpu {
    MoveEncoding load = copy_scalar_move(move_gpr(u8(0)), source, size, false, unsafe_address(at));
    return at + load.length;
}

u64 atomic_update(u64 at, AtomicOperation operation, bool wide, bool signed) on cpu {
    u16 opcode = u16(0x39);
    if (operation == AtomicOperation::Add) { opcode = u16(0x01); }
    if (operation == AtomicOperation::Sub) { opcode = u16(0x29); }
    if (operation == AtomicOperation::And) { opcode = u16(0x21); }
    if (operation == AtomicOperation::Or) { opcode = u16(0x09); }
    if (operation == AtomicOperation::Xor) { opcode = u16(0x31); }
    u64 cursor = register_instruction(at, opcode, u8(0), wide, u8(2), u8(1));
    if (operation == AtomicOperation::Min || operation == AtomicOperation::Max) {
        bool minimum = operation == AtomicOperation::Min;
        u8 condition = signed ? (minimum ? u8(0x4f) : u8(0x4c)) : (minimum ? u8(0x47) : u8(0x42));
        cursor = register_instruction(cursor, u16(0x0f) | (u16(condition) << 8), u8(0), wide, u8(1), u8(2));
    }
    return cursor;
}

u64 atomic_read_split(u64 at, MoveLocation low, MoveLocation high) on cpu {
    u64 cursor = encode_operand_instruction(at, u16(0x8b), u8(0), false, u8(0), low);
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(1), high);
    cursor = register_instruction(cursor, u16(0xc1), u8(0), true, u8(4), u8(1));
    put_byte(cursor, u8(32));
    return register_instruction(cursor + 1u64, u16(0x09), u8(0), true, u8(1), u8(0));
}

u64 atomic_write_split(u64 at, MoveLocation low, MoveLocation high, u8 source) on cpu {
    u64 cursor = encode_operand_instruction(at, u16(0x89), u8(0), false, source, low);
    cursor = register_instruction(cursor, u16(0xc1), u8(0), true, u8(5), source);
    put_byte(cursor, u8(32));
    return encode_operand_instruction(cursor + 1u64, u16(0x89), u8(0), false, source, high);
}

u64 atomic_split_at(u64 at, AtomicSequence input) on cpu {
    AtomicOperation operation = input.operation;
    if (operation == AtomicOperation::Load) {
        return atomic_read_split(at, input.low, input.high);
    }
    u64 cursor = atomic_input(at, input.first, 8);
    if (operation == AtomicOperation::Store) {
        return atomic_write_split(cursor, input.low, input.high, u8(0));
    }
    cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(0), u8(2));
    cursor = atomic_read_split(cursor, input.low, input.high);
    if (operation == AtomicOperation::Exchange) {
        return atomic_write_split(cursor, input.low, input.high, u8(2));
    }
    if (operation == AtomicOperation::CompareExchange) {
        cursor = register_instruction(cursor, u16(0x39), u8(0), true, u8(2), u8(0));
        u64 different = condition_field(cursor, u8(0x85));
        cursor = register_instruction(different + 4u64, u16(0x89), u8(0), true, u8(0), u8(1));
        cursor = atomic_input(cursor, input.second, 8);
        cursor = atomic_write_split(cursor, input.low, input.high, u8(0));
        cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(1), u8(0));
        patch_relative(unsafe_address(different), different - at, cursor - at);
        return cursor;
    }
    cursor = register_instruction(cursor, u16(0x89), u8(0), true, u8(0), u8(1));
    cursor = atomic_update(cursor, operation, true, input.scalar.kind == ScalarKind::Signed);
    return atomic_write_split(cursor, input.low, input.high, u8(1));
}

u64 atomic_contiguous_at(u64 at, AtomicSequence input) on cpu {
    AtomicOperation operation = input.operation;
    u32 size = input.scalar.width / 8;
    bool wide = size == 8;
    bool shared = input.storage == AtomicStorage::Shared;
    u8 lock = shared ? u8(0xf0) : u8(0);
    if (operation == AtomicOperation::Load) {
        return encode_operand_instruction(at, u16(0x8b), u8(0), wide, u8(0), input.low);
    }
    if (operation == AtomicOperation::CompareExchange) {
        u64 cursor = atomic_input(at, input.second, size);
        cursor = register_instruction(cursor, u16(0x89), u8(0), wide, u8(0), u8(2));
        cursor = atomic_input(cursor, input.first, size);
        return encode_operand_instruction(cursor, u16(0xb10f), lock, wide, u8(2), input.low);
    }
    u64 cursor = atomic_input(at, input.first, size);
    if (operation == AtomicOperation::Store) {
        return encode_operand_instruction(cursor, u16(0x89), u8(0), wide, u8(0), input.low);
    }
    if (operation == AtomicOperation::Add || operation == AtomicOperation::Sub) {
        if (operation == AtomicOperation::Sub) {
            cursor = register_instruction(cursor, u16(0xf7), u8(0), wide, u8(3), u8(0));
        }
        return encode_operand_instruction(cursor, u16(0xc10f), lock, wide, u8(0), input.low);
    }
    if (operation == AtomicOperation::Exchange && shared) {
        // Memory XCHG is implicitly locked.
        return encode_operand_instruction(cursor, u16(0x87), u8(0), wide, u8(0), input.low);
    }
    cursor = register_instruction(cursor, u16(0x89), u8(0), wide, u8(0), u8(2));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), wide, u8(0), input.low);
    if (operation == AtomicOperation::Exchange) {
        return encode_operand_instruction(cursor, u16(0x89), u8(0), wide, u8(2), input.low);
    }
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x89), u8(0), wide, u8(0), u8(1));
    cursor = atomic_update(cursor, operation, wide, input.scalar.kind == ScalarKind::Signed);
    if (shared) {
        // CMPXCHG supplies the new observed value in RAX after a failed exchange.
        cursor = encode_operand_instruction(cursor, u16(0xb10f), u8(0xf0), wide, u8(1), input.low);
        u64 again = condition_field(cursor, u8(0x85));
        patch_relative(unsafe_address(again), again - at, begin - at);
        return again + 4u64;
    }
    return encode_operand_instruction(cursor, u16(0x89), u8(0), wide, u8(1), input.low);
}

extern "C" u64 atomic_sequence(AtomicSequence input, address output) on cpu {
    u64 start = address_bits(output);
    if (input.storage == AtomicStorage::Split) {
        return atomic_split_at(start, input) - start;
    }
    return atomic_contiguous_at(start, input) - start;
}

u64 compact_disp32(u64 at, u32 instruction, u32 displacement) on cpu {
    put_byte(at, u8(instruction));
    put_byte(at + 1u64, u8(instruction >> 8));
    put_byte(at + 2u64, u8(instruction >> 16));
    write_word(at + 3u64, displacement);
    return at + 7u64;
}

u64 load_range_binding(u64 at, u8 target, u32 offset) on cpu {
    MoveLocation slot = MoveLocation(u64(offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    return encode_operand_instruction(at, u16(0x8b), u8(0), true, target, slot);
}

u64 compact_flag_test(u64 at, u32 word) on cpu {
    u64 immediate = compact_disp32(at, 0xba8341, word); // cmp dword [r10 + word], 0
    put_byte(immediate, u8(0));
    return immediate + 1u64;
}

u64 compact_advance(u64 at, u8 pointer, u8 stride, u32 bytes) on cpu {
    u64 cursor = register_instruction(at, u16(0x03), u8(0), true, pointer, stride);
    cursor = register_instruction(cursor, u16(0x81), u8(0), true, u8(5), pointer);
    write_word(cursor, bytes);
    return cursor + 4u64;
}

u64 copy_record_at(u64 at, u32 bytes) on cpu {
    u64 cursor = at;
    u32 vectors = bytes / 16;
    if (vectors != 0) {
        if (vectors != 1) {
            cursor = move_immediate(cursor, u8(1), u64(vectors), false);
        }
        u64 begin = cursor;
        // movups xmm0,[rsi]; movups [rdi],xmm0; advance both record cursors.
        write_word(cursor, 0x0f06100f);
        write_word(cursor + 4u64, 0x83480711);
        write_word(cursor + 8u64, 0x834810c6);
        put_byte(cursor + 12u64, u8(0xc7));
        put_byte(cursor + 13u64, u8(16));
        cursor += 14u64;
        if (vectors != 1) {
            cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(1)); // dec ecx
            u64 again = condition_field(cursor, u8(0x85));
            patch_relative(unsafe_address(again), again - at, begin - at);
            cursor = again + 4u64;
        }
    }
    if ((bytes & 8) != 0) { put_byte(cursor, u8(0x48)); put_byte(cursor + 1u64, u8(0xa5)); cursor += 2u64; }
    if ((bytes & 4) != 0) { put_byte(cursor, u8(0xa5)); cursor += 1u64; }
    if ((bytes & 2) != 0) { put_byte(cursor, u8(0x66)); put_byte(cursor + 1u64, u8(0xa5)); cursor += 2u64; }
    if ((bytes & 1) != 0) { put_byte(cursor, u8(0xa4)); cursor += 1u64; }
    return cursor;
}

extern "C" u64 compact_count(u32 flags, u32 stride_offset, u32 flag_word, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = load_range_binding(start, u8(12), flags + stride_offset);
    cursor = compact_disp32(cursor, 0x918b4c, flags); // mov r10,[rcx + flags]
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(1));
    cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(0), u8(0));
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(8), u8(1));
    u64 end = condition_field(cursor, u8(0x83));
    cursor = compact_flag_test(end + 4u64, flag_word);
    write_word(cursor, 0x0fc2950f); // setne dl; movzx edx,dl
    put_byte(cursor + 4u64, u8(0xb6));
    put_byte(cursor + 5u64, u8(0xd2));
    cursor = register_instruction(cursor + 6u64, u16(0x01), u8(0), false, u8(2), u8(0));
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(1));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back - start, begin - start);
    patch_relative(unsafe_address(end), end - start, cursor - start);
    // The fifth argument receives the algorithm output; EAX reports success.
    write_word(cursor, 0x24548b4c);
    write_word(cursor + 4u64, 0x02894130);
    write_word(cursor + 8u64, 0x5c41c031);
    put_byte(cursor + 12u64, u8(0xc3));
    return cursor + 13u64 - start;
}

extern "C" u64 compact_scatter(CompactScatter input, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = load_range_binding(start, u8(13), input.source + input.stride_offset);
    cursor = load_range_binding(cursor, u8(14), input.target + input.stride_offset);
    cursor = load_range_binding(cursor, u8(15), input.flags + input.stride_offset);
    cursor = compact_disp32(cursor, 0xb18b48, input.source); // rsi = source
    cursor = compact_disp32(cursor, 0xb98b48, input.target); // rdi = destination
    cursor = compact_disp32(cursor, 0x918b4c, input.flags);  // r10 = flags
    cursor = compact_disp32(cursor, 0x918b48, input.prefix); // rdx = range prefix
    MoveLocation prefix = MoveLocation(0u64, MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(0), prefix);
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(14));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(7));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(6));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(15));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(3));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(8), u8(12));
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(12), u8(3));
    u64 end = condition_field(cursor, u8(0x83));
    cursor = compact_flag_test(end + 4u64, input.flag_word);
    u64 skip = condition_field(cursor, u8(0x84));
    cursor = copy_record_at(skip + 4u64, input.record_bytes);
    cursor = compact_advance(cursor, u8(6), u8(13), input.record_bytes);
    cursor = compact_advance(cursor, u8(7), u8(14), input.record_bytes);
    u64 next = jump_field(cursor);
    cursor = next + 4u64;
    patch_relative(unsafe_address(skip), skip - start, cursor - start);
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(6), u8(13));
    patch_relative(unsafe_address(next), next - start, cursor - start);
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(15));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(3));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back - start, begin - start);
    patch_relative(unsafe_address(end), end - start, cursor - start);
    // xor eax,eax; pop r15,r14,r13,r12,rdi,rsi,rbx; ret.
    write_word(cursor, 0x5f41c031);
    write_word(cursor + 4u64, 0x5d415e41);
    write_word(cursor + 8u64, 0x5e5f5c41);
    put_byte(cursor + 12u64, u8(0x5b));
    put_byte(cursor + 13u64, u8(0xc3));
    return cursor + 14u64 - start;
}

u64 sort_range_end(u64 at) on cpu {
    u64 cursor = register_instruction(at, u16(0x39), u8(0), false, u8(8), u8(9)); // cmp r9d,r8d
    return condition_field(cursor, u8(0x83));
}

u64 sort_finish_range(u64 at, u64 start, u64 begin, u64 end) on cpu {
    u64 cursor = register_instruction(at, u16(0xff), u8(0), false, u8(0), u8(9)); // inc r9d
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back - start, begin - start);
    patch_relative(unsafe_address(end), end - start, cursor - start);
    return cursor;
}

extern "C" u64 sort_key(SortKeyEmission input, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = load_range_binding(start, u8(12), input.stride_offset);
    cursor = load_range_binding(cursor, u8(10), 0); // source record base
    cursor = load_range_binding(cursor, u8(11), 3 * input.buffer_bytes);
    if (input.first != 0) {
        cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
        cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
        cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    }
    u64 begin = cursor;
    u64 end = sort_range_end(cursor);
    cursor = end + 4u64;
    MoveLocation index = MoveLocation(4u64, MOVE_MEMORY, u8(11), u8(9), u8(3));
    MoveLocation key = MoveLocation(u64(input.word), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    if (input.first != 0) {
        cursor = encode_operand_instruction(cursor, u16(0x89), u8(0), false, u8(9), index);
    } else {
        cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(0), index);
        cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
        key.index = u8(0);
    }
    if (input.floating != 0 && input.bytes == 8) {
        MoveLocation high = key;
        high.value += 4u64;
        cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(1), high);
    }
    u16 load = input.bytes == 1 ? u16(0xb60f) : (input.bytes == 2 ? u16(0xb70f) : u16(0x8b));
    cursor = encode_operand_instruction(cursor, load, u8(0), false, u8(0), key);
    // Each radix pass consumes a 32-bit subkey and its original ordinal.
    if (input.bias != 0) {
        put_byte(cursor, u8(0x35)); // xor eax,imm32
        write_word(cursor + 1u64, input.bias);
        cursor += 5u64;
    } else if (input.floating != 0 && input.bytes == 4) {
        write_word(cursor, 0xfac1c289); // mov edx,eax; sar edx,31
        put_byte(cursor + 4u64, u8(31));
        put_byte(cursor + 5u64, u8(0x81)); // or edx,0x80000000
        put_byte(cursor + 6u64, u8(0xca));
        write_word(cursor + 7u64, 0x80000000);
        cursor = register_instruction(cursor + 11u64, u16(0x31), u8(0), false, u8(2), u8(0));
    } else if (input.floating != 0 && input.bytes == 8) {
        put_byte(cursor, u8(0xc1)); // sar ecx,31
        put_byte(cursor + 1u64, u8(0xf9));
        put_byte(cursor + 2u64, u8(31));
        cursor = register_instruction(cursor + 3u64, u16(0x31), u8(0), false, u8(1), u8(0));
    }
    if (input.descending != 0) {
        cursor = register_instruction(cursor, u16(0xf7), u8(0), false, u8(2), u8(0)); // not eax
    }
    MoveLocation row = MoveLocation(0u64, MOVE_MEMORY, u8(11), u8(9), u8(3));
    cursor = encode_operand_instruction(cursor, u16(0x89), u8(0), false, u8(0), row);
    if (input.first != 0) {
        cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    }
    cursor = sort_finish_range(cursor, start, begin, end);
    write_word(cursor, 0x5c41c031);
    put_byte(cursor + 4u64, u8(0xc3));
    return cursor + 5u64 - start;
}

u64 sort_digit(u64 at) on cpu {
    MoveLocation row = MoveLocation(0u64, MOVE_MEMORY, u8(10), u8(9), u8(3));
    u64 cursor = encode_operand_instruction(at, u16(0x8b), u8(0), false, u8(0), row);
    write_word(cursor, 0xe083e8d3); // shr eax,cl; and eax,15
    put_byte(cursor + 4u64, u8(15));
    return cursor + 5u64;
}

u64 sort_shift(u64 at, u32 buffer_bytes) on cpu {
    u64 cursor = load_range_binding(at, u8(2), 4 * buffer_bytes);
    MoveLocation params = MoveLocation(0u64, MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
    return encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(1), params);
}

extern "C" u64 sort_histogram(u32 buffer_bytes, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = load_range_binding(start, u8(10), 2 * buffer_bytes);
    cursor = sort_shift(cursor, buffer_bytes);
    u64 begin = cursor;
    u64 end = sort_range_end(cursor);
    cursor = sort_digit(end + 4u64);
    // Parameter row: shift followed by the range's private counts.
    MoveLocation count = MoveLocation(4u64, MOVE_MEMORY, u8(2), u8(0), u8(2));
    cursor = encode_operand_instruction(cursor, u16(0xff), u8(0), false, u8(0), count);
    cursor = sort_finish_range(cursor, start, begin, end);
    put_byte(cursor, u8(0x31));
    put_byte(cursor + 1u64, u8(0xc0));
    put_byte(cursor + 2u64, u8(0xc3));
    return cursor + 3u64 - start;
}

extern "C" u64 sort_scatter(u32 buffer_bytes, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = load_range_binding(start, u8(10), 2 * buffer_bytes);
    cursor = load_range_binding(cursor, u8(11), 3 * buffer_bytes);
    cursor = sort_shift(cursor, buffer_bytes);
    u64 begin = cursor;
    u64 end = sort_range_end(cursor);
    cursor = sort_digit(end + 4u64);
    MoveLocation bucket = MoveLocation(4u64, MOVE_MEMORY, u8(2), u8(0), u8(2));
    cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(0), bucket);
    MoveLocation next = MoveLocation(0u64, MOVE_MEMORY, u8(0), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(3), next);
    cursor = encode_operand_instruction(cursor, u16(0xff), u8(0), false, u8(0), next);
    MoveLocation source = MoveLocation(0u64, MOVE_MEMORY, u8(10), u8(9), u8(3));
    MoveLocation target = MoveLocation(0u64, MOVE_MEMORY, u8(11), u8(3), u8(3));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), true, u8(0), source);
    cursor = encode_operand_instruction(cursor, u16(0x89), u8(0), true, u8(0), target);
    cursor = sort_finish_range(cursor, start, begin, end);
    write_word(cursor, 0xc35bc031);
    return cursor + 4u64 - start;
}

extern "C" u64 sort_gather(u32 buffer_bytes, u32 stride_offset, u32 record_bytes, address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = load_range_binding(start, u8(12), stride_offset);
    cursor = load_range_binding(cursor, u8(13), buffer_bytes + stride_offset);
    cursor = load_range_binding(cursor, u8(10), 0);
    cursor = load_range_binding(cursor, u8(11), buffer_bytes);
    cursor = load_range_binding(cursor, u8(2), 2 * buffer_bytes);
    u64 begin = cursor;
    u64 end = sort_range_end(cursor);
    MoveLocation index = MoveLocation(4u64, MOVE_MEMORY, u8(2), u8(9), u8(3));
    cursor = encode_operand_instruction(end + 4u64, u16(0x8b), u8(0), false, u8(0), index);
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    MoveLocation source = MoveLocation(0u64, MOVE_MEMORY, u8(10), u8(0), u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(6), source);
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
    MoveLocation target = MoveLocation(0u64, MOVE_MEMORY, u8(11), u8(0), u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(7), target);
    cursor = copy_record_at(cursor, record_bytes);
    cursor = sort_finish_range(cursor, start, begin, end);
    write_word(cursor, 0x5d41c031);
    write_word(cursor + 4u64, 0x5e5f5c41);
    put_byte(cursor + 8u64, u8(0x5b));
    put_byte(cursor + 9u64, u8(0xc3));
    return cursor + 10u64 - start;
}

extern "C" AppendBranch reserve_append(u32 counter_slot, u32 counter_word,
    u32 capacity_slot, u32 target_slot, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation count = MoveLocation(u64(counter_slot), MOVE_MEMORY, u8(12), NO_INDEX, u8(0));
    u64 cursor = encode_operand_instruction(start, u16(0x8b), u8(0), true, u8(15), count);
    MoveLocation counter = MoveLocation(u64(counter_word), MOVE_MEMORY, u8(15), NO_INDEX, u8(0));
    MoveLocation capacity = MoveLocation(u64(capacity_slot), MOVE_MEMORY, u8(13), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(1), capacity);
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), false, u8(0), counter);
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(1), u8(0));
    u64 full = condition_field(cursor, u8(0x83));
    write_word(full, 0); // The emitter patches across the record stores.
    MoveLocation successor = MoveLocation(1u64, MOVE_MEMORY, u8(0), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(full + 4u64, u16(0x8d), u8(0), false, u8(2), successor);
    cursor = encode_operand_instruction(cursor, u16(0xb10f), u8(0xf0), false, u8(2), counter);
    u64 again = condition_field(cursor, u8(0x85));
    patch_relative(unsafe_address(again), again - start, begin - start);
    // EAX still holds the successful old count; R15 receives the target base.
    MoveLocation target = MoveLocation(u64(target_slot), MOVE_MEMORY, u8(12), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(again + 4u64, u16(0x8b), u8(0), true, u8(15), target);
    return AppendBranch(cursor - start, full - start);
}

extern "C" AppendBranch append_result(address output) on cpu {
    u64 start = address_bits(output);
    u64 cursor = move_immediate(start, u8(0), 1u64, false);
    u64 done = jump_field(cursor);
    u64 full = done + 4u64;
    cursor = register_instruction(full, u16(0x31), u8(0), false, u8(0), u8(0));
    patch_relative(unsafe_address(done), done - start, cursor - start);
    return AppendBranch(cursor - start, full - start);
}

u8 reduction_opcode(ReductionOperator operation, bool floating) on cpu {
    if (operation == ReductionOperator::Sum) { return floating ? u8(0x58) : u8(0x03); }
    if (operation == ReductionOperator::Min) { return u8(0x5d); }
    if (operation == ReductionOperator::Max) { return u8(0x5f); }
    if (operation == ReductionOperator::BitAnd) { return u8(0x23); }
    if (operation == ReductionOperator::BitOr) { return u8(0x0b); }
    return u8(0x33);
}

u64 collective_accumulate(u64 at, ReductionOperator operation, ScalarEncoding scalar, u8 register, MoveLocation operand) on cpu {
    u32 size = scalar.width / 8;
    bool wide = size == 8;
    bool floating = scalar.kind == ScalarKind::Float;
    u8 opcode = reduction_opcode(operation, floating);
    if (floating) {
        u16 instruction = (u16(opcode) << 8) | u16(0x0f);
        u8 prefix = wide ? u8(0xf2) : u8(0xf3);
        return encode_operand_instruction(at, instruction, prefix, false, register, operand);
    }
    u8 prefix = size == 2 ? u8(0x66) : u8(0);
    u16 byte_form = size == 1 ? u16(1) : u16(0);
    bool minimum = operation == ReductionOperator::Min;
    if (!minimum && operation != ReductionOperator::Max) {
        return encode_operand_instruction(at, u16(opcode) - byte_form, prefix, wide, register, operand);
    }
    bool signed = scalar.kind == ScalarKind::Signed;
    u8 condition = minimum ? (signed ? u8(0x4f) : u8(0x47)) : (signed ? u8(0x4c) : u8(0x42));
    u64 cursor = encode_operand_instruction(at, u16(0x3b) - byte_form, prefix, wide, register, operand);
    if (size != 1) {
        return encode_operand_instruction(cursor, (u16(condition) << 8) | u16(0x0f), prefix, wide, register, operand);
    }
    // There is no byte CMOV. Read the selected byte only when it wins.
    u64 skip = condition_field(cursor, (condition + u8(0x40)) ^ u8(1));
    cursor = encode_operand_instruction(skip + 4u64, u16(0x8a), u8(0), false, register, operand);
    patch_relative(unsafe_address(skip), skip, cursor);
    return cursor;
}

extern "C" ReductionCode reduce_range(ReductionRange input, address output) on cpu {
    u64 start = address_bits(output);
    MoveLocation stride = MoveLocation(u64(input.source + input.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    u64 cursor = encode_operand_instruction(start, u16(0x8b), u8(0), true, u8(12), stride);
    // Keep source address in R10, stride in R12, and range index in ECX.
    MoveLocation source = MoveLocation(u64(input.source), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), true, u8(10), source);
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(1));

    let integer_registers = array<u8,4>(u8(0), u8(2), u8(9), u8(11));
    array<u64,4> literals = array<u64,4>(0u64, 0u64, 0u64, 0u64);
    u32 size = input.scalar.width / 8;
    bool floating = input.scalar.kind == ScalarKind::Float;
    for (component in 0..4) {
        if (component == input.count) { break; }
        if (floating) {
            MoveLocation dst = MoveLocation(0u64, MOVE_XMM, u8(component), NO_INDEX, u8(0));
            MoveLocation src = MoveLocation(input.identity, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
            MoveEncoding value = copy_scalar_move(dst, src, size, false, unsafe_address(cursor));
            if (value.literal_field != 0u64) { literals[component] = cursor - start + value.literal_field; }
            cursor += value.length;
        } else {
            cursor = move_immediate(cursor, integer_registers[component], input.identity, size == 8);
        }
    }

    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(8), u8(1));
    u64 end = condition_field(cursor, u8(0x83));
    cursor = end + 4u64;
    for (component in 0..4) {
        if (component == input.count) { break; }
        u8 register = floating ? u8(component) : integer_registers[component];
        MoveLocation operand = MoveLocation(u64(input.source_word + input.offsets[component]), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = collective_accumulate(cursor, input.operation, input.scalar, register, operand);
    }
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(1));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back, begin);
    patch_relative(unsafe_address(end), end, cursor);

    // Return address, four shadow slots and saved R12 precede the result pointer.
    MoveLocation result = MoveLocation(0x30u64, MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(cursor, u16(0x8b), u8(0), true, u8(10), result);
    for (component in 0..4) {
        if (component == input.count) { break; }
        MoveLocation destination = MoveLocation(u64(input.offsets[component]), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        if (floating) {
            cursor = store_float(cursor, destination, u8(component), size == 8);
        } else {
            cursor = store_bits(cursor, destination, integer_registers[component], size);
        }
    }
    write_word(cursor, 0x5c41c031);
    put_byte(cursor + 4u64, u8(0xc3));
    return ReductionCode(cursor + 5u64 - start, literals);
}

u64 segmented_identity(u64 at, ReductionRange values) on cpu {
    bool wide = values.scalar.width == 64;
    u64 cursor = move_immediate(at, u8(0), values.identity, wide);
    for (component in 0..4) {
        if (component == values.count) { break; }
        cursor = register_instruction(cursor, u16(0x6e0f), u8(0x66), wide, u8(component), u8(0));
    }
    return cursor;
}

u64 segmented_store(u64 at, ReductionRange values, u8 base, u32 field) on cpu {
    u64 cursor = at;
    u32 size = values.scalar.width / 8;
    for (component in 0..4) {
        if (component == values.count) { break; }
        MoveLocation destination = MoveLocation(u64(field + values.offsets[component]), MOVE_MEMORY, base, NO_INDEX, u8(0));
        MoveLocation source = MoveLocation(0u64, MOVE_XMM, u8(component), NO_INDEX, u8(0));
        cursor += write_scalar_move(destination, source, size, unsafe_address(cursor));
    }
    return cursor;
}

u64 segmented_accumulate(u64 at, ReductionRange values) on cpu {
    u64 cursor = at;
    bool wide = values.scalar.width == 64;
    bool floating = values.scalar.kind == ScalarKind::Float;
    for (component in 0..4) {
        if (component == values.count) { break; }
        u8 lane = u8(component);
        u8 register = lane;
        if (!floating) {
            cursor = register_instruction(cursor, u16(0x7e0f), u8(0x66), wide, lane, u8(0));
            register = u8(0);
        }
        MoveLocation operand = MoveLocation(u64(values.source_word + values.offsets[component]), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = collective_accumulate(cursor, values.operation, values.scalar, register, operand);
        if (!floating) {
            cursor = register_instruction(cursor, u16(0x6e0f), u8(0x66), wide, lane, u8(0));
        }
    }
    return cursor;
}

extern "C" u64 segmented_reduce_range(SegmentedRange input, u32 finish, address output) on cpu {
    u64 start = address_bits(output);
    ReductionRange values = input.values;
    bool wide = values.scalar.width == 64;
    u32 summary_flag = input.value_words * 4;
    u32 summary_count = summary_flag + 4;
    MoveLocation source_stride = MoveLocation(u64(values.source + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    u64 cursor = load_bits(start, u8(12), source_stride, true);
    MoveLocation source = MoveLocation(u64(values.source), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(10), source, true);
    if (finish != 0) {
        MoveLocation target_stride = MoveLocation(u64(input.target + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(13), target_stride, true);
        MoveLocation target = MoveLocation(u64(input.target), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(11), target, true);
    }
    // R10 walks source records, R9D holds the logical index, R8D the remaining count.
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(9), u8(8));
    if (finish != 0) {
        // The existing prefix row holds the incoming value, total length and earlier segment count.
        MoveLocation parameters = MoveLocation(u64(input.parameters), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(2), parameters, true);
        MoveLocation earlier = MoveLocation(u64(summary_count), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(1), earlier, false);
        MoveLocation extent = MoveLocation(u64(summary_flag), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x6e0f), u8(0x66), false, u8(4), extent);
        for (component in 0..4) {
            if (component == values.count) { break; }
            MoveLocation prefix = MoveLocation(u64(values.offsets[component]), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
            cursor = load_float(cursor, u8(component), prefix, wide);
        }
    } else {
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(1), u8(1));
        cursor = register_instruction(cursor, u16(0xef0f), u8(0x66), false, u8(5), u8(5));
        cursor = segmented_identity(cursor, values);
    }

    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(8), u8(8));
    u64 end = condition_field(cursor, u8(0x84));
    cursor = register_instruction(end + 4u64, u16(0x85), u8(0), false, u8(9), u8(9));
    u64 first = condition_field(cursor, u8(0x84));
    MoveLocation head = MoveLocation(u64(input.head_word), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(first + 4u64, u16(0x83), u8(0), false, u8(7), head);
    put_byte(cursor, u8(0));
    u64 no_head = condition_field(cursor + 1u64, u8(0x84));
    cursor = no_head + 4u64;
    patch_relative(unsafe_address(first), first, cursor);
    cursor = segmented_identity(cursor, values);
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(1));
    if (finish == 0) {
        cursor = register_instruction(cursor, u16(0x760f), u8(0x66), false, u8(5), u8(5));
    }
    patch_relative(unsafe_address(no_head), no_head, cursor);
    cursor = segmented_accumulate(cursor, values);
    if (finish != 0) {
        // A segment ends at the logical last record or before the next record's head.
        MoveLocation next_index = MoveLocation(1u64, MOVE_MEMORY, u8(9), NO_INDEX, u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), false, u8(0), next_index);
        cursor = register_instruction(cursor, u16(0x7e0f), u8(0x66), false, u8(4), u8(2));
        cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(2), u8(0));
        u64 last = condition_field(cursor, u8(0x84));
        MoveLocation next_head = MoveLocation(u64(input.head_word), MOVE_MEMORY, u8(10), u8(12), u8(0));
        cursor = encode_operand_instruction(last + 4u64, u16(0x83), u8(0), false, u8(7), next_head);
        put_byte(cursor, u8(0));
        u64 not_tail = condition_field(cursor + 1u64, u8(0x84));
        cursor = not_tail + 4u64;
        patch_relative(unsafe_address(last), last, cursor);
        cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(1), u8(0));
        cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(0));
        cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
        MoveLocation destination = MoveLocation(0u64, MOVE_MEMORY, u8(11), u8(0), u8(0));
        cursor = encode_operand_instruction(cursor, u16(0x8d), u8(0), true, u8(2), destination);
        cursor = segmented_store(cursor, values, u8(2), input.target_word);
        patch_relative(unsafe_address(not_tail), not_tail, cursor);
    }
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(0), u8(9));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(8));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back, begin);
    patch_relative(unsafe_address(end), end, cursor);
    if (finish == 0) {
        // The fifth RangeEntry argument follows the return address, shadow space and saved registers.
        MoveLocation result = MoveLocation(0x38u64, MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(10), result, true);
        cursor = segmented_store(cursor, values, u8(10), 0);
        MoveLocation reset = MoveLocation(u64(summary_flag), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = store_float(cursor, reset, u8(5), false);
        MoveLocation segments = MoveLocation(u64(summary_count), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = store_bits(cursor, segments, u8(1), 4);
    }
    write_word(cursor, 0x5d41c031);
    put_byte(cursor + 4u64, u8(0x41));
    put_byte(cursor + 5u64, u8(0x5c));
    put_byte(cursor + 6u64, u8(0xc3));
    return cursor + 7u64 - start;
}

u64 scan_float_correction(u64 at, ReductionOperator operation, ScalarEncoding scalar, u8 lane, MoveLocation destination) on cpu {
    u64 cursor = register_instruction(at, u16(0x280f), u8(0), false, u8(4), lane);
    if (operation == ReductionOperator::Sum) {
        cursor = collective_accumulate(cursor, operation, scalar, u8(4), destination);
    } else {
        // The first local element has no previous value in this range.
        cursor = register_instruction(cursor, u16(0x39), u8(0), false, u8(9), u8(8));
        u64 first = condition_field(cursor, u8(0x84));
        cursor = collective_accumulate(first + 4u64, operation, scalar, u8(4), destination);
        patch_relative(unsafe_address(first), first, cursor);
    }
    return store_float(cursor, destination, u8(4), scalar.width == 64);
}

extern "C" ReductionCode scan_range(ScanRange input, u32 add, address output) on cpu {
    u64 start = address_bits(output);
    ReductionRange values = input.values;
    bool correction = add != 0;
    u32 size = values.scalar.width / 8;
    bool wide = size == 8;
    bool floating = values.scalar.kind == ScalarKind::Float;
    bool extrema = values.operation == ReductionOperator::Min || values.operation == ReductionOperator::Max;
    u32 binding = correction ? input.target : values.source;
    MoveLocation stride = MoveLocation(u64(binding + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    u64 cursor = load_bits(start, u8(12), stride, true);
    MoveLocation current = MoveLocation(u64(binding), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(10), current, true);
    if (correction) {
        MoveLocation parameters = MoveLocation(u64(input.parameters), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(2), parameters, true);
    } else {
        MoveLocation output_stride = MoveLocation(u64(input.target + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(13), output_stride, true);
        MoveLocation target = MoveLocation(u64(input.target), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(11), target, true);
    }
    // R8D holds the remaining count. R10 walks the current binding.
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    if (!correction) {
        cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
        cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
        cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(11));
    }
    cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(9), u8(8));
    array<u8,4> registers = array<u8,4>(u8(0), u8(1), u8(2), u8(9));
    if (correction) { registers = array<u8,4>(u8(0), u8(1), u8(9), u8(11)); }
    array<u64,4> literals = array<u64,4>(0u64, 0u64, 0u64, 0u64);
    u8 prefix = size == 2 ? u8(0x66) : u8(0);
    u16 byte_form = size == 1 ? u16(1) : u16(0);
    for (component in 0..4) {
        if (component == values.count) { break; }
        u8 lane = u8(component);
        if (correction) {
            MoveLocation incoming = MoveLocation(u64(values.offsets[component]), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
            if (floating) {
                cursor = load_float(cursor, lane, incoming, wide);
            } else {
                cursor = encode_operand_instruction(cursor, u16(0x8b) - byte_form, prefix, wide, registers[component], incoming);
            }
        } else if (!floating) {
            cursor = move_immediate(cursor, registers[component], values.identity, wide);
        } else if (values.operation == ReductionOperator::Sum) {
            cursor = register_instruction(cursor, u16(0x570f), u8(0), false, lane, lane);
        } else {
            MoveLocation dst = MoveLocation(0u64, MOVE_XMM, lane, NO_INDEX, u8(0));
            MoveLocation src = MoveLocation(values.identity, MOVE_IMMEDIATE, u8(0), NO_INDEX, u8(0));
            MoveEncoding identity = copy_scalar_move(dst, src, size, false, unsafe_address(cursor));
            if (identity.literal_field != 0u64) { literals[component] = cursor - start + identity.literal_field; }
            cursor += identity.length;
        }
    }
    if (correction && floating && extrema) {
        cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(8), u8(9));
    }
    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(8), u8(8));
    u64 end = condition_field(cursor, u8(0x84));
    cursor = end + 4u64;
    for (component in 0..4) {
        if (component == values.count) { break; }
        u8 lane = u8(component);
        u8 register = registers[component];
        u32 offset = values.offsets[component];
        u8 target_base = correction ? u8(10) : u8(11);
        MoveLocation destination = MoveLocation(u64(input.target_word + offset), MOVE_MEMORY, target_base, NO_INDEX, u8(0));
        if (!correction) {
            MoveLocation source = MoveLocation(u64(values.source_word + offset), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
            if (floating) {
                cursor = store_float(cursor, destination, lane, wide);
                register = lane;
            } else {
                cursor = store_bits(cursor, destination, register, size);
            }
            cursor = collective_accumulate(cursor, values.operation, values.scalar, register, source);
        } else if (floating) {
            cursor = scan_float_correction(cursor, values.operation, values.scalar, lane, destination);
        } else if (extrema) {
            cursor = encode_operand_instruction(cursor, u16(0x8b) - byte_form, prefix, wide, u8(2), destination);
            cursor = collective_accumulate(cursor, values.operation, values.scalar, u8(2), move_gpr(register));
            cursor = store_bits(cursor, destination, u8(2), size);
        } else {
            // Integer sum and bitwise operators update the target field directly.
            u16 opcode = u16(reduction_opcode(values.operation, false)) - u16(2) - byte_form;
            cursor = encode_operand_instruction(cursor, opcode, prefix, wide, register, destination);
        }
    }
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    if (!correction) { cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(11), u8(13)); }
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(8));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back, begin);
    patch_relative(unsafe_address(end), end, cursor);
    if (!correction) {
        MoveLocation result = MoveLocation(0x38u64, MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(10), result, true);
        for (component in 0..4) {
            if (component == values.count) { break; }
            MoveLocation destination = MoveLocation(u64(values.offsets[component]), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
            if (floating) {
                cursor = store_float(cursor, destination, u8(component), wide);
            } else {
                cursor = store_bits(cursor, destination, registers[component], size);
            }
        }
    }
    cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(0), u8(0));
    if (!correction) {
        put_byte(cursor, u8(0x41));
        put_byte(cursor + 1u64, u8(0x5d));
        cursor += 2u64;
    }
    put_byte(cursor, u8(0x41));
    put_byte(cursor + 1u64, u8(0x5c));
    put_byte(cursor + 2u64, u8(0xc3));
    return ReductionCode(cursor + 3u64 - start, literals);
}

extern "C" u64 segmented_scan_range(SegmentedScanRange input, u32 add, address output) on cpu {
    u64 start = address_bits(output);
    ScanRange scan = input.scan;
    ReductionRange values = scan.values;
    bool correction = add != 0;
    u32 size = values.scalar.width / 8;
    bool wide = size == 8;
    bool floating = values.scalar.kind == ScalarKind::Float;
    MoveLocation source_stride = MoveLocation(u64(values.source + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    u64 cursor = load_bits(start, u8(12), source_stride, true);
    MoveLocation target_stride = MoveLocation(u64(scan.target + values.stride_offset), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(13), target_stride, true);
    MoveLocation source = MoveLocation(u64(values.source), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(10), source, true);
    MoveLocation target = MoveLocation(u64(scan.target), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
    cursor = load_bits(cursor, u8(11), target, true);
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(12));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(10));
    cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(9), u8(0));
    cursor = register_instruction(cursor, u16(0xaf0f), u8(0), true, u8(0), u8(13));
    cursor = register_instruction(cursor, u16(0x01), u8(0), true, u8(0), u8(11));
    cursor = register_instruction(cursor, u16(0x29), u8(0), false, u8(9), u8(8));
    if (correction) {
        MoveLocation parameters = MoveLocation(u64(scan.parameters), MOVE_MEMORY, u8(1), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(2), parameters, true);
        for (component in 0..4) {
            if (component == values.count) { break; }
            MoveLocation incoming = MoveLocation(u64(values.offsets[component]), MOVE_MEMORY, u8(2), NO_INDEX, u8(0));
            cursor = load_float(cursor, u8(component), incoming, wide);
        }
        if (floating && values.operation != ReductionOperator::Sum) {
            cursor = register_instruction(cursor, u16(0x89), u8(0), false, u8(8), u8(9));
        }
    } else {
        cursor = segmented_identity(cursor, values);
        // R9D records whether the local range reset at a segment head.
        cursor = register_instruction(cursor, u16(0x31), u8(0), false, u8(9), u8(9));
    }

    u64 begin = cursor;
    cursor = register_instruction(cursor, u16(0x85), u8(0), false, u8(8), u8(8));
    u64 end = condition_field(cursor, u8(0x84));
    MoveLocation head = MoveLocation(u64(input.head_word), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
    cursor = encode_operand_instruction(end + 4u64, u16(0x83), u8(0), false, u8(7), head);
    put_byte(cursor, u8(0));
    cursor += 1u64;
    u64 head_end = 0u64;
    if (correction) {
        // At a head, the local scan already owns the rest of the range.
        head_end = condition_field(cursor, u8(0x85));
        cursor = head_end + 4u64;
        for (component in 0..4) {
            if (component == values.count) { break; }
            u8 lane = u8(component);
            MoveLocation destination = MoveLocation(u64(scan.target_word + values.offsets[component]), MOVE_MEMORY, u8(11), NO_INDEX, u8(0));
            if (floating) {
                cursor = scan_float_correction(cursor, values.operation, values.scalar, lane, destination);
            } else {
                cursor = register_instruction(cursor, u16(0x7e0f), u8(0x66), wide, lane, u8(0));
                cursor = collective_accumulate(cursor, values.operation, values.scalar, u8(0), destination);
                cursor = store_bits(cursor, destination, u8(0), size);
            }
        }
    } else {
        u64 no_head = condition_field(cursor, u8(0x84));
        cursor = segmented_identity(no_head + 4u64, values);
        cursor = move_immediate(cursor, u8(9), 1u64, false);
        patch_relative(unsafe_address(no_head), no_head, cursor);
        cursor = segmented_store(cursor, values, u8(11), scan.target_word);
        cursor = segmented_accumulate(cursor, values);
    }
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(10), u8(12));
    cursor = register_instruction(cursor, u16(0x03), u8(0), true, u8(11), u8(13));
    cursor = register_instruction(cursor, u16(0xff), u8(0), false, u8(1), u8(8));
    u64 back = jump_field(cursor);
    cursor = back + 4u64;
    patch_relative(unsafe_address(back), back, begin);
    patch_relative(unsafe_address(end), end, cursor);
    if (correction) {
        patch_relative(unsafe_address(head_end), head_end, cursor);
    } else {
        MoveLocation result = MoveLocation(0x38u64, MOVE_MEMORY, u8(4), NO_INDEX, u8(0));
        cursor = load_bits(cursor, u8(10), result, true);
        cursor = segmented_store(cursor, values, u8(10), 0);
        MoveLocation reset = MoveLocation(u64(input.value_words * 4), MOVE_MEMORY, u8(10), NO_INDEX, u8(0));
        cursor = store_bits(cursor, reset, u8(9), 4);
    }
    write_word(cursor, 0x5d41c031);
    put_byte(cursor + 4u64, u8(0x41));
    put_byte(cursor + 5u64, u8(0x5c));
    put_byte(cursor + 6u64, u8(0xc3));
    return cursor + 7u64 - start;
}
