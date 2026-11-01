module bootstrap.lexer;

u32 byte_at(address source, u64 at) on cpu {
    return u32(unsafe_position<u8>(unsafe_address(address_bits(source) + at)));
}
bool alphabetic(u32 value) on cpu {
    return value >= 65 && value <= 90 || value >= 97 && value <= 122;
}
bool digit(u32 value) on cpu { return value >= 48 && value <= 57; }
bool word(u32 value) on cpu { return alphabetic(value) || digit(value) || value == 95; }
bool whitespace(u32 value) on cpu {
    return value == 9 || value == 10 || value == 12 || value == 13 || value == 32;
}
bool removed_word(address source, u64 start, u64 end) on cpu {
    return end - start == 7u64
        && byte_at(source,start) == 112 && byte_at(source,start+1u64) == 114
        && byte_at(source,start+2u64) == 111 && byte_at(source,start+3u64) == 103
        && byte_at(source,start+4u64) == 114 && byte_at(source,start+5u64) == 97
        && byte_at(source,start+6u64) == 109;
}
bool double_operator(u32 a, u32 b) on cpu {
    return a == 58 && b == 58 || a == 46 && b == 46
        || b == 61 && (a == 43 || a == 45 || a == 42 || a == 47 || a == 37
            || a == 38 || a == 124 || a == 94 || a == 61 || a == 33 || a == 60 || a == 62)
        || a == b && (a == 38 || a == 124 || a == 60 || a == 62);
}
bool single_operator(u32 value) on cpu {
    return value == 123 || value == 125 || value == 40 || value == 41
        || value == 91 || value == 93 || value == 59 || value == 44 || value == 46
        || value == 63 || value == 58 || value == 43 || value == 45 || value == 42
        || value == 47 || value == 37 || value == 33 || value == 61 || value == 60
        || value == 62 || value == 38 || value == 124 || value == 94 || value == 126;
}
void token(address output, u64 index, u64 start, u64 end) on cpu {
    unsafe_position<TokenSpan>(unsafe_address(address_bits(output) + index * 16u64)) = TokenSpan(start,end);
}
extern "C" ScanResult scan(address source, u64 length, address tokens) on cpu {
    u64 at = 0u64;
    u64 count = 0u64;
    while (at < length) {
        u64 start = at;
        u32 c = byte_at(source,at);
        if (whitespace(c)) { at += 1u64; continue; }
        if (c == 47 && at + 1u64 < length && byte_at(source,at+1u64) == 47) {
            while (at < length && byte_at(source,at) != 10) { at += 1u64; }
            continue;
        }
        if (c == 47 && at + 1u64 < length && byte_at(source,at+1u64) == 42) {
            at += 2u64;
            while (at + 1u64 < length && !(byte_at(source,at) == 42 && byte_at(source,at+1u64) == 47)) {
                at += 1u64;
            }
            if (at + 1u64 >= length) { return ScanResult(0u64,start,at,1); }
            at += 2u64;
            continue;
        }
        if (c == 34) {
            at += 1u64;
            while (at < length && byte_at(source,at) != 34) {
                if (byte_at(source,at) == 92) { at += 1u64; }
                at += 1u64;
            }
            if (at >= length) { return ScanResult(0u64,start,length,3); }
            at += 1u64;
        } else if (alphabetic(c) || c == 95) {
            at += 1u64;
            while (at < length && word(byte_at(source,at))) { at += 1u64; }
            if (removed_word(source,start,at)) { return ScanResult(0u64,start,at,4); }
        } else if (digit(c) || c == 46 && at + 1u64 < length && digit(byte_at(source,at+1u64))) {
            bool hexadecimal = c == 48 && at + 1u64 < length && byte_at(source,at+1u64) == 120;
            at += 1u64;
            while (at < length) {
                u32 x = byte_at(source,at);
                if (alphabetic(x) || digit(x)
                    || !hexadecimal && x == 46 && !(at + 1u64 < length && byte_at(source,at+1u64) == 46)
                    || !hexadecimal && (x == 43 || x == 45)
                        && (byte_at(source,at-1u64) == 101 || byte_at(source,at-1u64) == 69)) {
                    at += 1u64;
                } else { break; }
            }
        } else if (at + 2u64 < length && byte_at(source,at+2u64) == 61
            && (c == 60 || c == 62) && byte_at(source,at+1u64) == c) {
            at += 3u64;
        } else if (at + 1u64 < length && double_operator(c,byte_at(source,at+1u64))) {
            at += 2u64;
        } else if (single_operator(c)) { at += 1u64; }
        else { return ScanResult(0u64,start,start+1u64,2); }
        token(tokens,count,start,at);
        count += 1u64;
    }
    token(tokens,count,at,at);
    return ScanResult(count+1u64,0u64,0u64,0);
}
