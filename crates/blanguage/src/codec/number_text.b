module codec.number_text;

u8 number_byte(address bytes, u64 index) on cpu {
    return unsafe_position<u8>(unsafe_address(address_bits(bytes) + index));
}

ParsedNumber unsigned_number<const Radix:u32 in {10,16}>(address bytes, u64 begin, u64 end, u64 maximum) on cpu {
    if (begin == end) { return ParsedNumber(0u64, NumberError::Empty); }
    u64 index = begin;
    if (number_byte(bytes, index) == 43) { index += 1u64; }
    if (index == end) { return ParsedNumber(0u64, NumberError::Invalid); }
    u64 value = 0u64;
    while (index < end) {
        u8 byte = number_byte(bytes, index);
        u64 digit;
        if (byte >= 48 && byte <= 57) {
            digit = u64(byte) - 48u64;
        } else {
            u8 lower = byte | u8(32);
            if (Radix != 16 || lower < 97 || lower > 102) { return ParsedNumber(0u64, NumberError::Invalid); }
            digit = u64(lower) - 87u64;
        }
        if (value > (maximum - digit) / u64(Radix)) { return ParsedNumber(0u64, NumberError::Overflow); }
        value = value * u64(Radix) + digit;
        index += 1u64;
    }
    return ParsedNumber(value, NumberError::None);
}

extern "C" ParsedNumber decimal_number(address bytes, u64 length, u64 maximum) on cpu {
    return unsigned_number<10>(bytes, 0u64, length, maximum);
}

extern "C" ParsedNumber hexadecimal_number(address bytes, u64 length, u64 maximum) on cpu {
    return unsigned_number<16>(bytes, 0u64, length, maximum);
}
