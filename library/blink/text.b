module blink.text;
import blink.text_core;

dataset<u8> decimal_magnitude(u64 value, bool negative) on cpu {
    u32 digits = 1;
    u64 remaining = value;
    while (remaining >= 10u64) {
        remaining /= 10u64;
        digits += 1;
    }

    u32 length = digits;
    if (negative) {
        length += 1;
    }
    dataset<u8> text = dataset<u8>(length);
    remaining = value;
    u32 cursor = text.length;
    while (cursor > 0) {
        if (negative && cursor == 1) {
            text[cursor - 1] = u8(45);
        } else {
            text[cursor - 1] = u8(48u64 + remaining % 10u64);
            remaining /= 10u64;
        }
        cursor -= 1;
    }
    return text;
}

dataset<u8> decimal_u64(u64 value) on cpu {
    return decimal_magnitude(value, false);
}

dataset<u8> decimal_i64(i64 value) on cpu {
    if (value < i64(0)) {
        return decimal_magnitude(u64(-(value + i64(1))) + 1u64, true);
    }
    return decimal_magnitude(u64(value), false);
}

result<dataset<u8>,u32> hexadecimal_bytes(read dataset<u8> data) on cpu {
    if (data.length>2147483647) { return Err(data.length); }
    u32 length=data.length*2;
    dataset<u8> text=dataset<u8>(length);
    if (data.length!=0) {
        write_hexadecimal(address_of(data[0]),u64(data.length),address_of(text[0]));
    }
    return Ok(text);
}
