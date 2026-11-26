module bootstrap.build_schedule;

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

// Existing CPU and GPU workers emit into their original ordered stage batches.
struct ResourceProgram { u64 stages; u32 cpu; };
struct ResourceHost {
 cfn<ResourceProgram,address,u64> ensures memory(read) describe;
 cfn<void,address,u64,u64> cpu_jobs;
 cfn<u32,address> compile_cpu;
 cfn<u32,address,u64> gpu_pages;
 cfn<void,address,u64,u64> gpu_jobs;
 cfn<u32,address> compile_gpu;
 cfn<void,address,u64,u64> emit_cpu;
 cfn<void,address,u64,u64> emit_gpu;
};
extern "C" void compile_resource_images(u64 count,address context,ResourceHost host) on cpu
 requires nonzero(host.describe) requires nonzero(host.cpu_jobs) requires nonzero(host.compile_cpu)
 requires nonzero(host.gpu_pages) requires nonzero(host.gpu_jobs) requires nonzero(host.compile_gpu)
 requires nonzero(host.emit_cpu) requires nonzero(host.emit_gpu);
