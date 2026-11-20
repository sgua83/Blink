module bootstrap.scalar_fold;

u64 truncate(u64 bits, u32 width) on cpu {
    return bits & (18446744073709551615u64 >> u64(64 - width));
}

i64 signed_value(u64 bits, u32 width) on cpu {
    u32 padding = 64 - width;
    return i64(bits << u64(padding)) >> i64(padding);
}

extern "C" u64 unary(UnaryOperation operation, u32 width, u64 bits) on cpu {
    if (operation == UnaryOperation::Negate) {
        return truncate(0u64 - bits, width);
    }
    if (operation == UnaryOperation::LogicalNot) {
        return u64(bits == 0u64);
    }
    if (operation == UnaryOperation::FloatNegate) {
        return bits ^ (1u64 << u64(width - 1));
    }
    return truncate(~bits, width);
}

extern "C" u64 binary(BinaryOperation operation, u32 width, u32 signed, u64 left, u64 right) on cpu {
    u64 shift = right & u64(width - 1);
    u64 bits;
    if (operation == BinaryOperation::Add) {
        bits = left + right;
    } else if (operation == BinaryOperation::Subtract) {
        bits = left - right;
    } else if (operation == BinaryOperation::Multiply) {
        bits = left * right;
    } else if (operation == BinaryOperation::And) {
        bits = left & right;
    } else if (operation == BinaryOperation::Or) {
        bits = left | right;
    } else if (operation == BinaryOperation::Xor) {
        bits = left ^ right;
    } else if (operation == BinaryOperation::ShiftLeft) {
        bits = left << shift;
    } else if (operation == BinaryOperation::ShiftRight) {
        bits = left >> shift;
    } else if (operation == BinaryOperation::ShiftRightSigned) {
        bits = u64(signed_value(left, width) >> i64(shift));
    } else {
        u64 ordered_left = left;
        u64 ordered_right = right;
        if (signed != 0) {
            u64 bias = 1u64 << u64(width - 1);
            ordered_left = truncate(left, width) ^ bias;
            ordered_right = truncate(right, width) ^ bias;
        }
        if (operation == BinaryOperation::Equal) {
            bits = u64(ordered_left == ordered_right);
        } else if (operation == BinaryOperation::NotEqual) {
            bits = u64(ordered_left != ordered_right);
        } else if (operation == BinaryOperation::Less) {
            bits = u64(ordered_left < ordered_right);
        } else if (operation == BinaryOperation::Greater) {
            bits = u64(ordered_left > ordered_right);
        } else if (operation == BinaryOperation::LessEqual) {
            bits = u64(ordered_left <= ordered_right);
        } else {
            bits = u64(ordered_left >= ordered_right);
        }
    }
    return truncate(bits, width);
}

typeset ScalarFloat { f32; f64; };

T float_arithmetic<T: ScalarFloat>(BinaryOperation operation, T left, T right) on cpu {
    if (operation == BinaryOperation::Add) {
        return left + right;
    }
    if (operation == BinaryOperation::Subtract) {
        return left - right;
    }
    if (operation == BinaryOperation::Multiply) {
        return left * right;
    }
    return left / right;
}

bool float_compare<T: ScalarFloat>(BinaryOperation operation, T left, T right) on cpu {
    if (operation == BinaryOperation::Equal) {
        return left == right;
    }
    if (operation == BinaryOperation::NotEqual) {
        return left != right;
    }
    if (operation == BinaryOperation::Less) {
        return left < right;
    }
    if (operation == BinaryOperation::Greater) {
        return left > right;
    }
    if (operation == BinaryOperation::LessEqual) {
        return left <= right;
    }
    return left >= right;
}

extern "C" u64 float_binary(BinaryOperation operation, u32 width, u64 left, u64 right) on cpu {
    bool compare = u32(operation) >= u32(BinaryOperation::Equal);
    if (width == 64) {
        f64 a = bitcast_f64(left);
        f64 b = bitcast_f64(right);
        if (compare) {
            return u64(float_compare(operation, a, b));
        }
        return bitcast_u64(float_arithmetic(operation, a, b));
    }
    f32 a = bitcast_f32(u32(left));
    f32 b = bitcast_f32(u32(right));
    if (compare) {
        return u64(float_compare(operation, a, b));
    }
    return u64(bitcast_u32(float_arithmetic(operation, a, b)));
}

extern "C" u64 divide_unsigned(u64 left, u64 right, u32 remainder) on cpu requires nonzero(right) {
    if (remainder != 0) {
        return left % right;
    }
    return left / right;
}

extern "C" i64 divide_signed(i64 left, i64 right, u32 remainder) on cpu requires nonzero(right) {
    if (remainder != 0) {
        return left % right;
    }
    return left / right;
}

f64 floating_value(u64 bits, u32 width) on cpu {
    if (width == 64) {
        return bitcast_f64(bits);
    }
    return f64(bitcast_f32(u32(bits)));
}

u64 cast_to_float(ScalarEncoding source, u32 width, u64 bits) on cpu {
    if (source.kind == ScalarKind::Float) {
        if (width == 64) {
            return bitcast_u64(floating_value(bits, source.width));
        }
        return u64(bitcast_u32(f32(floating_value(bits, source.width))));
    }
    if (source.kind == ScalarKind::Signed) {
        i64 value = signed_value(bits, source.width);
        if (width == 64) {
            return bitcast_u64(f64(value));
        }
        return u64(bitcast_u32(f32(value)));
    }
    if (width == 64) {
        return bitcast_u64(f64(bits));
    }
    return u64(bitcast_u32(f32(bits)));
}

extern "C" FoldedCast cast(ScalarEncoding source, ScalarEncoding target, u64 bits) on cpu {
    if (target.kind == ScalarKind::Boolean) {
        if (source.kind == ScalarKind::Float) {
            return FoldedCast(u64(floating_value(bits, source.width) != f64(0)), 1);
        }
        return FoldedCast(u64(bits != 0u64), 1);
    }
    if (target.kind == ScalarKind::Float) {
        return FoldedCast(cast_to_float(source, target.width, bits), 1);
    }
    if (source.kind == ScalarKind::Float) {
        f64 value = floating_value(bits, source.width);
        bool signed = target.kind == ScalarKind::Signed;
        // The exclusive upper bound is exactly a power of two in f64.
        u32 exponent = 1023 + target.width - u32(signed);
        f64 maximum = bitcast_f64(u64(exponent) << 52u64);
        f64 minimum = f64(0);
        if (signed) {
            minimum = -maximum;
        }
        if (!(value >= minimum && value < maximum)) {
            return FoldedCast(0u64, 0);
        }
        if (signed) {
            return FoldedCast(truncate(u64(i64(value)), target.width), 1);
        }
        return FoldedCast(truncate(u64(value), target.width), 1);
    }
    if (source.kind == ScalarKind::Signed) {
        return FoldedCast(truncate(u64(signed_value(bits, source.width)), target.width), 1);
    }
    return FoldedCast(truncate(bits, target.width), 1);
}

u32 conflict_width(u64 bits) on cpu {
    u64 value = bits;
    u32 width = 1;
    if (value >= 4294967296u64) { value = value >> 32u64; width += 32; }
    if (value >= 65536u64) { value = value >> 16u64; width += 16; }
    if (value >= 256u64) { value = value >> 8u64; width += 8; }
    if (value >= 16u64) { value = value >> 4u64; width += 4; }
    if (value >= 4u64) { value = value >> 2u64; width += 2; }
    if (value >= 2u64) { width += 1; }
    return width;
}

bool bits_intersect(BitRange range, BitPattern bits, u64 order_bias) on cpu {
    u64 fixed = (bits.value ^ order_bias) & bits.mask;
    u64 value = range.lower;
    while (value <= range.upper) {
        u64 conflict = (value ^ fixed) & bits.mask;
        if (conflict == 0u64) { return true; }
        u32 carry = conflict_width(conflict);
        u64 increment = (fixed & (1u64 << u64(carry - 1))) == 0u64 ? 1u64 : 0u64;
        // The search can move beyond the complete u64 domain. Only that
        // carry is needed; no wide value or enumerated machine values remain.
        if (carry == 64) {
            if (increment != 0u64) { return false; }
            value = fixed;
        } else {
            u64 high = (value >> u64(carry)) + increment;
            if ((high >> (64u64 - u64(carry))) != 0u64) { return false; }
            value = (high << u64(carry)) | fixed;
        }
    }
    return false;
}

bool implies_bits(read RangeQuery query) on cpu {
    u64 mask = query.required.mask;
    while (mask != 0u64) {
        u64 bit = mask & (0u64 - mask);
        mask = mask ^ bit;
        if ((query.known.mask & bit) != 0u64 && ((query.known.value ^ query.required.value) & bit) == 0u64) { continue; }
        u64 opposite = (query.known.value & ~bit) | ((query.required.value ^ bit) & bit);
        if (bits_intersect(query.range, BitPattern(query.known.mask | bit, opposite), query.order_bias)) { return false; }
    }
    return true;
}

extern "C" bool covers_range(read RangeQuery query, address intervals, u64 count,
    cfn<BitRange,address,u64> interval) on cpu requires nonzero(interval) {
    u64 cursor = query.range.lower;
    u64 index = 0u64;
    while (index < count) {
        BitRange required = interval(intervals, index);
        if (cursor < required.lower) {
            u64 upper = min(query.range.upper, required.lower - 1u64);
            if (bits_intersect(BitRange(cursor, upper), query.known, query.order_bias)) { return false; }
        }
        // required.upper+1 is unnecessary once the current range ends here,
        // including MAX_U64; proceed directly to the same required-bit query.
        if (cursor > query.range.upper || required.upper >= query.range.upper) { return implies_bits(query); }
        cursor = max(cursor, required.upper + 1u64);
        index += 1u64;
    }
    if (bits_intersect(BitRange(cursor, query.range.upper), query.known, query.order_bias)) { return false; }
    return implies_bits(query);
}


u64 fp_sign(u32 width) on cpu {return 1u64<<(u64(width)-1u64);}
f64 fp_power(i32 exponent) on cpu {return bitcast_f64(u64(exponent+1023)<<52u64);}
f64 fp_normal(u32 width) on cpu {return width==32 ? fp_power(-126) : bitcast_f64(0x0010000000000000u64);}
f64 fp_maximum(u32 width) on cpu {return bitcast_f64(width==32 ? 0x47efffffe0000000u64 : 0x7fefffffffffffffu64);}
f64 fp_abs(f64 value) on cpu {return bitcast_f64(bitcast_u64(value)&0x7fffffffffffffffu64);}
bool fp_finite(f64 value) on cpu {return (bitcast_u64(value)&0x7ff0000000000000u64)!=0x7ff0000000000000u64;}
f64 fp_min(f64 left,f64 right) on cpu {return left<right ? left : right;}
f64 fp_max(f64 left,f64 right) on cpu {return left>right ? left : right;}
f64 fp_next(f64 value,u32 up) on cpu {
    u64 raw=bitcast_u64(value);u64 magnitude=raw&0x7fffffffffffffffu64;
    if(magnitude>0x7ff0000000000000u64){return value;}
    if(up!=0){
        if(raw==0x7ff0000000000000u64){return value;}
        if(magnitude==0u64){return bitcast_f64(1u64);}
        return bitcast_f64((raw&0x8000000000000000u64)==0u64 ? raw+1u64 : raw-1u64);
    }
    if(raw==0xfff0000000000000u64){return value;}
    if(magnitude==0u64){return bitcast_f64(0x8000000000000001u64);}
    return bitcast_f64((raw&0x8000000000000000u64)==0u64 ? raw-1u64 : raw+1u64);
}
f64 fp_next_scalar(u32 width,f64 value,u32 up) on cpu {
    if(width==64){return fp_next(value,up);}
    u32 raw=bitcast_u32(f32(value));u32 magnitude=raw&0x7fffffffu;
    if(magnitude>0x7f800000u){return value;}
    if(up!=0){
        if(raw==0x7f800000u){return value;}
        if(magnitude==0u){return f64(bitcast_f32(1u));}
        return f64(bitcast_f32((raw&0x80000000u)==0u ? raw+1u : raw-1u));
    }
    if(raw==0xff800000u){return value;}
    if(magnitude==0u){return f64(bitcast_f32(0x80000001u));}
    return f64(bitcast_f32((raw&0x80000000u)==0u ? raw-1u : raw+1u));
}
f64 fp_truncated(f64 value) on cpu {
    u64 raw=bitcast_u64(value);u64 exponent=(raw>>52u64)&2047u64;
    if(exponent<1023u64){return bitcast_f64(raw&0x8000000000000000u64);}
    if(exponent>=1075u64){return value;}
    u64 count=1075u64-exponent;
    return bitcast_f64(raw&~((1u64<<count)-1u64));
}
f64 fp_floor(f64 value) on cpu {f64 whole=fp_truncated(value);return whole>value ? whole-1.0f64 : whole;}
f64 fp_ceil(f64 value) on cpu {f64 whole=fp_truncated(value);return whole<value ? whole+1.0f64 : whole;}
u64 fp_raw(u32 width,f64 value) on cpu {return width==32 ? u64(bitcast_u32(f32(value))) : bitcast_u64(value);}
FloatRange fp_unknown() on cpu {return FloatRange(0u64,0u64,0);}
FloatRange fp_empty() on cpu {return FloatRange(0u64,0u64,2);}
extern "C" u64 float_order(u32 width,u64 value) on cpu {
    u64 sign=fp_sign(width);return (value&sign)==0u64 ? value^sign : (~value)&truncate(0xffffffffffffffffu64,width);
}
extern "C" u64 float_bits(u32 width,u64 value) on cpu {
    u64 sign=fp_sign(width);return (value&sign)==0u64 ? (~value)&truncate(0xffffffffffffffffu64,width) : value^sign;
}
extern "C" f64 float_number(u32 width,u64 value) on cpu {return floating_value(float_bits(width,value),width);}
extern "C" FloatInterval float_finite(u32 width,BitRange range) on cpu {
    if(range.lower>range.upper){return FloatInterval(0.0f64,0.0f64,0);}
    f64 lower=float_number(width,range.lower);f64 upper=float_number(width,range.upper);
    if(!fp_finite(lower)||!fp_finite(upper)){return FloatInterval(0.0f64,0.0f64,0);}
    f64 normal=fp_normal(width);
    if(lower>0.0f64&&lower<normal){lower=0.0f64;}
    if(upper<0.0f64&&upper>-normal){upper=0.0f64;}
    return FloatInterval(lower,upper,1);
}
extern "C" f64 float_rounded(u32 width,f64 value,u32 lower) on cpu {
    if(width==64){return value;}
    f64 converted=f64(f32(value));
    if(lower!=0&&converted>value){return fp_next_scalar(32,converted,0);}
    if(lower==0&&converted<value){return fp_next_scalar(32,converted,1);}
    return converted;
}
extern "C" f64 float_sum_bound(u32 width,f64 left,f64 right,u32 lower) on cpu {
    f64 sum=left+right;f64 back=sum-left;
    f64 tail=(left-(sum-back))+(right-back);f64 value=sum;
    if(lower!=0&&tail<0.0f64){value=fp_next(sum,0);}
    else if(lower==0&&tail>0.0f64){value=fp_next(sum,1);}
    return float_rounded(width,value,lower);
}
extern "C" FloatRange float_interval(u32 width,f64 low,f64 high) on cpu {
    f64 lower=float_rounded(width,low,1);f64 upper=float_rounded(width,high,0);
    if(!fp_finite(lower)||!fp_finite(upper)||lower>upper){return fp_unknown();}
    f64 normal=fp_normal(width);
    if(lower>0.0f64&&lower<normal){lower=0.0f64;}
    if(upper<0.0f64&&upper>-normal){upper=0.0f64;}
    if(lower==0.0f64){lower=bitcast_f64(0x8000000000000000u64);}
    if(upper==0.0f64){upper=0.0f64;}
    return FloatRange(float_order(width,fp_raw(width,lower)),float_order(width,fp_raw(width,upper)),1);
}
FloatInterval fp_numeric(u32 width,BitRange range) on cpu {
    u64 lower=range.lower;u64 upper=range.upper;
    u64 first=float_order(width,fp_raw(width,bitcast_f64(0xfff0000000000000u64)));
    u64 last=float_order(width,fp_raw(width,bitcast_f64(0x7ff0000000000000u64)));
    if(lower<first){lower=first;}if(upper>last){upper=last;}
    if(lower>upper){return FloatInterval(0.0f64,0.0f64,0);}
    f64 a=float_number(width,lower);f64 b=float_number(width,upper);f64 normal=fp_normal(width);
    if(a>0.0f64&&a<normal){a=0.0f64;}if(b<0.0f64&&b>-normal){b=0.0f64;}
    return FloatInterval(a,b,1);
}
extern "C" u32 float_comparison_possible(BinaryOperation operation,u32 width,BitRange left,BitRange right,u32 truth_code) on cpu {
    if(left.lower>left.upper||right.lower>right.upper){return 0;}
    u64 first=float_order(width,fp_raw(width,bitcast_f64(0xfff0000000000000u64)));
    u64 last=float_order(width,fp_raw(width,bitcast_f64(0x7ff0000000000000u64)));
    bool nan=left.lower<first||left.upper>last||right.lower<first||right.upper>last;bool truth=truth_code!=0;
    if(nan&&truth==(operation==BinaryOperation::NotEqual)){return 1;}
    FloatInterval a=fp_numeric(width,left);FloatInterval b=fp_numeric(width,right);
    if(a.present==0||b.present==0){return 0;}
    bool possible=false;
    if(operation==BinaryOperation::Less){possible=truth ? a.lower<b.upper : a.upper>=b.lower;}
    else if(operation==BinaryOperation::LessEqual){possible=truth ? a.lower<=b.upper : a.upper>b.lower;}
    else if(operation==BinaryOperation::Greater){possible=truth ? a.upper>b.lower : a.lower<=b.upper;}
    else if(operation==BinaryOperation::GreaterEqual){possible=truth ? a.upper>=b.lower : a.lower<b.upper;}
    else if(truth==(operation==BinaryOperation::Equal)){possible=a.lower<=b.upper&&b.lower<=a.upper;}
    else {possible=a.lower!=a.upper||b.lower!=b.upper||a.lower!=b.lower;}
    return possible ? 1 : 0;
}
BitRange fp_converted(u32 width,u64 lower,u64 upper,u32 source_float) on cpu {
    u64 a=source_float!=0 ? float_bits(width,lower) : float_order(width,lower);
    u64 b=source_float!=0 ? float_bits(width,upper) : float_order(width,upper);
    return a<b ? BitRange(a,b) : BitRange(b,a);
}
extern "C" FloatPieces float_bitcast(u32 width,BitRange range,u32 source_float) on cpu {
    u64 sign=fp_sign(width);u64 low_upper=range.upper<sign-1u64 ? range.upper : sign-1u64;
    u64 high_lower=range.lower>sign ? range.lower : sign;
    BitRange first=BitRange(0u64,0u64);BitRange second=BitRange(0u64,0u64);u32 count=0;
    if(range.lower<=low_upper){first=fp_converted(width,range.lower,low_upper,source_float);count=1;}
    if(high_lower<=range.upper){BitRange next=fp_converted(width,high_lower,range.upper,source_float);if(count==0){first=next;}else{second=next;}count+=1;}
    return FloatPieces(first,second,count);
}
// Compare an integer's rounded floating value without introducing an i128 ABI.
// The original integer magnitude is at most u64::MAX; exponent 64 denotes 2^64.
i32 fp_integer_relation(f64 converted,u64 magnitude,bool negative) on cpu {
    u64 raw=bitcast_u64(converted)&0x7fffffffffffffffu64;
    u64 exponent=(raw>>52u64)&2047u64;u64 actual=0u64;i32 relation=i32(0);
    if(exponent>=1087u64){relation=i32(1);}
    else {
        if(exponent>=1023u64){
            u64 significand=(raw&0x000fffffffffffffu64)|0x0010000000000000u64;
            actual=exponent>=1075u64 ? significand<<(exponent-1075u64) : significand>>(1075u64-exponent);
        }
        relation=actual>magnitude ? i32(1) : (actual<magnitude ? i32(-1) : i32(0));
    }
    return negative ? -relation : relation;
}
f64 fp_integer_endpoint(ScalarEncoding source,u32 width,u64 ordered,u32 lower) on cpu {
    bool signed=source.kind==ScalarKind::Signed;u64 bias=signed ? fp_sign(source.width) : 0u64;u64 raw=ordered^bias;
    bool negative=signed&&(raw&bias)!=0u64;u64 magnitude=raw;
    if(negative){magnitude=0u64-u64(signed_value(raw,source.width));}
    f64 result=floating_value(cast_to_float(source,width,raw),width);
    i32 relation=fp_integer_relation(result,magnitude,negative);
    if(lower!=0&&relation>0){return fp_next_scalar(width,result,0);}
    if(lower==0&&relation<0){return fp_next_scalar(width,result,1);}
    return result;
}
extern "C" FloatRange float_cast_range(ScalarEncoding source,ScalarEncoding target,BitRange range) on cpu {
    bool integer=source.kind==ScalarKind::Unsigned||source.kind==ScalarKind::Signed;
    if(integer&&target.kind==ScalarKind::Float){
        if(range.lower>range.upper){return fp_empty();}
        return float_interval(target.width,fp_integer_endpoint(source,target.width,range.lower,1),fp_integer_endpoint(source,target.width,range.upper,0));
    }
    if(source.kind==ScalarKind::Float){
        FloatInterval finite=float_finite(source.width,range);if(finite.present==0){return fp_unknown();}
        if(target.kind==ScalarKind::Float){return float_interval(target.width,finite.lower,finite.upper);}
        if(target.kind==ScalarKind::Unsigned||target.kind==ScalarKind::Signed){
            ScalarEncoding floating_source=ScalarEncoding(ScalarKind::Float,64);
            FoldedCast lower=cast(floating_source,target,bitcast_u64(fp_truncated(finite.lower)));
            FoldedCast upper=cast(floating_source,target,bitcast_u64(fp_truncated(finite.upper)));
            if(lower.present!=0&&upper.present!=0){u64 bias=target.kind==ScalarKind::Signed ? fp_sign(target.width) : 0u64;return FloatRange(lower.bits^bias,upper.bits^bias,1);}
        }
    }
    return fp_unknown();
}
extern "C" f64 float_add_up(f64 left,f64 right) on cpu {return float_sum_bound(64,left,right,0);}
extern "C" f64 float_multiply_up(f64 left,f64 right) on cpu {f64 product=left*right;return left==0.0f64||right==0.0f64 ? product : fp_next(product,1);}
extern "C" f64 float_divide_up(f64 left,f64 right) on cpu {f64 quotient=left/right;return left==0.0f64 ? quotient : fp_next(quotient,1);}
f64 fp_min_four(f64 a,f64 b,f64 c,f64 d) on cpu {return fp_min(fp_min(fp_min(a,b),c),d);}
f64 fp_max_four(f64 a,f64 b,f64 c,f64 d) on cpu {return fp_max(fp_max(fp_max(a,b),c),d);}
extern "C" FloatRange float_range_arithmetic(BinaryOperation operation,u32 width,BitRange left,BitRange right) on cpu {
    FloatInterval a=float_finite(width,left);FloatInterval b=float_finite(width,right);
    if(a.present==0||b.present==0){return fp_unknown();}f64 lower=0.0f64;f64 upper=0.0f64;
    if(operation==BinaryOperation::Add){lower=float_sum_bound(width,a.lower,b.lower,1);upper=float_sum_bound(width,a.upper,b.upper,0);}
    else if(operation==BinaryOperation::Subtract){lower=float_sum_bound(width,a.lower,-b.upper,1);upper=float_sum_bound(width,a.upper,-b.lower,0);}
    else if(operation==BinaryOperation::Multiply){
        if(width==32){lower=fp_min_four(a.lower*b.lower,a.lower*b.upper,a.upper*b.lower,a.upper*b.upper);upper=fp_max_four(a.lower*b.lower,a.lower*b.upper,a.upper*b.lower,a.upper*b.upper);}
        else {lower=fp_min_four(-float_multiply_up(-a.lower,b.lower),-float_multiply_up(-a.lower,b.upper),-float_multiply_up(-a.upper,b.lower),-float_multiply_up(-a.upper,b.upper));upper=fp_max_four(float_multiply_up(a.lower,b.lower),float_multiply_up(a.lower,b.upper),float_multiply_up(a.upper,b.lower),float_multiply_up(a.upper,b.upper));}
    }
    else if(operation==BinaryOperation::Divide){
        f64 normal=fp_normal(width);f64 maximum=fp_power(i32(width==32 ? 126 : 1022));
        if((b.lower<=0.0f64&&b.upper>=0.0f64)||fp_min(fp_abs(b.lower),fp_abs(b.upper))<normal||fp_max(fp_abs(b.lower),fp_abs(b.upper))>maximum){return fp_unknown();}
        lower=fp_min_four(a.lower/b.lower,a.lower/b.upper,a.upper/b.lower,a.upper/b.upper);upper=fp_max_four(a.lower/b.lower,a.lower/b.upper,a.upper/b.lower,a.upper/b.upper);
        if(width==32){
            f64 low_error=3.0f64*fp_max(fp_abs(lower)*fp_power(-23),fp_power(-149));f64 high_error=3.0f64*fp_max(fp_abs(upper)*fp_power(-23),fp_power(-149));
            lower-=low_error;upper+=high_error;
        }else{
            f64 low_error=float_multiply_up(4.0f64,fp_max(float_multiply_up(fp_abs(lower),fp_power(-52)),bitcast_f64(1u64)));
            f64 high_error=float_multiply_up(4.0f64,fp_max(float_multiply_up(fp_abs(upper),fp_power(-52)),bitcast_f64(1u64)));
            lower=-float_add_up(-lower,low_error);upper=float_add_up(upper,high_error);
        }
    }else{return fp_unknown();}
    return float_interval(width,lower,upper);
}
extern "C" FloatRange float_math(u32 operation,u32 width,BitRange left,BitRange right) on cpu {
    FloatInterval a=float_finite(width,left);if(a.present==0){return fp_unknown();}f64 lower=0.0f64;f64 upper=0.0f64;
    if(operation==1||operation==2){
        FloatInterval b=float_finite(width,right);if(b.present==0){return fp_unknown();}
        lower=operation==1 ? fp_min(a.lower,b.lower) : fp_max(a.lower,b.lower);upper=operation==1 ? fp_min(a.upper,b.upper) : fp_max(a.upper,b.upper);
    }else if(operation==3){lower=a.lower<=0.0f64&&a.upper>=0.0f64 ? 0.0f64 : fp_min(fp_abs(a.lower),fp_abs(a.upper));upper=fp_max(fp_abs(a.lower),fp_abs(a.upper));}
    else if(operation==4){lower=fp_floor(a.lower);upper=fp_floor(a.upper);}
    else if(operation==7){lower=fp_ceil(a.lower);upper=fp_ceil(a.upper);}
    else if((operation==5||operation==6)&&a.lower>0.0f64){lower=-fp_maximum(width);upper=fp_maximum(width);}
    else{return fp_unknown();}
    return float_interval(width,lower,upper);
}
extern "C" FloatRange float_sum_range(u32 width,BitRange range,u64 maximum) on cpu {
    if(range.lower>range.upper){return fp_empty();}
    FloatInterval input=float_finite(width,range);if(input.present==0){return fp_unknown();}
    if(maximum==1u64){return float_interval(width,input.lower,input.upper);}
    f64 epsilon=fp_power(width==32 ? -23 : -52);f64 normal=fp_normal(width);f64 finite_maximum=fp_maximum(width);
    u64 power=maximum-1u64;f64 base=1.0f64+epsilon;f64 factor=1.0f64;
    while(power!=0u64){if((power&1u64)!=0u64){factor=float_multiply_up(factor,base);}power=power>>1u64;if(power!=0u64){base=float_multiply_up(base,base);}}
    f64 count=f64(maximum);f64 magnitude=float_multiply_up(count,fp_max(fp_abs(input.lower),fp_abs(input.upper)));
    f64 flushed=float_multiply_up(float_multiply_up(factor,count-1.0f64),fp_min(normal,magnitude));
    f64 error=float_add_up(float_multiply_up(float_add_up(factor,-1.0f64),magnitude),flushed);
    if(float_add_up(magnitude,error)>finite_maximum){return fp_unknown();}
    f64 lower=input.lower>=0.0f64 ? input.lower : -float_add_up(float_multiply_up(-input.lower,count),error);
    f64 upper=input.upper<=0.0f64 ? input.upper : float_add_up(float_multiply_up(input.upper,count),error);
    return float_interval(width,lower,upper);
}
extern "C" FloatRange float_negate(u32 width,BitRange range) on cpu {
    FloatInterval input=float_finite(width,range);return input.present!=0 ? float_interval(width,-input.upper,-input.lower) : fp_unknown();
}
extern "C" f64 float_integer_bound(u64 value,u32 lower) on cpu {return fp_integer_endpoint(ScalarEncoding(ScalarKind::Unsigned,64),64,value,lower);}
extern "C" f64 float_conversion_error(u64 upper) on cpu {return upper<=16777216u64 ? 0.0f64 : fp_power(-23);}

struct BitPrefix { bool present; u64 lower; u64 upper; };
extern "C" BitRange integer_bitwise_range(IntegerBitwise operation,u32 width,u64 order_bias,BitRange left,BitRange right) on cpu {
 array<BitPrefix,16> prefixes=array<BitPrefix,16>(BitPrefix(false,0u64,0u64));
 prefixes[0]=BitPrefix(true,0u64,0u64);u32 bit=width;
 while(bit!=0) {
  bit-=1;u32 al=u32((left.lower>>u64(bit))&1u64);u32 ah=u32((left.upper>>u64(bit))&1u64);
  u32 bl=u32((right.lower>>u64(bit))&1u64);u32 bh=u32((right.upper>>u64(bit))&1u64);
  u64 sign=(order_bias>>u64(bit))&1u64;
  array<BitPrefix,16> next=array<BitPrefix,16>(BitPrefix(false,0u64,0u64));
  for(loose in 0..16) {
   BitPrefix prefix=prefixes[loose];if(!prefix.present){continue;}
   for(a in 0..2) {for(b in 0..2) {
    if(((loose&1)==0 && a<al) || ((loose&2)==0 && a>ah)
      || ((loose&4)==0 && b<bl) || ((loose&8)==0 && b>bh)){continue;}
    u32 state=loose|u32(a>al)|(u32(a<ah)<<1)|(u32(b>bl)<<2)|(u32(b<bh)<<3);
    u64 av=u64(a)^sign;u64 bv=u64(b)^sign;u64 value;
    if(operation==IntegerBitwise::And){value=av&bv;}
    else if(operation==IntegerBitwise::Or){value=av|bv;}else{value=av^bv;}
    value=value^sign;u64 lower=(prefix.lower<<1u64)|value;u64 upper=(prefix.upper<<1u64)|value;
    BitPrefix previous=next[state];
    if(previous.present){lower=min(previous.lower,lower);upper=max(previous.upper,upper);}
    next[state]=BitPrefix(true,lower,upper);
   }}
  }
  prefixes=next;
 }
 u64 lower=18446744073709551615u64>>u64(64-width);u64 upper=0u64;
 for(state in 0..16){BitPrefix prefix=prefixes[state];if(prefix.present){lower=min(lower,prefix.lower);upper=max(upper,prefix.upper);}}
 return BitRange(lower,upper);
}

extern "C" BitPattern known_cast(u64 source_mask,u64 source_sign,u64 target_mask,BitPattern value) on cpu {
 u64 mask=value.mask&target_mask;u64 bits=value.value&mask;
 if(target_mask>source_mask){u64 extra=target_mask&~source_mask;
  if(source_sign==0u64){mask|=extra;}
  else if((value.mask&source_sign)!=0u64){mask|=extra;if((value.value&source_sign)!=0u64){bits|=extra;}}
 }return BitPattern(mask,bits);
}
extern "C" BitPattern known_combined(u64 target_mask,u64 am,u64 sign_a,BitPattern a,u64 bm,u64 sign_b,BitPattern b) on cpu {
 BitPattern left=known_cast(am,sign_a,target_mask,a);BitPattern right=known_cast(bm,sign_b,target_mask,b);
 return BitPattern(left.mask|right.mask,left.value|right.value);
}
BitPattern common_pattern(BitPattern a,BitPattern b) on cpu {
 u64 mask=a.mask&b.mask&~(a.value^b.value);return BitPattern(mask,a.value&mask);
}
extern "C" BitPattern known_common(u64 target_mask,BitPattern a,u64 bm,u64 sign_b,BitPattern b) on cpu {
 return common_pattern(a,known_cast(bm,sign_b,target_mask,b));
}
extern "C" BitPattern known_bitwise(IntegerBitwise operation,BitPattern a,BitPattern b) on cpu {
 u64 az=a.mask&~a.value;u64 bz=b.mask&~b.value;
 if(operation==IntegerBitwise::And){return BitPattern(az|bz|(a.value&b.value),a.value&b.value);}
 if(operation==IntegerBitwise::Or){return BitPattern((az&bz)|a.value|b.value,a.value|b.value);}
 return BitPattern(a.mask&b.mask,(a.value^b.value)&a.mask&b.mask);
}
extern "C" BitPattern known_ranges(u64 mask,u64 bias,address context,u64 count,cfn<BitRange,address,u64> read) on cpu requires nonzero(read) {
 BitPattern result=BitPattern(0u64,0u64);bool present=false;
 u64 index=0u64;while(index<count){BitRange range=read(context,index);index+=1u64;
  u64 upper=min(range.upper,mask);if(range.lower>upper){continue;}
  u64 different=range.lower^upper;
  different|=different>>1u64;different|=different>>2u64;different|=different>>4u64;
  different|=different>>8u64;different|=different>>16u64;different|=different>>32u64;
  u64 fixed=mask&~different;BitPattern bits=BitPattern(fixed,(range.lower^bias)&fixed);if(bits.mask==0u64){return bits;}
  if(present){result=common_pattern(result,bits);}else{result=bits;present=true;}
  if(result.mask==0u64){return result;}
 }return result;
}
u64 shifted_pattern(u64 bits,u32 count,bool left,bool fill,u64 mask) on cpu {
 u64 moved;u64 padding;
 if(left){moved=bits<<u64(count);padding=mask&~(mask<<u64(count));}
 else{moved=bits>>u64(count);padding=mask&~(mask>>u64(count));}
 if(fill){moved|=padding;}return moved&mask;
}
extern "C" BitPattern known_shifted(u64 mask,u64 bias,u32 direction,BitPattern value,address context,u64 count,cfn<BitRange,address,u64> read) on cpu requires nonzero(read) {
 bool left=direction!=0;bool zero_fill=left || bias==0u64 || ((value.mask&bias)!=0u64 && (value.value&bias)==0u64);
 bool one_fill=!left && bias!=0u64 && (value.mask&value.value&bias)!=0u64;
 BitPattern result=BitPattern(0u64,0u64);bool present=false;
 u64 index=0u64;while(index<count){BitRange range=read(context,index);index+=1u64;u32 lower=u32(range.lower^bias);u32 upper=u32(range.upper^bias);
  u64 zero=shifted_pattern(value.mask&~value.value,lower,left,zero_fill,mask);
  u64 one=shifted_pattern(value.value,lower,left,one_fill,mask);u32 covered=1;
  while(covered<=upper-lower){u32 offset=min(covered,upper-lower+1-covered);
   zero&=shifted_pattern(zero,offset,left,zero_fill,mask);one&=shifted_pattern(one,offset,left,one_fill,mask);covered+=offset;
  }
  BitPattern bits=BitPattern(zero|one,one);
  if(present){result=common_pattern(result,bits);}else{result=bits;present=true;}
  if(result.mask==0u64){return result;}
 }return result;
}

struct IntegerWideRange {IntegerWide lower;IntegerWide upper;};
IntegerWide range_unsigned(u64 value) on cpu {return IntegerWide(value,0u64);}
IntegerWide range_signed(i64 value) on cpu {return IntegerWide(u64(value),value<i64(0) ? 18446744073709551615u64 : 0u64);}
extern "C" IntegerWide range_add(IntegerWide a,IntegerWide b) on cpu {
 u64 low=a.low+b.low;return IntegerWide(low,a.high+b.high+u64(low<a.low));
}
extern "C" IntegerWide range_subtract(IntegerWide a,IntegerWide b) on cpu {
 return IntegerWide(a.low-b.low,a.high-b.high-u64(a.low<b.low));
}
extern "C" IntegerWide range_negate(IntegerWide value) on cpu {
 u64 low=0u64-value.low;return IntegerWide(low,~value.high+u64(low==0u64));
}
extern "C" bool range_less(IntegerWide a,IntegerWide b) on cpu {
 i64 ah=i64(a.high);i64 bh=i64(b.high);return ah<bh || (ah==bh && a.low<b.low);
}
extern "C" IntegerWide range_min(IntegerWide a,IntegerWide b) on cpu {return range_less(a,b) ? a : b;}
extern "C" IntegerWide range_max(IntegerWide a,IntegerWide b) on cpu {return range_less(a,b) ? b : a;}
IntegerWideRange range_corners(IntegerWide a,IntegerWide b,IntegerWide c,IntegerWide d) on cpu {
 return IntegerWideRange(range_min(range_min(a,b),range_min(c,d)),range_max(range_max(a,b),range_max(c,d)));
}
u64 range_magnitude(i64 value) on cpu {return value<i64(0) ? 0u64-u64(value) : u64(value);}
extern "C" IntegerWide range_product(u64 a,u64 b) on cpu {
 u64 al=a&4294967295u64;u64 ah=a>>32u64;u64 bl=b&4294967295u64;u64 bh=b>>32u64;
 u64 low=al*bl;u64 middle=ah*bl+(low>>32u64);u64 high=middle>>32u64;
 middle=(middle&4294967295u64)+al*bh;
 return IntegerWide((middle<<32u64)|(low&4294967295u64),ah*bh+high+(middle>>32u64));
}
IntegerWide range_signed_product(i64 a,i64 b) on cpu {
 IntegerWide product=range_product(range_magnitude(a),range_magnitude(b));
 return (a<i64(0))!=(b<i64(0)) ? range_negate(product) : product;
}
IntegerWide range_shift(IntegerWide value,u32 count) on cpu {
 if(count==0){return value;}
 return IntegerWide(value.low<<u64(count),(value.high<<u64(count))|(value.low>>u64(64-count)));
}
IntegerWide range_quotient(i64 left,i64 right) on cpu requires nonzero(right) {
 u64 numerator=range_magnitude(left);u64 denominator;
 if(right<i64(0)){denominator=0u64-u64(right);}else{denominator=u64(right);}
 IntegerWide result=range_unsigned(numerator/denominator);
 return (left<i64(0))!=(right<i64(0)) ? range_negate(result) : result;
}
IntegerRangePieces range_single(u64 lower,u64 upper) on cpu {return IntegerRangePieces(BitRange(lower,upper),BitRange(0u64,0u64),1);}
extern "C" IntegerRangePieces integer_range_project(u32 width,u64 bias,IntegerWide lower,IntegerWide upper) on cpu {
 u64 mask=truncate(18446744073709551615u64,width);IntegerWide distance=range_subtract(upper,lower);
 if(distance.high!=0u64 || distance.low>=mask){return range_single(0u64,mask);}
 u64 a=(lower.low&mask)^bias;u64 b=(upper.low&mask)^bias;
 IntegerWide offset=range_unsigned(bias);IntegerWide first=range_add(lower,offset);IntegerWide last=range_add(upper,offset);
 if(first.high==last.high && (first.low&~mask)==(last.low&~mask)){return range_single(a,b);}
 return IntegerRangePieces(BitRange(0u64,b),BitRange(a,mask),2);
}
IntegerWide range_number(u32 width,u32 signed,u64 ordered) on cpu {
 if(signed!=0){return range_signed(signed_value(ordered^(1u64<<u64(width-1)),width));}
 return range_unsigned(ordered);
}
extern "C" IntegerRangePieces integer_range_cast(u32 source_width,u32 source_signed,u32 target_width,u64 target_bias,BitRange value) on cpu {
 return integer_range_project(target_width,target_bias,range_number(source_width,source_signed,value.lower),range_number(source_width,source_signed,value.upper));
}
u64 tile_residue_prefix(u64 end,u64 period,u64 first,u64 last) on cpu requires nonzero(period) {
    u64 tail=min(end%period,last+1u64);
    return end/period*(last-first+1u64)+tail-min(first,tail);
}

extern "C" TileNonzeroBound tile_nonzero_bound(address context,u32 width,u32 maximum,
    cfn<TileLane,address> next_lane,cfn<TileResidue,address> next_residue) on cpu
    requires nonzero(next_lane) requires nonzero(next_residue) {
    u64 limit=u64(min(width,maximum))-1u64;
    TileNonzeroBound bound=TileNonzeroBound(0,maximum);
    while(true){
        TileLane lane=next_lane(context);
        if(lane.present==0){return bound;}
        u64 lower=u64(lane.lower);
        u64 upper=min(u64(lane.upper),limit);
        if(lower>upper){return TileNonzeroBound(1,0);}
        u32 population=u32(upper-lower+1u64);
        while(true){
            TileResidue residue=next_residue(context);
            u64 period=residue.period;
            if(period==0u64){break;}
            u64 first=u64(residue.first);
            u64 last=min(u64(residue.last),period-1u64);
            if(first>last){return TileNonzeroBound(1,0);}
            u64 selected=tile_residue_prefix(upper+1u64,period,first,last)
                -tile_residue_prefix(lower,period,first,last);
            population=min(population,u32(selected));
            if(population==0){return TileNonzeroBound(1,0);}
        }
        bound=TileNonzeroBound(1,min(bound.value,population));
    }
}

extern "C" IntegerRangePieces integer_range_tile_sum(u32 width,u32 signed,BitRange values,TileSumLanes lanes) on cpu {
    u64 minimum=u64(lanes.minimum);
    u64 maximum=u64(lanes.maximum);
    if(lanes.has_nonzero!=0){maximum=min(maximum,u64(lanes.nonzero));}

    u64 bias=0u64;
    IntegerWide lower;
    IntegerWide upper;
    if(signed!=0){
        bias=1u64<<u64(width-1);
        i64 first=signed_value(values.lower^bias,width);
        i64 last=signed_value(values.upper^bias,width);
        lower=range_signed_product(first,i64(first<i64(0)?maximum:minimum));
        upper=range_signed_product(last,i64(last<i64(0)?minimum:maximum));
    }else{
        lower=range_product(values.lower,minimum);
        upper=range_product(values.upper,maximum);
    }
    if(range_less(upper,lower)){return IntegerRangePieces(BitRange(0u64,0u64),BitRange(0u64,0u64),0);}
    return integer_range_project(width,bias,lower,upper);
}
extern "C" IntegerRangePieces integer_range_unary(IntegerRangeUnary operation,u32 width,u32 signed,BitRange value) on cpu {
 u64 mask=truncate(18446744073709551615u64,width);u64 bias=signed!=0 ? 1u64<<u64(width-1) : 0u64;
 if(operation==IntegerRangeUnary::BitNot){return range_single((~value.upper)&mask,(~value.lower)&mask);}
 return integer_range_project(width,bias,range_negate(range_number(width,signed,value.upper)),range_negate(range_number(width,signed,value.lower)));
}
extern "C" IntegerRangePieces integer_range_counts(u32 width,u64 bias,BitRange value) on cpu {
 u64 mask=u64(width-1);if(value.upper-value.lower>=mask){return range_single(bias,bias|mask);}
 u64 a=value.lower&mask;u64 b=value.upper&mask;
 if(a<=b){return range_single(bias|a,bias|b);}
 return IntegerRangePieces(BitRange(bias,bias|b),BitRange(bias|a,bias|mask),2);
}
IntegerWideRange range_signed_arithmetic(IntegerRangeOperation operation,u32 width,BitRange left,BitRange right) on cpu {
 u64 bias=1u64<<u64(width-1);i64 al=signed_value(left.lower^bias,width);i64 ah=signed_value(left.upper^bias,width);
 i64 bl=signed_value(right.lower^bias,width);i64 bh=signed_value(right.upper^bias,width);
 if(operation==IntegerRangeOperation::Minimum){return IntegerWideRange(range_signed(min(al,bl)),range_signed(min(ah,bh)));}
 if(operation==IntegerRangeOperation::Maximum){return IntegerWideRange(range_signed(max(al,bl)),range_signed(max(ah,bh)));}
 if(operation==IntegerRangeOperation::Add){return IntegerWideRange(range_add(range_signed(al),range_signed(bl)),range_add(range_signed(ah),range_signed(bh)));}
 if(operation==IntegerRangeOperation::Subtract){return IntegerWideRange(range_subtract(range_signed(al),range_signed(bh)),range_subtract(range_signed(ah),range_signed(bl)));}
 if(operation==IntegerRangeOperation::Multiply){return range_corners(range_signed_product(al,bl),range_signed_product(al,bh),range_signed_product(ah,bl),range_signed_product(ah,bh));}
 if(operation==IntegerRangeOperation::Divide || operation==IntegerRangeOperation::Remainder){
  if(bl!=i64(0) && bh!=i64(0) && (bl<i64(0))==(bh<i64(0))){
   if(operation==IntegerRangeOperation::Divide){return range_corners(range_quotient(al,bl),range_quotient(al,bh),range_quotient(ah,bl),range_quotient(ah,bh));}
   u64 maximum=max(range_magnitude(bl),range_magnitude(bh))-1u64;
   return IntegerWideRange(range_signed(max(min(al,i64(0)),-i64(maximum))),range_signed(min(max(ah,i64(0)),i64(maximum))));
  }
  return IntegerWideRange(range_signed(-i64(1u64<<u64(width-1))),range_signed(i64((1u64<<u64(width-1))-1u64)));
 }
 if(operation==IntegerRangeOperation::ShiftRight){return range_corners(range_signed(al>>bl),range_signed(al>>bh),range_signed(ah>>bl),range_signed(ah>>bh));}
 return range_corners(range_shift(range_signed(al),u32(bl)),range_shift(range_signed(al),u32(bh)),range_shift(range_signed(ah),u32(bl)),range_shift(range_signed(ah),u32(bh)));
}
IntegerWideRange range_unsigned_arithmetic(IntegerRangeOperation operation,u32 width,BitRange left,BitRange right) on cpu {
 u64 al=left.lower;u64 ah=left.upper;u64 bl=right.lower;u64 bh=right.upper;
 if(operation==IntegerRangeOperation::Minimum){return IntegerWideRange(range_unsigned(min(al,bl)),range_unsigned(min(ah,bh)));}
 if(operation==IntegerRangeOperation::Maximum){return IntegerWideRange(range_unsigned(max(al,bl)),range_unsigned(max(ah,bh)));}
 if(operation==IntegerRangeOperation::Add){return IntegerWideRange(range_add(range_unsigned(al),range_unsigned(bl)),range_add(range_unsigned(ah),range_unsigned(bh)));}
 if(operation==IntegerRangeOperation::Subtract){return IntegerWideRange(range_subtract(range_unsigned(al),range_unsigned(bh)),range_subtract(range_unsigned(ah),range_unsigned(bl)));}
 if(operation==IntegerRangeOperation::Multiply){return IntegerWideRange(range_product(al,bl),range_product(ah,bh));}
 if(operation==IntegerRangeOperation::Divide || operation==IntegerRangeOperation::Remainder){
  if(bl!=0u64 && bh!=0u64){
   if(operation==IntegerRangeOperation::Divide){return IntegerWideRange(range_unsigned(al/bh),range_unsigned(ah/bl));}
   return IntegerWideRange(range_unsigned(0u64),range_unsigned(min(ah,bh-1u64)));
  }
  return IntegerWideRange(range_unsigned(0u64),range_unsigned(truncate(18446744073709551615u64,width)));
 }
 if(operation==IntegerRangeOperation::ShiftRight){return IntegerWideRange(range_unsigned(al>>bh),range_unsigned(ah>>bl));}
 return IntegerWideRange(range_shift(range_unsigned(al),u32(bl)),range_shift(range_unsigned(ah),u32(bh)));
}
extern "C" IntegerRangePieces integer_range(IntegerRangeOperation operation,u32 width,u32 signed,BitRange left,BitRange right) on cpu {
 IntegerWideRange result;u64 bias=0u64;
 if(signed!=0){result=range_signed_arithmetic(operation,width,left,right);bias=1u64<<u64(width-1);}
 else{result=range_unsigned_arithmetic(operation,width,left,right);}
 return integer_range_project(width,bias,result.lower,result.upper);
}

extern "C" bool integer_wide_unsigned_less(IntegerWide a,IntegerWide b) on cpu {return a.high<b.high||(a.high==b.high&&a.low<b.low);}
extern "C" IntegerWideChecked integer_wide_checked_add(IntegerWide a,IntegerWide b) on cpu {
 IntegerWide value=range_add(a,b);u64 sign=a.high>>63u64;
 return IntegerWideChecked(sign!=(b.high>>63u64)||(value.high>>63u64)==sign ? 1 : 0,value);
}
extern "C" IntegerWideChecked integer_wide_checked_multiply(IntegerWide a,IntegerWide b) on cpu {
 u64 sa=a.high>>63u64;u64 sb=b.high>>63u64;
 IntegerWide x=sa!=0u64 ? range_negate(a) : a;IntegerWide y=sb!=0u64 ? range_negate(b) : b;
 IntegerWide p=range_product(x.low,y.low);IntegerWide first=range_product(x.low,y.high);
 IntegerWide second=range_product(x.high,y.low);IntegerWide last=range_product(x.high,y.high);
 u64 high=p.high+first.low;u32 overflow=high<p.high ? 1 : 0;
 u64 merged=high+second.low;if(merged<high||first.high!=0u64||second.high!=0u64||last.low!=0u64||last.high!=0u64){overflow=1;}
 IntegerWide magnitude=IntegerWide(p.low,merged);
 IntegerWide limit=sa!=sb ? IntegerWide(0u64,9223372036854775808u64) : IntegerWide(18446744073709551615u64,9223372036854775807u64);
 if(integer_wide_unsigned_less(limit,magnitude)){overflow=1;}
 return IntegerWideChecked(overflow==0 ? 1 : 0,sa!=sb ? range_negate(magnitude) : magnitude);
}
// Callers supply the nonzero magnitude of the original affine scale.
extern "C" IntegerWideDivision integer_wide_unsigned_divide(IntegerWide numerator,IntegerWide denominator) on cpu {
 if(numerator.high==0u64&&denominator.high==0u64&&denominator.low!=0u64){
  u64 quotient=numerator.low/denominator.low;
  return IntegerWideDivision(IntegerWide(quotient,0u64),IntegerWide(numerator.low-quotient*denominator.low,0u64));
 }
 IntegerWide quotient=IntegerWide(0u64,0u64);IntegerWide remainder=IntegerWide(0u64,0u64);u32 at=128;
 while(at!=0){at-=1;u64 bit=at>=64 ? ((numerator.high>>u64(at-64))&1u64) : ((numerator.low>>u64(at))&1u64);
  u64 carry=remainder.high>>63u64;remainder=IntegerWide((remainder.low<<1u64)|bit,(remainder.high<<1u64)|(remainder.low>>63u64));
  if(carry!=0u64||!integer_wide_unsigned_less(remainder,denominator)){
   remainder=range_subtract(remainder,denominator);if(at>=64){quotient.high|=1u64<<u64(at-64);}else{quotient.low|=1u64<<u64(at);}
  }
 }return IntegerWideDivision(quotient,remainder);
}

bool range_set_less(BitRange a,BitRange b) on cpu {
    return a.lower<b.lower||(a.lower==b.lower&&a.upper<b.upper);
}
bool range_set_joined(BitRange a,BitRange b) on cpu {
    return b.lower<=a.upper||(a.upper!=18446744073709551615u64&&b.lower==a.upper+1u64);
}
RangeSetNext range_set_accept(RangeSetNext pending,BitRange value,address output,cfn<void,address,BitRange> push) on cpu
    requires nonzero(push) {
    if(pending.present==0){return RangeSetNext(value,1);}
    if(range_set_joined(pending.value,value)){
        u64 upper=value.upper>pending.value.upper?value.upper:pending.value.upper;
        return RangeSetNext(BitRange(pending.value.lower,upper),1);
    }
    push(output,pending.value);return RangeSetNext(value,1);
}
void range_set_sift(address output,u64 root,u64 count,RangeSetOutput host) on cpu
    requires nonzero(host.get) requires nonzero(host.set) {
    u64 current=root;
    while(current<count/2u64){
        u64 child=current+current+1u64;
        if(child+1u64<count&&range_set_less(host.get(output,child),host.get(output,child+1u64))){child+=1u64;}
        BitRange a=host.get(output,current);BitRange b=host.get(output,child);
        if(!range_set_less(a,b)){return;}
        host.set(output,current,b);host.set(output,child,a);current=child;
    }
}
extern "C" u64 range_set_normalize(address output,u64 count,RangeSetOutput host) on cpu
    requires nonzero(host.get) requires nonzero(host.set) {
    u64 used=0u64;
    u64 range_index_0=0u64;while(range_index_0<count){
        BitRange value=host.get(output,range_index_0);
        if(value.lower<=value.upper){host.set(output,used,value);used+=1u64;}
    range_index_0+=1u64;}
    u64 root=used/2u64;
    while(root!=0u64){root-=1u64;range_set_sift(output,root,used,host);}
    u64 end=used;
    while(end>1u64){
        end-=1u64;BitRange a=host.get(output,0u64);BitRange b=host.get(output,end);
        host.set(output,0u64,b);host.set(output,end,a);range_set_sift(output,0u64,end,host);
    }
    if(used==0u64){return 0u64;}
    BitRange pending=host.get(output,0u64);u64 result=0u64;
    u64 range_index_1=1u64;while(range_index_1<used){
        BitRange value=host.get(output,range_index_1);
        if(range_set_joined(pending,value)){
            if(value.upper>pending.upper){pending.upper=value.upper;}
        }else{host.set(output,result,pending);result+=1u64;pending=value;}
    range_index_1+=1u64;}
    host.set(output,result,pending);return result+1u64;
}
extern "C" void range_set_sorted(address input,cfn<RangeSetNext,address> pull,address output,cfn<void,address,BitRange> push) on cpu
    requires nonzero(pull) requires nonzero(push) {
    RangeSetNext pending=RangeSetNext(BitRange(0u64,0u64),0);
    while(true){
        RangeSetNext value=pull(input);if(value.present==0){break;}
        pending=range_set_accept(pending,value.value,output,push);
    }
    if(pending.present!=0){push(output,pending.value);}
}
extern "C" void range_set_full(address output,cfn<void,address,BitRange> push) on cpu requires nonzero(push) {
    push(output,BitRange(0u64,18446744073709551615u64));
}
extern "C" void range_set_interval(address output,cfn<void,address,BitRange> push,u64 lower,u64 upper) on cpu requires nonzero(push) {
    if(lower<=upper){push(output,BitRange(lower,upper));}
}
extern "C" BitRange range_set_hull(address input,u64 count,cfn<BitRange,address,u32,u64> get) on cpu requires nonzero(get) {
    if(count==0u64){return BitRange(1u64,0u64);}
    return BitRange(get(input,0,0u64).lower,get(input,0,count-1u64).upper);
}
extern "C" bool range_set_contains(address input,u64 count,cfn<BitRange,address,u32,u64> get,u64 value) on cpu requires nonzero(get) {
    u64 range_index_2=0u64;while(range_index_2<count){BitRange range=get(input,0,range_index_2);if(range.lower<=value&&value<=range.upper){return true;}range_index_2+=1u64;}
    return false;
}
extern "C" void range_set_union(address input,u64 left,u64 right,cfn<BitRange,address,u32,u64> get,address output,cfn<void,address,BitRange> push) on cpu
    requires nonzero(get) requires nonzero(push) {
    u64 i=0u64;u64 j=0u64;RangeSetNext pending=RangeSetNext(BitRange(0u64,0u64),0);
    while(i<left||j<right){
        BitRange value=BitRange(0u64,0u64);
        if(j==right){value=get(input,0,i);i+=1u64;}
        else if(i==left){value=get(input,1,j);j+=1u64;}
        else{
            BitRange a=get(input,0,i);BitRange b=get(input,1,j);
            if(!range_set_less(b,a)){value=a;i+=1u64;}else{value=b;j+=1u64;}
        }
        pending=range_set_accept(pending,value,output,push);
    }
    if(pending.present!=0){push(output,pending.value);}
}
extern "C" void range_set_intersection(address input,u64 left,u64 right,cfn<BitRange,address,u32,u64> get,
    address output,cfn<void,address,BitRange> push) on cpu
    requires nonzero(get) requires nonzero(push) {
    if(left==0u64 || right==0u64){return;}
    u64 i=0u64;
    u64 j=0u64;
    BitRange a=get(input,0,i);
    BitRange b=get(input,1,j);
    while(true){
        u64 lower=max(a.lower,b.lower);
        u64 upper=min(a.upper,b.upper);
        if(lower<=upper){push(output,BitRange(lower,upper));}
        bool move_left=a.upper<=b.upper;
        bool move_right=b.upper<=a.upper;
        i+=move_left ? 1u64 : 0u64;
        j+=move_right ? 1u64 : 0u64;
        if(i==left || j==right){return;}
        if(move_left){a=get(input,0,i);}
        if(move_right){b=get(input,1,j);}
    }
}

struct RangeSetCursor { u64 index; RangeSetNext pending; };

RangeSetNext range_set_next(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    read RangeSetProjection projection,inout RangeSetCursor cursor) on cpu requires nonzero(get) {
    while(cursor.index<count){
        BitRange range=get(input,0,cursor.index);
        cursor.index+=1u64;
        range.lower=max(range.lower,projection.lower);
        range.upper=min(range.upper,projection.upper);
        if(range.lower<=range.upper){
            if(cursor.pending.present==0){
                cursor.pending=RangeSetNext(range,1);
            }else if(range_set_joined(cursor.pending.value,range)){
                cursor.pending.value.upper=max(cursor.pending.value.upper,range.upper);
            }else{
                RangeSetNext previous=cursor.pending;
                cursor.pending=RangeSetNext(range,1);
                return previous;
            }
        }
    }
    RangeSetNext final_range=cursor.pending;
    cursor.pending.present=0;
    return final_range;
}

IntegerRangePieces range_set_excluding(BitRange range,u64 excluded,u32 exclude) on cpu {
    if(exclude==0||excluded<range.lower||excluded>range.upper){
        return range_single(range.lower,range.upper);
    }
    if(range.lower<excluded){
        if(excluded<range.upper){
            return IntegerRangePieces(BitRange(range.lower,excluded-1u64),BitRange(excluded+1u64,range.upper),2);
        }
        return range_single(range.lower,excluded-1u64);
    }
    if(excluded<range.upper){return range_single(excluded+1u64,range.upper);}
    return IntegerRangePieces(range,range,0);
}

void range_set_publish(BitRange range,address output,cfn<void,address,BitRange> push,u64 excluded,u32 exclude) on cpu
    requires nonzero(push) {
    IntegerRangePieces pieces=range_set_excluding(range,excluded,exclude);
    if(pieces.count!=0){push(output,pieces.first);}
    if(pieces.count>1){push(output,pieces.second);}
}

extern "C" void range_set_project(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,cfn<void,address,BitRange> push,RangeSetProjection projection) on cpu
    requires nonzero(get) requires nonzero(push) {
    RangeSetCursor cursor=RangeSetCursor(0u64,RangeSetNext(BitRange(0u64,0u64),0));
    while(true){
        RangeSetNext next=range_set_next(input,count,get,projection,cursor);
        if(next.present==0){return;}
        range_set_publish(next.value,output,push,projection.excluded,projection.exclude);
    }
}

extern "C" bool range_set_project_contains(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    address output,cfn<void,address,BitRange> push,RangeSetProjection projection,u64 point) on cpu
    requires nonzero(get) requires nonzero(push) {
    RangeSetCursor cursor=RangeSetCursor(0u64,RangeSetNext(BitRange(0u64,0u64),0));
    bool admits_point=projection.exclude==0 || projection.excluded!=point;
    bool contains=false;
    while(true){
        RangeSetNext next=range_set_next(input,count,get,projection,cursor);
        if(next.present==0){return contains;}
        if(admits_point && next.value.lower<=point && point<=next.value.upper){contains=true;}
        range_set_publish(next.value,output,push,projection.excluded,projection.exclude);
    }
}

extern "C" BitRange range_set_projected_hull(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    RangeSetProjection projection) on cpu requires nonzero(get) {
    RangeSetCursor cursor=RangeSetCursor(0u64,RangeSetNext(BitRange(0u64,0u64),0));
    BitRange hull=BitRange(1u64,0u64);
    bool present=false;
    while(true){
        RangeSetNext next=range_set_next(input,count,get,projection,cursor);
        if(next.present==0){return hull;}
        IntegerRangePieces pieces=range_set_excluding(next.value,projection.excluded,projection.exclude);
        if(pieces.count==0){continue;}
        if(!present){
            hull.lower=pieces.first.lower;
            present=true;
        }
        hull.upper=pieces.count>1?pieces.second.upper:pieces.first.upper;
    }
}

extern "C" bool range_set_covers_projected(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    RangeSetProjection projection,RangeSetRequirement requirement) on cpu
    requires nonzero(get) requires nonzero(requirement.interval) {
    RangeSetCursor cursor=RangeSetCursor(0u64,RangeSetNext(BitRange(0u64,0u64),0));
    while(true){
        RangeSetNext next=range_set_next(input,count,get,projection,cursor);
        if(next.present==0){return true;}
        IntegerRangePieces pieces=range_set_excluding(next.value,projection.excluded,projection.exclude);
        if(pieces.count==0){continue;}
        RangeQuery query=RangeQuery(requirement.order_bias,pieces.first,requirement.known,requirement.required);
        if(!covers_range(query,requirement.intervals,requirement.count,requirement.interval)){return false;}
        if(pieces.count>1){
            query.range=pieces.second;
            if(!covers_range(query,requirement.intervals,requirement.count,requirement.interval)){return false;}
        }
    }
}

extern "C" void range_set_restrict(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,cfn<void,address,BitRange> push,u64 lower,u64 upper) on cpu
    requires nonzero(get) requires nonzero(push) {
    range_set_project(input,count,get,output,push,RangeSetProjection(lower,upper,0u64,0));
}

extern "C" void range_set_without(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,cfn<void,address,BitRange> push,u64 value) on cpu
    requires nonzero(get) requires nonzero(push) {
    u64 index=0u64;
    while(index<count){
        range_set_publish(get(input,0,index),output,push,value,1);
        index+=1u64;
    }
}
u64 range_set_parts(address output,RangeSetOutput host,IntegerRangePieces pieces) on cpu requires nonzero(host.push) {
    u64 count=0u64;
    if(pieces.count!=0){host.push(output,pieces.first);count+=1u64;}
    if(pieces.count>1){host.push(output,pieces.second);count+=1u64;}
    return count;
}
u64 range_set_cast_piece(address output,RangeSetOutput host,RangeSetCastInfo query,BitRange range) on cpu requires nonzero(host.push) {
    if(query.same!=0){host.push(output,range);return 1u64;}
    return range_set_parts(output,host,integer_range_cast(query.source_width,query.source_signed,query.target_width,query.target_bias,range));
}
extern "C" u64 range_set_cast(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,RangeSetOutput host,RangeSetCastInfo query) on cpu
    requires nonzero(get) requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push) {
    u64 result=0u64;
    u64 range_index_5=0u64;while(range_index_5<count){
        BitRange range=get(input,0,range_index_5);range.upper=range.upper<query.source_mask?range.upper:query.source_mask;
        if(range.lower<=range.upper){
            if(query.source_integer!=0&&query.nonzero!=0&&range.lower<=query.source_bias&&query.source_bias<=range.upper){
                if(range.lower<query.source_bias){result+=range_set_cast_piece(output,host,query,BitRange(range.lower,query.source_bias-1u64));}
                if(query.source_bias<range.upper){result+=range_set_cast_piece(output,host,query,BitRange(query.source_bias+1u64,range.upper));}
            }else{result+=range_set_cast_piece(output,host,query,range);}
        }
    range_index_5+=1u64;}
    if(query.same!=0){return result;}
    return range_set_normalize(output,result,host);
}
extern "C" u64 range_set_counts(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,RangeSetOutput host,u32 width,u64 bias) on cpu
    requires nonzero(get) requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push) {
    u64 result=0u64;
    u64 range_index_6=0u64;while(range_index_6<count){result+=range_set_parts(output,host,integer_range_counts(width,bias,get(input,0,range_index_6)));range_index_6+=1u64;}
    return range_set_normalize(output,result,host);
}
extern "C" u64 range_set_apply(address input,u64 left,u64 right,cfn<BitRange,address,u32,u64> get,address operation,cfn<u64,address,BitRange,BitRange,address> invoke,address output,RangeSetOutput host,u32 combine) on cpu
    requires nonzero(get) requires nonzero(invoke) requires nonzero(host.get) requires nonzero(host.set) {
    u64 count=0u64;
    u64 range_index_7=0u64;while(range_index_7<left){
        BitRange a=get(input,0,range_index_7);
        if(combine==0){count=invoke(operation,a,BitRange(0u64,0u64),output);}
        else{u64 range_index_8=0u64;while(range_index_8<right){count=invoke(operation,a,get(input,1,range_index_8),output);range_index_8+=1u64;}}
    range_index_7+=1u64;}
    return range_set_normalize(output,count,host);
}
