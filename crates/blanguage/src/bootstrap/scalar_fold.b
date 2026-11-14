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
