module blink.windows.cpu_sort;

import blink.windows.work;

struct SortBuffer { address data; u64 stride; };
struct SortPass {
    address entry;
    address buffers;
    RangePlan plan;
    address parameters;
    u64 parameter_slot;
    u64 parameter_words;
};
struct SortInput {
    SortBuffer source;
    SortBuffer target;
    u32 count;
    u32 workers;
    address initializers;
    u64 keys;
    address histogram;
    address scatter;
    address gather;
    address context;
    cfn<u32,address,SortPass> execute;
};
enum SortStatus : u32 { Complete = 0; Allocation = 1; NativePass = 2; };

// A pass completes all its ranges before returning. The initializer table is
// borrowed in the order supplied by the native sort image.
extern "C" SortStatus sort_records(SortInput input) on cpu
    requires nonzero(input.workers) requires nonzero(input.execute);
