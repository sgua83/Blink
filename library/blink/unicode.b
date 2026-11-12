module blink.unicode;
import blink.unicode_scalar;

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

result<u32,DecodeError> scalar_length_utf8(read dataset<u8> text) on cpu {
    u32 offset=0;
    u32 count=0;
    while (offset<text.length) {
        DecodedScalar decoded=try(decode_utf8(text,offset));
        offset=decoded.next;
        count+=1;
    }
    return Ok(count);
}

result<u32,DecodeError> scalar_length_utf16(read dataset<u16> text) on cpu {
    u32 offset=0;
    u32 count=0;
    while (offset<text.length) {
        DecodedScalar decoded=try(decode_utf16(text,offset));
        offset=decoded.next;
        count+=1;
    }
    return Ok(count);
}

result<view<u8>,SliceError> borrow_utf8(read dataset<u8> text,u32 start,u32 end) on cpu {
    view<u8> selected=view<u8>(text,text.length,text.length);
    u32 offset=0;
    u32 count=0;
    for(unit in 0..text.length){
        if(unit<offset){continue;}
        if(count==start){selected=view<u8>(text,unit,text.length);break;}
        match(decode_utf8(text,unit)){
            Ok(decoded){offset=decoded.next;}
            Err(error){return Err(SliceError::Decode(error));}
        }
        count+=1;
    }
    u32 begin=offset;
    for(unit in begin..text.length){
        if(unit<offset){continue;}
        if(count==end){selected=view<u8>(text,begin,unit);}
        match(decode_utf8(text,unit)){
            Ok(decoded){offset=decoded.next;}
            Err(error){return Err(SliceError::Decode(error));}
        }
        count+=1;
    }
    if(start>end||end>count){return Err(SliceError::Range(ScalarRangeError(start,end,count)));}
    return Ok(selected);
}

result<dataset<u8>,SliceError> slice_utf8(read dataset<u8> text,u32 start,u32 end) on cpu {
    dataset<u8> output=dataset<u8>(0);
    u32 offset=0;
    u32 count=0;
    while (offset<text.length) {
        match (decode_utf8(text,offset)) {
            Ok(decoded) {
                if (count>=start && count<end) {
                    for (unit in offset..text.length) {
                        if (unit>=decoded.next) { break; }
                        u32 size=output.length;
                        output=resize(output,size+1,text[unit]);
                    }
                }
                offset=decoded.next;
            }
            Err(error) { return Err(SliceError::Decode(error)); }
        }
        count+=1;
    }
    if (start>end || end>count) {
        return Err(SliceError::Range(ScalarRangeError(start,end,count)));
    }
    return Ok(output);
}

result<view<u16>,SliceError> borrow_utf16(read dataset<u16> text,u32 start,u32 end) on cpu {
    view<u16> selected=view<u16>(text,text.length,text.length);
    u32 offset=0;
    u32 count=0;
    for(unit in 0..text.length){
        if(unit<offset){continue;}
        if(count==start){selected=view<u16>(text,unit,text.length);break;}
        match(decode_utf16(text,unit)){
            Ok(decoded){offset=decoded.next;}
            Err(error){return Err(SliceError::Decode(error));}
        }
        count+=1;
    }
    u32 begin=offset;
    for(unit in begin..text.length){
        if(unit<offset){continue;}
        if(count==end){selected=view<u16>(text,begin,unit);}
        match(decode_utf16(text,unit)){
            Ok(decoded){offset=decoded.next;}
            Err(error){return Err(SliceError::Decode(error));}
        }
        count+=1;
    }
    if(start>end||end>count){return Err(SliceError::Range(ScalarRangeError(start,end,count)));}
    return Ok(selected);
}

result<dataset<u16>,SliceError> slice_utf16(read dataset<u16> text,u32 start,u32 end) on cpu {
    dataset<u16> output=dataset<u16>(0);
    u32 offset=0;
    u32 count=0;
    while (offset<text.length) {
        match (decode_utf16(text,offset)) {
            Ok(decoded) {
                if (count>=start && count<end) {
                    for (unit in offset..text.length) {
                        if (unit>=decoded.next) { break; }
                        u32 size=output.length;
                        output=resize(output,size+1,text[unit]);
                    }
                }
                offset=decoded.next;
            }
            Err(error) { return Err(SliceError::Decode(error)); }
        }
        count+=1;
    }
    if (start>end || end>count) {
        return Err(SliceError::Range(ScalarRangeError(start,end,count)));
    }
    return Ok(output);
}

Utf8Encoding utf8_encoding(u32 value) on cpu {
    Utf8Units units=utf8_units(value);
    if(units.length==1){return Utf8Encoding::One(u8(units.bits));}
    if(units.length==2){return Utf8Encoding::Two(array<u8,2>(u8(units.bits),u8(units.bits>>8)));}
    if(units.length==3){return Utf8Encoding::Three(array<u8,3>(u8(units.bits),u8(units.bits>>8),u8(units.bits>>16)));}
    return Utf8Encoding::Four(array<u8,4>(u8(units.bits),u8(units.bits>>8),u8(units.bits>>16),u8(units.bits>>24)));
}

Utf16Encoding utf16_encoding(u32 value) on cpu {
    if (value < 0x10000) { return Utf16Encoding::One(u16(value)); }
    u32 rest=value-0x10000;
    return Utf16Encoding::Two(array<u16,2>(u16(0xd800+(rest>>10)),u16(0xdc00+(rest&0x3ff))));
}

result<Utf8Encoding,u32> encode_utf8(u32 value) on cpu {
    if (!is_scalar(value)) { return Err(value); }
    return Ok(utf8_encoding(value));
}

result<Utf16Encoding,u32> encode_utf16(u32 value) on cpu {
    if (value > 0x10ffff || (value >= 0xd800 && value <= 0xdfff)) { return Err(value); }
    return Ok(utf16_encoding(value));
}

result<dataset<T>,TextError> append_units<T,const N:u32>(dataset<T> output,array<T,N> units) on cpu {
    for (index in 0..N) {
        match (blink.sequence::push(output,units[index])) {
            Ok(next) { output=next; }
            Err(failure) { return Err(TextError::Sequence(failure.reason)); }
        }
    }
    return Ok(output);
}

result<dataset<u16>,TextError> to_utf16(read dataset<u8> text) on cpu {
    dataset<u16> output=dataset<u16>(0);
    u32 offset=0;
    while (offset<text.length) {
        match (decode_utf8(text,offset)) {
            Ok(decoded) {
                match (utf16_encoding(decoded.value)) {
                    Utf16Encoding::One(units) { output=try(append_units(output,array<u16,1>(units))); }
                    Utf16Encoding::Two(units) { output=try(append_units(output,units)); }
                }
                offset=decoded.next;
            }
            Err(error) { return Err(TextError::Decode(error)); }
        }
    }
    return Ok(output);
}

result<dataset<u8>,TextError> to_utf8(read dataset<u16> text) on cpu {
    dataset<u8> output=dataset<u8>(0);
    u32 offset=0;
    while (offset<text.length) {
        match (decode_utf16(text,offset)) {
            Ok(decoded) {
                match (utf8_encoding(decoded.value)) {
                    Utf8Encoding::One(units) { output=try(append_units(output,array<u8,1>(units))); }
                    Utf8Encoding::Two(units) { output=try(append_units(output,units)); }
                    Utf8Encoding::Three(units) { output=try(append_units(output,units)); }
                    Utf8Encoding::Four(units) { output=try(append_units(output,units)); }
                }
                offset=decoded.next;
            }
            Err(error) { return Err(TextError::Decode(error)); }
        }
    }
    return Ok(output);
}

