module blink.windows.system;

const u32 PERFORMANCE_INFORMATION_SIZE=104;

extern "C" i32 K32GetPerformanceInfo(write SystemMemoryCounters counters,u32 bytes) on cpu
    ensures initialized(counters,returned != 0 && bytes == PERFORMANCE_INFORMATION_SIZE)
    from "KERNEL32.dll";

i32 system_memory_counters(write SystemMemoryCounters counters) on cpu
    ensures initialized(counters,returned != 0) {
    return K32GetPerformanceInfo(counters,PERFORMANCE_INFORMATION_SIZE);
}
