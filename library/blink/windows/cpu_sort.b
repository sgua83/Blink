module blink.windows.cpu_sort;

import blink.windows.work;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address bytes) on cpu from "KERNEL32.dll";

struct Histogram { u32 shift; array<u32,16> bins; };
struct SortRecord { u32 key; u32 index; };

address offset_at(address start, u64 bytes) on cpu {
    return unsafe_address(address_bits(start) + bytes);
}

void reset_histograms(address rows, u64 ranges, u32 shift) on cpu {
    for (range in 0..u32(ranges)) {
        unsafe_position<Histogram>(offset_at(rows, u64(range) * size_of<Histogram>())) =
            Histogram(shift, array<u32,16>(0));
    }
}

void prefix_histograms(address rows, u64 ranges) on cpu {
    u32 offset = 0;
    for (bin in 0..16) {
        for (range in 0..u32(ranges)) {
            u64 position = u64(range) * size_of<Histogram>() + u64(bin + 1) * size_of<u32>();
            address slot = offset_at(rows, position);
            u32 count = unsafe_position<u32>(slot);
            unsafe_position<u32>(slot) = offset;
            offset += count;
        }
    }
}

SortStatus sort_passes(SortInput input, RangePlan plan, address first, address second,
    address rows) on cpu requires nonzero(input.execute) {
    address source = first;
    address target = second;
    array<SortBuffer,4> buffers = array<SortBuffer,4>(
        input.source, input.target, SortBuffer(source, size_of<SortRecord>()),
        SortBuffer(target, size_of<SortRecord>()));
    SortPass pass = SortPass(address(), address_of(buffers), plan, address(), 4u64,
        size_of<Histogram>() / size_of<u32>());

    u64 key = 0u64;
    while (key < input.keys) {
        buffers[2] = SortBuffer(source, size_of<SortRecord>());
        buffers[3] = buffers[2];
        pass.entry = unsafe_position<address>(offset_at(input.initializers, key * size_of<address>()));
        pass.parameters = address();
        if (input.execute(input.context, pass) != 0) { return SortStatus::NativePass; }
        for (digit in 0..8) {
            reset_histograms(rows, plan.ranges, digit * 4);
            buffers[2] = SortBuffer(source, size_of<SortRecord>());
            buffers[3] = SortBuffer(target, size_of<SortRecord>());
            pass.entry = input.histogram;
            pass.parameters = rows;
            if (input.execute(input.context, pass) != 0) { return SortStatus::NativePass; }
            prefix_histograms(rows, plan.ranges);
            pass.entry = input.scatter;
            if (input.execute(input.context, pass) != 0) { return SortStatus::NativePass; }
            address previous = source;
            source = target;
            target = previous;
        }
        key += 1u64;
    }
    buffers[2] = SortBuffer(source, size_of<SortRecord>());
    pass.entry = input.gather;
    pass.parameters = address();
    if (input.execute(input.context, pass) != 0) { return SortStatus::NativePass; }
    return SortStatus::Complete;
}

extern "C" SortStatus sort_records(SortInput input) on cpu
    requires nonzero(input.workers) requires nonzero(input.execute) {
    if (input.count == 0) { return SortStatus::Complete; }
    RangePlan plan = plan_ranges(input.count, input.workers, 1);
    u64 record_bytes = u64(input.count) * size_of<SortRecord>();
    u64 rows_begin = record_bytes * 2u64;
    handle heap = GetProcessHeap();
    address scratch = HeapAlloc(heap, 0, rows_begin + plan.ranges * size_of<Histogram>());
    if (scratch == address()) { return SortStatus::Allocation; }
    SortStatus status = sort_passes(input, plan, scratch, offset_at(scratch, record_bytes),
        offset_at(scratch, rows_begin));
    HeapFree(heap, 0, scratch);
    return status;
}
