module blink.windows.time;

struct FileTime { u32 low; u32 high; };

extern "C" i32 QueryPerformanceCounter(write i64 count) on cpu
    ensures initialized(count, returned != 0)
    from "KERNEL32.dll";

extern "C" i32 QueryPerformanceFrequency(write i64 frequency) on cpu
    ensures initialized(frequency, returned != 0)
    from "KERNEL32.dll";
