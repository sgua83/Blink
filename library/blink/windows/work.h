module blink.windows.work;

struct CreatedWork { handle work; u32 error; };

// The context and callback hold their original lifetime through complete_work.
extern "C" CreatedWork open_work(cfn<void,address,address,address> entry, address context) on cpu
    requires nonzero(entry);
extern "C" void submit_work(handle work) on cpu;
extern "C" void complete_work(handle work) on cpu;

struct RangePlan {
    u64 count;
    u64 grain;
    u64 ranges;
    u64 submissions;
};
extern "C" RangePlan plan_ranges(u32 count, u32 workers, u32 width) on cpu
    requires nonzero(workers) requires nonzero(width);

struct RangeInput {
    RangePlan plan;
    cfn<address,address,address,u32,u32,address> entry;
    address buffers;
    u64 parameter_slot;
    address uniforms;
    u64 frame_words;
    address partials;
    u64 result_words;
    address errors;
    address parameters;
    u64 parameter_words;
    address context;
    cfn<address,address,address,address,u32,u32,address,u64> observed_entry;
    address cursor;
    address memory_failures;
    cfn<u64,address,u64> add;
};
struct RangeCompletion { u32 error; u32 joined; };

// The caller lends one original slot per range and keeps all input and code
// storage alive until the returned completion. On join, every error slot is written.
extern "C" RangeCompletion run_ranges(RangeInput input) on cpu
    requires nonzero(input.entry) requires nonzero(input.add);
