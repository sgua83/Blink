module blink.unicode;

result<DecodedScalar,DecodeError> decode_utf8(read dataset<u8> text,u32 offset) on cpu {
    if (offset >= text.length) {
        return Err(DecodeError(offset,DecodeReason::EndOfInput));
    }
    u32 first=u32(text[offset]);
    u32 second_index=offset+1;
    if (first < 0x80) { return Ok(DecodedScalar(first,second_index)); }
    if (first < 0xc2 || first > 0xf4) {
        return Err(DecodeError(offset,DecodeReason::InvalidLead));
    }
    if (second_index >= text.length) {
        return Err(DecodeError(second_index,DecodeReason::Truncated));
    }
    u32 second=u32(text[second_index]);
    if (second < 0x80 || second > 0xbf) {
        return Err(DecodeError(second_index,DecodeReason::InvalidContinuation));
    }
    u32 third_index=second_index+1;
    if (first <= 0xdf) {
        return Ok(DecodedScalar(((first&0x1f)<<6)|(second&0x3f),third_index));
    }
    if ((first == 0xe0 && second < 0xa0)
        || (first == 0xed && second > 0x9f)
        || (first == 0xf0 && second < 0x90)
        || (first == 0xf4 && second > 0x8f)) {
        return Err(DecodeError(second_index,DecodeReason::InvalidSequence));
    }
    if (third_index >= text.length) {
        return Err(DecodeError(third_index,DecodeReason::Truncated));
    }
    u32 third=u32(text[third_index]);
    if (third < 0x80 || third > 0xbf) {
        return Err(DecodeError(third_index,DecodeReason::InvalidContinuation));
    }
    u32 fourth_index=third_index+1;
    if (first <= 0xef) {
        return Ok(DecodedScalar(((first&0x0f)<<12)|((second&0x3f)<<6)|(third&0x3f),fourth_index));
    }
    if (fourth_index >= text.length) {
        return Err(DecodeError(fourth_index,DecodeReason::Truncated));
    }
    u32 fourth=u32(text[fourth_index]);
    if (fourth < 0x80 || fourth > 0xbf) {
        return Err(DecodeError(fourth_index,DecodeReason::InvalidContinuation));
    }
    return Ok(DecodedScalar(((first&7)<<18)|((second&0x3f)<<12)|((third&0x3f)<<6)|(fourth&0x3f),fourth_index+1));
}

result<DecodedScalar,DecodeError> decode_utf16(read dataset<u16> text,u32 offset) on cpu {
    if (offset >= text.length) {
        return Err(DecodeError(offset,DecodeReason::EndOfInput));
    }
    u32 first=u32(text[offset]);
    u32 second_index=offset+1;
    if (first < 0xd800 || first > 0xdfff) {
        return Ok(DecodedScalar(first,second_index));
    }
    if (first >= 0xdc00) {
        return Err(DecodeError(offset,DecodeReason::InvalidSurrogate));
    }
    if (second_index >= text.length) {
        return Err(DecodeError(second_index,DecodeReason::Truncated));
    }
    u32 second=u32(text[second_index]);
    if (second < 0xdc00 || second > 0xdfff) {
        return Err(DecodeError(second_index,DecodeReason::InvalidSurrogate));
    }
    return Ok(DecodedScalar(0x10000+((first-0xd800)<<10)+(second-0xdc00),second_index+1));
}

result<Utf8Encoding,u32> encode_utf8(u32 value) on cpu {
    if (value > 0x10ffff || (value >= 0xd800 && value <= 0xdfff)) {
        return Err(value);
    }
    if (value < 0x80) { return Ok(Utf8Encoding::One(u8(value))); }
    if (value < 0x800) {
        return Ok(Utf8Encoding::Two(array<u8,2>(u8(0xc0|(value>>6)),u8(0x80|(value&0x3f)))));
    }
    if (value < 0x10000) {
        return Ok(Utf8Encoding::Three(array<u8,3>(u8(0xe0|(value>>12)),u8(0x80|((value>>6)&0x3f)),u8(0x80|(value&0x3f)))));
    }
    return Ok(Utf8Encoding::Four(array<u8,4>(u8(0xf0|(value>>18)),u8(0x80|((value>>12)&0x3f)),u8(0x80|((value>>6)&0x3f)),u8(0x80|(value&0x3f)))));
}

result<Utf16Encoding,u32> encode_utf16(u32 value) on cpu {
    if (value > 0x10ffff || (value >= 0xd800 && value <= 0xdfff)) {
        return Err(value);
    }
    if (value < 0x10000) { return Ok(Utf16Encoding::One(u16(value))); }
    u32 rest=value-0x10000;
    return Ok(Utf16Encoding::Two(array<u16,2>(u16(0xd800+(rest>>10)),u16(0xdc00+(rest&0x3ff)))));
}
