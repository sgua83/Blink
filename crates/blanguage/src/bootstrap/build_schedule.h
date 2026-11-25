module bootstrap.build_schedule;

extern "C" u64 initialize_ready(address ready,address waiting,u64 length) on cpu;

struct ScheduleCounts { u64 ready; u64 unfinished; };
struct UnitRange { u64 start; u64 count; };
struct WorkQueue {
 address ready;
 address waiting;
 address following;
 address followers;
 address counts;
 u64 failure;
};
struct WorkResult { u64 failure; u32 success; };
struct WorkHost {
 cfn<WorkQueue,address> acquire;
 cfn<WorkQueue,address> wait;
 cfn<WorkResult,address,u64> execute;
 cfn<WorkQueue,address,u32> retire;
 cfn<void,address> release;
};

extern "C" void run_worker(address context,WorkHost host) on cpu
 requires nonzero(host.acquire) requires nonzero(host.wait) requires nonzero(host.execute)
 requires nonzero(host.retire) requires nonzero(host.release);
