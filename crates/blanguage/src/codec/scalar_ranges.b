module codec.scalar_ranges;

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

bool covers_from(read RangeQuery query, address intervals, u64 start, u64 count,
    cfn<BitRange,address,u64> interval) on cpu requires nonzero(interval) {
    u64 cursor = query.range.lower;
    u64 index = start;
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

extern "C" bool covers_range(read RangeQuery query, address intervals, u64 count,
    cfn<BitRange,address,u64> interval) on cpu requires nonzero(interval) {
    return covers_from(query, intervals, 0u64, count, interval);
}

extern "C" bool element_ranges_within(RangeInput actual, RangeInput allowed,
    u64 order_bias, BitPattern known, BitPattern required) on cpu
    requires nonzero(actual.get) requires nonzero(allowed.get) {
    u64 index = 0u64;
    u64 start = 0u64;
    u64 previous = 0u64;
    while (index < actual.count) {
        BitRange value = actual.get(actual.values, index);
        if (value.lower < previous) { start = 0u64; }
        previous = value.lower;
        while (start < allowed.count) {
            BitRange candidate = allowed.get(allowed.values, start);
            if (max(candidate.lower, candidate.upper) >= value.lower) { break; }
            start += 1u64;
        }
        RangeQuery query = RangeQuery(order_bias, value, known, required);
        if (!covers_from(query, allowed.values, start, allowed.count, allowed.get)) { return false; }
        index += 1u64;
    }
    return true;
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

void sift_ranges(address values, u64 root, u64 count, cfn<BitRange,address,u64> get,
    cfn<void,address,u64,BitRange> set) on cpu requires nonzero(get) requires nonzero(set) {
    u64 current = root;
    while (current < count / 2u64) {
        u64 child = current + current + 1u64;
        if (child + 1u64 < count && range_set_less(get(values, child), get(values, child + 1u64))) {
            child += 1u64;
        }
        BitRange parent = get(values, current);
        BitRange value = get(values, child);
        if (!range_set_less(parent, value)) { return; }
        set(values, current, value);
        set(values, child, parent);
        current = child;
    }
}

void sort_ranges(address values, u64 count, cfn<BitRange,address,u64> get,
    cfn<void,address,u64,BitRange> set) on cpu requires nonzero(get) requires nonzero(set) {
    u64 root = count / 2u64;
    while (root != 0u64) {
        root -= 1u64;
        sift_ranges(values, root, count, get, set);
    }
    u64 end = count;
    while (end > 1u64) {
        end -= 1u64;
        BitRange first = get(values, 0u64);
        BitRange last = get(values, end);
        set(values, 0u64, last);
        set(values, end, first);
        sift_ranges(values, 0u64, end, get, set);
    }
}

void sort_if_unordered(RangeInput source, cfn<void,address,u64,BitRange> set) on cpu
    requires nonzero(source.get) requires nonzero(set) {
    if (source.count < 2u64) { return; }
    BitRange previous = source.get(source.values, 0u64);
    u64 index = 1u64;
    while (index < source.count) {
        BitRange value = source.get(source.values, index);
        if (range_set_less(value, previous)) {
            sort_ranges(source.values, source.count, source.get, set);
            return;
        }
        previous = value;
        index += 1u64;
    }
}

void intersect<const Join:u32>(RangeInput left, RangeInput right,
    address output, cfn<void,address,BitRange> push) on cpu
    requires nonzero(left.get) requires nonzero(right.get) requires nonzero(push) {
    if (left.count == 0u64 || right.count == 0u64) { return; }
    u64 i = 0u64;
    u64 j = 0u64;
    BitRange a = left.get(left.values, i);
    BitRange b = right.get(right.values, j);
    RangeSetNext pending = RangeSetNext(BitRange(0u64, 0u64), 0);
    while (true) {
        u64 lower = max(a.lower, b.lower);
        u64 upper = min(a.upper, b.upper);
        if (lower <= upper) {
            BitRange value = BitRange(lower, upper);
            if (Join != 0) {
                pending = range_set_accept(pending, value, output, push);
            } else {
                push(output, value);
            }
        }
        bool move_left = a.upper <= b.upper;
        bool move_right = b.upper <= a.upper;
        i += move_left ? 1u64 : 0u64;
        j += move_right ? 1u64 : 0u64;
        if (i == left.count || j == right.count) { break; }
        if (move_left) { a = left.get(left.values, i); }
        if (move_right) { b = right.get(right.values, j); }
    }
    if (Join != 0 && pending.present != 0) { push(output, pending.value); }
}

extern "C" void intersect_ranges(RangeInput left, RangeInput right,
    address output, cfn<void,address,BitRange> push) on cpu
    requires nonzero(left.get) requires nonzero(right.get) requires nonzero(push) {
    intersect<0>(left, right, output, push);
}

extern "C" BitPattern intersect_element_ranges(RangeInput left, RangeInput right,
    cfn<void,address,u64,BitRange> write_left, cfn<void,address,u64,BitRange> write_right,
    BitPattern left_bits, BitPattern right_bits, address output, cfn<void,address,BitRange> push) on cpu
    requires nonzero(left.get) requires nonzero(right.get)
    requires nonzero(write_left) requires nonzero(write_right) requires nonzero(push) {
    BitPattern combined = BitPattern(left_bits.mask | right_bits.mask, left_bits.value | right_bits.value);
    u64 conflict = (left_bits.value ^ right_bits.value) & left_bits.mask & right_bits.mask;
    if (conflict != 0u64 || left.count == 0u64 || right.count == 0u64) { return combined; }
    sort_if_unordered(left, write_left);
    sort_if_unordered(right, write_right);
    intersect<1>(left, right, output, push);
    return combined;
}
