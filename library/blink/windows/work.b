module blink.windows.work;

extern "C" handle CreateThreadpoolWork(cfn<void,address,address,address> callback,
    address context, address environment) on cpu requires nonzero(callback) from "KERNEL32.dll";
extern "C" void SubmitThreadpoolWork(handle work) on cpu from "KERNEL32.dll";
extern "C" void WaitForThreadpoolWorkCallbacks(handle work, i32 cancel) on cpu from "KERNEL32.dll";
extern "C" void CloseThreadpoolWork(handle work) on cpu from "KERNEL32.dll";
extern "C" u32 GetLastError() on cpu from "KERNEL32.dll";
extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address bytes) on cpu from "KERNEL32.dll";
extern "C" void RtlMoveMemory(address output, address input, u64 bytes) on cpu from "KERNEL32.dll";

extern "C" CreatedWork open_work(cfn<void,address,address,address> entry, address context) on cpu
    requires nonzero(entry) {
    handle work = CreateThreadpoolWork(entry, context, address());
    return CreatedWork(work, work == handle() ? GetLastError() : 0);
}

extern "C" void submit_work(handle work) on cpu { SubmitThreadpoolWork(work); }

extern "C" void complete_work(handle work) on cpu {
    WaitForThreadpoolWorkCallbacks(work, i32(0));
    CloseThreadpoolWork(work);
}

extern "C" RangePlan plan_ranges(u32 count, u32 workers, u32 width) on cpu
    requires nonzero(workers) requires nonzero(width) {
    u64 target = u64(workers) * 4u64;
    if (count == 0) { return RangePlan(0u64, u64(width), 0u64, 0u64); }
    u64 last = u64(count - 1);
    u64 grain = last / target + 1u64;
    u64 aligned = ((grain - 1u64) / u64(width) + 1u64) * u64(width);
    u64 ranges = last / aligned + 1u64;
    return RangePlan(u64(count), aligned, ranges, min(u64(workers), ranges));
}

struct BufferSlot { address data; u64 stride; };

address offset_at(address start, u64 bytes) on cpu {
    return unsafe_address(address_bits(start) + bytes);
}

void skip_ranges(RangeInput input, u64 first) on cpu requires nonzero(input.add) {
    input.add(input.memory_failures, 1u64);
    u64 range = first;
    while (range < input.plan.ranges) {
        unsafe_position<address>(offset_at(input.errors, range * 8u64)) = address();
        range = input.add(input.cursor, 1u64);
    }
}

void execute_worker(RangeInput input) on cpu requires nonzero(input.entry) requires nonzero(input.add) {
    u64 range = input.add(input.cursor, 1u64);
    if (range >= input.plan.ranges) { return; }

    u64 uniform_bytes = input.frame_words * 4u64;
    u64 prefix_bytes = input.parameter_slot * size_of<BufferSlot>();
    bool private_slot = address_bits(input.parameters) != 0u64;
    u64 slot_start = (uniform_bytes + 7u64) & ~7u64;
    u64 bytes = private_slot ? slot_start + prefix_bytes + size_of<BufferSlot>() : uniform_bytes;
    address scratch = address();
    address uniforms = input.uniforms;
    address buffers = input.buffers;
    address slot = address();
    if (bytes != 0u64) {
        scratch = HeapAlloc(GetProcessHeap(), 0, bytes);
        if (address_bits(scratch) == 0u64) {
            skip_ranges(input, range);
            return;
        }
        if (uniform_bytes != 0u64) {
            uniforms = scratch;
            RtlMoveMemory(uniforms, input.uniforms, uniform_bytes);
        }
        if (private_slot) {
            buffers = offset_at(scratch, slot_start);
            if (prefix_bytes != 0u64) { RtlMoveMemory(buffers, input.buffers, prefix_bytes); }
            slot = offset_at(buffers, prefix_bytes);
        }
    }

    while (range < input.plan.ranges) {
        if (private_slot) {
            u64 stride = input.parameter_words * 4u64;
            unsafe_position<BufferSlot>(slot) = BufferSlot(offset_at(input.parameters, range * stride), stride);
        }
        u64 begin = range * input.plan.grain;
        u64 end = min(begin + input.plan.grain, input.plan.count);
        address output = input.result_words == 0u64 ? address()
            : offset_at(input.partials, range * input.result_words * 4u64);
        address failure = address();
        if (input.observed_entry != cfn<address,address,address,address,u32,u32,address,u64>()) {
            failure = input.observed_entry(input.context, buffers, uniforms, u32(end), u32(begin), output, range);
        } else {
            failure = input.entry(buffers, uniforms, u32(end), u32(begin), output);
        }
        unsafe_position<address>(offset_at(input.errors, range * 8u64)) = failure;
        range = input.add(input.cursor, 1u64);
    }
    if (address_bits(scratch) != 0u64) { HeapFree(GetProcessHeap(), 0, scratch); }
}

extern "C" void range_callback(address instance, address context, address work) on cpu {
    RangeInput input = unsafe_position<RangeInput>(context);
    if (input.entry != cfn<address,address,address,u32,u32,address>()
        && input.add != cfn<u64,address,u64>()) {
        execute_worker(input);
    }
}

extern "C" RangeCompletion run_ranges(RangeInput input) on cpu
    requires nonzero(input.entry) requires nonzero(input.add) {
    if (input.plan.ranges == 0u64) { return RangeCompletion(0, 1); }
    CreatedWork work = open_work(c_function(range_callback), address_of(input));
    if (work.work == handle()) { return RangeCompletion(work.error, 0); }
    u64 submitted = 0u64;
    while (submitted < input.plan.submissions) {
        submit_work(work.work);
        submitted += 1u64;
    }
    complete_work(work.work);
    // All writers to the failure word have joined. The caller still holds it.
    u32 error = unsafe_position<u64>(input.memory_failures) == 0u64 ? 0 : 8;
    return RangeCompletion(error, 1);
}
