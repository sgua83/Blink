module bootstrap.scalar_fold;
import bootstrap.fact_key;
import codec.scalar_ranges;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap, u32 flags, address data, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";
extern "C" void RtlMoveMemory(address output, address input, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" void abort() on cpu from "ucrtbase.dll";

struct ProofRangeHeap { u64 capacity; address records; };

address proof_range_address(address base, u64 bytes) on cpu {
    return unsafe_address(address_bits(base) + bytes);
}

address proof_range_payload(address storage) on cpu {
    return proof_range_address(storage, size_of<u64>());
}

address proof_range_records(address storage) on cpu {
    address payload = proof_range_payload(storage);
    if (unsafe_position<u64>(storage) <= 1u64) { return payload; }
    return unsafe_position<ProofRangeHeap>(payload).records;
}

extern "C" BitRange proof_range_record(address records, u64 index) on cpu {
    return unsafe_position<BitRange>(proof_range_address(records, index * size_of<BitRange>()));
}

extern "C" BitRange proof_range_side(address sides, u32 side, u64 index) on cpu {
    address records = unsafe_position<address>(proof_range_address(sides, u64(side) * size_of<address>()));
    return proof_range_record(records, index);
}

extern "C" BitRange proof_range_get(address storage, u64 index) on cpu {
    return proof_range_record(proof_range_records(storage), index);
}

extern "C" void proof_range_set(address storage, u64 index, BitRange value) on cpu {
    unsafe_position<BitRange>(proof_range_address(proof_range_records(storage),
        index * size_of<BitRange>())) = value;
}

extern "C" void proof_range_push(address storage, BitRange value) on cpu {
    u64 count = unsafe_position<u64>(storage);
    address payload = proof_range_payload(storage);
    if (count == 0u64) {
        unsafe_position<BitRange>(payload) = value;
        unsafe_position<u64>(storage) = 1u64;
        return;
    }

    ProofRangeHeap heap;
    if (count == 1u64) {
        BitRange first = unsafe_position<BitRange>(payload);
        address records = HeapAlloc(GetProcessHeap(), 0, 4u64 * size_of<BitRange>());
        if (records == address()) { abort(); return; }
        unsafe_position<BitRange>(records) = first;
        heap = ProofRangeHeap(4u64, records);
        unsafe_position<ProofRangeHeap>(payload) = heap;
    } else {
        heap = unsafe_position<ProofRangeHeap>(payload);
        if (count == heap.capacity) {
            u64 limit = 9223372036854775807u64 / size_of<BitRange>();
            if (count == limit) { abort(); return; }
            u64 capacity = count + min(count, limit - count);
            address records = HeapReAlloc(GetProcessHeap(), 0, heap.records, capacity * size_of<BitRange>());
            if (records == address()) { abort(); return; }
            heap = ProofRangeHeap(capacity, records);
            unsafe_position<ProofRangeHeap>(payload) = heap;
        }
    }
    unsafe_position<BitRange>(proof_range_address(heap.records, count * size_of<BitRange>())) = value;
    unsafe_position<u64>(storage) = count + 1u64;
}

extern "C" void proof_range_push_ends(address storage, u64 lower, u64 upper) on cpu {
    proof_range_push(storage, BitRange(lower, upper));
}

extern "C" void proof_range_truncate(address storage, u64 count) on cpu {
    u64 length = unsafe_position<u64>(storage);
    if (count >= length) { return; }
    if (count <= 1u64 && length > 1u64) {
        address payload = proof_range_payload(storage);
        address records = unsafe_position<ProofRangeHeap>(payload).records;
        if (count == 1u64) { unsafe_position<BitRange>(payload) = unsafe_position<BitRange>(records); }
        HeapFree(GetProcessHeap(), 0, records);
    }
    unsafe_position<u64>(storage) = count;
}

extern "C" void proof_range_clone(address storage, address output) on cpu {
    u64 count = unsafe_position<u64>(storage);
    address records = proof_range_records(storage);
    if (count > 1u64) {
        u64 bytes = count * size_of<BitRange>();
        address copy = HeapAlloc(GetProcessHeap(), 0, bytes);
        if (copy == address()) { abort(); return; }
        RtlMoveMemory(copy, records, bytes);
        unsafe_position<ProofRangeHeap>(proof_range_payload(output)) = ProofRangeHeap(count, copy);
    } else if (count == 1u64) {
        unsafe_position<BitRange>(proof_range_payload(output)) = unsafe_position<BitRange>(records);
    }
    unsafe_position<u64>(output) = count;
}

extern "C" void proof_range_drop(address storage) on cpu {
    proof_range_truncate(storage, 0u64);
}

extern "C" u64 proof_range_append(address storage, address source) on cpu {
    u64 count = unsafe_position<u64>(source);
    address records = proof_range_records(source);
    u64 index = 0u64;
    while (index < count) {
        proof_range_push(storage, proof_range_record(records, index));
        index += 1u64;
    }
    return unsafe_position<u64>(storage);
}

extern "C" void proof_range_collect(address input, cfn<RangeSetNext,address> pull, address storage) on cpu
    requires nonzero(pull) {
    while (true) {
        RangeSetNext next = pull(input);
        if (next.present == 0) { break; }
        proof_range_push(storage, next.value);
    }
    RangeSetOutput host = RangeSetOutput(c_function(proof_range_get), c_function(proof_range_set),
        c_function(proof_range_push));
    u64 count = range_set_normalize(storage, unsafe_position<u64>(storage), host);
    proof_range_truncate(storage, count);
}

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

address cast_path_scalar(address path, u64 index) on cpu {
    return unsafe_address(address_bits(path) + index);
}

u8 unsigned_cast_scalar(address scalar) on cpu {
    BuiltinScalar tag = fact_scalar_tag(scalar);
    if (tag == BuiltinScalar::I8) { return u8(u32(BuiltinScalar::U8)); }
    if (tag == BuiltinScalar::I16) { return u8(u32(BuiltinScalar::U16)); }
    if (tag == BuiltinScalar::I32) { return u8(u32(BuiltinScalar::U32)); }
    if (tag == BuiltinScalar::I64) { return u8(u32(BuiltinScalar::U64)); }
    return unsafe_position<u8>(scalar);
}

bool cast_common_range(address domain, DomainCastView view, address path, u64 count) on cpu {
    FactScalarView source = inspect_fact_scalar(path);
    u64 at = 1u64;
    if (source.kind > 1) {
        while (at < count) {
            if (unsafe_position<u8>(path) != unsafe_position<u8>(cast_path_scalar(path, at))) { return false; }
            at += 1u64;
        }
        return true;
    }

    u32 low_width = inspect_fact_scalar(view.low).width;
    u32 extended_width = inspect_fact_scalar(view.extended).width;
    u8 low = unsafe_position<u8>(view.low);
    u8 extended = unsafe_position<u8>(view.extended);
    while (at < count) {
        address target_scalar = cast_path_scalar(path, at);
        FactScalarView target = inspect_fact_scalar(target_scalar);
        if (target.kind > 1) { return false; }
        if (target.width <= low_width) {
            low = unsafe_position<u8>(target_scalar);
            low_width = target.width;
            extended = unsigned_cast_scalar(target_scalar);
            extended_width = target.width;
        } else if (target.width < extended_width) {
            extended = unsigned_cast_scalar(target_scalar);
            extended_width = target.width;
        } else if (source.kind == 1 && source.width == extended_width) {
            if (source.width == low_width) {
                low = unsafe_position<u8>(cast_path_scalar(path, at - 1u64));
                low_width = source.width;
            }
            extended = unsigned_cast_scalar(target_scalar);
            extended_width = target.width;
        }
        source = target;
        at += 1u64;
    }
    write_common_range_cast(domain, low, extended, unsafe_position<u8>(cast_path_scalar(path, count - 1u64)));
    return true;
}

bool cast_domain_keeps_identity(DomainKind kind) on cpu {
    return kind == DomainKind::Data || kind == DomainKind::Fixed || kind == DomainKind::Tile
        || kind == DomainKind::View || kind == DomainKind::ApplicationData || kind == DomainKind::Upper
        || kind == DomainKind::Lower || kind == DomainKind::Room || kind == DomainKind::Shifted
        || kind == DomainKind::Floating;
}

extern "C" bool cast_index_domain(address domain, address path, u64 count) on cpu {
    DomainCastView view = inspect_domain_cast(domain);
    if (view.kind == DomainKind::CommonRange) { return cast_common_range(domain, view, path, count); }
    if (view.kind == DomainKind::KnownBits) {
        // The first target normalizes the original bit pattern before the first cast edge.
        BitPattern pattern = known_cast_path(view.low, path, count, 1u64, BitPattern(view.mask, view.value));
        if (pattern.mask == 0u64) { return false; }
        write_known_bits_cast(domain, unsafe_position<u8>(cast_path_scalar(path, count - 1u64)), pattern.mask, pattern.value);
        return true;
    }

    bool keep_identity = cast_domain_keeps_identity(view.kind);
    FactScalarView source = inspect_fact_scalar(path);
    u64 at = 1u64;
    while (at < count) {
        FactScalarView target = inspect_fact_scalar(cast_path_scalar(path, at));
        bool same = keep_identity && source.kind == target.kind && source.width == target.width;
        bool widened = source.kind == 0 && target.kind == 0 && target.width > source.width;
        if (!same && !widened) { return false; }
        source = target;
        at += 1u64;
    }
    return true;
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

extern "C" bool float_ranges_require_preserve(u32 width, RangeInput values, BitRange clip) on cpu
    requires nonzero(values.get) {
    u64 first = fp_raw(width, fp_normal(width));
    u64 last = fp_raw(width, fp_maximum(width));
    u64 sign = fp_sign(width);
    BitRange negative = BitRange(float_order(width, last | sign), float_order(width, first | sign));
    BitRange positive = BitRange(float_order(width, first), float_order(width, last));
    u64 zero = float_order(width, 0u64);
    u64 index = 0u64;
    while (index < values.count) {
        BitRange range = values.get(values.values, index);
        index += 1u64;
        range.lower = max(range.lower, clip.lower);
        range.upper = min(range.upper, clip.upper);
        if (range.lower > range.upper) { continue; }
        if (negative.lower <= range.lower && range.upper <= negative.upper) { continue; }
        if (range.lower == zero && range.upper == zero) { continue; }
        if (positive.lower <= range.lower && range.upper <= positive.upper) { continue; }
        return true;
    }
    return false;
}
FloatInterval float_finite(u32 width,BitRange range) on cpu {
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
FloatRange float_cast_range(ScalarEncoding source, ScalarEncoding target, BitRange range,
    address origin, address context, cfn<FloatCastRelation,address> next, cfn<void,address,FloatCastFact> emit) on cpu
    requires nonzero(next) requires nonzero(emit) {
    bool integer = source.kind == ScalarKind::Unsigned || source.kind == ScalarKind::Signed;
    if (integer && target.kind == ScalarKind::Float) {
        if (range.lower > range.upper) { return fp_empty(); }
        if (source.kind == ScalarKind::Unsigned && target.width == 32) {
            emit(context, FloatCastFact(origin, range, 0.0f64, 0.0f64, 0));
            FloatCastRelation relation = next(context);
            if (relation.kind != FloatCastRelationKind::End) {
                f64 factor = 1.0f64 + float_conversion_error(range.upper);
                u64 maximum = truncate(0xffffffffffffffffu64, source.width);
                while (relation.kind != FloatCastRelationKind::End) {
                    if (relation.kind == FloatCastRelationKind::Upper
                        || (relation.encoding.kind == source.kind && relation.encoding.width == source.width)) {
                        BitRange valid = BitRange(u64(relation.distance), maximum);
                        f64 offset = float_multiply_up(-f64(relation.distance), factor);
                        emit(context, FloatCastFact(relation.origin, valid, factor, offset, 1));
                    }
                    relation = next(context);
                }
            }
        }
        return float_interval(target.width, fp_integer_endpoint(source, target.width, range.lower, 1),
            fp_integer_endpoint(source, target.width, range.upper, 0));
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
f64 float_add_up(f64 left,f64 right) on cpu {return float_sum_bound(64,left,right,0);}
f64 float_multiply_up(f64 left,f64 right) on cpu {f64 product=left*right;return left==0.0f64||right==0.0f64 ? product : fp_next(product,1);}
f64 float_divide_up(f64 left,f64 right) on cpu {f64 quotient=left/right;return left==0.0f64 ? quotient : fp_next(quotient,1);}
f64 fp_min_four(f64 a,f64 b,f64 c,f64 d) on cpu {return fp_min(fp_min(fp_min(a,b),c),d);}
f64 fp_max_four(f64 a,f64 b,f64 c,f64 d) on cpu {return fp_max(fp_max(fp_max(a,b),c),d);}
FloatRange float_interval_arithmetic(BinaryOperation operation, u32 width, FloatInterval a, FloatInterval b) on cpu {
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
bool texture_coordinates_bounded(BitRange range) on cpu {
    FloatInterval coordinates = float_finite(32, range);
    return coordinates.present != 0 && coordinates.lower >= -1.0f64 && coordinates.upper <= 1.0f64;
}

extern "C" FloatRange texture_sample_range(TexelFormat format, TextureSampleBounds input) on cpu {
    if (format != TexelFormat::Rgba8Unorm && format != TexelFormat::Rgba8Srgb
        && format != TexelFormat::R8Unorm && format != TexelFormat::Rg16Unorm
        && format != TexelFormat::Rgb10A2Unorm) {
        return fp_unknown();
    }
    if (!texture_coordinates_bounded(input.x) || !texture_coordinates_bounded(input.y)
        || (input.has_z != 0 && !texture_coordinates_bounded(input.z))) {
        return fp_unknown();
    }
    FloatInterval lod = float_finite(32, input.lod);
    if (lod.present == 0 || lod.lower < -4294967296.0f64 || lod.upper > 4294967296.0f64) {
        return fp_unknown();
    }
    // 3D plus mip has four linear dimensions. Each nonnegative contribution
    // includes at most 25 rounding factors with allowance 1e-4 per factor.
    // 0x3f805205 is the outward f32 endpoint of the original directed (1+1e-4)^25.
    u64 upper = format == TexelFormat::Rgba8Srgb ? 0x40000000u64 : 0x3f805205u64;
    return FloatRange(float_order(32, 0x80000000u64), float_order(32, upper), 1);
}

extern "C" FloatRange float_negate(u32 width,BitRange range) on cpu {
    FloatInterval input=float_finite(width,range);return input.present!=0 ? float_interval(width,-input.upper,-input.lower) : fp_unknown();
}
f64 evaluate_float_upper(f64 input, f64 scale, f64 offset, f64 reciprocal) on cpu {
    f64 linear = float_add_up(float_multiply_up(scale, input), offset);
    f64 quotient = reciprocal == 0.0f64 ? 0.0f64 : float_divide_up(reciprocal, input);
    return float_add_up(linear, quotient);
}

f64 float_upper_maximum(BitRange inputs, f64 scale, f64 offset, f64 reciprocal) on cpu {
    // For positive d, a*d+b+c/d is convex when c>=0 and increasing when c<0 and a>=0.
    ScalarEncoding encoding = ScalarEncoding(ScalarKind::Unsigned, 64);
    f64 first = evaluate_float_upper(fp_integer_endpoint(encoding, 64, inputs.lower, 1), scale, offset, reciprocal);
    f64 last = evaluate_float_upper(fp_integer_endpoint(encoding, 64, inputs.upper, 0), scale, offset, reciprocal);
    f64 maximum = bitcast_f64(0xfff0000000000000u64);
    if (first > maximum) { maximum = first; }
    if (last > maximum) { maximum = last; }
    return maximum;
}
extern "C" FloatBoundProjection float_bound_at_call(RangeInput input, u64 maximum, BitRange source,
    f64 scale, f64 offset, f64 reciprocal, bool include_numeric) on cpu requires nonzero(input.get) {
    BitRange hull = BitRange(1u64, 0u64);
    u64 index = 0u64;
    while (index < input.count) {
        BitRange interval = input.get(input.values, index);
        if (interval.lower > maximum) { break; }
        if (index == 0u64) { hull.lower = interval.lower; }
        hull.upper = min(interval.upper, maximum);
        if (hull.upper == maximum) { break; }
        index += 1u64;
    }
    BitRange projected = BitRange(max(source.lower, hull.lower), min(source.upper, hull.upper));
    FloatRange range = fp_unknown();
    if (include_numeric && projected.lower <= projected.upper) {
        f64 upper = float_upper_maximum(projected, scale, offset, reciprocal);
        range = float_interval(32, -fp_maximum(32), upper);
    }
    return FloatBoundProjection(projected, range);
}

f64 float_conversion_error(u64 upper) on cpu {return upper<=16777216u64 ? 0.0f64 : fp_power(-23);}

// Form and bounds here describe the original domain's integer source.
FloatArithmeticBound rounded_float_bound(FloatArithmeticBound bound, f64 factor, f64 extra) on cpu {
    return FloatArithmeticBound(bound.origin, bound.scalar, bound.range,
        float_multiply_up(bound.scale, factor), float_add_up(float_multiply_up(bound.offset, factor), extra),
        float_multiply_up(bound.reciprocal, factor), FloatArithmeticForm::Upper);
}

void emit_finite_float_bound(address context, FloatArithmeticHost host, FloatArithmeticBound bound,
    inout FloatRange result) on cpu requires nonzero(host.emit) {
    // The same finite endpoint establishes the fact and narrows the result.
    f64 limit = fp_maximum(32);
    f64 maximum = float_upper_maximum(bound.range, bound.scale, bound.offset, bound.reciprocal);
    if (!fp_finite(maximum) || maximum > limit) { return; }
    host.emit(context, bound);
    FloatRange range = float_interval(32, -limit, maximum);
    if (range.state == 0) { return; }
    if (result.state == 0) {
        result = range;
    } else if (result.state == 1) {
        u64 lower = max(result.lower, range.lower);
        u64 upper = min(result.upper, range.upper);
        result = lower <= upper ? FloatRange(lower, upper, 1) : fp_empty();
    }
}

void float_shifted_domains(address context, FloatArithmeticHost host, u32 side, BinaryOperation op,
    FloatInterval other, inout FloatRange result) on cpu requires nonzero(host.next) requires nonzero(host.emit) {
    FloatArithmeticBound bound = host.next(context, side);
    while (bound.form != FloatArithmeticForm::End) {
        if (op == BinaryOperation::Add) {
            if (bound.form == FloatArithmeticForm::Cast) {
                bound = FloatArithmeticBound(bound.origin, bound.scalar, bound.range,
                    1.0f64 + float_conversion_error(bound.range.upper), 0.0f64, 0.0f64, FloatArithmeticForm::Upper);
            }
            bound = FloatArithmeticBound(bound.origin, bound.scalar, bound.range, bound.scale,
                float_add_up(bound.offset, other.upper), bound.reciprocal, FloatArithmeticForm::Upper);
            emit_finite_float_bound(context, host, rounded_float_bound(bound, 1.0f64 + fp_power(-23), fp_power(-149)), result);
        } else if (bound.form == FloatArithmeticForm::Upper) {
            bound = rounded_float_bound(bound, other.upper, 0.0f64);
            emit_finite_float_bound(context, host, rounded_float_bound(bound, 1.0f64 + fp_power(-23), fp_power(-149)), result);
        }
        bound = host.next(context, side);
    }
}

void float_divided_domains(address context, FloatArithmeticHost host, f64 maximum, inout FloatRange result) on cpu
    requires nonzero(host.rewind) requires nonzero(host.next) requires nonzero(host.same_scalar)
    requires nonzero(host.emit) {
    f64 minimum_positive = 3.0f64 * fp_power(-149);
    FloatArithmeticBound divisor = host.next(context, 1);
    while (divisor.form != FloatArithmeticForm::End) {
        if (divisor.form == FloatArithmeticForm::Cast && divisor.range.lower != 0u64) {
            f64 factor = float_divide_up(1.0f64 + 3.0f64 * fp_power(-23), 1.0f64 - float_conversion_error(divisor.range.upper));
            emit_finite_float_bound(context, host, FloatArithmeticBound(divisor.origin, divisor.scalar, divisor.range,
                0.0f64, minimum_positive, float_multiply_up(maximum, factor), FloatArithmeticForm::Upper), result);
            host.rewind(context, 0);
            FloatArithmeticBound numerator = host.next(context, 0);
            while (numerator.form != FloatArithmeticForm::End) {
                if (numerator.form == FloatArithmeticForm::Upper && numerator.reciprocal == 0.0f64
                    && host.same_scalar(numerator.scalar, divisor.scalar) != 0
                    && order_identity(numerator.origin, divisor.origin) == 0) {
                    BitRange range = BitRange(max(numerator.range.lower, divisor.range.lower), min(numerator.range.upper, divisor.range.upper));
                    if (range.lower <= range.upper) {
                        emit_finite_float_bound(context, host, FloatArithmeticBound(numerator.origin, numerator.scalar, range, 0.0f64,
                            float_add_up(float_multiply_up(numerator.scale, factor), minimum_positive),
                            float_multiply_up(numerator.offset, factor), FloatArithmeticForm::Upper), result);
                    }
                }
                numerator = host.next(context, 0);
            }
        }
        divisor = host.next(context, 1);
    }
}

void float_multiplied_domains(address context, FloatArithmeticHost host, u32 side, inout FloatRange result) on cpu
    requires nonzero(host.rewind) requires nonzero(host.next) requires nonzero(host.same_scalar)
    requires nonzero(host.below) requires nonzero(host.emit) {
    u32 opposite = side ^ 1;
    host.rewind(context, side);
    FloatArithmeticBound left = host.next(context, side);
    while (left.form != FloatArithmeticForm::End) {
        if (left.form == FloatArithmeticForm::Upper && left.reciprocal == 0.0f64) {
            host.rewind(context, opposite);
            FloatArithmeticBound right = host.next(context, opposite);
            while (right.form != FloatArithmeticForm::End) {
                if (right.form == FloatArithmeticForm::Upper && right.scale == 0.0f64 && right.offset >= 0.0f64
                    && host.same_scalar(left.scalar, right.scalar) != 0) {
                    f64 offset = left.offset;
                    BitRange range = BitRange(max(left.range.lower, right.range.lower), min(left.range.upper, right.range.upper));
                    if (order_identity(left.origin, right.origin) != 0) {
                        range = BitRange(1u64, 0u64);
                        if (left.scale >= 0.0f64 && right.reciprocal != 0.0f64
                            && host.below(context, left.origin, right.origin, left.scalar)) {
                            offset = float_add_up(offset, -left.scale);
                            range = right.range;
                        }
                    }
                    if (range.lower <= range.upper) {
                        FloatArithmeticBound product = FloatArithmeticBound(right.origin, right.scalar, range,
                            float_multiply_up(left.scale, right.offset),
                            float_add_up(float_multiply_up(left.scale, right.reciprocal), float_multiply_up(offset, right.offset)),
                            float_multiply_up(offset, right.reciprocal), FloatArithmeticForm::Upper);
                        emit_finite_float_bound(context, host, rounded_float_bound(product, 1.0f64 + fp_power(-23), fp_power(-149)), result);
                    }
                }
                right = host.next(context, opposite);
            }
        }
        left = host.next(context, side);
    }
}

extern "C" FloatRange float_arithmetic_proof(BinaryOperation op, u32 width, BitRange left, BitRange right,
    bool with_domains, address context, FloatArithmeticHost host) on cpu
    requires nonzero(host.rewind) requires nonzero(host.next) requires nonzero(host.same_scalar)
    requires nonzero(host.below) requires nonzero(host.emit) {
    FloatInterval a = float_finite(width, left);
    FloatInterval b = float_finite(width, right);
    FloatRange result = float_interval_arithmetic(op, width, a, b);
    if (!with_domains || a.present == 0 || b.present == 0 || a.lower < 0.0f64 || b.lower < 0.0f64) {
        return result;
    }
    if (op == BinaryOperation::Add) {
        float_shifted_domains(context, host, 0, op, b, result);
        float_shifted_domains(context, host, 1, op, a, result);
    } else if (op == BinaryOperation::Multiply) {
        if (b.lower == b.upper) { float_shifted_domains(context, host, 0, op, b, result); }
        if (a.lower == a.upper) { float_shifted_domains(context, host, 1, op, a, result); }
        float_multiplied_domains(context, host, 0, result);
        float_multiplied_domains(context, host, 1, result);
    } else if (op == BinaryOperation::Divide && b.lower > 0.0f64) {
        float_divided_domains(context, host, a.upper, result);
    }
    return result;
}

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

BitPattern known_cast(u64 source_mask,u64 source_sign,u64 target_mask,BitPattern value) on cpu {
 u64 mask=value.mask&target_mask;u64 bits=value.value&mask;
 if(target_mask>source_mask){u64 extra=target_mask&~source_mask;
  if(source_sign==0u64){mask|=extra;}
  else if((value.mask&source_sign)!=0u64){mask|=extra;if((value.value&source_sign)!=0u64){bits|=extra;}}
 }return BitPattern(mask,bits);
}
extern "C" BitPattern known_cast_path(address source, address targets, u64 count,
    u64 first_required, BitPattern value) on cpu {
    FactScalarView current = inspect_fact_scalar(source);
    BitPattern pattern = value;
    u64 at = 0u64;
    while (at < count) {
        FactScalarView target = inspect_fact_scalar(cast_path_scalar(targets, at));
        if (target.kind > 1) { return BitPattern(0u64, 0u64); }
        pattern = known_cast(current.mask, current.bias, target.mask, pattern);
        if (at >= first_required && pattern.mask == 0u64) { return pattern; }
        current = target;
        at += 1u64;
    }
    return pattern;
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
BitPattern known_interval(u64 mask,u64 bias,u64 lower,u64 upper) on cpu {
    u64 different=lower^upper;
    different|=different>>1u64;different|=different>>2u64;different|=different>>4u64;
    different|=different>>8u64;different|=different>>16u64;different|=different>>32u64;
    u64 fixed=mask&~different;
    return BitPattern(fixed,(lower^bias)&fixed);
}

BitPattern known_ranges(u64 mask,u64 bias,address context,u64 count,cfn<BitRange,address,u64> read) on cpu requires nonzero(read) {
    BitPattern result=BitPattern(0u64,0u64);bool present=false;
    u64 index=0u64;while(index<count){
        BitRange range=read(context,index);index+=1u64;
        u64 upper=min(range.upper,mask);if(range.lower>upper){continue;}
        BitPattern bits=known_interval(mask,bias,range.lower,upper);if(bits.mask==0u64){return bits;}
        if(present){result=common_pattern(result,bits);}else{result=bits;present=true;}
        if(result.mask==0u64){return result;}
    }
    return result;
}

KnownProjection merge_known_domains(u64 mask,BitPattern initial,address domains,cfn<KnownDomain,address> next_domain) on cpu requires nonzero(next_domain) {
    BitPattern bits=initial;
    KnownDomain domain=next_domain(domains);
    u32 have_domain=domain.present;
    while(domain.present!=0){
        BitPattern incoming=known_cast(domain.source_mask,domain.source_sign,mask,domain.pattern);
        bits=BitPattern(bits.mask|incoming.mask,bits.value|incoming.value);
        domain=next_domain(domains);
    }
    return KnownProjection(bits,u32(have_domain!=0 || bits.mask!=0u64));
}

extern "C" BitPattern known_proof(u64 mask,u64 bias,address ranges,u64 count,
    cfn<BitRange,address,u64> read,address domains,cfn<KnownDomain,address> next_domain) on cpu
    requires nonzero(read) requires nonzero(next_domain) {
    BitPattern bits=known_ranges(mask,bias,ranges,count,read);
    return merge_known_domains(mask,bits,domains,next_domain).pattern;
}

extern "C" KnownProjection known_facts(u64 mask,u64 bias,address context,u64 count,
    cfn<KnownBounds,address,u64> read_bound,cfn<KnownDomain,address> next_domain) on cpu
    requires nonzero(read_bound) requires nonzero(next_domain) {
    u64 lower=0u64;u64 upper=mask;
    u64 index=0u64;
    while(index<count){
        KnownBounds bounds=read_bound(context,index);
        lower=max(lower,bounds.range.lower);
        upper=min(upper,bounds.range.upper);
        if(bounds.zero!=0){lower=max(lower,bias);upper=min(upper,bias);}
        index+=1u64;
    }
    BitPattern bits=BitPattern(0u64,0u64);
    if(lower<=upper){bits=known_interval(mask,bias,lower,upper);}
    return merge_known_domains(mask,bits,context,next_domain);
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
extern "C" CallIndexBinding bind_call_index(address context,address identity,CallIndexAffine affine,
    cfn<CallIndexInput,address,address> lookup) on cpu requires nonzero(lookup) {
    CallIndexInput input = lookup(context,identity);
    let symbolic = CallIndexBinding(input.has_identity != 0 ? CallIndexKind::Identity : CallIndexKind::Unbound,0u64);
    if (affine.present == 0 && input.has_identity != 0) { return symbolic; }
    if (input.count != 1u64 || input.lower != input.upper) { return symbolic; }
    if (affine.present == 0) { return CallIndexBinding(CallIndexKind::Constant,input.lower); }

    IntegerWideChecked product = integer_wide_checked_multiply(range_unsigned(input.lower),affine.scale);
    if (product.present == 0) { return CallIndexBinding(CallIndexKind::Unbound,0u64); }
    IntegerWideChecked value = integer_wide_checked_add(product.value,affine.offset);
    if (value.present == 0 || value.value.high != 0u64 || value.value.low > 4294967295u64) {
        return CallIndexBinding(CallIndexKind::Unbound,0u64);
    }
    return CallIndexBinding(CallIndexKind::Constant,value.value.low);
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

extern "C" u64 range_set_normalize(address output,u64 count,RangeSetOutput host) on cpu
    requires nonzero(host.get) requires nonzero(host.set) {
    u64 used=0u64;
    u64 range_index_0=0u64;while(range_index_0<count){
        BitRange value=host.get(output,range_index_0);
        if(value.lower<=value.upper){host.set(output,used,value);used+=1u64;}
    range_index_0+=1u64;}
    sort_ranges(output,used,host.get,host.set);
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
u64 range_set_parts(address output,cfn<void,address,BitRange> push,IntegerRangePieces pieces) on cpu requires nonzero(push) {
    if(pieces.count!=0){push(output,pieces.first);}
    if(pieces.count>1){push(output,pieces.second);}
    return u64(pieces.count);
}
RangeSetCastStep range_set_cast_step(RangeSetCastInfo query) on cpu {
    return RangeSetCastStep(query.source_width,query.source_signed,query.target_width,query.same,query.target_bias);
}

RangeSetProjection range_set_cast_projection(RangeSetCastInfo query) on cpu {
    return RangeSetProjection(0u64, query.source_mask, query.source_bias,
        query.source_integer!=0 ? query.nonzero : 0);
}

IntegerRangePieces range_set_cast_input(read RangeSetProjection projection,BitRange input) on cpu {
    BitRange clipped=BitRange(max(input.lower,projection.lower),min(input.upper,projection.upper));
    if(clipped.lower>clipped.upper){return IntegerRangePieces(clipped,clipped,0);}
    return range_set_excluding(clipped,projection.excluded,projection.exclude);
}

IntegerRangePieces range_set_cast_value(RangeSetCastStep step,BitRange range) on cpu {
    if(step.same!=0){return range_single(range.lower,range.upper);}
    return integer_range_cast(step.source_width,step.source_signed,step.target_width,step.target_bias,range);
}

BitRange range_set_cast_part(IntegerRangePieces pieces,u32 part) on cpu {
    return part==0 ? pieces.first : pieces.second;
}

u64 range_set_cast_emit(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    address output,cfn<void,address,BitRange> push,RangeSetCastStep step,read RangeSetProjection projection) on cpu
    requires nonzero(get) requires nonzero(push) {
    u64 result=0u64;
    u64 index=0u64;
    while(index<count){
        IntegerRangePieces source=range_set_cast_input(projection,get(input,0,index));
        u32 part=0;
        while(part<source.count){
            result+=range_set_parts(output,push,range_set_cast_value(step,range_set_cast_part(source,part)));
            part+=1;
        }
        index+=1u64;
    }
    return result;
}

extern "C" u64 range_set_cast(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,RangeSetOutput host,RangeSetCastInfo query) on cpu
    requires nonzero(get) requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push) {
    u64 result=range_set_cast_emit(input,count,get,output,host.push,range_set_cast_step(query),range_set_cast_projection(query));
    if(query.same!=0){return result;}
    return range_set_normalize(output,result,host);
}

struct CastProofRanges { address output; u64 zero; u32 nonzero; };

extern "C" void push_cast_proof_range(address context, BitRange range) on cpu {
    CastProofRanges result = unsafe_position<CastProofRanges>(context);
    proof_range_push(result.output, range);
    if (result.nonzero != 0 && range.lower <= result.zero && result.zero <= range.upper) {
        result.nonzero = 0;
        unsafe_position<CastProofRanges>(context) = result;
    }
}

extern "C" CastEffect proof_cast_component(ScalarEncoding source, ScalarEncoding target,
    address records, u64 count, address output, u64 constant_bits, u32 has_constant, u32 nonzero,
    address origin, address context,
    cfn<FloatCastRelation,address> next, cfn<void,address,FloatCastFact> emit) on cpu
    requires nonzero(next) requires nonzero(emit) {
    bool source_integer = source.kind == ScalarKind::Unsigned || source.kind == ScalarKind::Signed;
    bool target_integer = target.kind == ScalarKind::Unsigned || target.kind == ScalarKind::Signed;
    u64 bias = target.kind == ScalarKind::Signed ? fp_sign(target.width) : 0u64;
    u64 source_bias = source.kind == ScalarKind::Signed ? fp_sign(source.width) : 0u64;
    u64 mask = truncate(18446744073709551615u64, source.width);
    u64 lower = 0u64;
    u64 upper = mask;
    if (has_constant != 0) {
        lower = source.kind == ScalarKind::Float ? float_order(source.width, constant_bits) : constant_bits ^ source_bias;
        upper = min(lower, mask);
    }
    RangeSetProjection projection = RangeSetProjection(lower, upper, source_bias, source_integer ? nonzero : 0);
    if (source_integer && target_integer) {
        bool widen_unsigned = source.kind == ScalarKind::Unsigned && target.kind == ScalarKind::Unsigned
            && target.width > source.width;
        RangeSetCastStep step = RangeSetCastStep(source.width, u32(source.kind == ScalarKind::Signed), target.width, 0, bias);
        CastProofRanges result = CastProofRanges(output, bias, 1);
        u64 emitted = range_set_cast_emit(address_of(records), count, c_function(proof_range_side),
            address_of(result), c_function(push_cast_proof_range), step, projection);
        RangeSetOutput storage = RangeSetOutput(c_function(proof_range_get), c_function(proof_range_set),
            c_function(proof_range_push));
        proof_range_truncate(output, range_set_normalize(output, emitted, storage));
        CastOrigin identity = widen_unsigned ? CastOrigin::Preserve : CastOrigin::Relate;
        return CastEffect(identity, u32(source.width == 64 && target.width == 64),
            u32(target.width >= source.width), result.nonzero);
    }
    if (source.kind == ScalarKind::Boolean && target_integer) {
        proof_range_push(output, BitRange(bias, bias + 1u64));
    } else {
        BitRange hull = range_set_projected_hull(address_of(records), count, c_function(proof_range_side), projection);
        FloatRange range = float_cast_range(source, target, hull, origin, context, next, emit);
        if (range.state == 1) {
            proof_range_push(output, BitRange(range.lower, range.upper));
        } else if (range.state == 0) {
            proof_range_push(output, BitRange(0u64, 18446744073709551615u64));
        }
    }
    return CastEffect(CastOrigin::Discard, 0, 0, 0);
}

u64 range_set_cast_tail(address output,cfn<void,address,BitRange> push,RangeSetCastStep second,
    RangeSetCastStep last,IntegerRangePieces first) on cpu requires nonzero(push) {
    // Each step receives intervals already bounded by the preceding target encoding.
    u64 result=0u64;
    u32 first_part=0;
    while(first_part<first.count){
        IntegerRangePieces middle=range_set_cast_value(second,range_set_cast_part(first,first_part));
        u32 middle_part=0;
        while(middle_part<middle.count){
            result+=range_set_parts(output,push,range_set_cast_value(last,range_set_cast_part(middle,middle_part)));
            middle_part+=1;
        }
        first_part+=1;
    }
    return result;
}

u64 range_set_cast_chain_emit(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    address output,cfn<void,address,BitRange> push,RangeSetCastInfo first,RangeSetCastStep second,RangeSetCastStep last) on cpu
    requires nonzero(get) requires nonzero(push) {
    RangeSetCastStep step=range_set_cast_step(first);
    RangeSetProjection projection=range_set_cast_projection(first);
    if(second.same!=0 && last.same!=0){return range_set_cast_emit(input,count,get,output,push,step,projection);}
    u64 result=0u64;
    u64 index=0u64;
    while(index<count){
        IntegerRangePieces source=range_set_cast_input(projection,get(input,0,index));
        u32 part=0;
        while(part<source.count){
            IntegerRangePieces first_value=range_set_cast_value(step,range_set_cast_part(source,part));
            result+=range_set_cast_tail(output,push,second,last,first_value);
            part+=1;
        }
        index+=1u64;
    }
    return result;
}

extern "C" u64 range_set_cast_chain(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    address output,RangeSetOutput host,RangeSetCastInfo first,RangeSetCastStep second,RangeSetCastStep last) on cpu
    requires nonzero(get) requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push) {
    u64 result=range_set_cast_chain_emit(input,count,get,output,host.push,first,second,last);
    if(first.same!=0 && second.same!=0 && last.same!=0){return result;}
    return range_set_normalize(output,result,host);
}

struct RangeSetCastObservation { u64 point; RangeSetCastSummary summary; };

extern "C" void observe_cast_range(address output,BitRange piece) on cpu {
    RangeSetCastObservation observation=unsafe_position<RangeSetCastObservation>(output);
    observation.summary.hull.lower=min(observation.summary.hull.lower,piece.lower);
    observation.summary.hull.upper=max(observation.summary.hull.upper,piece.upper);
    if(piece.lower<=observation.point && observation.point<=piece.upper){observation.summary.contains=1;}
    unsafe_position<RangeSetCastObservation>(output)=observation;
}

extern "C" RangeSetCastSummary range_set_cast_summary(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    RangeSetCastInfo first,RangeSetCastStep second,RangeSetCastStep last) on cpu requires nonzero(get) {
    RangeSetCastObservation observation=RangeSetCastObservation(last.target_bias,
        RangeSetCastSummary(BitRange(18446744073709551615u64,0u64),0));
    u64 produced=range_set_cast_chain_emit(input,count,get,address_of(observation),c_function(observe_cast_range),first,second,last);
    if(produced==0u64){observation.summary.hull=BitRange(1u64,0u64);}
    return observation.summary;
}

extern "C" u64 range_set_counts(RangeInput input, address output, RangeSetOutput host, u32 width, u64 bias) on cpu
    requires nonzero(input.get) requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push) {
    u64 result=0u64;
    u64 i=0u64;while(i<input.count){result+=range_set_parts(output,host.push,integer_range_counts(width,bias,input.get(input.values,i)));i+=1u64;}
    return range_set_normalize(output,result,host);
}

BitRange input_hull(RangeInput input) on cpu requires nonzero(input.get) {
    if (input.count == 0u64) { return BitRange(1u64, 0u64); }
    return BitRange(input.get(input.values, 0u64).lower,
        input.get(input.values, input.count - 1u64).upper);
}

bool order_selects_operand(IntegerRangeOperation operation) on cpu {
    return operation == IntegerRangeOperation::Minimum || operation == IntegerRangeOperation::Maximum;
}

bool same_value_preserves_operand(IntegerRangeOperation operation) on cpu {
    return order_selects_operand(operation) || operation == IntegerRangeOperation::BitAnd
        || operation == IntegerRangeOperation::BitOr;
}

bool range_is_value(BitRange range, u64 bias, u64 value) on cpu {
    return range.lower == range.upper && (range.lower ^ bias) == value;
}

bool singleton_shift(RangeInput input, RangeInput counts, IntegerBinaryProof proof) on cpu
    requires nonzero(input.get) requires nonzero(counts.get) {
    if (proof.right_width == proof.width && proof.right_bias == proof.bias) {
        return range_is_value(input_hull(counts), proof.right_bias, 0u64);
    }
    bool found = false;
    u64 i = 0u64;
    while (i < input.count) {
        IntegerRangePieces pieces = integer_range_counts(proof.right_width, proof.right_bias,
            input.get(input.values, i));
        if (pieces.count != 0) {
            if (!range_is_value(pieces.first, proof.right_bias, 0u64)) { return false; }
            if (pieces.count == 2 && !range_is_value(pieces.second, proof.right_bias, 0u64)) {
                return false;
            }
            found = true;
        }
        i += 1u64;
    }
    return found;
}

u32 binary_origin(RangeInput left, RangeInput right, RangeInput counts, IntegerBinaryProof proof) on cpu
    requires nonzero(left.get) requires nonzero(right.get) requires nonzero(counts.get) {
    IntegerRangeOperation operation = proof.operation;
    if (proof.same_value != 0 && same_value_preserves_operand(operation)) { return 1; }
    if (order_selects_operand(operation)) {
        BitRange a = input_hull(left);
        BitRange b = input_hull(right);
        bool minimum = operation == IntegerRangeOperation::Minimum;
        if (a.upper <= b.lower) { return minimum ? 1 : 2; }
        if (b.upper <= a.lower) { return minimum ? 2 : 1; }
        return 0;
    }
    if (operation == IntegerRangeOperation::ShiftLeft || operation == IntegerRangeOperation::ShiftRight) {
        return singleton_shift(right, counts, proof) ? 1 : 0;
    }
    u64 neutral = 0u64;
    if (operation == IntegerRangeOperation::Multiply || operation == IntegerRangeOperation::Divide) {
        neutral = 1u64;
    } else if (operation == IntegerRangeOperation::BitAnd) {
        neutral = proof.mask;
    } else if (operation != IntegerRangeOperation::Add && operation != IntegerRangeOperation::Subtract
        && operation != IntegerRangeOperation::BitOr && operation != IntegerRangeOperation::BitXor) {
        return 0;
    }
    bool commutes = operation != IntegerRangeOperation::Subtract && operation != IntegerRangeOperation::Divide;
    if (range_is_value(input_hull(right), proof.right_bias, neutral)) { return 1; }
    if (commutes && range_is_value(input_hull(left), proof.bias, neutral)) { return 2; }
    return 0;
}

IntegerRangePieces binary_range_pair(IntegerBinaryProof proof, BitRange left, BitRange right) on cpu {
    IntegerRangeOperation op = proof.operation;
    BitRange empty = BitRange(1u64, 0u64);
    if (op == IntegerRangeOperation::BitAnd || op == IntegerRangeOperation::BitOr
        || op == IntegerRangeOperation::BitXor) {
        IntegerBitwise operation = op == IntegerRangeOperation::BitAnd ? IntegerBitwise::And
            : op == IntegerRangeOperation::BitOr ? IntegerBitwise::Or : IntegerBitwise::Xor;
        return IntegerRangePieces(integer_bitwise_range(operation, proof.width, proof.bias, left, right),
            empty, 1);
    }
    if (op == IntegerRangeOperation::Unknown) {
        return IntegerRangePieces(BitRange(0u64, proof.mask), empty, 1);
    }
    return integer_range(op, proof.width, proof.signed, left, right);
}

u64 append_binary_range(address output, cfn<void,address,BitRange> push, BitRange range,
    u64 minimum) on cpu requires nonzero(push) {
    u64 lower = max(range.lower, minimum);
    if (lower > range.upper) { return 0u64; }
    push(output, BitRange(lower, range.upper));
    return 1u64;
}

extern "C" IntegerBinaryResult range_set_integer_binary(RangeInput left, RangeInput right,
    RangeInput right_counts, address output, RangeSetOutput host, IntegerBinaryProof proof) on cpu
    requires nonzero(left.get) requires nonzero(right.get) requires nonzero(right_counts.get)
    requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push) {
    u32 origin = proof.track_origin == 0 ? 0 : binary_origin(left, right, right_counts, proof);
    if (left.count == 0u64 || right.count == 0u64) { return IntegerBinaryResult(0u64, origin); }
    if (proof.zero_quotient != 0) {
        host.push(output, BitRange(proof.bias, proof.bias));
        return IntegerBinaryResult(1u64, origin);
    }
    if (proof.same_value != 0) {
        IntegerRangeOperation op = proof.operation;
        if (same_value_preserves_operand(op)) {
            return IntegerBinaryResult(intersect_ranges(left, right, output, host.push), origin);
        }
        if (op == IntegerRangeOperation::Subtract || op == IntegerRangeOperation::BitXor
            || op == IntegerRangeOperation::Divide || op == IntegerRangeOperation::Remainder) {
            if (!ranges_overlap(left, right)) { return IntegerBinaryResult(0u64, origin); }
            u64 value = op == IntegerRangeOperation::Divide ? proof.bias ^ 1u64 : proof.bias;
            host.push(output, BitRange(value, value));
            return IntegerBinaryResult(1u64, origin);
        }
    }

    u64 count = 0u64;
    u64 i = 0u64;
    while (i < left.count) {
        BitRange lhs = left.get(left.values, i);
        u64 j = 0u64;
        while (j < right_counts.count) {
            IntegerRangePieces pieces = binary_range_pair(proof, lhs,
                right_counts.get(right_counts.values, j));
            if (pieces.count != 0) {
                count += append_binary_range(output, host.push, pieces.first, proof.minimum_difference);
            }
            if (pieces.count == 2) {
                count += append_binary_range(output, host.push, pieces.second, proof.minimum_difference);
            }
            j += 1u64;
        }
        i += 1u64;
    }
    return IntegerBinaryResult(range_set_normalize(output, count, host), origin);
}

IntegerRangeOperation computed_operation(u32 operation) on cpu {
    if (operation == 0) { return IntegerRangeOperation::Add; }
    if (operation == 1) { return IntegerRangeOperation::Subtract; }
    if (operation == 2) { return IntegerRangeOperation::Multiply; }
    if (operation == 3) { return IntegerRangeOperation::Divide; }
    if (operation == 4) { return IntegerRangeOperation::Remainder; }
    if (operation == 5) { return IntegerRangeOperation::BitAnd; }
    if (operation == 6) { return IntegerRangeOperation::BitOr; }
    if (operation == 7) { return IntegerRangeOperation::BitXor; }
    if (operation == 8) { return IntegerRangeOperation::ShiftLeft; }
    return IntegerRangeOperation::ShiftRight;
}

IntegerBinaryProof computed_proof(u32 opcode, ComputedIntegerType scalar) on cpu {
    return IntegerBinaryProof(computed_operation(opcode), scalar.width, scalar.signed, 0,
        scalar.bias, scalar.mask, 0u64, scalar.bias, scalar.width, 0, 0);
}

bool counted_shift(IntegerRangeOperation operation) on cpu {
    return operation == IntegerRangeOperation::ShiftLeft || operation == IntegerRangeOperation::ShiftRight;
}

BitRange include_clipped_hull(BitRange hull, BitRange range, BitRange limit) on cpu {
    BitRange clipped = BitRange(max(range.lower, limit.lower), min(range.upper, limit.upper));
    if (clipped.lower > clipped.upper) { return hull; }
    if (hull.lower > hull.upper) { return clipped; }
    return BitRange(min(hull.lower, clipped.lower), max(hull.upper, clipped.upper));
}

BitRange computed_range_hull(u32 operation, ComputedIntegerType scalar, BitRange left,
    BitRange right, BitRange limit) on cpu {
    BitRange hull = BitRange(1u64, 0u64);
    if (left.lower > left.upper || right.lower > right.upper) { return hull; }
    IntegerBinaryProof plan = computed_proof(operation, scalar);
    IntegerRangePieces operands = counted_shift(plan.operation)
        ? integer_range_counts(plan.width, plan.bias, right)
        : IntegerRangePieces(right, hull, 1);
    for (part in 0..operands.count) {
        BitRange rhs = part == 0 ? operands.first : operands.second;
        IntegerRangePieces result = binary_range_pair(plan, left, rhs);
        if (result.count != 0) { hull = include_clipped_hull(hull, result.first, limit); }
        if (result.count == 2) { hull = include_clipped_hull(hull, result.second, limit); }
    }
    return hull;
}

// An operand key and its borrowed numeric input are separate results. A
// non-singleton argument without a public identity can still supply intervals.
struct BoundIntegerCallOperand {
    u32 present;
    IntegerCallOperand key;
    address intervals;
    u64 count;
};

u64 prefix_in_mask(IntegerCallBinding input, u64 mask,
    cfn<BitRange,address,u64> interval) on cpu requires nonzero(interval) {
    u64 first = 0u64;
    u64 last = input.count;
    while (first < last) {
        u64 mid = first + (last - first) / 2u64;
        if (interval(input.intervals, mid).lower <= mask) { first = mid + 1u64; }
        else { last = mid; }
    }
    return first;
}

BoundIntegerCallOperand bind_integer_argument(address context, u32 side, IntegerCallOperand operand,
    ComputedIntegerType scalar, IntegerCallHost host) on cpu
    requires nonzero(host.input) requires nonzero(host.interval) {
    if (operand.kind == 2) { return BoundIntegerCallOperand(1, operand, address(), 1u64); }
    IntegerCallBinding input = host.input(context, side, operand.identity);
    if (input.present == 0) {
        return BoundIntegerCallOperand(0, IntegerCallOperand(0, address(), 0u64), address(), 0u64);
    }
    u64 count = prefix_in_mask(input, scalar.mask, host.interval);
    if (count == 1u64) {
        BitRange range = host.interval(input.intervals, 0u64);
        if (range.lower == min(range.upper, scalar.mask)) {
            IntegerCallOperand key = IntegerCallOperand(2, address(), range.lower ^ scalar.bias);
            return BoundIntegerCallOperand(1, key, address(), count);
        }
    }
    u32 kind = input.identity == address() ? 0 : 1;
    return BoundIntegerCallOperand(1, IntegerCallOperand(kind, input.identity, 0u64), input.intervals, count);
}

extern "C" BitRange integer_call_singleton(address value, u64 index) on cpu {
    return unsafe_position<BitRange>(value);
}

RangeInput integer_call_ranges(BoundIntegerCallOperand input, ComputedIntegerType scalar,
    address singleton, address scratch, IntegerCallHost host) on cpu
    requires nonzero(host.interval) requires nonzero(host.ranges.get) requires nonzero(host.ranges.push) {
    if (input.key.kind == 2) {
        u64 at = input.key.bits ^ scalar.bias;
        unsafe_position<BitRange>(singleton) = BitRange(at, at);
        return RangeInput(singleton, 1u64, c_function(integer_call_singleton));
    }
    if (input.count == 0u64 || host.interval(input.intervals, input.count - 1u64).upper <= scalar.mask) {
        return RangeInput(input.intervals, input.count, host.interval);
    }
    // Only the last retained interval can cross the type boundary.
    u64 i = 0u64;
    while (i < input.count) {
        BitRange part = host.interval(input.intervals, i);
        host.ranges.push(scratch, BitRange(part.lower, min(part.upper, scalar.mask)));
        i += 1u64;
    }
    return RangeInput(scratch, input.count, host.ranges.get);
}

struct ComputedIntegerConstant { u32 present; u64 bits; };
ComputedIntegerConstant computed_constant(u32 operation, ComputedIntegerType scalar, u64 left, u64 right) on cpu {
    if (operation == 3 || operation == 4) {
        u32 remainder = operation == 4 ? 1 : 0;
        if (right == 0u64) { return ComputedIntegerConstant(0, 0u64); }
        if (scalar.signed == 0) {
            return ComputedIntegerConstant(1, truncate(divide_unsigned(left, right, remainder), scalar.width));
        }
        i64 divisor = signed_value(right, scalar.width);
        if (divisor == i64(0)) { return ComputedIntegerConstant(0, 0u64); }
        i64 result = divide_signed(signed_value(left, scalar.width), divisor, remainder);
        return ComputedIntegerConstant(1, truncate(u64(result), scalar.width));
    }
    if (scalar.floating != 0) { return ComputedIntegerConstant(0, 0u64); }
    BinaryOperation opcode = BinaryOperation::Add;
    if (operation == 1) { opcode = BinaryOperation::Subtract; }
    else if (operation == 2) { opcode = BinaryOperation::Multiply; }
    else if (operation == 5) { opcode = BinaryOperation::And; }
    else if (operation == 6) { opcode = BinaryOperation::Or; }
    else if (operation == 7) { opcode = BinaryOperation::Xor; }
    else if (operation == 8) { opcode = BinaryOperation::ShiftLeft; }
    else if (operation == 9) { opcode = scalar.signed == 0 ? BinaryOperation::ShiftRight : BinaryOperation::ShiftRightSigned; }
    return ComputedIntegerConstant(1, binary(opcode, scalar.width, scalar.signed, left, right));
}

extern "C" IntegerCallResult project_integer_call(address context, u32 operation, ComputedIntegerType scalar,
    IntegerCallOperand left, IntegerCallOperand right, u32 with_ranges,
    IntegerCallStorage storage, IntegerCallHost host) on cpu
    requires nonzero(host.input) requires nonzero(host.interval)
    requires nonzero(host.ranges.get) requires nonzero(host.ranges.set) requires nonzero(host.ranges.push) {
    IntegerCallOperand missing = IntegerCallOperand(0, address(), 0u64);
    IntegerCallResult result = IntegerCallResult(0, 0, 0u64, missing, missing);
    BoundIntegerCallOperand a = bind_integer_argument(context, 0, left, scalar, host);
    if (a.present == 0 || (with_ranges == 0 && a.key.kind == 0)) { return result; }
    BoundIntegerCallOperand b = bind_integer_argument(context, 1, right, scalar, host);
    if (b.present == 0) { return result; }
    result.present = 1;
    result.left = a.key;
    result.right = b.key;
    if (with_ranges == 0) { return result; }
    if (a.key.kind == 2 && b.key.kind == 2) {
        ComputedIntegerConstant exact = computed_constant(operation, scalar, a.key.bits, b.key.bits);
        if (exact.present != 0) {
            u64 at = exact.bits ^ scalar.bias;
            host.ranges.push(storage.output, BitRange(at, at));
            result.count = 1u64;
            result.has_ranges = 1;
        }
        return result;
    }

    BitRange first = BitRange(0u64, 0u64);
    BitRange second = BitRange(0u64, 0u64);
    RangeInput lhs = integer_call_ranges(a, scalar, address_of(first), storage.left, host);
    RangeInput rhs = integer_call_ranges(b, scalar, address_of(second), storage.right, host);
    RangeInput counts = rhs;
    IntegerBinaryProof proof = computed_proof(operation, scalar);
    if (counted_shift(proof.operation)) {
        u64 count = range_set_counts(rhs, storage.counts, host.ranges, scalar.width, scalar.bias);
        counts = RangeInput(storage.counts, count, host.ranges.get);
    }
    result.count = range_set_integer_binary(lhs, rhs, counts, storage.output, host.ranges, proof).count;
    result.has_ranges = 1;
    return result;
}

extern "C" u64 range_set_map(RangeInput input, address operation,
    cfn<u64,address,BitRange,address> invoke, address output, RangeSetOutput host) on cpu
    requires nonzero(input.get) requires nonzero(invoke) requires nonzero(host.get) requires nonzero(host.set) {
    u64 count = 0u64;
    u64 index = 0u64;
    while (index < input.count) {
        count = invoke(operation, input.get(input.values, index), output);
        index += 1u64;
    }
    return range_set_normalize(output, count, host);
}
