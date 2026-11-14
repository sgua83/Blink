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
