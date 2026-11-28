module blink.windows.cpu_collective;

import codec.builtin_type;

extern "C" void RtlMoveMemory(address destination, address source, u64 bytes) on cpu from "KERNEL32.dll";

address offset_at(address data, u64 offset) on cpu {
    return unsafe_address(address_bits(data) + offset);
}

address row_at(SummaryRows rows, u64 index) on cpu {
    return offset_at(rows.data, index * rows.stride);
}

u64 read_bits(address source, u64 bytes) on cpu {
    u64 bits = 0u64;
    RtlMoveMemory(address_of(bits), source, bytes);
    return bits;
}

void write_bits(address target, u64 bytes, u64 bits) on cpu {
    RtlMoveMemory(target, address_of(bits), bytes);
}

u64 combine(SummaryColumn column, u64 left, u64 right) on cpu {
    CollectiveOperation operation = column.operation;
    if (operation == CollectiveOperation::BitAnd) { return left & right; }
    if (operation == CollectiveOperation::BitOr) { return left | right; }
    if (operation == CollectiveOperation::BitXor) { return left ^ right; }
    if (column.floating != 0) {
        if (column.width == 32) {
            f32 a = bitcast_f32(u32(left));
            f32 b = bitcast_f32(u32(right));
            if (operation == CollectiveOperation::Sum) { return u64(bitcast_u32(a + b)); }
            bool select_left = operation == CollectiveOperation::Min ? a < b : a > b;
            return select_left ? left : right;
        }
        f64 a = bitcast_f64(left);
        f64 b = bitcast_f64(right);
        if (operation == CollectiveOperation::Sum) { return bitcast_u64(a + b); }
        bool select_left = operation == CollectiveOperation::Min ? a < b : a > b;
        return select_left ? left : right;
    }
    if (operation == CollectiveOperation::Sum) {
        return (left + right) & (~0u64 >> u64(64 - column.width));
    }
    u64 bias = column.signed != 0 ? 1u64 << u64(column.width - 1) : 0u64;
    u64 ordered_left = left ^ bias;
    u64 ordered_right = right ^ bias;
    bool select_left = operation == CollectiveOperation::Min ? ordered_left < ordered_right : ordered_left > ordered_right;
    return select_left ? left : right;
}

extern "C" void reduce_column(SummaryRows rows, SummaryColumn column, address output) on cpu {
    u64 bytes = u64(column.width / 8);
    u64 total = collective_identity(column.operation, column.width, column.signed, column.floating);
    u64 index = 0u64;
    while (index < rows.count) {
        address field = offset_at(row_at(rows, index), column.offset);
        total = combine(column, total, read_bits(field, bytes));
        index += 1u64;
    }
    write_bits(offset_at(output, column.offset), bytes, total);
}

extern "C" void scan_column(SummaryRows rows, SummaryColumn column, u64 head, u32 segmented) on cpu {
    u64 bytes = u64(column.width / 8);
    u64 prefix = collective_identity(column.operation, column.width, column.signed, column.floating);
    u64 index = 0u64;
    while (index < rows.count) {
        address row = row_at(rows, index);
        address field = offset_at(row, column.offset);
        u64 current = read_bits(field, bytes);
        bool reset = segmented != 0 && unsafe_position<u32>(offset_at(row, head)) != 0;
        u64 next = reset ? current : combine(column, prefix, current);
        write_bits(field, bytes, prefix);
        prefix = next;
        index += 1u64;
    }
}

extern "C" void segment_counts(SummaryRows rows, u64 metadata, u32 count, address output) on cpu {
    u32 segments = 0;
    u64 index = 0u64;
    while (index < rows.count) {
        address head = offset_at(row_at(rows, index), metadata);
        address partial = offset_at(head, size_of<u32>());
        u32 next = segments + unsafe_position<u32>(partial);
        unsafe_position<u32>(head) = count;
        unsafe_position<u32>(partial) = segments;
        segments = next;
        index += 1u64;
    }
    unsafe_position<u32>(output) = segments;
}

extern "C" void compact_prefix(SummaryRows rows, address output) on cpu {
    u32 prefix = 0;
    u64 index = 0u64;
    while (index < rows.count) {
        address first = row_at(rows, index);
        u32 next = prefix + unsafe_position<u32>(first);
        unsafe_position<u32>(first) = prefix;
        prefix = next;
        index += 1u64;
    }
    if (output != address()) { unsafe_position<u32>(output) = prefix; }
}
