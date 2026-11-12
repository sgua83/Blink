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

extern "C" u64 unsigned_to_float(u8 target, u8 prefix, address output) on cpu {
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

extern "C" u64 float_to_unsigned(u8 target, u8 prefix, address output) on cpu {
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

extern "C" DivisionPlan division_plan(u32 divisor) on cpu requires nonzero(divisor) {
    u32 high = divisor;
    u32 shift = 0;
    if (high >= 0x10000) { high >>= 16; shift += 16; }
    if (high >= 0x100) { high >>= 8; shift += 8; }
    if (high >= 0x10) { high >>= 4; shift += 4; }
    if (high >= 4) { high >>= 2; shift += 2; }
    if (high >= 2) { shift += 1; }
    if ((divisor & (divisor - 1)) == 0) {
        return DivisionPlan(0, u8(shift), DIVISION_SHIFT);
    }

    u64 d = u64(divisor);
    u64 power = 1u64 << u64(shift);
    u64 numerator = 0x100000000u64 * power;
    u64 quotient = numerator / d;
    u64 remainder = numerator % d;
    // A non-power-of-two divisor cannot divide the power-of-two numerator.
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

extern "C" MoveEncoding cast_boolean(MoveLocation dst, MoveLocation src,
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
