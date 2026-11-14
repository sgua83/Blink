module bootstrap.numeric_literal;

// Decimal shifts implement Simple Decimal Conversion as described by
// Nigel Tao: https://nigeltao.github.io/blog/2020/parse-number-f64-simple.html
// Binary64 needs at most 767 significant decimal digits to decide rounding.
// One further digit and the lost-nonzero bit preserve exact halfway decisions.
const u32 PRECISION_DIGITS=768;
const u32 SHIFT_SPARE=20;

// The byte array is owned by decimal_float's stack. This private descriptor
// never leaves that call. Its first count bytes are significant digits;
// SHIFT_SPARE bytes support a complete u64 multiply before compaction.
struct Decimal { address digits; u32 count; i32 point; bool lost; };

address offset(address bytes,u64 index) on cpu {
    return unsafe_address(address_bits(bytes)+index);
}
u8 byte(address bytes,u64 index) on cpu {
    return unsafe_position<u8>(offset(bytes,index));
}
void put(address bytes,u64 index,u8 value) on cpu {
    unsafe_position<u8>(offset(bytes,index))=value;
}
bool digit(u8 value) on cpu { return value>=48 && value<=57; }
void trim(inout Decimal value) on cpu {
    while (value.count!=0 && byte(value.digits,u64(value.count-1))==0) { value.count-=1; }
}

void multiply_power_two(inout Decimal value,u32 shift) on cpu {
    u32 source_index=value.count;
    u32 end=value.count+SHIFT_SPARE;
    u32 destination=end;
    u64 carry=0u64;
    while (source_index!=0) {
        source_index-=1;
        carry+=u64(byte(value.digits,u64(source_index)))<<u64(shift);
        u64 quotient=carry/10u64;
        destination-=1;
        put(value.digits,u64(destination),u8(carry-10u64*quotient));
        carry=quotient;
    }
    while (carry!=0u64) {
        u64 quotient=carry/10u64;
        destination-=1;
        put(value.digits,u64(destination),u8(carry-10u64*quotient));
        carry=quotient;
    }
    u32 count=end-destination;
    value.point+=i32(count)-i32(value.count);
    u32 retained=min(count,PRECISION_DIGITS);
    for (index in 0..retained) {
        put(value.digits,u64(index),byte(value.digits,u64(destination+index)));
    }
    for (index in retained..count) {
        if (byte(value.digits,u64(destination+index))!=0) { value.lost=true; }
    }
    value.count=retained;
    trim(value);
}

void divide_power_two(inout Decimal value,u32 shift) on cpu {
    u32 original=value.count;
    u32 source_index=0;
    u32 written=0;
    u64 remainder=0u64;
    u64 mask=(1u64<<u64(shift))-1u64;
    while (source_index<original || remainder!=0u64) {
        u64 next=source_index<original ? u64(byte(value.digits,u64(source_index))) : 0u64;
        source_index+=1;
        u64 expanded=remainder*10u64+next;
        u8 part=u8(expanded>>u64(shift));
        remainder=expanded&mask;
        if (written==0 && part==0) {
            value.point-=1;
        } else {
            if (written<PRECISION_DIGITS) { put(value.digits,u64(written),part); }
            else if (part!=0) { value.lost=true; }
            written+=1;
        }
    }
    value.count=min(written,PRECISION_DIGITS);
    trim(value);
}

u64 rounded_integer(read Decimal value) on cpu {
    if (value.count==0 || value.point<0) { return 0u64; }
    u32 point=u32(value.point);
    u64 result=0u64;
    for (index in 0..point) {
        result*=10u64;
        if (index<value.count) { result+=u64(byte(value.digits,u64(index))); }
    }
    if (point<value.count) {
        u8 next=byte(value.digits,u64(point));
        bool upward=next>=5;
        if (next==5 && point+1==value.count) { upward=value.lost || (result&1u64)!=0u64; }
        if (upward) { result+=1u64; }
    }
    return result;
}

i32 difference(u64 positive,u64 negative) on cpu {
    if (positive>=negative) { return i32(min(positive-negative,2047u64)); }
    return -i32(min(negative-positive,2047u64));
}
i32 same_direction(u64 first,u64 second) on cpu {
    return i32(min(min(first,2047u64)+min(second,2047u64),2047u64));
}
i32 decimal_point(u64 before,u64 leading,u64 exponent,bool subtract_exponent) on cpu {
    if (before>=leading) {
        u64 amount=before-leading;
        return subtract_exponent ? difference(amount,exponent) : same_direction(amount,exponent);
    }
    u64 amount=leading-before;
    return subtract_exponent ? -same_direction(amount,exponent) : difference(exponent,amount);
}

bool scan_decimal(address bytes,u64 length,u64 start,inout Decimal value) on cpu {
    u64 index=start;
    u64 before=0u64;
    u64 leading=0u64;
    u64 digits=0u64;
    bool dot=false;
    bool significant=false;
    while (index<length) {
        u8 current=byte(bytes,index);
        if (digit(current)) {
            digits+=1u64;
            if (!dot) { before+=1u64; }
            u8 part=current-48;
            if (!significant && part==0) { leading+=1u64; }
            else {
                significant=true;
                if (value.count<PRECISION_DIGITS) {
                    put(value.digits,u64(value.count),part);
                    value.count+=1;
                } else if (part!=0) { value.lost=true; }
            }
        } else if (current==46 && !dot) { dot=true; }
        else { break; }
        index+=1u64;
    }
    if (digits==0u64) { return false; }
    u64 exponent=0u64;
    bool subtract_exponent=false;
    if (index<length && (byte(bytes,index)==101 || byte(bytes,index)==69)) {
        index+=1u64;
        if (index<length && (byte(bytes,index)==43 || byte(bytes,index)==45)) {
            subtract_exponent=byte(bytes,index)==45;
            index+=1u64;
        }
        u64 exponent_start=index;
        // Source is an actual borrowed Rust string. Beyond its byte length
        // plus the complete binary64 decimal range, exponent magnitude cannot
        // be cancelled by any leading zeros or fractional digits in this input.
        u64 limit=length+2048u64;
        while (index<length && digit(byte(bytes,index))) {
            u64 part=u64(byte(bytes,index)-48);
            if (exponent<limit) {
                if (exponent>limit/10u64 || (exponent==limit/10u64 && part>limit%10u64)) { exponent=limit; }
                else { exponent=exponent*10u64+part; }
            }
            index+=1u64;
        }
        if (index==exponent_start) { return false; }
    }
    if (index!=length) { return false; }
    value.point=decimal_point(before,leading,exponent,subtract_exponent);
    trim(value);
    return true;
}

bool ascii_word<const N:u32 in {3,8}>(address bytes,u64 length,read array<u8,N> word) on cpu {
    if (length!=u64(N)) { return false; }
    for (index in 0..N) {
        if ((byte(bytes,u64(index))|u8(32))!=word[index]) { return false; }
    }
    return true;
}

Literal decimal_float(address bytes,u64 length,LiteralKind kind) on cpu {
    u64 start=0u64;
    bool negative=false;
    if (length!=0u64 && (byte(bytes,0u64)==43 || byte(bytes,0u64)==45)) {
        negative=byte(bytes,0u64)==45;
        start=1u64;
    }
    if (ascii_word(offset(bytes,start),length-start,"nan")
        || ascii_word(offset(bytes,start),length-start,"inf")
        || ascii_word(offset(bytes,start),length-start,"infinity")) {
        return Literal(0u64,kind,LiteralError::NonFinite);
    }
    array<u8,788> storage;
    for (index in 0..788) { storage[index]=u8(0); }
    Decimal value=Decimal(address_of(storage),0,i32(0),false);
    if (!scan_decimal(bytes,length,start,value)) { return Literal(0u64,kind,LiteralError::Float); }
    u32 precision=kind==LiteralKind::F64 ? 53 : 24;
    i32 minimum=kind==LiteralKind::F64 ? -1022 : -126;
    i32 infinity=kind==LiteralKind::F64 ? i32(2047) : i32(255);
    u64 sign=negative ? 1u64<<u64(kind==LiteralKind::F64 ? 63 : 31) : 0u64;
    if (value.count==0 || value.point < -324) { return Literal(sign,kind,LiteralError::None); }
    if (value.point>=310) { return Literal(0u64,kind,LiteralError::NonFinite); }
    i32 exponent=i32(0);
    while (value.point>0) {
        u32 shift=u32(min(i32(60),value.point*i32(3)));
        divide_power_two(value,shift);
        exponent+=i32(shift);
    }
    while (value.point<=0) {
        u32 shift;
        if (value.point==0) {
            u8 first=byte(value.digits,0u64);
            if (first>=5) { break; }
            shift=first<2 ? 2 : 1;
        } else { shift=u32(min(i32(60),-value.point*i32(3))); }
        multiply_power_two(value,shift);
        exponent-=i32(shift);
    }
    exponent-=1;
    while (exponent<minimum) {
        u32 shift=u32(min(i32(60),minimum-exponent));
        divide_power_two(value,shift);
        exponent+=i32(shift);
    }
    if (exponent-minimum+1>=infinity) { return Literal(0u64,kind,LiteralError::NonFinite); }
    multiply_power_two(value,precision);
    u64 mantissa=rounded_integer(value);
    if (mantissa==(1u64<<u64(precision))) {
        // This rounding carry is already an exact power of two.
        mantissa=mantissa>>1u64;
        exponent+=1;
        if (exponent-minimum+1>=infinity) { return Literal(0u64,kind,LiteralError::NonFinite); }
    }
    i32 power=exponent-minimum+1;
    u64 hidden=1u64<<u64(precision-1);
    if (mantissa<hidden) { power-=1; }
    return Literal(sign | (u64(power)<<u64(precision-1)) | (mantissa&(hidden-1u64)),kind,LiteralError::None);
}

Literal unsigned_literal(address bytes,u64 length,LiteralKind kind) on cpu {
    u64 maximum=kind==LiteralKind::U64 ? 18446744073709551615u64 : 4294967295u64;
    ParsedNumber value;
    if (length>=2u64 && byte(bytes,0u64)==48 && byte(bytes,1u64)==120) {
        value=hexadecimal_number(offset(bytes,2u64),length-2u64,maximum);
    } else { value=decimal_number(bytes,length,maximum); }
    return Literal(value.value,kind,value.error==NumberError::None ? LiteralError::None : LiteralError::Integer);
}

Literal positive_literal(address bytes,u64 length) on cpu {
    bool hex=length>=2u64 && byte(bytes,0u64)==48 && byte(bytes,1u64)==120;
    if (length>=3u64) {
        if (!hex && byte(bytes,length-3u64)==102 && byte(bytes,length-2u64)==54 && byte(bytes,length-1u64)==52) {
            return decimal_float(bytes,length-3u64,LiteralKind::F64);
        }
        if (byte(bytes,length-3u64)==117 && byte(bytes,length-2u64)==54 && byte(bytes,length-1u64)==52) {
            return unsigned_literal(bytes,length-3u64,LiteralKind::U64);
        }
    }
    bool floating=length!=0u64 && byte(bytes,length-1u64)==102;
    u64 index=0u64;
    while (index<length && !floating) {
        u8 value=byte(bytes,index);
        floating=value==46 || value==101 || value==69;
        index+=1u64;
    }
    u64 end=length;
    if (!hex && floating) {
        while (end!=0u64 && byte(bytes,end-1u64)==102) { end-=1u64; }
        return decimal_float(bytes,end,LiteralKind::F32);
    }
    while (end!=0u64 && byte(bytes,end-1u64)==117) { end-=1u64; }
    return unsigned_literal(bytes,end,LiteralKind::U32);
}

extern "C" Literal decode(address bytes,u64 length,bool negative) on cpu {
    Literal value=positive_literal(bytes,length);
    if (value.error!=LiteralError::None || !negative) { return value; }
    if (value.kind==LiteralKind::F32) { return Literal(value.bits^2147483648u64,value.kind,LiteralError::None); }
    if (value.kind==LiteralKind::F64) { return Literal(value.bits^9223372036854775808u64,value.kind,LiteralError::None); }
    if (value.kind==LiteralKind::U32 && value.bits<=2147483648u64) {
        return Literal((0u64-value.bits)&4294967295u64,LiteralKind::I32,LiteralError::None);
    }
    if (value.kind==LiteralKind::U64 && value.bits<=9223372036854775808u64) {
        return Literal(0u64-value.bits,LiteralKind::I64,LiteralError::None);
    }
    return Literal(0u64,value.kind,LiteralError::NegativeRange);
}
